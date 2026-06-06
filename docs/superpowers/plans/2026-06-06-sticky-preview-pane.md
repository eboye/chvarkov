# Sticky Right-Hand Preview Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dock the file preview to the right edge of the content area (visible only when a single file is selected, in all three views) instead of appending it as an inline Miller column that scrolls away.

**Architecture:** A horizontal `content_row` wraps the active view (expanding) and a fixed-width, resizable `preview_dock` (hidden by default) that lives outside the Miller horizontal scroll. `ColumnManager` gains `update_dock(show)` driven from the single existing selection path so Miller/Icon/List all feed it. A pure `should_dock_preview` helper decides visibility. The old inline-preview-column code (and `Preview::new`) is removed.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22.

**Spec:** `docs/superpowers/specs/2026-06-06-sticky-preview-pane-design.md`

---

## File Structure

- `src/utils.rs` — add the pure `should_dock_preview` helper; (re)create the `#[cfg(test)] mod tests` module with its tests **and restore the previously-lost utils tests** (Task 1).
- `src/main.rs` — `ColumnManager` dock fields + `set_preview_dock`/`update_dock`; a `build_preview_dock` helper; `build_ui` `content_row` wiring; `handle_selection_change_multi` rewire (Task 2).
- `src/preview.rs` — remove the now-unused `Preview::new` and reduce `Preview` to a unit struct (Task 2).
- `README.md`, `CHANGELOG.md` — document the feature (Task 3).

---

## Task 1: `should_dock_preview` helper + restore utils tests

`src/utils.rs` currently has **no `mod tests`** — four unit tests were lost earlier. This task adds the new helper and (re)creates the test module containing both the new test and the restored ones. All functions referenced by the restored tests (`combine_caps`, `caps_from_info`, `format_size`, `get_list_icon_size`, `get_grid_icon_size`, `get_font_size`) already exist in `src/utils.rs`.

**Files:**
- Modify: `src/utils.rs` (add helper near the other small pure helpers; add `mod tests` at end of file)

- [ ] **Step 1: Write the failing tests**

Append this block to the very end of `src/utils.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_dock_only_for_single_file() {
        assert!(should_dock_preview(1, false));   // one file
        assert!(!should_dock_preview(1, true));   // one folder
        assert!(!should_dock_preview(0, false));  // nothing selected
        assert!(!should_dock_preview(2, false));  // multiple files
    }

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

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn zoom_size_tables() {
        assert_eq!(get_list_icon_size(0), 16);
        assert_eq!(get_list_icon_size(4), 64);
        assert_eq!(get_list_icon_size(99), 96);
        assert_eq!(get_grid_icon_size(0), 48);
        assert_eq!(get_grid_icon_size(99), 128);
        assert_eq!(get_font_size(0), 10);
        assert_eq!(get_font_size(99), 18);
    }

    #[test]
    fn caps_from_info_reads_and_defaults() {
        let info = gio::FileInfo::new();
        info.set_attribute_boolean("access::can-read", true);
        info.set_attribute_boolean("access::can-delete", false);
        let c = caps_from_info(&info);
        assert!(c.read);
        assert!(!c.delete);
        assert!(c.write); // unset attribute defaults to permitted (true)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib should_dock 2>&1 | tail -20`
Expected: FAIL — `cannot find function `should_dock_preview` in this scope`.

- [ ] **Step 3: Implement `should_dock_preview`**

Add this function to `src/utils.rs` immediately after the `combine_caps` function (it sits naturally with the other small pure helpers):

```rust
/// Whether the docked preview pane should be shown for the current selection.
/// True only for a single selected file (not a folder, not a multi-selection,
/// not an empty selection).
pub fn should_dock_preview(selection_count: usize, is_dir: bool) -> bool {
    selection_count == 1 && !is_dir
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib 2>&1 | tail -20`
Expected: PASS — the new `should_dock_*` test and the 4 restored tests pass alongside the rest.

- [ ] **Step 5: Clippy**

Run: `cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head` (expect no output)

