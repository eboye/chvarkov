# Robustness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the curated High/Medium + cheap-Low bugs from the code audit (UI-thread blocking, stale-state targeting, leaks, clipboard/file-op correctness) without changing features.

**Architecture:** Targeted fixes grouped by concern: focus/state source-of-truth, preview safety (async + regular-file gate), lifecycle/leaks, clipboard/file-op correctness, defensive unwraps. Each task is independently committable.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22, sourceview5, std.

**Spec:** `docs/superpowers/specs/2026-06-07-robustness-hardening-design.md`

---

## File Structure

- `src/main.rs` — focus source-of-truth (A1/A2), batch-delete reconciliation (A3), docked-video stop (B3), `build_ui` timer leak (C1), copy-path non-UTF-8 (D2), unwraps (E1).
- `src/preview.rs` — regular-file gate + async text + no-autoplay-in-dock (B1/B2/B3).
- `src/file_ops.rs` — batch delete (A3), reap children (C2), non-UTF-8 email/zip (D2), symlink replace (D3), canonicalize guard (D4).
- `src/quicklook.rs` — reap child (C2).
- `src/clipboard.rs` — honor cut/copy verb (D1).
- `src/column.rs`, `src/icon_view.rs`, `src/list_view.rs` — factory unwraps (E1).
- `CHANGELOG.md` — document.

---

## Task 1: Focus source-of-truth (A1 + A2)

Remove the stale CSS-class fallback so destructive actions never target the wrong view, and make `focused_dir` prefer the authoritative focused view.

**Files:** Modify `src/main.rs` — `get_focused_list_view` (~lines 1865-1901), `focused_dir` (~lines 1644-1657).

- [ ] **Step 1: Remove the CSS-class fallback from `get_focused_list_view`**

Replace the fallback block (the part after the PRIMARY focus lookup, starting at the `// FALLBACK` comment through the final `None`) so the function ends right after the primary lookup:

```rust
        if let Some(focus) = root.and_then(|r| r.focus()) {
            let entries = self.entries.borrow();
            for entry in entries.iter() {
                if focus == entry.focus_target || focus.is_ancestor(&entry.focus_target) {
                    return Some(entry.focus_target.clone());
                }
            }
            if let Some(main) = self.main_view.borrow().as_ref()
                && (focus == *main || focus.is_ancestor(main)) {
                    return Some(main.clone());
                }
        }

        // No live keyboard focus: return None so destructive actions no-op rather
        // than act on the wrong view. CSS classes (focused-column/grid/list) are
        // styling only and are deliberately NOT consulted here (they are updated
        // by arrow-key nav only and can be stale — that mismatch caused a
        // delete-the-wrong-folder data-loss bug).
        None
    }
```

(Delete the two `has_css_class(...)` loops entirely.)

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished`, no errors (no now-unused warnings; `has_css_class` simply no longer called).

- [ ] **Step 3: Harden `focused_dir` to prefer the focused view**

`focused_dir` currently does `let view = self.get_focused_list_view()?;` then matches `entries`. Keep that, but make the no-focus case fall through to the existing fallbacks instead of early-returning `None`. Replace the function body's first lines:

```rust
    pub(crate) fn focused_dir(&self) -> Option<PathBuf> {
        if let Some(view) = self.get_focused_list_view() {
            for entry in self.entries.borrow().iter() {
                if entry.focus_target == view {
                    return Some(entry.path.clone());
                }
            }
        }
        if let Some(sel) = self.current_selection.borrow().as_ref()
            && let Some(parent) = sel.path.parent() { return Some(parent.to_path_buf()); }
        let settings = gio::Settings::new("net.nocopypaste.chvarkov");
        let p: String = settings.get("current-path");
        if p.is_empty() { Some(glib::home_dir()) } else { Some(PathBuf::from(p)) }
    }
```

(This preserves the icon/list-view case — where the focused `main_view` isn't in `entries`, the `current-path` GSetting is the correct listed dir — while a focused Miller column still wins.)

- [ ] **Step 4: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean build, no clippy output, tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "fix(focus): drop stale CSS-class fallback for destructive targeting; harden focused_dir"
```

---

## Task 2: Batch-delete state reconciliation (A3)

After a multi-file trash/delete, clear the focused view's selection once and reconcile columns once (instead of per-path against a mutating `entries`).

