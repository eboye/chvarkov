# File-Manager Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 13 stubbed file-manager actions (cut/copy/paste, move-to/copy-to, create-link, copy-path/uri/name, open-terminal, compress, email, sharing) so they operate on the full multi-selection, with system-clipboard interop and collision prompts — and fold in four reviewed correctness fixes.

**Architecture:** A dedicated async engine (`file_ops.rs`) and clipboard module (`clipboard.rs`), plus a `collect_selection` method on `ColumnManager`. The 13 `SimpleAction` closures in `main.rs` become thin wrappers that gather the selection (+ destination) and call into those modules. File copy/move run on a worker thread via `gio::spawn_blocking` using `std::fs` (recursive, portable); trash/delete keep gio's async APIs.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22, `zip` crate (added in Task 9). Spec: `docs/superpowers/specs/2026-06-06-file-operations-design.md`.

**Implementation note on APIs:** Async signatures vary slightly across gtk-rs versions. If the compiler rejects a call, check the gtk4 0.11 / gio 0.22 docs and adjust — the intent (async, non-blocking, non-panicking) is what matters. Every task ends with a warning-free `cargo build`.

---

## File structure

- `src/file_ops.rs` (new) — async copy/move/trash/delete/symlink/compress engine + conflict dialog + pure helpers (`dedupe_file_name`, `copy_recursive`).
- `src/clipboard.rs` (new) — system-clipboard read/write + internal cut/copy state type.
- `src/main.rs` (modify) — add `mod file_ops; mod clipboard;`, a `ColumnManager::collect_selection`, a `ColumnManager::refresh`, the clipboard state field, and rewrite the 13 action closures; fold in bug fixes.
- `src/sidebar.rs` (modify) — platform-correct Trash path.
- `src/utils.rs` (modify) — add `access::*` attributes to `get_directory_list`; add `Caps` + `caps_from_info` + `combine_caps`; rewrite `create_context_menu`/`create_context_menu_shift` to be permission-aware (Task 1B).
- `src/list_view.rs` (modify) — add `access::*` attributes to the child `DirectoryList` (Task 1B).
- `Cargo.toml` (modify) — add `zip` dependency (Task 9).

## Permission gates (apply to every action task)

Each action wrapper begins by gathering the selection and checking capabilities;
if the gate fails it returns silently (so the keyboard shortcut no-ops). The
permission-aware context menu (Task 1B) hides the same items. `Caps` /
`selection_caps` are defined in Task 1B.

| Action(s) | Gate |
| --- | --- |
| Move to Trash (`delete`) | non-empty and all `caps.trash` |
| Delete Permanently (`permanent-delete`) | non-empty and all `caps.delete` |
| Cut, Move to… | non-empty and all `caps.delete` |
| Copy, Copy to…, Compress, Email, Sharing | non-empty and all `caps.read` |
| Rename | exactly 1 and `caps.rename` |
| Create Link | non-empty and all `caps.read` |
| Copy Path / URI / Name, Open in Terminal, Properties, Open | non-empty (no gate) |
| Paste | clipboard non-empty; engine toasts destination errors |

---

## Task 1: Selection collection + nested-path fix

**Files:**
- Modify: `src/main.rs` (the `get_selection_model` area near line 180; `ColumnManager` impl; `handle_selection_change_multi` near line 1565)

- [ ] **Step 1: Add a path-from-FileInfo helper**

In `src/main.rs`, add this free function near `get_selection_model` (top-level, after the `thread_local!`/helpers region):

```rust
/// Resolve a FileInfo's real path. Uses the `standard::file` attribute (always
/// requested in our DirectoryLists), which is correct even for nested rows in
/// the tree-based List view. Falls back to `base/name` if the attribute is absent.
fn file_info_path(info: &gio::FileInfo, base: &std::path::Path) -> PathBuf {
    info.attribute_object("standard::file")
        .and_downcast::<gio::File>()
        .and_then(|f| f.path())
        .unwrap_or_else(|| base.join(info.name()))
}
```

- [ ] **Step 2: Add `collect_selection` to `ColumnManager`**

Inside `impl ColumnManager` (in `src/main.rs`), add:

```rust
/// Gather every selected item in the currently focused view as SelectionInfo.
/// Empty if nothing is focused/selected. Paths resolve via `standard::file`,
/// so nested List-view rows are correct.
fn collect_selection(&self) -> Vec<SelectionInfo> {
    let Some(view) = self.get_focused_list_view() else { return Vec::new() };
    let Some(sm) = get_selection_model(&view) else { return Vec::new() };
    let base = self
        .current_selection
        .borrow()
        .as_ref()
        .and_then(|s| s.path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(glib::home_dir);

    let selection = sm.selection();
    let Some(model) = sm.model() else { return Vec::new() };
    let mut out = Vec::new();
    for i in 0..selection.size() {
        let pos = selection.nth(i as u32);
        let Some(item) = model.item(pos) else { continue };
        let info = if let Ok(tree_row) = item.clone().downcast::<gtk::TreeListRow>() {
            tree_row.item().and_downcast::<gio::FileInfo>()
        } else {
            item.downcast::<gio::FileInfo>().ok()
        };
        if let Some(info) = info {
            let path = file_info_path(&info, &base);
            out.push(SelectionInfo { file_info: info, path });
        }
    }
    out
}
```

- [ ] **Step 3: Use `file_info_path` in `handle_selection_change_multi`**

In `src/main.rs`, replace the path reconstruction (currently around lines 1565-1567):

```rust
            let name = file_info.name();
            let mut new_path = base_path.clone();
            new_path.push(&name);
```

with:

```rust
            let new_path = file_info_path(&file_info, base_path);
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: compiles, 0 warnings. (`collect_selection` is unused for now — if dead-code warns, that's expected and resolved in Task 3 where it's first called; to keep the build clean until then, temporarily annotate the method with `#[allow(dead_code)]` and remove the annotation in Task 3.)

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: add collect_selection and fix nested List-view path resolution"
```

---

## Task 1B: Permission model + permission-aware context menus

**Files:**
- Modify: `src/utils.rs` (attributes; `Caps`/`caps_from_info`/`combine_caps`; rewrite the two menu builders)
- Modify: `src/list_view.rs` (child DirectoryList attributes)
- Modify: `src/main.rs` (add `selection_caps`; drop `#[allow(dead_code)]` on `collect_selection`)

