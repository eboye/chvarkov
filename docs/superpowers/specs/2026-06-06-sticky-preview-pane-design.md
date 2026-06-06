# Sticky Right-Hand Preview Pane — Design

**Date:** 2026-06-06
**Status:** Approved (pending implementation)

## Goal

Make the file preview a fixed pane docked to the **right edge** of the content area,
visible whenever a single file is selected, instead of an inline column appended
after the selected file. Today the preview is appended into the Miller `columns_box`,
so it scrolls out of view as you navigate deeper. The docked pane stays pinned to the
right regardless of how many columns are open, and works in all three views (Miller,
Icon, List).

## Behavior & UX

### Visibility

- The dock appears **only when exactly one file is selected** (in any view).
- Selecting a **folder**, **multiple items**, or **nothing** → the dock is hidden and
  its content cleared (so e.g. `GtkVideo` playback stops).
- In Miller view, selecting a folder still spawns a child column as it does today; the
  dock is independent of that navigation.

### Content

- The dock renders the existing rich preview via `Preview::create_preview_layout`
  (image via `gtk::Picture`, video via `GtkVideo`, text via `GtkSourceView`, otherwise
  the file-info layout) — the same content used by the `Space` Quick Look window.
- As the user arrows between files, the dock updates live (the same selection path that
  already updates the floating Quick Look window when open).

### Position & sizing

- The dock sits in a horizontal `content_row` to the right of the active view, OUTSIDE
  the Miller horizontal scroll, so it stays pinned to the right edge.
- The dock is **resizable** via a drag handle on its left edge (reusing the existing
  resizer pattern from `Preview::new`), with a default width of ~300px.
- The width is **not persisted** across sessions (no GSettings schema change). This is a
  deliberate scope limit; see Out of Scope.

### Relationship to `Space` / Quick Look

- The `Space` key and the floating Quick Look window are **unchanged**. `Space` opens the
  larger floating "zoom" view of the same file (with its `Esc` + close button); the
  docked pane stays put underneath. `update_preview_if_open` continues to update the
  floating window when it is open.

## Architecture / components

### Layout (`build_ui`)

Today: `main_content` (vertical Box) appends header → active view → breadcrumb, and the
active view (one of: Miller `scrolled_window`, Icon `grid_view` widget, List
`column_view` widget, or the not-implemented fallback label) is appended directly.

Change: introduce a horizontal **`content_row`** Box that holds the active view (with
`hexpand(true)` so it takes remaining width) and the **`preview_dock`** (fixed width,
hidden initially). `content_row` is appended to `main_content` in place of the bare view.

```
main_content (vertical)
 ├─ header bar
 ├─ content_row (horizontal)
 │    ├─ <active view>   hexpand=true
 │    └─ preview_dock    fixed width, visible=false initially
 └─ breadcrumb bar
```

### `preview_dock`

A managed widget owned by `ColumnManager`:
- A horizontal `gtk::Box` containing a left-edge resizer (`gtk::Separator` with a
  `col-resize` cursor + `GestureDrag`, mirroring `Preview::new`) and a
  `gtk::ScrolledWindow` whose child is swapped to the current preview layout.
- Starts with `set_visible(false)`.
- `ColumnManager::update_dock(selection: Option<&SelectionInfo>)`:
  - If `selection` is `Some` and the item is a file → build
    `Preview::create_preview_layout(file_info, path, false)`, set it as the scrolled
    window's child, and `set_visible(true)`.
  - Otherwise → clear the scrolled window's child (drop the old preview widget) and
    `set_visible(false)`.

### Selection flow (`ColumnManager::handle_selection_change_multi`)

- The existing method already computes the current selection and updates breadcrumbs and
  the floating Quick Look (`update_preview_if_open`). Add a call to `update_dock(...)`
  here so **all three views** drive the dock through one path.
- Remove the Miller-only inline-preview branch that appends `Preview::new(...)` into
  `columns_box`. Selecting a file still collapses deeper child columns (pop all
  `entries` after the selected file's column); it just no longer appends a preview
  column. Folder selection still calls `add_column` for navigation.
- The dock is **not** added to the `entries` list and is **not** a focus target, so
  Left/Right arrow navigation ignores it (it is a passive preview).

### Pure helper (`utils.rs` or `main.rs`)

`should_dock_preview(selection_count: usize, is_dir: bool) -> bool` returns
`selection_count == 1 && !is_dir`. Small, unit-tested, and used by the selection flow to
decide whether to show the dock.

## Data flow

```
selection changes (Miller column / Icon grid / List column)
   └─ handle_selection_change_multi(selection_model, path, index)
        ├─ update current_selection
        ├─ update_breadcrumbs
        ├─ update_preview_if_open      (floating Quick Look, unchanged)
        ├─ update_dock(selection)      ← NEW: show file preview / hide otherwise
        └─ (Miller only) folder -> add_column(child);
                          file   -> collapse deeper columns (no preview column appended)
```

## Error handling

- No new `unwrap()` on the UI thread; `update_dock` takes an `Option` and handles the
  `None`/non-file case by hiding.
- `Preview::create_preview_layout` already handles missing thumbnails / unknown content
  types (falls back to the file-info layout).
- Clearing the scrolled window's child on hide drops the previous preview widget so video
  playback stops and resources are released.

## Testing

### Unit (headless)

- `should_dock_preview`:
  - `(1, false)` → true (single file)
  - `(1, true)` → false (single folder)
  - `(0, false)` → false (nothing)
  - `(2, false)` → false (multiple)

### Manual (Verification Checklist)

- `cargo check` / `cargo clippy --all-targets` pass with zero warnings.
- Select a file in Miller → preview docks on the right; navigate several folders deep and
  scroll the columns → the pane stays pinned to the right edge (does not scroll away).
- Select a folder, multiple items, or click empty space → the dock hides.
- Selecting a video then hiding the dock stops playback.
- Arrowing between files updates the docked preview live.
- The dock works the same way in Icon and List views.
- `Space` still opens the larger floating Quick Look window of the same file; `Esc` and
  the close button still close it; the docked pane is unaffected.
- Left/Right arrows move between columns and never focus the dock.
- The dock's left-edge handle resizes it; columns reflow to fill the remaining width.

## Out of scope (YAGNI)

- Persisting the dock width across sessions (would require a GSettings schema key +
  recompile). Width resets to the default each launch.
- A user toggle/setting to disable the dock entirely.
- A placeholder/folder-info state in the dock when no file is selected (the dock simply
  hides).
- Changing the `Space` floating Quick Look window behavior.
