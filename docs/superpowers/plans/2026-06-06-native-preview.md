# Richer Docked Preview + Native Space Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render real content (syntax-highlighted text) in the docked preview pane, and make `Space` invoke the OS-native previewer (macOS `qlmanage -p`, Linux `sushi`) with a fallback to our GTK preview window.

**Architecture:** Part 1 is a one-line gate change in `preview.rs` so text content renders at any size. Part 2 adds a focused `quicklook.rs` module (a pure `native_preview_command` + `binary_on_path` + `open_native_preview`) and rewires the `Space` action in `main.rs` to try native first, then fall back to the existing GTK window.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22, std::process.

**Spec:** `docs/superpowers/specs/2026-06-06-native-preview-design.md`

---

## File Structure

- `src/preview.rs` — remove the `&& large` gate so text content renders in the dock (Task 1).
- `src/quicklook.rs` — **new**: native-previewer resolution + spawn (Task 2).
- `src/main.rs` — register `mod quicklook;`, add `current_selection_path`, rewire the `preview` action (Task 3).
- `README.md`, `CHANGELOG.md` — document (Task 4).

---

## Task 1: Docked pane renders text content

The text branch in `create_preview_layout` is gated on `is_text && large`, so the
docked pane (`large = false`) shows only an icon for text files. Drop the `&& large`
so the source view renders at both sizes; `large` keeps controlling sizing.

**Files:**
- Modify: `src/preview.rs` (the `else if is_text && large {` branch, ~line 45)

- [ ] **Step 1: Make the change**

In `src/preview.rs`, change this line:

```rust
        } else if is_text && large {
```

to:

```rust
        } else if is_text {
```

Leave the entire body of that branch unchanged (it already uses `large` only via the
shared margins/heights set on the container; the source view itself has no `large`
dependency).

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished` with no errors.

- [ ] **Step 3: Clippy**

Run: `cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head` (expect no output)

- [ ] **Step 4: Commit**

```bash
git add src/preview.rs
git commit -m "feat(preview): render text content in the docked pane (not just an icon)"
```

This is a UI rendering change (not unit-testable); it's covered by manual verification
in Task 4.

---

## Task 2: `quicklook.rs` — native previewer resolution + spawn

**Files:**
- Create: `src/quicklook.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/quicklook.rs` with ONLY the test module first (the functions come next):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_qlmanage_when_available() {
        assert_eq!(
            native_preview_command(Path::new("/tmp/x.txt"), true, false),
            Some(vec!["qlmanage".to_string(), "-p".to_string(), "/tmp/x.txt".to_string()])
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_none_without_qlmanage() {
        assert_eq!(native_preview_command(Path::new("/tmp/x.txt"), false, true), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_uses_sushi_when_available() {
        let cmd = native_preview_command(Path::new("/tmp/x.txt"), false, true).unwrap();
        assert_eq!(cmd[0], "sushi");
        assert!(cmd[1].starts_with("file://"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_none_without_sushi() {
        assert_eq!(native_preview_command(Path::new("/tmp/x.txt"), true, false), None);
    }
}
```

- [ ] **Step 2: Register the module so the tests compile**

In `src/main.rs`, add `mod quicklook;` immediately after `mod preview;` (line 4):

```rust
mod preview;
mod quicklook;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test quicklook 2>&1 | tail -20`
Expected: FAIL — `cannot find function `native_preview_command` in this scope`.

- [ ] **Step 4: Implement the module functions**

Add this ABOVE the `#[cfg(test)] mod tests` block in `src/quicklook.rs`:

```rust
use std::path::Path;
use std::process::{Command, Stdio};

/// Resolve the native-previewer command (argv) to spawn for `path`, or None to
/// signal "no native previewer — fall back to our GTK window". Availability is
/// passed in so this is testable on any host.
pub fn native_preview_command(path: &Path, has_qlmanage: bool, has_sushi: bool) -> Option<Vec<String>> {
    if cfg!(target_os = "macos") {
        if has_qlmanage {
            Some(vec![
                "qlmanage".to_string(),
                "-p".to_string(),
                path.to_string_lossy().into_owned(),
            ])
        } else {
            None
        }
    } else if has_sushi {
        let uri = gio::File::for_path(path).uri().to_string();
        Some(vec!["sushi".to_string(), uri])
    } else {
        None
    }
}

/// True if `name` is found in any $PATH entry as a file.
pub fn binary_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

/// Try to open a native preview for `path`. Returns true if a native previewer was
/// launched, false if the caller should fall back to the GTK preview window.
pub fn open_native_preview(path: &Path) -> bool {
    let (has_qlmanage, has_sushi) = if cfg!(target_os = "macos") {
        (binary_on_path("qlmanage"), false)
    } else {
        (false, binary_on_path("sushi"))
    };

    let Some(argv) = native_preview_command(path, has_qlmanage, has_sushi) else {
        return false;
    };

    let mut it = argv.into_iter();
    let Some(cmd) = it.next() else { return false };
    let args: Vec<String> = it.collect();

    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test quicklook 2>&1 | tail -20`
Expected: PASS — the two tests for the current host platform pass (the other two are
`#[cfg]`-d out).