- [ ] **Step 1: Request `access::*` attributes in `get_directory_list`**

In `src/utils.rs`, in `get_directory_list`, append the access attributes to the existing `.attributes(...)` string. The string currently ends with `...thumbnail::path,thumbnail::is-valid`. Change it to end with:

```
...thumbnail::path,thumbnail::is-valid,access::can-read,access::can-write,access::can-execute,access::can-delete,access::can-trash,access::can-rename
```

- [ ] **Step 2: Request `access::*` attributes in the List view's child list**

In `src/list_view.rs`, the child `gtk::DirectoryList::builder().attributes("...")` (around line 36) currently ends with `...standard::n-children,standard::file`. Append the same access attributes:

```
...standard::n-children,standard::file,access::can-read,access::can-write,access::can-execute,access::can-delete,access::can-trash,access::can-rename
```

- [ ] **Step 3: Add `Caps` + helpers to `utils.rs` (with a unit test)**

Add near the top of `src/utils.rs` (after the imports):

```rust
/// Filesystem capabilities for a selection, from GIO `access::*` attributes.
#[derive(Clone, Copy)]
pub struct Caps {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub delete: bool,
    pub trash: bool,
    pub rename: bool,
}

/// Read capabilities from a FileInfo. Missing attributes default to permitted
/// (true) so actions are not hidden spuriously.
pub fn caps_from_info(info: &gio::FileInfo) -> Caps {
    let get = |attr: &str| if info.has_attribute(attr) { info.boolean(attr) } else { true };
    Caps {
        read: get("access::can-read"),
        write: get("access::can-write"),
        execute: get("access::can-execute"),
        delete: get("access::can-delete"),
        trash: get("access::can-trash"),
        rename: get("access::can-rename"),
    }
}

/// AND-combine capabilities across a selection. Empty selection -> all false.
pub fn combine_caps(items: impl IntoIterator<Item = Caps>) -> Caps {
    let mut it = items.into_iter();
    match it.next() {
        None => Caps { read: false, write: false, execute: false, delete: false, trash: false, rename: false },
        Some(first) => it.fold(first, |a, b| Caps {
            read: a.read && b.read,
            write: a.write && b.write,
            execute: a.execute && b.execute,
            delete: a.delete && b.delete,
            trash: a.trash && b.trash,
            rename: a.rename && b.rename,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_caps_ands_and_empty_is_none() {
        let a = Caps { read: true, write: true, execute: true, delete: true, trash: true, rename: true };
        let b = Caps { read: true, write: false, execute: true, delete: false, trash: true, rename: true };
        let c = combine_caps([a, b]);
        assert!(c.read && c.trash && c.rename && c.execute);
        assert!(!c.write && !c.delete);
        let none = combine_caps(std::iter::empty::<Caps>());
        assert!(!none.read && !none.delete && !none.trash);
    }
}
```

- [ ] **Step 4: Add `selection_caps` to `main.rs`**

In `src/main.rs`, add a crate-level free function near `get_selection_model`:

```rust
/// (selected count, AND-combined capabilities) for the focused view's selection.
pub(crate) fn selection_caps() -> (usize, utils::Caps) {
    ACTIVE_MANAGER.with(|m| {
        if let Some(manager) = m.borrow().as_ref() {
            let sel = manager.collect_selection();
            let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
            (sel.len(), caps)
        } else {
            (0, utils::combine_caps(std::iter::empty()))
        }
    })
}
```

Then remove the `#[allow(dead_code)]` attribute above `fn collect_selection` (it is now used by `selection_caps`).

- [ ] **Step 5: Rewrite the two menu builders to be permission-aware**

In `src/utils.rs`, replace the entire bodies of `create_context_menu` and `create_context_menu_shift` (keep their `pub fn` signatures) with delegations to one shared builder, and add the builder:

```rust
pub fn create_context_menu() -> gio::Menu {
    build_context_menu(false)
}

pub fn create_context_menu_shift() -> gio::Menu {
    build_context_menu(true)
}

/// Build the context menu for the current selection, omitting actions the user
/// is not permitted to perform. `shift` puts Delete Permanently above Move to Trash.
fn build_context_menu(shift: bool) -> gio::Menu {
    let (count, caps) = crate::selection_caps();
    let menu = gio::Menu::new();

    if count >= 1 {
        let s = gio::Menu::new();
        s.append(Some("Open"), Some("app.open"));
        menu.append_section(None, &s);
    }

    let s2 = gio::Menu::new();
    if count >= 1 && caps.delete { s2.append(Some("Cut"), Some("app.cut")); }
    if count >= 1 && caps.read { s2.append(Some("Copy"), Some("app.copy")); }
    if count >= 1 && caps.delete { s2.append(Some("Move to..."), Some("app.move-to")); }
    if count >= 1 && caps.read { s2.append(Some("Copy to..."), Some("app.copy-to")); }
    if s2.n_items() > 0 { menu.append_section(None, &s2); }

    let s3 = gio::Menu::new();
    if count == 1 && caps.rename { s3.append(Some("Rename..."), Some("app.rename")); }
    if count >= 1 && caps.read { s3.append(Some("Create Link"), Some("app.create-link")); }
    if count >= 1 && caps.read { s3.append(Some("Compress..."), Some("app.compress")); }
    if count >= 1 && caps.read { s3.append(Some("Email..."), Some("app.email")); }
    if shift {
        if count >= 1 && caps.delete { s3.append(Some("Delete Permanently"), Some("app.permanent-delete")); }
        if count >= 1 && caps.trash { s3.append(Some("Move to Trash"), Some("app.delete")); }
    } else {
        if count >= 1 && caps.trash { s3.append(Some("Move to Trash"), Some("app.delete")); }
        if count >= 1 && caps.delete { s3.append(Some("Delete Permanently"), Some("app.permanent-delete")); }
    }
    if s3.n_items() > 0 { menu.append_section(None, &s3); }

    let s4 = gio::Menu::new();
    if count >= 1 {
        s4.append(Some("Open in Terminal"), Some("app.open-terminal"));
        s4.append(Some("Copy Path"), Some("app.copy-path"));
        s4.append(Some("Copy URI"), Some("app.copy-uri"));
        s4.append(Some("Copy Name"), Some("app.copy-name"));
    }
    #[cfg(target_os = "macos")]
    if count >= 1 && caps.read { s4.append(Some("Sharing Options"), Some("app.sharing-options")); }
    if s4.n_items() > 0 { menu.append_section(None, &s4); }

    if count == 1 {
        let s5 = gio::Menu::new();
        s5.append(Some("Properties"), Some("app.properties"));
        menu.append_section(None, &s5);
    }

    menu
}
```

