# New Folder / New Empty File Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add "New Folder" and "New Empty File" creation to chvarkov via context menu, header button, and keyboard shortcuts, with a name-it-on-creation flow that reuses the rename dialog.

**Architecture:** A pure `untitled_name` helper computes a non-colliding default name. Two async `file_ops` functions create the item via GIO, then open a generalised naming dialog (the old rename dialog, refactored). Actions are wired through the existing `ACTIVE_MANAGER` pattern; the context-menu section is permission-gated; the header gets a "New" `MenuButton`. The monitored `DirectoryList` surfaces the new item automatically.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22.

**Spec:** `docs/superpowers/specs/2026-06-06-new-folder-file-design.md`

---

## File Structure

- `src/file_ops.rs` — add pure `untitled_name` + tests (Task 1); add async `create_folder` / `create_file` (Task 3).
- `src/main.rs` — generalise the rename dialog into `pub(crate) show_name_dialog` (Task 2); add `new-folder`/`new-file` actions + accels, a `target_dir_writable` helper, and the header "New" button (Task 4).
- `src/utils.rs` — extend `build_context_menu` with the permission-gated "New" section (Task 4).
- `README.md`, `CHANGELOG.md` — document the feature (Task 5).

---

## Task 1: `untitled_name` pure helper

**Files:**
- Modify: `src/file_ops.rs` (add fn near `dedupe_file_name` at line 21; add tests in the `mod tests` block at line 544)

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests { ... }` block in `src/file_ops.rs` (after `split_name_cases`, around line 586):

```rust
    #[test]
    fn untitled_no_collision_returns_base() {
        assert_eq!(untitled_name("untitled folder", "", |_| false), "untitled folder");
    }

    #[test]
    fn untitled_first_collision_appends_2() {
        assert_eq!(untitled_name("untitled folder", "", |n| n == "untitled folder"), "untitled folder 2");
    }

    #[test]
    fn untitled_second_collision_appends_3() {
        let taken = |n: &str| n == "untitled folder" || n == "untitled folder 2";
        assert_eq!(untitled_name("untitled folder", "", taken), "untitled folder 3");
    }

    #[test]
    fn untitled_inserts_number_before_extension() {
        assert_eq!(untitled_name("report", ".txt", |n| n == "report.txt"), "report 2.txt");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib untitled 2>&1 | tail -20`
Expected: FAIL — `cannot find function `untitled_name` in this scope`.

- [ ] **Step 3: Implement `untitled_name`**

Add this function in `src/file_ops.rs` immediately after `dedupe_file_name` (after line 38, before `unique_destination`):

```rust
/// Return a name for a brand-new item that does not collide, per `exists`.
/// Uses an "untitled" numbering scheme ("base", "base 2", "base 3", ...),
/// distinct from the " (copy)" scheme used for duplicates. The number is
/// inserted before `ext` (pass "" for no extension).
pub fn untitled_name(base: &str, ext: &str, exists: impl Fn(&str) -> bool) -> String {
    let first = format!("{base}{ext}");
    if !exists(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let cand = format!("{base} {n}{ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib untitled 2>&1 | tail -20`
Expected: PASS — 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/file_ops.rs
git commit -m "feat(file-ops): add untitled_name helper for new-item naming"
```

---

## Task 2: Generalise the rename dialog into `show_name_dialog`

This is a pure refactor — rename behaviour must not change. It exposes a reusable
dialog for creation, adds text pre-selection, and switches the confirm response id
to a stable `"confirm"`.

**Files:**
- Modify: `src/main.rs` — `show_rename_dialog` at lines 1358-1415.

- [ ] **Step 1: Replace `show_rename_dialog` with `show_name_dialog` + a thin wrapper**

Replace the entire `show_rename_dialog` function (lines 1358-1415) in `src/main.rs` with:

```rust
/// Generic "enter a name" dialog. Validates and renames `path` on disk via
/// `set_display_name_async` when confirmed. The entry text is pre-selected so
/// typing replaces it. Confirm response id is "confirm".
pub(crate) fn show_name_dialog(
    parent: &impl IsA<gtk::Widget>,
    manager: Rc<ColumnManager>,
    heading: &str,
    body: &str,
    confirm_label: &str,
    initial: &str,
    path: PathBuf,
) {
    let initial = initial.to_string();
    let entry = gtk::Entry::builder()
        .text(&initial)
        .activates_default(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    // Clone for the post-present selection closure (the response closure moves `entry`).
    let entry_sel = entry.clone();

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .extra_child(&entry)
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("confirm", confirm_label);
    dialog.set_default_response(Some("confirm"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);

    let path_clone = path.clone();
    let manager_c = manager.clone();
    let initial_c = initial.clone();
    dialog.connect_response(None, move |_d, response| {
        if response == "confirm" {
            let new_name = entry.text().to_string();
            if new_name != initial_c {
                if let Err(reason) = file_ops::validate_filename(&new_name) {
                    manager_c.send_toast(&reason);
                    return;
                }
                let file = gio::File::for_path(&path_clone);
                let manager_inner = manager_c.clone();
                let name_to_report = new_name.clone();
                file.set_display_name_async(
                    &new_name,
                    glib::Priority::DEFAULT,
                    gio::Cancellable::NONE,
                    move |res| match res {
                        Ok(_) => manager_inner.send_toast(&format!("Renamed to {}", name_to_report)),
                        Err(e) => manager_inner.send_toast(&format!("Error: {}", e)),
                    },
                );
            }
        }
    });

    dialog.present(Some(parent));

    // Pre-select the name after the dialog is mapped so typing replaces it.
    glib::idle_add_local_once(move || {
        entry_sel.grab_focus();
        entry_sel.select_region(0, -1);
    });
}

fn show_rename_dialog(parent: &ApplicationWindow, manager: Rc<ColumnManager>, old_name_str: &str, path: PathBuf) {
    show_name_dialog(
        parent,
        manager,
        "Rename File",
        &format!("Enter a new name for '{}':", old_name_str),
        "Rename",
        old_name_str,
        path,
    );
}
```

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished` with no errors. If `IsA` is unresolved, confirm `use gtk::prelude::*;` is present at the top of `main.rs` (it is — gtk prelude re-exports `IsA`).

- [ ] **Step 3: Manual check — rename still works**

Run: `cargo run` — select a file, press `F2`, confirm the dialog opens with the
name **pre-selected**, type a new name, press Enter, and confirm the file is
renamed (toast "Renamed to …"). Press `F2` again and `Esc` — nothing changes.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "refactor: generalise rename dialog into show_name_dialog with text pre-select"
```

---

## Task 3: `create_folder` / `create_file` async operations

**Files:**
- Modify: `src/file_ops.rs` — add two functions (place after `symlink`, the last fn before `mod tests`, around line 542).

- [ ] **Step 1: Implement `create_folder` and `create_file`**

Add to `src/file_ops.rs` after the `symlink` function (before `#[cfg(test)]`):

```rust
/// Create a new empty folder in `dir` with a non-colliding "untitled folder"
/// name, then open the naming dialog so the user can rename it. The monitored
/// DirectoryList surfaces the new folder automatically.
pub fn create_folder(manager: Rc<ColumnManager>, parent: gtk::Window, dir: PathBuf) {
    let name = untitled_name("untitled folder", "", |n| dir.join(n).exists());
    let new_path = dir.join(&name);
    let file = gio::File::for_path(&new_path);
    file.make_directory_async(
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |res| match res {
            Ok(_) => crate::show_name_dialog(
                &parent,
                manager,
                "New Folder",
                "Name the new folder:",
                "Create",
                &name,
                new_path,
            ),
            Err(e) => manager.send_toast(&format!("Could not create folder: {}", e)),
        },
    );
}

/// Create a new empty file in `dir` with a non-colliding "untitled file" name,
/// then open the naming dialog so the user can rename it.
pub fn create_file(manager: Rc<ColumnManager>, parent: gtk::Window, dir: PathBuf) {
    let name = untitled_name("untitled file", "", |n| dir.join(n).exists());
    let new_path = dir.join(&name);
    let file = gio::File::for_path(&new_path);
    file.create_async(
        gio::FileCreateFlags::NONE,
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |res| match res {
            Ok(_stream) => crate::show_name_dialog(
                &parent,
                manager,
                "New Empty File",
                "Name the new file:",
                "Create",
                &name,
                new_path,
            ),
            Err(e) => manager.send_toast(&format!("Could not create file: {}", e)),
        },
    );
}
```

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished` with no errors. (These are not unit-testable — they touch the
UI and filesystem asynchronously; they are covered by manual verification in Task 6.)

- [ ] **Step 3: Commit**

```bash
git add src/file_ops.rs
git commit -m "feat(file-ops): add create_folder and create_file"
```

---

## Task 4: Wire actions, shortcuts, context menu, and header button

**Files:**
- Modify: `src/main.rs` — add `target_dir_writable` (after `selection_caps`, line 223); register two actions in `setup_actions` (after the `rename` action, line 424); add the header "New" `MenuButton` (after `sort_type_btn`, line 1073).
- Modify: `src/utils.rs` — add the "New" section in `build_context_menu` (line 151).

- [ ] **Step 1: Add `target_dir_writable` helper**

In `src/main.rs`, add immediately after the `selection_caps` function (after line 223):

```rust
/// Whether the directory new items would be created in is writable. Used to
/// gate the context-menu "New" section. Missing attribute defaults to writable.
pub(crate) fn target_dir_writable() -> bool {
    ACTIVE_MANAGER.with(|m| {
        let Some(manager) = m.borrow().as_ref() else { return false };
        let Some(dir) = manager.focused_dir() else { return false };
        let file = gio::File::for_path(&dir);
        match file.query_info(
            "access::can-write",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        ) {
            Ok(info) => {
                if info.has_attribute("access::can-write") {
                    info.boolean("access::can-write")
                } else {
                    true
                }
            }
            Err(_) => false,
        }
    })
}
```

- [ ] **Step 2: Register the `new-folder` and `new-file` actions**

In `src/main.rs`, in `setup_actions`, add immediately after the `rename` action
registration (after line 424, `app.set_accels_for_action("app.rename", &["F2"]);`):

```rust
    let new_folder_action = gio::SimpleAction::new("new-folder", None);
    let nf_app_weak = app.downgrade();
    new_folder_action.connect_activate(move |_, _| {
        let Some(app) = nf_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let Some(dir) = manager.focused_dir() else { return };
                file_ops::create_folder(manager.clone(), window.clone().upcast(), dir);
            }
        });
    });
    app.add_action(&new_folder_action);
    app.set_accels_for_action("app.new-folder", &["<Shift><Control>n"]);

    let new_file_action = gio::SimpleAction::new("new-file", None);
    let nfile_app_weak = app.downgrade();
    new_file_action.connect_activate(move |_, _| {
        let Some(app) = nfile_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let Some(dir) = manager.focused_dir() else { return };
                file_ops::create_file(manager.clone(), window.clone().upcast(), dir);
            }
        });
    });
    app.add_action(&new_file_action);
    app.set_accels_for_action("app.new-file", &["<Control>n"]);