- [ ] **Step 6: Build + clippy**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head`
Expected: `Finished`; no clippy output. If clippy flags the `cfg!(...)` if/else as a
constant condition, that is acceptable here (it keeps one testable function); only
address genuine warnings.

- [ ] **Step 7: Commit**

```bash
git add src/quicklook.rs src/main.rs
git commit -m "feat(quicklook): native previewer resolution and detached spawn"
```

---

## Task 3: Wire `Space` to try native preview, then fall back

**Files:**
- Modify: `src/main.rs` — add `current_selection_path` to `ColumnManager`; change the `preview` action (~lines 691-704).

- [ ] **Step 1: Add the `current_selection_path` accessor**

In `src/main.rs`, add this method to the `impl ColumnManager` block, immediately after
the `focused_dir` method:

```rust
    /// Path of the current single selection, if any.
    fn current_selection_path(&self) -> Option<PathBuf> {
        self.current_selection.borrow().as_ref().map(|s| s.path.clone())
    }
```

- [ ] **Step 2: Rewire the `preview` action**

In `src/main.rs`, replace the existing `preview` action body:

```rust
    let preview_action = gio::SimpleAction::new("preview", None);
    let app_weak_p = app.downgrade();
    preview_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_p.upgrade() {
            ACTIVE_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref()
                    && let Some(window) = app.windows().into_iter().find_map(|w| w.downcast::<ApplicationWindow>().ok()) {
                        manager.toggle_preview(&window);
                    }
            });
        }
    });
    app.add_action(&preview_action);
    app.set_accels_for_action("app.preview", &["space"]);
```

with:

```rust
    let preview_action = gio::SimpleAction::new("preview", None);
    let app_weak_p = app.downgrade();
    preview_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_p.upgrade() {
            ACTIVE_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref()
                    && let Some(window) = app.windows().into_iter().find_map(|w| w.downcast::<ApplicationWindow>().ok()) {
                        // Prefer the OS-native previewer; fall back to our GTK window.
                        if let Some(path) = manager.current_selection_path()
                            && quicklook::open_native_preview(&path) {
                                return;
                            }
                        manager.toggle_preview(&window);
                    }
            });
        }
    });
    app.add_action(&preview_action);
    app.set_accels_for_action("app.preview", &["space"]);
```

- [ ] **Step 3: Build + test**

Run: `cargo build 2>&1 | tail -5 && cargo test 2>cargo test --lib 2>&1 | tail -31 | grep "test result"`
Expected: `Finished` with no errors; all tests pass.

- [ ] **Step 4: Clippy**

Run: `cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head` (expect no output)

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: Space uses native preview (qlmanage/sushi) with GTK fallback"
```

---

## Task 4: Documentation + verification

**Files:**
- Modify: `README.md` (Keyboard Shortcuts table row for Quick Look, ~line 38)
- Modify: `CHANGELOG.md` (add an Unreleased entry at the top)

- [ ] **Step 1: Update the README Quick Look shortcut description**

In `README.md`, replace the Quick Look row:

```markdown
| **Quick Look Preview** | `Space` |
```

with:

```markdown
| **Quick Look Preview** | `Space` (macOS QuickLook / GNOME Sushi when available, else built-in preview) |
```

- [ ] **Step 2: Add a CHANGELOG entry**

In `CHANGELOG.md`, add at the top (after the intro paragraph, before the most recent
`## [...]` heading):

```markdown
## [Unreleased]

### Added
- `Space` now opens the **OS-native previewer** when available — macOS QuickLook (`qlmanage`) or GNOME Sushi on Linux — and falls back to the built-in GTK preview window otherwise.

### Changed
- The docked preview pane now renders file **contents** (e.g. syntax-highlighted text), matching the `Space` view, instead of showing only an icon for text files.

```

- [ ] **Step 3: Commit docs**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: document native Space preview and richer docked pane"
```

- [ ] **Step 4: Full build + lint + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean build, no clippy output, all tests pass.

- [ ] **Step 5: Manual verification**

Run `cargo run` and confirm:
- **Part 1:** Select a text/source file → the docked pane shows its syntax-highlighted
  contents (not just an icon). Images and video still preview in the dock.
- **Part 2 (macOS):** Select a file and press `Space` → the macOS QuickLook panel opens
  with the real preview (images, PDFs, text, etc.).
- **Part 2 (fallback):** On a system without the native previewer (or temporarily rename
  it off `$PATH`), `Space` opens the built-in GTK preview window with its `Esc` + close
  button, exactly as before.
- Pressing `Space` with no selection does nothing.

- [ ] **Step 6: Finish the branch**

Use the `superpowers:finishing-a-development-branch` skill to merge/PR.

---

## Notes for the implementer

- `gio` is available crate-wide (used in other modules); `gio::File::for_path(path).uri()`
  needs no extra `use`. `.uri()` returns a `glib::GString`; `.to_string()` converts it.
- `cfg!(target_os = "macos")` is a runtime const bool — both branches compile, so both
  `has_qlmanage` and `has_sushi` are "used" and there is no dead-parameter warning.
- The `preview` action uses let-chains (`if let ... && quicklook::open_native_preview(&path)`),
  consistent with the rest of `main.rs` (edition 2024).
- Do not change `toggle_preview` or the floating window — it stays as the fallback.
- `PathBuf` is already imported in `main.rs`.