Note: this removes the static `section1..section5` bodies that previously listed every item unconditionally. `gio::Menu` (a `MenuModel`) has `.n_items()`.

- [ ] **Step 6: Build + test**

Run: `cargo build` — 0 warnings, 0 errors.
Run: `cargo test` — the new `combine_caps_ands_and_empty_is_none` test (and Task 2's, once present) pass.

- [ ] **Step 7: Manual verification**

`cargo run --release`. Right-click a normal file → full menu. Right-click a file inside a directory you lack write access to (e.g. something under `/usr` or a root-owned file) → Cut / Move to… / Rename / Delete / Move to Trash are absent; Copy / Copy Path / Open remain. Select multiple where one is read-only → write actions disappear.

- [ ] **Step 8: Commit**

```bash
git add src/utils.rs src/list_view.rs src/main.rs
git commit -m "feat: permission-aware context menus via GIO access attributes"
```

---

## Task 2: file_ops engine — pure helpers with unit tests

**Files:**
- Create: `src/file_ops.rs`
- Modify: `src/main.rs` (add `mod file_ops;` near the other `mod` lines at the top)

- [ ] **Step 1: Create the module with pure helpers**

Create `src/file_ops.rs`:

```rust
use std::path::{Path, PathBuf};

/// Split a file name into (stem, extension-with-dot). Leading-dot files
/// ("\.bashrc") are treated as having no extension.
fn split_name(file_name: &str) -> (String, String) {
    if let Some(idx) = file_name.rfind('.') {
        if idx > 0 {
            return (file_name[..idx].to_string(), file_name[idx..].to_string());
        }
    }
    (file_name.to_string(), String::new())
}

/// Return a file name that does not collide, per the `exists` predicate.
/// First tries "<stem> (copy)<ext>", then "<stem> (copy N)<ext>".
pub fn dedupe_file_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(file_name) {
        return file_name.to_string();
    }
    let (stem, ext) = split_name(file_name);
    let first = format!("{stem} (copy){ext}");
    if !exists(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let cand = format!("{stem} (copy {n}){ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// Compute a non-colliding destination path inside `dir` for `file_name`.
pub fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let name = dedupe_file_name(file_name, |n| dir.join(n).exists());
    dir.join(name)
}

/// Recursively copy `src` to `dst` (file or directory). `dst` is the full
/// target path (not a parent directory).
pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_no_collision_returns_original() {
        assert_eq!(dedupe_file_name("a.txt", |_| false), "a.txt");
    }

    #[test]
    fn dedupe_first_collision_adds_copy() {
        assert_eq!(dedupe_file_name("a.txt", |n| n == "a.txt"), "a (copy).txt");
    }

    #[test]
    fn dedupe_second_collision_numbers() {
        let taken = |n: &str| n == "a.txt" || n == "a (copy).txt";
        assert_eq!(dedupe_file_name("a.txt", taken), "a (copy 2).txt");
    }

    #[test]
    fn dedupe_dotfile_has_no_extension() {
        assert_eq!(dedupe_file_name(".bashrc", |n| n == ".bashrc"), ".bashrc (copy)");
    }

    #[test]
    fn copy_recursive_copies_tree() {
        let tmp = std::env::temp_dir().join(format!("chv_test_{}", std::process::id()));
        let src = tmp.join("src");
        let dst = tmp.join("dst");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("sub").join("f.txt"), b"hi").unwrap();
        copy_recursive(&src, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("sub").join("f.txt")).unwrap(), b"hi");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
```

- [ ] **Step 2: Register the module**

In `src/main.rs`, add to the `mod` block at the top (alongside `mod column;` etc.):

```rust
mod file_ops;
```

- [ ] **Step 3: Run the unit tests**

Run: `cargo test --lib file_ops`
Expected: 5 tests pass. (If the binary crate name makes `--lib` empty, use `cargo test file_ops`.)

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: 0 warnings (the `unique_destination`/`copy_recursive` pub fns may warn as unused until Task 4/5; add `#[allow(dead_code)]` on them now and remove in the task that first uses each).

- [ ] **Step 5: Commit**

```bash
git add src/file_ops.rs src/main.rs
git commit -m "feat: add file_ops pure helpers (dedupe, recursive copy) with tests"
```

---

## Task 3: Engine operations + switch Trash/Delete onto it (multi-item) + bug fixes

**Files:**
- Modify: `src/file_ops.rs` (add async ops + conflict dialog)
- Modify: `src/main.rs` (delete/permanent-delete closures; preview Down-arrow; preferences unwrap; remove println!; drop the Task 1/2 `#[allow(dead_code)]`)
- Modify: `src/sidebar.rs` (Trash path)

- [ ] **Step 1: Add operation mode + async batch engine to `file_ops.rs`**

Append to `src/file_ops.rs`:

```rust
use std::rc::Rc;
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use crate::ColumnManager;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransferKind { Copy, Move }

/// Move or copy a batch of source paths into `dest_dir`, prompting on collisions.
/// Reports via the manager's toast overlay and refreshes the UI when done.
pub fn transfer(
    manager: Rc<ColumnManager>,
    parent: gtk::Window,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    kind: TransferKind,
) {
    glib::spawn_future_local(async move {
        let mut apply_to_all: Option<&'static str> = None; // "replace" | "skip" | "keep"
        let mut done = 0usize;
        let mut skipped = 0usize;

        for src in sources {
            let Some(name) = src.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let mut target = dest_dir.join(&name);

            if target.exists() {
                let choice = match apply_to_all {
                    Some(c) => c,
                    None => {
                        let (c, all) = ask_conflict(&parent, &name).await;
                        if all { apply_to_all = Some(c); }
                        c
                    }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => { target = unique_destination(&dest_dir, &name); }
                    _ => { /* replace: remove existing first */
                        let t = target.clone();
                        let _ = gio::spawn_blocking(move || {
                            if t.is_dir() { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) }
                        }).await;
                    }
                }
            }

            let src_c = src.clone();
            let target_c = target.clone();
            let res = gio::spawn_blocking(move || -> std::io::Result<()> {
                match kind {
                    TransferKind::Copy => copy_recursive(&src_c, &target_c),
                    TransferKind::Move => {
                        // Try a fast rename; fall back to copy+delete across filesystems.
                        match std::fs::rename(&src_c, &target_c) {
                            Ok(()) => Ok(()),
                            Err(_) => {
                                copy_recursive(&src_c, &target_c)?;
                                if src_c.is_dir() { std::fs::remove_dir_all(&src_c) } else { std::fs::remove_file(&src_c) }
                            }
                        }
                    }
                }
            }).await;

            match res {
                Ok(Ok(())) => done += 1,
                _ => manager.send_toast(&format!("Failed to transfer {name}")),
            }
        }

        let verb = if kind == TransferKind::Move { "Moved" } else { "Copied" };
        manager.send_toast(&format!("{verb} {done} item(s){}", if skipped > 0 { format!(", skipped {skipped}") } else { String::new() }));
        manager.refresh();
    });
}

/// Show the collision dialog; returns (choice, apply_to_all).
/// choice is one of "replace" | "skip" | "keep".
async fn ask_conflict(parent: &gtk::Window, name: &str) -> (&'static str, bool) {
    let dialog = adw::AlertDialog::builder()
        .heading("Item already exists")
        .body(format!("\u{201c}{name}\u{201d} already exists in the destination. What do you want to do?"))
        .build();
    dialog.add_response("skip", "Skip");
    dialog.add_response("keep", "Keep Both");
    dialog.add_response("replace", "Replace");
    dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("skip");

    let check = gtk::CheckButton::with_label("Apply to all remaining");
    check.set_margin_top(8);
    check.set_margin_start(12);
    check.set_margin_end(12);
    dialog.set_extra_child(Some(&check));

    let response = dialog.choose_future(Some(parent)).await;
    let choice: &'static str = match response.as_str() {
        "replace" => "replace",
        "keep" => "keep",
        _ => "skip",
    };
    (choice, check.is_active())
}

/// Move a batch of paths to the trash.
pub fn trash(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    for path in paths {
        let file = gio::File::for_path(&path);
        let manager_c = manager.clone();
        file.trash_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, move |res| {
            match res {
                Ok(_) => { manager_c.send_toast("Moved to Trash"); manager_c.on_file_deleted(&path); }
                Err(e) => manager_c.send_toast(&format!("Error moving to trash: {e}")),
            }
        });
    }
}

/// Permanently delete a batch of paths.
pub fn delete(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    for path in paths {
        let file = gio::File::for_path(&path);
        let manager_c = manager.clone();
        file.delete_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, move |res| {
            match res {
                Ok(_) => { manager_c.send_toast("Deleted permanently"); manager_c.on_file_deleted(&path); }
                Err(e) => manager_c.send_toast(&format!("Error deleting: {e}")),
            }
        });
    }
}
```

Note: `crate::ColumnManager`, `send_toast`, `on_file_deleted` already exist. `refresh` is added in Step 2. Use bare `gio::` (gio is a direct dependency, referenced as `gio::` everywhere else in this project). `gio::spawn_blocking(f).await` resolves to the closure's return — if its `Output` is not wrapped in a `Result` on the installed version, drop the outer arm of the `match` (use `Ok(())` / `Err(_)` instead of `Ok(Ok(()))`).

- [ ] **Step 2: Add `ColumnManager::refresh`**

In `impl ColumnManager` (`src/main.rs`), add:

```rust
/// Rebuild the UI from the current path (reuses the existing window).
fn refresh(&self) {
    if let Some(app) = gio::Application::default() {
        glib::idle_add_local(move || { app.activate(); glib::ControlFlow::Break });
    }
}
```

- [ ] **Step 3: Make `on_file_deleted` callable from the module**

`on_file_deleted` and `send_toast` are currently private methods on `ColumnManager`. Change their visibility so `file_ops` can call them: in `src/main.rs` change `fn send_toast` to `pub(crate) fn send_toast` and `fn on_file_deleted` to `pub(crate) fn on_file_deleted`. Also make `refresh` `pub(crate) fn refresh`, and `collect_selection` `pub(crate) fn collect_selection` (remove its `#[allow(dead_code)]`). Make `SelectionInfo` fields visible: change the struct to `pub(crate)` fields if the module needs them (it only needs `.path`, which it reads via `collect_selection` results in `main.rs`, so the struct can stay private — verify during build).

- [ ] **Step 4: Rewrite delete / permanent-delete closures to use the engine + multi-selection**

In `src/main.rs`, replace the `delete_action` closure body (around lines 289-296) with:

```rust
    delete_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.trash { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::trash(manager.clone(), paths);
            }
        });
    });
```

And replace the `permanent_delete_action` closure body (around lines 302-309) with (guarded on `caps.delete`):

```rust
    permanent_delete_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.delete { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::delete(manager.clone(), paths);
            }
        });
    });
```

Then delete the now-unused standalone `fn trash_file` and `fn permanent_delete_file` from `src/main.rs` (around lines 1157-1196).

- [ ] **Step 5: Bug fix — preview Down-arrow bounds**

In `src/main.rs`, in `create_preview_window`'s key handler (around lines 1389-1393), replace:

```rust
                                 if key == gtk::gdk::Key::Up && current > 0 {
                                     sm.select_item(current - 1, true);
                                 } else if key == gtk::gdk::Key::Down {
                                     sm.select_item(current + 1, true);
                                 }
```

with:

```rust
                                 if key == gtk::gdk::Key::Up && current > 0 {
                                     sm.select_item(current - 1, true);
                                 } else if key == gtk::gdk::Key::Down && current + 1 < sm.n_items() {
                                     sm.select_item(current + 1, true);
                                 }
```

- [ ] **Step 6: Bug fix — preferences unwrap**

In `src/main.rs`, `show_preferences_window` (around line 604), replace:

```rust
    let window = app.active_window().unwrap();
```

with:

```rust
    let Some(window) = app.active_window() else { return; };
```

- [ ] **Step 7: Bug fix — Trash sidebar path (cross-platform)**

In `src/sidebar.rs`, replace the Trash item (line 68):

```rust
            SidebarItem { name: "Trash", icon: "user-trash-symbolic", path: glib::home_dir().join(".local/share/Trash/files") },
```

with:

```rust
            SidebarItem { name: "Trash", icon: "user-trash-symbolic", path: trash_dir() },
```

and add this free function at the bottom of `src/sidebar.rs`:

```rust
fn trash_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    { glib::home_dir().join(".Trash") }
    #[cfg(not(target_os = "macos"))]
    { glib::home_dir().join(".local/share/Trash/files") }
}
```

- [ ] **Step 8: Remove debug println!s**

In `src/main.rs`, delete the debug `println!` lines in the action closures and in `handle_selection_change_multi` (the "Open action triggered", "Attempting to open", "Cut action triggered", … "Selection cleared in Column {}", "Selection [Column ...]" lines). Keep the surrounding logic.

- [ ] **Step 9: Build + test**

Run: `cargo build`
Expected: 0 warnings, 0 errors.
Run: `cargo test file_ops`
Expected: pass.

- [ ] **Step 10: Manual verification**

Run: `cargo run --release`. Select multiple files (Ctrl+click), press Delete → all move to Trash and child columns collapse. Open the preview (Space), press Down on the last item → no warning/crash. Open Preferences → no panic. Sidebar "Trash" opens the correct OS trash folder.

- [ ] **Step 11: Commit**

```bash
git add src/file_ops.rs src/main.rs src/sidebar.rs
git commit -m "feat: multi-item trash/delete via engine; fix preview bound, prefs unwrap, trash path; drop debug logs"
```

---

## Task 4: Clipboard module + Copy / Cut / Paste

**Files:**
- Create: `src/clipboard.rs`
- Modify: `src/main.rs` (`mod clipboard;`; add clipboard state to `ColumnManager`; rewrite copy/cut/paste closures)

- [ ] **Step 1: Create `src/clipboard.rs`**

```rust
use std::path::PathBuf;
use gtk4 as gtk;
use gtk::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode { Copy, Cut }

#[derive(Clone)]
pub struct ClipboardOp {
    pub mode: Mode,
    pub paths: Vec<PathBuf>,
}

/// Publish a cut/copy of `paths` to the system clipboard (uri-list +
/// GNOME convention) so Finder / GNOME Files interoperate.
pub fn publish(paths: &[PathBuf], mode: Mode) {
    let Some(display) = gtk::gdk::Display::default() else { return };
    let clipboard = display.clipboard();

    let uris: Vec<String> = paths.iter().map(|p| gio::File::for_path(p).uri().to_string()).collect();
    let uri_list = uris.join("\r\n");
    let verb = if mode == Mode::Cut { "cut" } else { "copy" };
    let gnome = format!("{verb}\n{}", uris.join("\n"));

    let p_gnome = gtk::gdk::ContentProvider::for_bytes(
        "x-special/gnome-copied-files",
        &glib::Bytes::from(gnome.as_bytes()),
    );
    let p_uris = gtk::gdk::ContentProvider::for_bytes(
        "text/uri-list",
        &glib::Bytes::from(uri_list.as_bytes()),
    );
    let provider = gtk::gdk::ContentProvider::new_union(&[p_gnome, p_uris]);
    let _ = clipboard.set_content(Some(&provider));
}

/// Best-effort read of file paths from the system clipboard (when our internal
/// state is empty, e.g. copied from another app). Returns Copy mode by default.
pub fn read_external<F: Fn(Option<ClipboardOp>) + 'static>(callback: F) {
    let Some(display) = gtk::gdk::Display::default() else { callback(None); return };
    let clipboard = display.clipboard();
    clipboard.read_text_async(gio::Cancellable::NONE, move |res| {
        let op = res.ok().flatten().and_then(|text| {
            let paths: Vec<PathBuf> = text
                .lines()
                .filter(|l| l.starts_with("file://"))
                .filter_map(|l| gio::File::for_uri(l).path())
                .collect();
            if paths.is_empty() { None } else { Some(ClipboardOp { mode: Mode::Copy, paths }) }
        });
        callback(op);
    });
}
```

- [ ] **Step 2: Register module + add state field**

In `src/main.rs`: add `mod clipboard;` to the module block. Add a field to `struct ColumnManager`:

```rust
    clipboard: Rc<RefCell<Option<clipboard::ClipboardOp>>>,
```

and initialize it in `ColumnManager::new` (`Self { … }`):

```rust
            clipboard: Rc::new(RefCell::new(None)),
```

- [ ] **Step 3: Add a `focused_dir` helper to `ColumnManager`**

Paste needs the destination directory = the directory shown by the focused view. Add to `impl ColumnManager`:

```rust
/// The directory currently shown by the focused column/view (paste target).
pub(crate) fn focused_dir(&self) -> Option<PathBuf> {
    let view = self.get_focused_list_view()?;
    // Miller columns: match the focused widget to its ColumnEntry path.
    for entry in self.entries.borrow().iter() {
        if entry.focus_target == view {
            return Some(entry.path.clone());
        }
    }
    // Icon/List view: fall back to the parent of the current selection,
    // else the persisted current-path.
    if let Some(sel) = self.current_selection.borrow().as_ref() {
        if let Some(parent) = sel.path.parent() { return Some(parent.to_path_buf()); }
    }
    let settings = gio::Settings::new("net.nocopypaste.chvarkov");
    let p: String = settings.get("current-path");
    if p.is_empty() { Some(glib::home_dir()) } else { Some(PathBuf::from(p)) }
}
```

- [ ] **Step 4: Rewrite copy / cut closures**

Replace the `copy_action` and `cut_action` closure bodies (lines ~234-242):

```rust
    cut_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if !paths.is_empty() {
                    clipboard::publish(&paths, clipboard::Mode::Cut);
                    *manager.clipboard.borrow_mut() = Some(clipboard::ClipboardOp { mode: clipboard::Mode::Cut, paths });
                    manager.send_toast("Cut");
                }
            }
        });
    });
```

```rust
    copy_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if !paths.is_empty() {
                    clipboard::publish(&paths, clipboard::Mode::Copy);
                    *manager.clipboard.borrow_mut() = Some(clipboard::ClipboardOp { mode: clipboard::Mode::Copy, paths });
                    manager.send_toast("Copied");
                }
            }
        });
    });
```

- [ ] **Step 5: Rewrite paste closure**

Replace the `paste_action` closure body (lines ~244-246). It needs the active window for conflict dialogs:

```rust
    let paste_app_weak = app.downgrade();
    paste_action.connect_activate(move |_, _| {
        let Some(app) = paste_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let Some(dest) = manager.focused_dir() else { return };
                let internal = manager.clipboard.borrow().clone();
                if let Some(op) = internal {
                    let kind = match op.mode {
                        clipboard::Mode::Copy => file_ops::TransferKind::Copy,
                        clipboard::Mode::Cut => file_ops::TransferKind::Move,
                    };
                    file_ops::transfer(manager.clone(), window.clone().upcast(), op.paths, dest, kind);
                    if op.mode == clipboard::Mode::Cut {
                        *manager.clipboard.borrow_mut() = None;
                    }
                } else {
                    // Fall back to the system clipboard (copied from another app).
                    let manager_c = manager.clone();
                    let window_c = window.clone();
                    clipboard::read_external(move |op| {
                        if let Some(op) = op {
                            file_ops::transfer(manager_c.clone(), window_c.clone().upcast(), op.paths, dest.clone(), file_ops::TransferKind::Copy);
                        }
                    });
                }
            }
        });
    });
```

Note: `paste_action` is created earlier with `let paste_action = …`. Because the closure now captures `app`, move the `let paste_app_weak = app.downgrade();` line just above this closure (mirroring the `rename`/`properties` pattern already in the file).

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: 0 warnings.

- [ ] **Step 7: Manual verification**

`cargo run --release`. Copy a file (Ctrl+C), focus another folder column, Paste (Ctrl+V) → file copied; collision triggers the Replace/Skip/Keep dialog. Cut (Ctrl+X) then Paste → file moves and clipboard clears. Copy in chvarkov, paste in Finder/GNOME Files → file appears.

- [ ] **Step 8: Commit**

```bash
git add src/clipboard.rs src/main.rs
git commit -m "feat: implement Copy/Cut/Paste with system-clipboard interop and conflict prompts"
```

---

## Task 5: Move to… / Copy to…

**Files:**
- Modify: `src/main.rs` (`move_to_action`, `copy_to_action` closures)

- [ ] **Step 1: Rewrite move-to / copy-to closures**

Replace the `move_to_action` and `copy_to_action` bodies (lines ~249-255). Both open a folder picker, then call the engine:

```rust
    let move_app_weak = app.downgrade();
    move_to_action.connect_activate(move |_, _| {
        let Some(app) = move_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if paths.is_empty() { return; }
                let manager_c = manager.clone();
                let win = window.clone();
                let dialog = gtk::FileDialog::builder().title("Move to Folder").build();
                dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |res| {
                    if let Ok(folder) = res {
                        if let Some(dest) = folder.path() {
                            file_ops::transfer(manager_c.clone(), win.clone().upcast(), paths.clone(), dest, file_ops::TransferKind::Move);
                        }
                    }
                });
            }
        });
    });
```

```rust
    let copy_app_weak = app.downgrade();
    copy_to_action.connect_activate(move |_, _| {
        let Some(app) = copy_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if paths.is_empty() { return; }
                let manager_c = manager.clone();
                let win = window.clone();
                let dialog = gtk::FileDialog::builder().title("Copy to Folder").build();
                dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |res| {
                    if let Ok(folder) = res {
                        if let Some(dest) = folder.path() {
                            file_ops::transfer(manager_c.clone(), win.clone().upcast(), paths.clone(), dest, file_ops::TransferKind::Copy);
                        }
                    }
                });
            }
        });
    });
```

Move the `let move_app_weak`/`let copy_app_weak` lines just above each closure as needed.

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: 0 warnings.

- [ ] **Step 3: Manual verification**

Select items → context menu → Move to… / Copy to… → pick a folder → items move/copy, collisions prompt.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement Move to / Copy to via folder picker + engine"
```

---

## Task 6: Copy Path / Copy URI / Copy Name

**Files:**
- Modify: `src/main.rs` (the three `copy_*` closures)

- [ ] **Step 1: Rewrite the three closures**

Replace `copy_path_action`, `copy_uri_action`, `copy_name_action` bodies (lines ~318-328):

```rust
    copy_path_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let text = manager.collect_selection().into_iter()
                    .map(|s| s.path.to_string_lossy().to_string())
                    .collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    if let Some(d) = gtk::gdk::Display::default() { d.clipboard().set_text(&text); }
                    manager.send_toast("Path copied");
                }
            }
        });
    });