**Files:** Modify `src/main.rs` (`ColumnManager`), `src/file_ops.rs` (`trash`, `delete`).

- [ ] **Step 1: Add a batch reconciliation method on `ColumnManager`**

In `src/main.rs`, add after the existing `on_file_deleted` method:

```rust
    /// Called once after a batch trash/delete. Clears the focused view's selection
    /// (so deleted rows don't stay highlighted) and collapses any columns deeper
    /// than the shallowest affected parent. The monitored DirectoryList refreshes
    /// the rest.
    pub(crate) fn on_files_deleted(&self, deleted: &[std::path::PathBuf]) {
        *self.current_selection.borrow_mut() = None;

        // Clear the focused view's selection model, if any.
        if let Some(view) = self.get_focused_list_view()
            && let Some(model) = get_selection_model(&view) {
                model.unselect_all();
            }

        // Shallowest affected parent → how many columns to keep.
        let mut keep_count = self.entries.borrow().len();
        for path in deleted {
            if let Some(parent) = path.parent() {
                let entries = self.entries.borrow();
                if let Some(i) = entries.iter().position(|e| e.path == parent) {
                    keep_count = keep_count.min(i + 1);
                }
            }
        }

        let mut entries = self.entries.borrow_mut();
        while entries.len() > keep_count {
            let entry = entries.pop().unwrap();
            self.columns_box.remove(&entry.container);
        }
        drop(entries);

        self.update_preview_if_open();
        self.update_dock(false);
    }
```

- [ ] **Step 2: Call the batch method once from `trash` and `delete`**

In `src/file_ops.rs`, find where `trash` and `delete` currently call `manager.on_file_deleted(...)` per path. Replace the per-path `on_file_deleted` calls with a single `manager.on_files_deleted(&paths)` after the batch loop completes (where `paths` is the full set acted on). Keep the actual filesystem trash/remove logic unchanged; only move the UI reconciliation to one call at the end.

Concretely, in each of `trash` and `delete`, after the loop that processes every path, add:

```rust
        manager.on_files_deleted(&paths);
```

and remove the in-loop `manager.on_file_deleted(p)` call(s). If `paths` was consumed by the loop, clone it first (`let all = paths.clone();`) before the loop and pass `&all`.

- [ ] **Step 3: Keep `on_file_deleted` if still referenced; otherwise allow it**