- [ ] **Step 6: Commit**

```bash
git add src/utils.rs
git commit -m "feat(utils): add should_dock_preview; restore lost utils unit tests"
```

---

## Task 2: Integrate the docked preview pane

This is the core change: add the dock to `ColumnManager`, wrap the views in a
`content_row`, drive the dock from the selection path, and remove the old inline
preview column (and the now-unused `Preview::new`). Done as one task because these
pieces must change together to keep the build clean and avoid a double-preview state.

**Files:**
- Modify: `src/main.rs` — `ColumnManager` struct/`new` (~lines 1534-1578); add methods after `update_preview_if_open` (~line 1849); add `build_preview_dock` free fn; `build_ui` view section (~lines 1388-1430); `handle_selection_change_multi` (~lines 1953-2027).
- Modify: `src/preview.rs` — remove `Preview::new`, reduce `Preview` to a unit struct (~lines 9-59).

- [ ] **Step 1: Add dock fields to `ColumnManager`**

In `src/main.rs`, in the `struct ColumnManager` definition, add two fields after the
`preview_window` field:

```rust
    preview_dock: Rc<RefCell<Option<Box>>>,
    preview_dock_content: Rc<RefCell<Option<ScrolledWindow>>>,
```

And in `ColumnManager::new`, add their initialisers after the `preview_window` line:

```rust
            preview_dock: Rc::new(RefCell::new(None)),
            preview_dock_content: Rc::new(RefCell::new(None)),
```

- [ ] **Step 2: Add `set_preview_dock` and `update_dock` methods**

In `src/main.rs`, add these two methods immediately after the `update_preview_if_open`
method (which ends around line 1849):

```rust
    fn set_preview_dock(&self, container: Box, content: ScrolledWindow) {
        *self.preview_dock.borrow_mut() = Some(container);
        *self.preview_dock_content.borrow_mut() = Some(content);
    }

    /// Show the docked preview for the current single-file selection, or hide it.
    /// Clearing the content on hide drops the previous preview widget (so video
    /// playback stops).
    fn update_dock(&self, show: bool) {
        let container = self.preview_dock.borrow().clone();
        let content = self.preview_dock_content.borrow().clone();
        let (Some(container), Some(content)) = (container, content) else { return };
        if show
            && let Some(sel) = self.current_selection.borrow().as_ref() {
                let layout = Preview::create_preview_layout(&sel.file_info, &sel.path, false);
                content.set_child(Some(&layout));
                container.set_visible(true);
                return;
            }
        content.set_child(None::<&gtk::Widget>);
        container.set_visible(false);
    }
```

- [ ] **Step 3: Add the `build_preview_dock` helper**

In `src/main.rs`, add this free function (place it near `show_name_dialog`, outside the
`impl ColumnManager` block):

```rust
/// Build the docked preview pane: a fixed-width, resizable container (hidden by
/// default) plus the scrolled window whose child is swapped to the current preview.
/// Returns (container, content_scrolled). The resizer is on the dock's left edge.
fn build_preview_dock() -> (Box, ScrolledWindow) {
    let content = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .width_request(300)
        .vexpand(true)
        .build();

    let resizer = gtk::Separator::new(Orientation::Vertical);
    resizer.set_cursor_from_name(Some("col-resize"));
    resizer.add_css_class("resizer");

    let drag = gtk::GestureDrag::new();
    let content_weak = content.downgrade();
    let resizer_weak = resizer.downgrade();
    drag.connect_drag_update(move |g, off_x, off_y| {
        if let (Some(sw), Some(rz)) = (content_weak.upgrade(), resizer_weak.upgrade())
            && let Some((sx, sy)) = g.start_point() {
                let (cx, cy) = (sx + off_x, sy + off_y);
                // The dock content is to the RIGHT of the resizer; dragging the
                // handle left should widen it. Pointer x in the content's frame goes
                // negative as it moves left of the content edge, so width - dx grows.
                if let Some((dx, _)) = rz.translate_coordinates(&sw, cx, cy) {
                    let new_w = (sw.width() as f64 - dx).round() as i32;
                    sw.set_width_request(new_w.max(150));
                }
            }
    });
    resizer.add_controller(drag);

    let container = Box::builder().orientation(Orientation::Horizontal).build();
    container.append(&resizer);
    container.append(&content);
    container.set_visible(false);
    (container, content)
}
```