```

```rust
    copy_uri_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let text = manager.collect_selection().into_iter()
                    .map(|s| gio::File::for_path(&s.path).uri().to_string())
                    .collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    if let Some(d) = gtk::gdk::Display::default() { d.clipboard().set_text(&text); }
                    manager.send_toast("URI copied");
                }
            }
        });
    });
```

```rust
    copy_name_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let text = manager.collect_selection().into_iter()
                    .map(|s| s.file_info.display_name().to_string())
                    .collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    if let Some(d) = gtk::gdk::Display::default() { d.clipboard().set_text(&text); }
                    manager.send_toast("Name copied");
                }
            }
        });
    });
```

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: 0 warnings.

- [ ] **Step 3: Manual verification**

Select item(s) → Copy Path / URI / Name → paste into a text field; multi-selection yields newline-joined values.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement Copy Path / Copy URI / Copy Name"
```

---

## Task 7: Create Link

**Files:**
- Modify: `src/file_ops.rs` (add `symlink`)
- Modify: `src/main.rs` (`create_link_action`)

- [ ] **Step 1: Add `symlink` to `file_ops.rs`**

```rust
/// Create a symlink named "<name> link" beside each source (numbered on collision).
pub fn symlink(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    let mut made = 0usize;
    for src in &paths {
        let Some(dir) = src.parent() else { continue };
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
        let link_name = dedupe_file_name(&format!("{name} link"), |n| dir.join(n).exists());
        let link_path = dir.join(link_name);
        match std::os::unix::fs::symlink(src, &link_path) {
            Ok(()) => made += 1,
            Err(e) => manager.send_toast(&format!("Link failed: {e}")),
        }
    }
    if made > 0 {
        manager.send_toast(&format!("Created {made} link(s)"));
        manager.refresh();
    }
}
```