Run `grep -rn "on_file_deleted" src/`. If no callers remain, either delete the method or add nothing (it's `pub(crate)`; an unused `pub(crate)` fn triggers a dead-code warning). Prefer deleting `on_file_deleted` if `on_files_deleted` fully replaces it. If another caller remains, leave it.

- [ ] **Step 4: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean, tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/file_ops.rs
git commit -m "fix(delete): clear selection and reconcile columns once after batch delete"
```

---

## Task 3: Preview safety — regular-file gate, async text, dock no-autoplay (B1 + B2 + B3)

**Files:** Modify `src/preview.rs` (`create_preview_layout`), `src/main.rs` (`update_dock`).

- [ ] **Step 1: Gate media/text on regular files and stop dock autoplay**

In `src/preview.rs`, replace the `is_image`/`is_video`/`is_text` flag computation and the `if is_image { ... }` chain's media branches so they only run for regular files, and the video only autoplays in the `large` (floating) view. Change the flags:

```rust
        let content_type = file_info.content_type();
        let is_regular = file_info.file_type() == gio::FileType::Regular;
        let is_image = is_regular && content_type.as_ref().map(|ct| utils::is_content_type_a(ct, "image/*")).unwrap_or(false);
        let is_video = is_regular && content_type.as_ref().map(|ct| utils::is_content_type_a(ct, "video/*")).unwrap_or(false);
        let is_text = is_regular && content_type.as_ref().map(|ct| utils::is_content_type_a(ct, "text/*")).unwrap_or(false);
```

And change the video builder to only autoplay/loop when `large`:

```rust
            let video = gtk::Video::builder()
                .file(&file)
                .autoplay(large)
                .loop_(large)
                .hexpand(true)
                .vexpand(true)
                .height_request(if large { 400 } else { 200 })
                .build();
```

Non-regular files now fall through to the icon/info branch automatically (no blocking open of FIFOs/sockets/devices).

- [ ] **Step 2: Read text content off the UI thread**

In `src/preview.rs`, replace the synchronous text read block:

```rust
            if let Ok(file) = std::fs::File::open(path) {
                use std::io::Read;
                let mut content = Vec::new();
                file.take(10000).read_to_end(&mut content).ok(); // Read first 10KB
                let text = String::from_utf8_lossy(&content);
                buffer.set_text(&text);
            }
```

with an async read that never blocks the UI thread:

```rust
            let path_buf = path.to_path_buf();
            let buffer_clone = buffer.clone();
            glib::spawn_future_local(async move {
                let content = gio::spawn_blocking(move || {
                    use std::io::Read;
                    let mut buf = Vec::new();
                    if let Ok(file) = std::fs::File::open(&path_buf) {
                        let _ = file.take(10000).read_to_end(&mut buf);
                    }
                    buf
                })
                .await
                .unwrap_or_default();
                let text = String::from_utf8_lossy(&content);
                buffer_clone.set_text(&text);
            });
```

(`buffer` is a `sourceview::Buffer`, which is a GObject — `.clone()` is a refcount bump and moving it into the async closure is fine. The view is added to the layout immediately; text fills in when the read completes.)

- [ ] **Step 3: Stop dock video playback on hide**

In `src/main.rs` `update_dock`, the hide path does `content.set_child(None::<&gtk::Widget>)`. Dropping the layout tears down the video, but make teardown deterministic: before clearing, walk the current child for a `gtk::Video` and clear its stream. Add a helper near `update_dock`:

```rust
    fn stop_dock_video(content: &ScrolledWindow) {
        // The preview layout is a Box; find a GtkVideo descendant and clear it.
        if let Some(child) = content.child() {
            let mut stack = vec![child];
            while let Some(w) = stack.pop() {
                if let Ok(video) = w.clone().downcast::<gtk::Video>() {
                    video.set_media_stream(gtk::MediaStream::NONE);
                }
                let mut c = w.first_child();
                while let Some(node) = c {
                    stack.push(node.clone());
                    c = node.next_sibling();
                }
            }
        }
    }
```

and call it in `update_dock` before BOTH `content.set_child(...)` calls (the show path replaces the child, the hide path clears it):

```rust
        // (in the show branch, before content.set_child(Some(&layout)))
        Self::stop_dock_video(&content);
        // (in the hide branch, before content.set_child(None::<&gtk::Widget>))
        Self::stop_dock_video(&content);
```

`gtk::MediaStream::NONE` is the typed `Option::None` for the stream parameter.

- [ ] **Step 4: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean. If `gtk::MediaStream::NONE` doesn't resolve, use `None::<&gtk::MediaStream>`.

- [ ] **Step 5: Commit**

```bash
git add src/preview.rs src/main.rs
git commit -m "fix(preview): gate media on regular files, async text load, stop dock video"
```

---

## Task 4: Lifecycle — build_ui timer leak + reap children (C1 + C2)

**Files:** Modify `src/main.rs` (timer), `src/quicklook.rs` and `src/file_ops.rs` (reaping).

- [ ] **Step 1: Stop the 500ms responsive-label timer from accumulating**

In `src/main.rs`, the responsive-label timer is created with `glib::timeout_add_local(std::time::Duration::from_millis(500), ...)` (~line 1197) on every `build_ui`. Store its `SourceId` in a `thread_local!` and remove the prior one before creating a new one. Near the top of `main.rs` (with the other `thread_local!`/statics), add:

```rust
thread_local! {
    static LABEL_TIMER: std::cell::RefCell<Option<glib::SourceId>> = const { std::cell::RefCell::new(None) };
}
```

Then wrap the timer creation: replace `glib::timeout_add_local(std::time::Duration::from_millis(500), move || { ... });` with:

```rust
    LABEL_TIMER.with(|t| {
        if let Some(old) = t.borrow_mut().take() {
            old.remove();
        }
        let id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            // ... existing closure body unchanged ...
        });
        *t.borrow_mut() = Some(id);
    });
```

(Keep the existing closure body verbatim.) This guarantees at most one label timer regardless of how many times `build_ui` runs.

- [ ] **Step 2: Build to confirm the timer change compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished`. (`SourceId::remove` consumes the id; `.take()` moves it out — correct.)

- [ ] **Step 3: Reap spawned child processes in `quicklook.rs`**