- [ ] **Step 4: Wrap the views in a `content_row` and attach the dock**

In `src/main.rs` `build_ui`, replace this block (currently ~lines 1388-1430):

```rust
    if view_type == "miller" {
        main_content.append(&manager.scrolled_window);
        let first_list_view = manager.add_column(initial_path.clone(), 0);

        if let Some(lv) = first_list_view {
            lv.add_css_class("focused-column");
            lv.grab_focus();
        }
    } else if view_type == "icons" {
        let icon_view = IconView::new(&initial_path, show_hidden, show_meta, zoom_level, &manager.sort_type, folders_first);

        let manager_icon_clone = manager.clone();
        let path_icon_clone = initial_path.clone();
        icon_view.grid_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::MultiSelection>().unwrap();
            manager_icon_clone.handle_selection_change_multi(selection_model, &path_icon_clone, 0);
        });

        main_content.append(&icon_view.widget);
        icon_view.grid_view.add_css_class("focused-grid");
        icon_view.grid_view.grab_focus();
        manager.set_main_view(icon_view.grid_view.clone().upcast::<gtk::Widget>());
    } else if view_type == "list" {
        let list_view_widget = ListView::new(&initial_path, show_hidden, show_meta, zoom_level, &manager.sort_type, folders_first);

        let manager_list_clone = manager.clone();
        let path_list_clone = initial_path.clone();
        list_view_widget.column_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::MultiSelection>().unwrap();
            manager_list_clone.handle_selection_change_multi(selection_model, &path_list_clone, 0);
        });

        main_content.append(&list_view_widget.widget);
        list_view_widget.column_view.add_css_class("focused-list");
        list_view_widget.column_view.grab_focus();
        manager.set_main_view(list_view_widget.column_view.clone().upcast::<gtk::Widget>());
    } else {
        let label = gtk::Label::new(Some(&format!("{} view is not yet implemented", view_type)));
        label.set_vexpand(true);
        main_content.append(&label);
    }

    main_content.append(&breadcrumb_scrolled);
```

with:

```rust
    // Content row: the active view (expands) plus the docked preview pane (right).
    let content_row = Box::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .vexpand(true)
        .build();
    let (dock_container, dock_content) = build_preview_dock();
    manager.set_preview_dock(dock_container.clone(), dock_content);

    if view_type == "miller" {
        content_row.append(&manager.scrolled_window);
        let first_list_view = manager.add_column(initial_path.clone(), 0);

        if let Some(lv) = first_list_view {
            lv.add_css_class("focused-column");
            lv.grab_focus();
        }
    } else if view_type == "icons" {
        let icon_view = IconView::new(&initial_path, show_hidden, show_meta, zoom_level, &manager.sort_type, folders_first);

        let manager_icon_clone = manager.clone();
        let path_icon_clone = initial_path.clone();
        icon_view.grid_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::MultiSelection>().unwrap();
            manager_icon_clone.handle_selection_change_multi(selection_model, &path_icon_clone, 0);
        });

        icon_view.widget.set_hexpand(true);
        content_row.append(&icon_view.widget);
        icon_view.grid_view.add_css_class("focused-grid");
        icon_view.grid_view.grab_focus();
        manager.set_main_view(icon_view.grid_view.clone().upcast::<gtk::Widget>());
    } else if view_type == "list" {
        let list_view_widget = ListView::new(&initial_path, show_hidden, show_meta, zoom_level, &manager.sort_type, folders_first);

        let manager_list_clone = manager.clone();
        let path_list_clone = initial_path.clone();
        list_view_widget.column_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::MultiSelection>().unwrap();
            manager_list_clone.handle_selection_change_multi(selection_model, &path_list_clone, 0);
        });

        list_view_widget.widget.set_hexpand(true);
        content_row.append(&list_view_widget.widget);
        list_view_widget.column_view.add_css_class("focused-list");
        list_view_widget.column_view.grab_focus();
        manager.set_main_view(list_view_widget.column_view.clone().upcast::<gtk::Widget>());
    } else {
        let label = gtk::Label::new(Some(&format!("{} view is not yet implemented", view_type)));
        label.set_vexpand(true);
        label.set_hexpand(true);
        content_row.append(&label);
    }

    content_row.append(&dock_container);
    main_content.append(&content_row);
    main_content.append(&breadcrumb_scrolled);
```