- [ ] **Step 2: Rewrite `create_link_action`**

Replace its body (line ~276):

```rust
    create_link_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if !paths.is_empty() { file_ops::symlink(manager.clone(), paths); }
            }
        });
    });
```

- [ ] **Step 3: Build + verify + commit**

Run: `cargo build` (0 warnings). `cargo run --release`, select a file → Create Link (Ctrl+Shift+M) → a "<name> link" symlink appears.

```bash
git add src/file_ops.rs src/main.rs
git commit -m "feat: implement Create Link (symlink)"
```

---

## Task 8: Open in Terminal

**Files:**
- Modify: `src/file_ops.rs` (add `open_terminal`)
- Modify: `src/main.rs` (`open_terminal_action`)

- [ ] **Step 1: Add `open_terminal` to `file_ops.rs`**

```rust
/// Open a terminal at `dir`.
pub fn open_terminal(manager: Rc<ColumnManager>, dir: PathBuf) {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    let spawned = Command::new("open").arg("-a").arg("Terminal").arg(&dir).spawn().is_ok();

    #[cfg(not(target_os = "macos"))]
    let spawned = {
        let mut ok = false;
        let candidates: Vec<(String, Vec<String>)> = {
            let mut v = Vec::new();
            if let Ok(t) = std::env::var("TERMINAL") { v.push((t, vec![])); }
            v.push(("gnome-terminal".into(), vec!["--working-directory".into(), dir.to_string_lossy().to_string()]));
            v.push(("konsole".into(), vec!["--workdir".into(), dir.to_string_lossy().to_string()]));
            v.push(("kitty".into(), vec!["--directory".into(), dir.to_string_lossy().to_string()]));
            v.push(("alacritty".into(), vec!["--working-directory".into(), dir.to_string_lossy().to_string()]));
            v.push(("foot".into(), vec![]));
            v.push(("xterm".into(), vec![]));
            v
        };
        for (cmd, args) in candidates {
            if Command::new(&cmd).args(&args).current_dir(&dir).spawn().is_ok() { ok = true; break; }
        }
        ok
    };

    if !spawned { manager.send_toast("No terminal found"); }
}
```