In `src/quicklook.rs` `open_native_preview`, the spawn currently is `Command::new(cmd).args(args)...spawn().is_ok()`. Capture the child and reap it on a detached thread so it never becomes a zombie:

```rust
    match Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            std::thread::spawn(move || {
                let mut child = child;
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
```

- [ ] **Step 4: Reap spawned children in `file_ops.rs`**

In `src/file_ops.rs`, find every `Command::new(...)....spawn()` (e.g. `open_terminal`, `email` via `osascript`/`xdg-email`, and any `share` spawn). For each, capture the `Child` and reap on a detached thread instead of dropping it. Apply this pattern to each spawn site:

```rust
        if let Ok(child) = <the existing Command builder>.spawn() {
            std::thread::spawn(move || {
                let mut child = child;
                let _ = child.wait();
            });
        }
```

Preserve any existing error/toast handling; only add the reaping thread where a `Child` was previously dropped. Do NOT change `gio::spawn_blocking` calls (those are not child processes).

- [ ] **Step 5: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean, tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/quicklook.rs src/file_ops.rs
git commit -m "fix(lifecycle): single label timer across rebuilds; reap spawned child processes"
```

---

## Task 5: Clipboard + file-op correctness (D1 + D2 + D3 + D4)

**Files:** Modify `src/clipboard.rs`, `src/file_ops.rs`, `src/main.rs`.

- [ ] **Step 1: D1 — write the failing test for the cut/copy verb parser**

In `src/clipboard.rs`, add to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn clipboard_mode_reads_verb() {
        assert_eq!(clipboard_mode("cut\nfile:///tmp/a"), Mode::Cut);
        assert_eq!(clipboard_mode("copy\nfile:///tmp/a"), Mode::Copy);
        assert_eq!(clipboard_mode("file:///tmp/a"), Mode::Copy); // no verb -> Copy
    }
```

- [ ] **Step 2: D1 — run the test to confirm it fails**

Run: `cargo test clipboard_mode 2>&1 | tail -10`
Expected: FAIL — `cannot find function `clipboard_mode``.

- [ ] **Step 3: D1 — implement `clipboard_mode` and honor it on paste**

Add the pure helper to `src/clipboard.rs` (near `parse_uri_lines`):

```rust
/// First line `cut` => Mode::Cut; anything else (incl. `copy` or no verb) => Copy.
fn clipboard_mode(text: &str) -> Mode {
    match text.lines().next().map(str::trim) {
        Some("cut") => Mode::Cut,
        _ => Mode::Copy,
    }
}
```

Then make `read_external` read the GNOME `x-special/gnome-copied-files` payload (which carries the verb), falling back to plain text. Replace the body of `read_external` with:

```rust
pub fn read_external<F: Fn(Option<ClipboardOp>) + 'static>(callback: F) {
    let Some(display) = gtk::gdk::Display::default() else { callback(None); return };
    let clipboard = display.clipboard();
    clipboard.read_async(
        &["x-special/gnome-copied-files", "text/uri-list", "text/plain;charset=utf-8"],
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |res| {
            let Ok((stream, _mime)) = res else { callback(None); return };
            let out = gio::MemoryOutputStream::new_resizable();
            let out2 = out.clone();
            out.clone().splice_async(
                &stream,
                gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
                glib::Priority::DEFAULT,
                gio::Cancellable::NONE,
                move |_res| {
                    let bytes = out2.steal_as_bytes();
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let paths = parse_uri_lines(&text);
                    let op = if paths.is_empty() {
                        None
                    } else {
                        Some(ClipboardOp { mode: clipboard_mode(&text), paths })
                    };
                    callback(op);
                },
            );
        },
    );
}
```

(If `read_async`/`splice_async` signatures differ in this gtk4/gio version, adjust to compile — the intent is: read the first available of those MIME types into a string, parse paths via `parse_uri_lines`, and derive mode via `clipboard_mode`. The previous `read_text_async` path is replaced.)

- [ ] **Step 4: D1 — run tests**

Run: `cargo test clipboard 2>&1 | tail -10`
Expected: PASS — `clipboard_mode_reads_verb` and the existing clipboard tests pass.

- [ ] **Step 5: D3 — symlink-aware replace in `transfer`**

In `src/file_ops.rs`, the conflict "replace" branch removes the existing target with `if t.is_dir() { remove_dir_all } else { remove_file }`. `is_dir()` follows symlinks. Change it to decide via `symlink_metadata` so a symlinked target is removed as a link, not its target tree. Replace that removal expression with:

```rust
                            let removed = gio::spawn_blocking(move || {
                                let is_real_dir = std::fs::symlink_metadata(&t)
                                    .map(|m| m.file_type().is_dir())
                                    .unwrap_or(false);
                                if is_real_dir { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) }
                            }).await;
```

(`symlink_metadata().file_type().is_dir()` is false for a symlink, so a symlink-to-dir is removed with `remove_file` — correct.)

- [ ] **Step 6: D4 — refuse the into-itself move when canonicalize fails**

In `src/file_ops.rs` `transfer`, the directory-into-itself guard is gated on both canonicalizations succeeding. Make a canonicalize failure on a directory source refuse that item rather than silently skip the guard. Right after `let src_canon = src.canonicalize().ok();` add:

```rust
            if src.is_dir() && src_canon.is_none() {
                manager.send_toast(&format!("Can't verify \u{201c}{name}\u{201d}; skipped for safety"));
                failed += 1;
                continue;
            }
```

(`name`, `failed` are already in scope in the loop.)

- [ ] **Step 7: D2 — non-UTF-8 guards**

In `src/file_ops.rs`:
- In `zip_paths`, the archive entry name uses `rel.to_string_lossy()`. Skip entries whose relative path is not valid UTF-8 (a zip entry name must be a real string):
  ```rust
      let Some(name) = rel.to_str() else { continue };
  ```
  (use `name` for the zip entry; `continue` skips non-UTF-8-named files.)
- In `email`, filter the paths to UTF-8 before building the argv (mirror the share sheet which already filters), e.g. when collecting paths: `.filter_map(|p| p.to_str().map(str::to_string))` or skip non-`to_str()` paths.

In `src/main.rs`, the `copy-path`/`copy-uri`/`copy-name` actions use `to_string_lossy()`. Leave copy-path/name as best-effort (display), but add a short comment noting it is lossy for non-UTF-8 names. No behavior change required there.

- [ ] **Step 8: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean, all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/clipboard.rs src/file_ops.rs src/main.rs
git commit -m "fix(fileops): honor cut/copy verb; symlink-safe replace; canonicalize guard; non-UTF-8 guards"
```

---

## Task 6: Defensive unwraps in hot paths (E1)

Convert reachable UI-thread `unwrap()`s in selection/key handlers and list factories to graceful guards. These are mechanical: each `X.downcast::<T>().unwrap()` / `X.downcast_ref::<T>().unwrap()` becomes a `let-else { return ... }` (or `and_downcast`), and each `first_child().unwrap()` / `next_sibling().unwrap()` becomes a guard that returns early.

**Files:** Modify `src/main.rs`, `src/column.rs`, `src/icon_view.rs`, `src/list_view.rs`.

- [ ] **Step 1: Enumerate the sites**

Run: `grep -rnE "\.unwrap\(\)" src/main.rs src/column.rs src/icon_view.rs src/list_view.rs | grep -vE "test|//"`
These are the candidate sites. Convert the ones in: the arrow-key handlers inside `add_column` (`main.rs` — `list_view_focus.model().unwrap()`, `.downcast::<gtk::MultiSelection>().unwrap()`, sibling `lv.model().unwrap()`), the idle selection closure (`.downcast::<gtk::MultiSelection>().unwrap()`), and the `connect_setup`/`connect_bind` factory closures in `column.rs`, `icon_view.rs`, `list_view.rs` (`item.downcast_ref::<gio::FileInfo>().unwrap()`, `list_item.child().and_downcast::<...>().unwrap()`, `first_child().unwrap()`, `next_sibling().unwrap()`, TreeListRow downcasts).

- [ ] **Step 2: Apply the conversion pattern**

For each site, replace the `unwrap()` with a `let-else` early return. Examples (apply the same shape to every listed site):

`main.rs` arrow handler:
```rust
// before
let selection_model = list_view_focus.model().unwrap().downcast::<gtk::MultiSelection>().unwrap();
// after
let Some(selection_model) = list_view_focus.model().and_downcast::<gtk::MultiSelection>() else { return glib::Propagation::Proceed; };
```

`main.rs` idle selection closure:
```rust
// before
let selection_idle = selection_model.clone().downcast::<gtk::MultiSelection>().unwrap();
// after
let Some(selection_idle) = selection_model.clone().downcast::<gtk::MultiSelection>().ok() else { return glib::ControlFlow::Break; };
```

Factory `connect_bind` (e.g. `column.rs`):
```rust
// before
let file_info = item.downcast_ref::<gio::FileInfo>().unwrap();
// after
let Some(file_info) = item.downcast_ref::<gio::FileInfo>() else { return; };
```

Widget-tree walks:
```rust
// before
let row = list_item.child().unwrap();
let icon = row.first_child().unwrap();
let label = icon.next_sibling().unwrap();
// after
let Some(row) = list_item.child() else { return; };
let Some(icon) = row.first_child() else { return; };
let Some(label) = icon.next_sibling() else { return; };
```

Use the correct early-return value for each closure's return type (`()` for setup/bind, `glib::Propagation::Proceed` for key handlers, `glib::ControlFlow::Break` for idle/timeout). Leave construction-time `unwrap()`s that run once at startup (e.g. `model().unwrap()` immediately after building a view in `build_ui`) — those are out of scope per the spec.

- [ ] **Step 3: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean, tests pass.

- [ ] **Step 4: Manual sanity — arrow nav + scrolling still work**

You can't run the GUI here; statically confirm each converted closure still type-checks and returns the right control-flow value, and that no behavior changed for the success path (the `let-else` only adds a graceful exit for the impossible-shape case).

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/column.rs src/icon_view.rs src/list_view.rs
git commit -m "fix(safety): replace UI-thread unwraps in key handlers and list factories with guards"
```