```

- [ ] **Step 3: Add the "New" section to the context menu**

In `src/utils.rs`, in `build_context_menu` (starts line 151), insert this block
immediately after `let menu = gio::Menu::new();` (line 153) and before the
`if count >= 1 {` Open section:

```rust
    if crate::target_dir_writable() {
        let new_section = gio::Menu::new();
        new_section.append(Some("New Folder"), Some("app.new-folder"));
        new_section.append(Some("New Empty File"), Some("app.new-file"));
        menu.append_section(None, &new_section);
    }
```

- [ ] **Step 4: Add the header "New" MenuButton**

In `src/main.rs`, in `build_ui`, add immediately after `header_bar.pack_start(&sort_type_btn);`
(line 1073):

```rust
    // New (create) Menu
    let new_menu = gio::Menu::new();
    new_menu.append(Some("New Folder"), Some("app.new-folder"));
    new_menu.append(Some("New Empty File"), Some("app.new-file"));

    let new_btn_content = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    new_btn_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    let new_btn_label = gtk::Label::new(Some("New"));
    new_btn_label.add_css_class("adaptive-label");
    new_btn_content.append(&new_btn_label);

    let new_type_btn = gtk::MenuButton::builder()
        .child(&new_btn_content)
        .tooltip_text("Create New")
        .menu_model(&new_menu)
        .build();
    header_bar.pack_start(&new_type_btn);
```

- [ ] **Step 5: Build + test**

Run: `cargo build 2>&1 | tail -5 && cargo test --lib 2>&1 | tail -3`
Expected: `Finished` with no errors; all unit tests pass.

- [ ] **Step 6: Clippy**

Run: `cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head` (expect no output)

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/utils.rs
git commit -m "feat: wire New Folder/New File actions, shortcuts, menu, and header button"
```

---

## Task 5: Documentation

**Files:**
- Modify: `README.md` (Keyboard Shortcuts table, line 33-56; File Operations section, line 19-23)
- Modify: `CHANGELOG.md` (add an Unreleased/next entry at the top, line 8)

- [ ] **Step 1: Add the shortcuts to the README table**

In `README.md`, add these two rows to the Keyboard Shortcuts table (after the
**Rename** row at line 46):

```markdown
| **New Folder** | `Ctrl` + `Shift` + `N` |
| **New Empty File** | `Ctrl` + `N` |
```

- [ ] **Step 2: Note creation in the File Operations section**

In `README.md`, update the Move/Rename bullet (line 21) to mention creation. Replace:

```markdown
- **Move to… / Copy to…**, **Rename**, **Create Link** (symlink), and **Compress to Zip**.
```

with:

```markdown
- **New Folder** and **New Empty File** (with inline naming), **Move to… / Copy to…**, **Rename**, **Create Link** (symlink), and **Compress to Zip**.
```

- [ ] **Step 3: Add a CHANGELOG entry**

In `CHANGELOG.md`, add at the top (after the intro paragraph, before `## [0.2.2]`
at line 8):

```markdown
## [Unreleased]

### Added
- **New Folder** and **New Empty File** creation — from the right-click menu, a "New" header-bar button, and keyboard shortcuts (`Ctrl/Cmd+Shift+N` and `Ctrl/Cmd+N`). New items get a non-colliding "untitled" name and open a naming dialog (text pre-selected) so you can rename immediately; the menu entries are hidden in read-only directories.

```

- [ ] **Step 4: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: document New Folder / New File"
```

---

## Task 6: Manual verification

**Files:** none (verification only)

- [ ] **Step 1: Full build + lint**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean build, no clippy output, all tests pass.

- [ ] **Step 2: Run the verification checklist**

Run `cargo run` and confirm:
- New Folder via `Ctrl+Shift+N`, the context menu, and the header "New" button — each creates `untitled folder` and opens the naming dialog with the text pre-selected.
- New Empty File via `Ctrl+N`, the context menu, and the header button — creates `untitled file`.
- Typing a name + Enter renames; `Esc` keeps the default name; both leave a valid item on disk.
- Creating two folders in a row yields `untitled folder` then `untitled folder 2`.
- In a read-only directory (e.g. `/`), the context-menu "New" section is hidden, and the header button shows a "Could not create…" toast and creates nothing.
- Works in Miller, Icon, and List views, and with Show Hidden on/off.
- The new item appears in the focused column without a manual refresh.

- [ ] **Step 3: Finish the branch**

Use the `superpowers:finishing-a-development-branch` skill to merge/PR.

---

## Notes for the implementer

- `<Control>` in GTK accelerators matches the existing convention in `setup_actions`
  (e.g. `<Control>q`, `<Control>v`); GTK maps it appropriately per platform. Keep it
  consistent — do **not** mix in `<Primary>`.
- `file_ops.rs` already imports `crate::ColumnManager`, `gtk`, `adw`, and their
  preludes, so `crate::show_name_dialog`, `gtk::Window`, and the GIO async methods
  resolve without new `use` lines.
- The new item appears automatically because `get_directory_list` builds the
  `DirectoryList` with `.monitored(true)` — do not add manual `refresh()` calls.