- [ ] **Step 2: Rewrite `open_terminal_action`**

Replace its body (line ~315). Use the selection's directory, or the focused dir:

```rust
    open_terminal_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let dir = manager.collect_selection().into_iter().next()
                    .and_then(|s| if s.path.is_dir() { Some(s.path) } else { s.path.parent().map(|p| p.to_path_buf()) })
                    .or_else(|| manager.focused_dir());
                if let Some(dir) = dir { file_ops::open_terminal(manager.clone(), dir); }
            }
        });
    });
```

- [ ] **Step 3: Build + verify + commit**

Run: `cargo build` (0 warnings). `cargo run --release` → Open in Terminal → a terminal opens at the folder.

```bash
git add src/file_ops.rs src/main.rs
git commit -m "feat: implement Open in Terminal (per-platform)"
```

---

## Task 9: Compress (zip)

**Files:**
- Modify: `Cargo.toml` (add `zip`)
- Modify: `src/file_ops.rs` (add `compress` + archive-name helper + test)
- Modify: `src/main.rs` (`compress_action`)

- [ ] **Step 1: Add the dependency**

In `Cargo.toml` under `[dependencies]`:

```toml
zip = { version = "2", default-features = false, features = ["deflate"] }
```

Run: `cargo build` (downloads the crate; expect success, 0 warnings).