---

## Task 7: Documentation + verification

**Files:** Modify `CHANGELOG.md`.

- [ ] **Step 1: Add a CHANGELOG entry**

In `CHANGELOG.md`, under the existing `## [Unreleased]` section (create it at the top if absent, before the most recent released heading), add:

```markdown
### Fixed
- Destructive actions (delete/trash) no longer rely on a styling-only CSS class to pick the target view, eliminating a stale-state mis-targeting hazard; when no view is focused they now safely do nothing.
- Previewing a special file (FIFO/socket/device) or a file on a slow mount no longer blocks the UI — preview rendering is gated to regular files and text is read off the UI thread.
- The docked preview no longer autoplays video (only the `Space` view does) and stops playback when hidden.
- Pasting from GNOME Files now honors cut vs copy (cut moves).
- Replacing a symlinked target removes the link rather than its target tree; a folder move is refused if its path can't be verified.
- Reaped spawned helper processes (preview/terminal/email) and stopped the responsive-label timer from accumulating across view rebuilds.
- Replaced reachable UI-thread `unwrap()`s in navigation and list rendering with graceful guards.
```

- [ ] **Step 2: Commit docs**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for robustness hardening pass"
```

- [ ] **Step 3: Full build + lint + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean build, no clippy output, all tests pass.

- [ ] **Step 4: Manual verification (run `cargo run`)**

- Delete with a mouse-focused Miller column targets that column; clicking empty space (no row focus) then triggering delete does nothing (never the wrong column).
- New Folder / Paste land in the focused directory.
- Select `/dev/null` or a named pipe → info shown, UI does not hang. A large text file loads without freezing the UI.
- Arrow through videos → no lingering audio/playback; hiding the dock stops it.
- Toggle view/sort/zoom/hidden repeatedly → app stays responsive (no timer pile-up).
- (Linux) Cut in GNOME Files → paste in chvarkov moves.

- [ ] **Step 5: Finish the branch**

Use the `superpowers:finishing-a-development-branch` skill to merge/PR.

---

## Notes for the implementer

- `get_selection_model(&view)` already exists in `main.rs` and returns `Option<gtk::MultiSelection>` for ListView/GridView/ColumnView.
- `gio::spawn_blocking(...).await` returns `std::thread::Result<T>`; the B2 read uses `.unwrap_or_default()` on the join result (a panic in the blocking closure is not expected; default `Vec` is a safe fallback).
- Do not introduce `unwrap()` in new code. The B2 `.unwrap_or_default()` is on a join handle, not UI state.
- Keep `focused-column`/`focused-grid`/`focused-list` CSS classes being added/removed for styling — only the *reads* are removed.
- After Task 2, verify with `grep -rn on_file_deleted src/` whether the old method is still referenced before deleting it.
