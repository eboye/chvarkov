# SP2 — Conflict & Naming Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Duplicate command (`(Copy)` naming), switch conflict/keep-both/compress/symlink naming to Nautilus's numeric `(2)` scheme, preserve `.tar.*` compound extensions, and make the conflict dialog's Keep-Both name editable.

**Architecture:** Two pure naming helpers (`copy_dup_name`, `conflict_name`) on an improved `split_name`, replacing `dedupe_file_name`; a `duplicate` file-op + action + menu entry; an `ask_conflict` that returns an (editable) Keep-Both name.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22.

**Spec:** `docs/superpowers/specs/2026-06-07-conflict-naming-design.md`

---

## File Structure
- `src/file_ops.rs` — `split_name` compound ext; `copy_dup_name` + `conflict_name` (replace `dedupe_file_name`); rewire `unique_destination`/compress/symlink; `duplicate` fn; `ask_conflict` editable name (Task 3).
- `src/main.rs` — `duplicate` action + accel (Task 2).
- `src/utils.rs` — "Duplicate" context-menu entry (Task 2).
- `CHANGELOG.md` — document (Task 4).

---

## Task 1: Naming helpers — compound extensions + two schemes (pure, TDD)

**Files:** Modify `src/file_ops.rs`.

- [ ] **Step 1: Update the naming unit tests** — in the `#[cfg(test)] mod tests` block, REPLACE the four `dedupe_*` tests (`dedupe_no_collision_returns_original`, `dedupe_first_collision_adds_copy`, `dedupe_second_collision_numbers`, `dedupe_dotfile_has_no_extension`) and REPLACE the body of `split_name_cases` with these tests (keep the `untitled_*` tests as-is):

```rust
    #[test]
    fn split_name_cases() {
        assert_eq!(split_name("a.txt"), ("a".to_string(), ".txt".to_string()));
        assert_eq!(split_name("noext"), ("noext".to_string(), String::new()));
        assert_eq!(split_name(".bashrc"), (".bashrc".to_string(), String::new()));
        assert_eq!(split_name("a.tar.gz"), ("a".to_string(), ".tar.gz".to_string()));
        assert_eq!(split_name("a.tar"), ("a".to_string(), ".tar".to_string()));
        assert_eq!(split_name("a.tar.bz2"), ("a".to_string(), ".tar.bz2".to_string()));
    }

    #[test]
    fn copy_dup_name_scheme() {
        assert_eq!(copy_dup_name("x.txt", |_| false), "x (Copy).txt");
        assert_eq!(copy_dup_name("x.txt", |n| n == "x (Copy).txt"), "x (Copy 2).txt");
        assert_eq!(copy_dup_name("a.tar.gz", |_| false), "a (Copy).tar.gz");
        assert_eq!(copy_dup_name(".bashrc", |_| false), ".bashrc (Copy)");
    }

    #[test]
    fn conflict_name_scheme() {
        assert_eq!(conflict_name("x.txt", |_| false), "x.txt"); // free -> bare
        assert_eq!(conflict_name("x.txt", |n| n == "x.txt"), "x (2).txt");
        let taken = |n: &str| n == "x.txt" || n == "x (2).txt";
        assert_eq!(conflict_name("x.txt", taken), "x (3).txt");
        assert_eq!(conflict_name("a.tar.gz", |n| n == "a.tar.gz"), "a (2).tar.gz");
    }
```