- [ ] **Step 5: Drive the dock from the selection path (and remove the inline preview column)**

In `src/main.rs` `handle_selection_change_multi`, make two edits.

(a) In the **empty-selection** branch, add `self.update_dock(false);` right after the
existing `self.update_preview_if_open();` call. The branch becomes:

```rust
        let selection = selection_model.selection();
        if selection.is_empty() {
            *self.current_selection.borrow_mut() = None;
            self.update_breadcrumbs(base_path);

            // Clear subsequent columns
            let mut entries = self.entries.borrow_mut();
            while entries.len() > index + 1 {
                let entry = entries.pop().unwrap();
                self.columns_box.remove(&entry.container);
            }

            self.update_preview_if_open();
            self.update_dock(false);
            return;
        }
```

(b) In the **non-empty** branch, replace this section (currently ~lines 1992-2026):

```rust
            self.update_breadcrumbs(&new_path);
            self.update_preview_if_open();

            // In List View, we don't necessarily want to jump columns unless it's Miller
            let settings = gio::Settings::new("net.nocopypaste.chvarkov");
            let view_type: String = settings.get("view-type");

            if view_type == "miller" {
                if file_info.file_type() == gio::FileType::Directory || new_path.is_dir() {
                    self.add_column(new_path, index + 1);
                } else {
                    let mut entries = self.entries.borrow_mut();
                    let should_replace = if index + 1 < entries.len() {
                        entries[index + 1].path != new_path
                    } else {
                        true
                    };

                    if should_replace {
                        while entries.len() > index + 1 {
                            let entry = entries.pop().unwrap();
                            self.columns_box.remove(&entry.container);
                        }

                        let preview = Preview::new(&file_info, &new_path);
                        let preview_widget = preview.widget.upcast::<gtk::Widget>();
                        self.columns_box.append(&preview_widget);
                        entries.push(ColumnEntry {
                            container: preview_widget.clone(),
                            focus_target: preview_widget,
                            path: new_path,
                        });
                    }
                }
            }
```

with:

```rust
            self.update_breadcrumbs(&new_path);
            self.update_preview_if_open();

            let is_dir = file_info.file_type() == gio::FileType::Directory || new_path.is_dir();
            self.update_dock(utils::should_dock_preview(selection.size() as usize, is_dir));

            // In List View, we don't necessarily want to jump columns unless it's Miller
            let settings = gio::Settings::new("net.nocopypaste.chvarkov");
            let view_type: String = settings.get("view-type");

            if view_type == "miller" {
                if is_dir {
                    self.add_column(new_path, index + 1);
                } else {
                    // Selecting a file collapses any deeper columns; the preview now
                    // lives in the docked pane rather than an appended column.
                    let mut entries = self.entries.borrow_mut();
                    while entries.len() > index + 1 {
                        let entry = entries.pop().unwrap();
                        self.columns_box.remove(&entry.container);
                    }
                }
            }
```

- [ ] **Step 6: Remove the now-unused `Preview::new` and reduce `Preview` to a unit struct**

In `src/preview.rs`, replace:

```rust
pub struct Preview {
    pub widget: gtk::Box,
}

impl Preview {
    pub fn new(file_info: &gio::FileInfo, path: &std::path::Path) -> Self {
```