- [ ] **Step 2: Add archive-name helper + test to `file_ops.rs`**

```rust
/// Choose the archive file name for a selection.
pub fn archive_name(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        if let Some(name) = paths[0].file_name().and_then(|n| n.to_str()) {
            return format!("{name}.zip");
        }
    }
    "Archive.zip".to_string()
}
```

Add to the `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn archive_name_single_and_multi() {
        assert_eq!(archive_name(&[PathBuf::from("/a/b/photo.png")]), "photo.png.zip");
        assert_eq!(archive_name(&[PathBuf::from("/a/x"), PathBuf::from("/a/y")]), "Archive.zip");
    }
```

- [ ] **Step 3: Add `compress` to `file_ops.rs`**

```rust
/// Zip the selected paths into a .zip in their parent directory.
pub fn compress(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    let Some(first) = paths.first() else { return };
    let Some(dir) = first.parent().map(|p| p.to_path_buf()) else { return };
    let name = dedupe_file_name(&archive_name(&paths), |n| dir.join(n).exists());
    let out = dir.join(name);

    let manager_c = manager.clone();
    glib::spawn_future_local(async move {
        let paths_c = paths.clone();
        let out_c = out.clone();
        let res = gio::spawn_blocking(move || zip_paths(&paths_c, &out_c)).await;
        match res {
            Ok(Ok(())) => { manager_c.send_toast("Compressed"); manager_c.refresh(); }
            _ => manager_c.send_toast("Compression failed"),
        }
    });
}

fn zip_paths(paths: &[PathBuf], out: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    fn add(zip: &mut zip::ZipWriter<std::fs::File>, base: &Path, path: &Path, opts: &zip::write::FileOptions<()>) -> std::io::Result<()> {
        let name = path.strip_prefix(base.parent().unwrap_or(base)).unwrap_or(path).to_string_lossy().to_string();
        if path.is_dir() {
            zip.add_directory(format!("{name}/"), *opts).map_err(std::io::Error::other)?;
            for entry in std::fs::read_dir(path)? {
                add(zip, base, &entry?.path(), opts)?;
            }
        } else {
            zip.start_file(name, *opts).map_err(std::io::Error::other)?;
            let data = std::fs::read(path)?;
            zip.write_all(&data)?;
        }
        Ok(())
    }

    for p in paths {
        add(&mut zip, p, p, &opts)?;
    }
    zip.finish().map_err(std::io::Error::other)?;
    Ok(())
}
```