Run: `cargo test conflict_name_scheme 2>&1 | tail -15` → FAIL (functions missing / dedupe tests still reference removed fn — that's expected at this step; proceed).

- [ ] **Step 2: Rewrite `split_name` for compound `.tar.*` extensions**

Replace the existing `split_name` with:

```rust
/// Split a file name into (stem, extension-with-dot). Leading-dot files
/// (".bashrc") have no extension. A compound `.tar.*` is kept whole
/// ("a.tar.gz" -> ("a", ".tar.gz")).
fn split_name(file_name: &str) -> (String, String) {
    if let Some(idx) = file_name.rfind('.')
        && idx > 0 {
            let mut stem = file_name[..idx].to_string();
            let mut ext = file_name[idx..].to_string();
            if let Some(tidx) = stem.rfind('.')
                && tidx > 0
                && &stem[tidx..] == ".tar" {
                    ext = format!(".tar{ext}");
                    stem.truncate(tidx);
                }
            return (stem, ext);
        }
    (file_name.to_string(), String::new())
}
```

- [ ] **Step 3: Replace `dedupe_file_name` with `copy_dup_name` + `conflict_name`**

Delete the `dedupe_file_name` function entirely and add these two in its place:

```rust
/// Name for an explicit Duplicate: always adds the appendix, even if the bare
/// name is free. "x.txt" -> "x (Copy).txt" -> "x (Copy 2).txt" ...
pub fn copy_dup_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_name(file_name);
    let first = format!("{stem} (Copy){ext}");
    if !exists(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let cand = format!("{stem} (Copy {n}){ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// A non-colliding name: the bare name if free, else numbered. "x.txt" (taken)
/// -> "x (2).txt" -> "x (3).txt" ... Used for conflict Keep-Both, same-path
/// copy, compress, and symlink dedup.
pub fn conflict_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(file_name) {
        return file_name.to_string();
    }
    let (stem, ext) = split_name(file_name);
    let mut n = 2;
    loop {
        let cand = format!("{stem} ({n}){ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}
```

- [ ] **Step 4: Rewire the three `dedupe_file_name` call sites to `conflict_name`**

- `unique_destination` (~line 73): `let name = dedupe_file_name(file_name, |n| dir.join(n).exists());` → `let name = conflict_name(file_name, |n| dir.join(n).exists());`
- compress (~line 603): `let name = dedupe_file_name(&archive_name(&paths), |n| dir.join(n).exists());` → `let name = conflict_name(&archive_name(&paths), |n| dir.join(n).exists());`
- symlink (~line 781): `let link_name = dedupe_file_name(&format!("{name} link"), |n| dir.join(n).exists());` → `let link_name = conflict_name(&format!("{name} link"), |n| dir.join(n).exists());`

- [ ] **Step 5: Run tests + clippy**

Run: `cargo test 2>&1 | grep "test result" && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head`
Expected: all tests pass (the 3 new naming tests + the rest); no clippy output. `copy_dup_name` has no caller yet (Task 2) — if a `dead_code` warning appears for it, add `#[allow(dead_code)] // TODO: remove when wired in Task 2` above it (removed in Task 2). `conflict_name` is used by `unique_destination`, so it's not dead.

- [ ] **Step 6: Commit**

```bash
git add src/file_ops.rs
git commit -m "feat(file-ops): compound-extension split + (Copy)/(2) naming schemes"
```

---

## Task 2: Duplicate command

**Files:** Modify `src/file_ops.rs` (the `duplicate` fn), `src/main.rs` (action + accel), `src/utils.rs` (menu entry).

- [ ] **Step 1: Add `duplicate` to `src/file_ops.rs`** (place near `symlink`):

```rust
/// Duplicate each path in place with a `(Copy)` name (symlink-preserving). Never
/// prompts: a fresh non-colliding name is always generated.
pub fn duplicate(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    glib::spawn_future_local(async move {
        let mut done = 0usize;
        let mut failed = 0usize;
        for src in &paths {
            let Some(dir) = src.parent().map(|p| p.to_path_buf()) else { failed += 1; continue };
            let Some(name) = src.file_name().and_then(|n| n.to_str()) else { failed += 1; continue };
            let dst = dir.join(copy_dup_name(name, |n| dir.join(n).exists()));
            let s = src.clone();
            match gio::spawn_blocking(move || copy_recursive(&s, &dst)).await {
                Ok(Ok(())) => done += 1,
                _ => failed += 1,
            }
        }
        let mut extra = String::new();
        if failed > 0 { extra.push_str(&format!(", {failed} failed")); }
        manager.send_toast(&format!("Duplicated {done} item(s){extra}"));
        manager.refresh();
    });
}
```

- [ ] **Step 2: `copy_recursive` is used again — drop its test-only attribute**

`copy_recursive` was marked `#[cfg_attr(not(test), allow(dead_code))]` in SP1 (it had become test-only). `duplicate` now calls it in non-test code, so REMOVE that `#[cfg_attr(not(test), allow(dead_code))]` line (and its retention comment) above `copy_recursive`. Also remove the temporary `#[allow(dead_code)]` from `copy_dup_name` if Task 1 added one.

- [ ] **Step 3: Register the `duplicate` action in `src/main.rs`** — add immediately after the `create_link_action` block (after `app.set_accels_for_action("app.create-link", &["<Shift><Control>m"]);`):

```rust
    let duplicate_action = gio::SimpleAction::new("duplicate", None);
    duplicate_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::duplicate(manager.clone(), paths);
            }
        });
    });
    app.add_action(&duplicate_action);
    app.set_accels_for_action("app.duplicate", &["<Primary>d"]);
```

- [ ] **Step 4: Add the "Duplicate" context-menu entry in `src/utils.rs`**

In `build_context_menu`, the `s3` section currently starts with the rename/create-link/compress entries. Add a Duplicate entry to `s3` immediately before the `Create Link` line. The `Create Link` line is:
```rust
    if count >= 1 && caps.read { s3.append(Some("Create Link"), Some("app.create-link")); }
```
Insert BEFORE it:
```rust
    if count >= 1 && caps.read && crate::target_dir_writable() { s3.append(Some("Duplicate"), Some("app.duplicate")); }
```

- [ ] **Step 5: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean build, no clippy output (no dead-code for `copy_dup_name`/`copy_recursive` now), all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/file_ops.rs src/main.rs src/utils.rs
git commit -m "feat: Duplicate command (Cmd/Ctrl+D, context menu) with (Copy) naming"
```

---

## Task 3: Editable Keep-Both name in the conflict dialog

**Files:** Modify `src/file_ops.rs` — `ask_conflict` (~line 467) and its caller in `transfer` (~lines 245-256).

- [ ] **Step 1: Rewrite `ask_conflict` to take a suggested name and return the (editable) Keep-Both name**

Replace the whole `ask_conflict` function with:

```rust
/// Conflict dialog. Returns (choice, apply_to_all, keep_both_name). The entry is
/// pre-filled with `suggested` and disabled when "apply to all" is ticked (a typed
/// name can't apply to a whole batch).
async fn ask_conflict(parent: &gtk::Window, name: &str, merge: bool, suggested: &str) -> (&'static str, bool, String) {
    let dialog = adw::AlertDialog::builder()
        .heading("Item already exists")
        .body(format!("\u{201c}{name}\u{201d} already exists in the destination. What do you want to do?"))
        .build();
    dialog.add_response("skip", "Skip");
    dialog.add_response("keep", "Keep Both");
    dialog.add_response("replace", if merge { "Merge" } else { "Replace" });
    if !merge {
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
    }
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("skip");

    let entry = gtk::Entry::builder()
        .text(suggested)
        .activates_default(true)
        .margin_top(8).margin_start(12).margin_end(12)
        .build();
    let check = gtk::CheckButton::with_label("Apply to all remaining");
    check.set_margin_top(8);
    check.set_margin_start(12);
    check.set_margin_end(12);
    check.set_margin_bottom(8);
    {
        let entry_c = entry.clone();
        check.connect_toggled(move |c| entry_c.set_sensitive(!c.is_active()));
    }
    let extra = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    extra.append(&entry);
    extra.append(&check);
    dialog.set_extra_child(Some(&extra));

    let entry_sel = entry.clone();
    glib::idle_add_local_once(move || {
        entry_sel.grab_focus();
        entry_sel.select_region(0, -1);
    });

    let response = dialog.choose_future(Some(parent)).await;
    let choice: &'static str = match response.as_str() {
        "replace" => "replace",
        "keep" => "keep",
        _ => "skip",
    };
    (choice, check.is_active(), entry.text().to_string())
}
```

- [ ] **Step 2: Update the caller in `transfer`**

The current conflict block is:
```rust
            let mut merge_dirs = false;
            if target.exists() {
                let target_is_symlink = target.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);
                let both_dirs = src.is_dir() && target.is_dir() && !target_is_symlink;
                let choice = match apply_to_all {
                    Some(c) => c,
                    None => { let (c, all) = ask_conflict(&parent, &name, both_dirs).await; if all { apply_to_all = Some(c); } c }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => { target = unique_destination(&dest_dir, &name); }
                    _ => {
```
Replace it with (compute a suggested name, capture the returned keep-name, and use it when valid):
```rust
            let mut merge_dirs = false;
            if target.exists() {
                let target_is_symlink = target.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);
                let both_dirs = src.is_dir() && target.is_dir() && !target_is_symlink;
                let suggested = conflict_name(&name, |n| dest_dir.join(n).exists());
                let (choice, keep_name) = match apply_to_all {
                    Some(c) => (c, String::new()),
                    None => {
                        let (c, all, kn) = ask_conflict(&parent, &name, both_dirs, &suggested).await;
                        if all { apply_to_all = Some(c); }
                        (c, kn)
                    }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => {
                        // Use the (possibly edited) name when it's valid and free;
                        // otherwise auto-generate. Apply-to-all always auto-names.
                        let edited_ok = apply_to_all.is_none()
                            && !keep_name.trim().is_empty()
                            && validate_filename(&keep_name).is_ok()
                            && !dest_dir.join(&keep_name).exists();
                        target = if edited_ok { dest_dir.join(&keep_name) } else { unique_destination(&dest_dir, &name) };
                    }
                    _ => {
```
(Leave the `_ =>` replace/merge arm and everything after it unchanged.)

- [ ] **Step 3: Build + clippy + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/file_ops.rs
git commit -m "feat(file-ops): editable Keep-Both name in the conflict dialog"
```

---

## Task 4: Documentation + verification

**Files:** Modify `README.md` (Keyboard Shortcuts + File Operations), `CHANGELOG.md`.

- [ ] **Step 1: README** — add a Duplicate shortcut row to the Keyboard Shortcuts table (after the **Rename** row):
```markdown
| **Duplicate** | `Ctrl` + `D` (Linux) / `Cmd` + `D` (macOS) |
```
And add "Duplicate" to the File Operations bullet that lists New Folder/Move/Rename/etc. (the line beginning `- **New Folder** and **New Empty File**`), e.g. append ", **Duplicate**" after "Rename".

- [ ] **Step 2: CHANGELOG** — under `## [Unreleased]` (create it at the top, before the latest released heading, if absent):
```markdown
### Added
- **Duplicate** command (`Ctrl/Cmd+D`, right-click → Duplicate) that copies the selection in place as `name (Copy).ext`.

### Changed
- Conflict "Keep Both" and other auto-naming now use Nautilus-style numbering — `name (2).ext`, `(3)`, … (was a lowercase `(copy)` scheme) — and preserve compound `.tar.*` extensions. The conflict dialog's Keep-Both name is now editable.
```

- [ ] **Step 3: Commit docs** — `git add README.md CHANGELOG.md && git commit -m "docs: Duplicate command + Nautilus naming"`

- [ ] **Step 4: Full build + lint + test** — `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"` → clean, all pass.

- [ ] **Step 5: Manual verification (`cargo run`)**
  - Right-click a file → **Duplicate** (and `Cmd/Ctrl+D`): `report.txt` → `report (Copy).txt`; again → `report (Copy 2).txt`; a folder duplicates recursively; `data.tar.gz` → `data (Copy).tar.gz`.
  - Duplicate is hidden when the directory isn't writable.
  - Copy a colliding file, **Keep Both** → `name (2).ext`; the dialog's name field is pre-filled + editable; an edited valid name is used; editing it to collide/blank falls back to auto `(2)`.
  - Tick **Apply to all** on a conflict → the name field disables; remaining conflicts auto-number.
  - Compress when `Archive.zip` exists → `Archive (2).zip`; Create Link dedup uses `(2)`.

- [ ] **Step 6: Finish the branch** — use `superpowers:finishing-a-development-branch`.

---

## Notes for the implementer
- `collect_selection`, `combine_caps`, `caps_from_info`, `crate::target_dir_writable`, `validate_filename`, `copy_recursive`, `archive_name`, `unique_destination` already exist.
- `<Primary>` is not used by other accels here; the existing convention is `<Control>`/`<Shift><Control>` — use `<Primary>d` (maps to Cmd on macOS / Ctrl on Linux) OR `<Control>d` to match the file's existing style. Prefer `<Primary>d` so it is Cmd+D on macOS as the README states; if that conflicts with anything, fall back to `<Control>d` and update the README row accordingly.
- `gtk::Entry`, `gtk::CheckButton`, `gtk::Box`, `glib::idle_add_local_once` are available (used elsewhere, e.g. `show_name_dialog`).
- No `unwrap()` on the UI thread; copies run in `spawn_blocking`. Zero warnings.