…(the whole `new` function, through its closing `}` at the end of `new`)… so that the
file goes from `pub struct Preview { pub widget: gtk::Box }` + `impl Preview { pub fn new(...) {...}` to:

```rust
pub struct Preview;

impl Preview {
```

Concretely: delete the `pub widget: gtk::Box,` field (make `Preview` a unit struct) and
delete the entire `pub fn new(...) -> Self { ... }` method, leaving
`create_preview_layout` and `create_properties_layout` as the impl's contents. Do not
touch those two functions.

- [ ] **Step 7: Build, clippy, test**

Run: `cargo build 2>&1 | tail -5 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: `Finished` with no errors; **no clippy output** (in particular, no
"unused import" left in `preview.rs` from removing `new`, and no dead-code warning for
`Preview`); all tests pass. If clippy reports a now-unused import in `preview.rs`
(e.g. a widget type only `new` used), remove that import.

- [ ] **Step 8: Commit**

```bash
git add src/main.rs src/preview.rs
git commit -m "feat: dock file preview to the right edge (all views); remove inline preview column"
```

---

## Task 3: Documentation + verification

**Files:**
- Modify: `README.md` (Live Previews bullet, ~line 13)
- Modify: `CHANGELOG.md` (add an Unreleased entry at the top, ~line 8)

- [ ] **Step 1: Update the README Live Previews bullet**

In `README.md`, replace:

```markdown
- **Live Previews:** Instantly view file details, metadata, large icons, images, video, and syntax-highlighted text.
```

with:

```markdown
- **Live Previews:** Selecting a file opens a preview docked to the right edge — file details, metadata, large icons, images, video, and syntax-highlighted text — staying pinned in place as you navigate. Press `Space` for a larger floating Quick Look.
```

- [ ] **Step 2: Add a CHANGELOG entry**

In `CHANGELOG.md`, add at the top (after the intro paragraph, before the most recent
`## [...]` heading):

```markdown
## [Unreleased]

### Changed
- The file preview is now a **fixed pane docked to the right edge** of the window, shown whenever a single file is selected (in Miller, Icon, and List views). It stays pinned in place instead of scrolling away as an inline Miller column. Selecting a folder, multiple items, or empty space hides it. The pane is resizable; `Space` still opens the larger floating Quick Look.

```

- [ ] **Step 3: Commit docs**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: document the docked right-hand preview pane"
```

- [ ] **Step 4: Full build + lint + test**

Run: `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: clean build, no clippy output, all tests pass.

- [ ] **Step 5: Manual verification**

Run `cargo run` and confirm:
- Select a file in Miller → preview docks on the right. Open several folders deep and
  scroll the columns horizontally → the dock stays pinned to the right edge.
- Select a folder, select multiple items (Shift/Ctrl), or click empty space → the dock
  hides.
- Select a video, then hide the dock (select a folder) → playback stops.
- Arrow between files → the docked preview updates live.
- Switch to Icon view and to List view → selecting a file docks the preview there too.
- `Space` opens the larger floating Quick Look of the same file; `Esc` / the close
  button close it; the docked pane is unaffected.
- Left/Right arrows move between Miller columns and never focus the dock.
- Drag the dock's left-edge handle → it resizes and the columns reflow.

- [ ] **Step 6: Finish the branch**

Use the `superpowers:finishing-a-development-branch` skill to merge/PR.

---

## Notes for the implementer

- `Box`, `Orientation`, `ScrolledWindow` are already imported in `main.rs` (used by the
  existing header/columns code) — do not add redundant imports.
- `selection.size()` returns `u64`; cast with `as usize` for `should_dock_preview`.
- The dock must **not** be added to `self.entries` and must **not** be a focus target —
  it is a passive preview, so Left/Right column navigation ignores it automatically.
- Do not add manual `refresh()` calls; the dock is updated directly via `update_dock`.
- Keep `Preview::create_preview_layout` and `Preview::create_properties_layout` exactly
  as they are — only `Preview::new` and the `widget` field are removed.