Note: `zip` API names (`FileOptions`, `CompressionMethod::Deflated`, `add_directory`, `start_file`, `finish`) are for `zip` v2; if the resolved minor version differs, adjust per its docs. `std::io::Error::other` exists in recent Rust; if unavailable, use `std::io::Error::new(std::io::ErrorKind::Other, e)`.

- [ ] **Step 4: Rewrite `compress_action`**

Replace its body (line ~281):

```rust
    compress_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if !paths.is_empty() { file_ops::compress(manager.clone(), paths); }
            }
        });
    });
```

- [ ] **Step 5: Build + test + verify + commit**

Run: `cargo build` (0 warnings). `cargo test file_ops` (passes incl. `archive_name_single_and_multi`). `cargo run --release` → select files → Compress → a `.zip` appears and opens correctly.

```bash
git add Cargo.toml Cargo.lock src/file_ops.rs src/main.rs
git commit -m "feat: implement Compress to zip"
```

---

## Task 10: Email + Sharing (native, best-effort) — highest risk, last

**Files:**
- Modify: `src/file_ops.rs` (add `email` + `share`)
- Modify: `src/main.rs` (`email_action`, `sharing_options_action`; hide Share on Linux)

- [ ] **Step 1: Add `email` to `file_ops.rs`**

```rust
/// Attach the given files to a new email via the platform's mechanism.
pub fn email(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    use std::process::Command;
    if paths.is_empty() { return; }

    #[cfg(target_os = "macos")]
    {
        // Build an AppleScript that creates a Mail message and attaches each file.
        let mut script = String::from("tell application \"Mail\"\nset m to make new outgoing message\ntell m\n");
        for p in &paths {
            script.push_str(&format!(
                "make new attachment with properties {{file name:POSIX file \"{}\"}}\n",
                p.to_string_lossy().replace('"', "\\\"")
            ));
        }
        script.push_str("end tell\nset visible of m to true\nactivate\nend tell\n");
        let ok = Command::new("osascript").arg("-e").arg(&script).spawn().is_ok();
        if !ok { manager.send_toast("Could not open Mail"); }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = Command::new("xdg-email");
        for p in &paths { cmd.arg("--attach").arg(p); }
        if cmd.spawn().is_err() { manager.send_toast("xdg-email not available"); }
    }
}
```

- [ ] **Step 2: Add `share` to `file_ops.rs`**

macOS: present `NSSharingServicePicker`. This requires Objective-C interop and anchoring to an `NSView`/rect; GTK's macOS backend does not expose its `NSView` cleanly. **Decision rule:** attempt the picker via `objc2` + `objc2-app-kit`; if anchoring to the GTK window proves infeasible during implementation, **fall back to `email`** (already implemented) and hide the separate "Sharing Options" menu item on all platforms. Do not leave a stub.

Add the contingency-safe version now (delegates to email; replace with the native picker only if anchoring works):

```rust
/// Share files via the platform share mechanism. Currently delegates to email
/// as the reliable cross-target path; a native macOS NSSharingServicePicker can
/// replace this if window anchoring is solved.
pub fn share(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    email(manager, paths);
}
```

- [ ] **Step 3: Rewrite `email_action` and `sharing_options_action`**

```rust
    email_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter()
                    .filter(|s| !s.path.is_dir()).map(|s| s.path).collect();
                if paths.is_empty() { manager.send_toast("Select file(s) to email"); return; }
                file_ops::email(manager.clone(), paths);
            }
        });
    });
```

```rust
    sharing_options_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let paths: Vec<PathBuf> = manager.collect_selection().into_iter().map(|s| s.path).collect();
                if !paths.is_empty() { file_ops::share(manager.clone(), paths); }
            }
        });
    });
```

- [ ] **Step 4: Confirm "Sharing Options" is macOS-only**

This is already handled by Task 1B's `build_context_menu`, which appends "Sharing Options" inside `#[cfg(target_os = "macos")]` and only when `caps.read`. No code change here — just verify on Linux that the item is absent, and on macOS that it appears for readable selections. If the cfg guard is missing for any reason, add it in `build_context_menu`.

- [ ] **Step 5: Build + verify + commit**

Run: `cargo build` (0 warnings). `cargo run --release` → select files → Email opens a draft with attachments (macOS Mail / `xdg-email`). Sharing Options behaves per platform.

```bash
git add src/file_ops.rs src/main.rs src/utils.rs
git commit -m "feat: implement Email and Sharing Options (native, best-effort)"
```

---

## Final verification

- [ ] `cargo build` — 0 warnings, 0 errors.
- [ ] `cargo test file_ops` — all pass.
- [ ] `cargo run --release` — exercise every context-menu item with single and multi-selection, including a paste collision (Replace/Skip/Keep both + apply-to-all), a cross-folder move, a nested List-view item (delete/properties target the correct file), and clipboard interop with Finder/GNOME Files.
- [ ] Update `README.md` if any advertised shortcut changed (none expected).
- [ ] Open a PR from `feature/file-operations` into `main`.
