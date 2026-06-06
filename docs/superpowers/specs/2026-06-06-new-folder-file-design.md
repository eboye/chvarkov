# New Folder / New Empty File — Design

**Date:** 2026-06-06
**Status:** Approved (pending implementation)

## Goal

Let users create a **new folder** or a **new empty file** in chvarkov. Today there
is no way to create anything — every action operates on existing files. This adds
creation via the context menu, a header-bar button, and keyboard shortcuts, with a
name-it-on-creation flow that reuses the existing rename dialog.

## Behavior & UX

### Actions

| Action | Shortcut (Linux) | Shortcut (macOS) | Default name |
| :--- | :--- | :--- | :--- |
| New Folder | `Ctrl+Shift+N` | `⇧⌘N` | `untitled folder` |
| New Empty File | `Ctrl+N` | `⌘N` | `untitled file` (no extension) |

`Ctrl/Cmd+N` is free — chvarkov has no "new window" action.

### Entry points

1. **Context menu** — a new top section with `New Folder` and `New Empty File`.
   Shown whether or not items are selected (creation always targets the current
   directory, not the selection). The section is **omitted entirely when the
   target directory is not writable**.
2. **Header bar** — a `MenuButton` labelled "New" (icon `list-add-symbolic`) whose
   popover menu contains both entries. Always enabled; creation failures surface as
   a toast.
3. **Keyboard** — the shortcuts above, registered as app accelerators.

### Target directory

The directory to create in is `ColumnManager::focused_dir()`:
- Miller view: the directory backing the focused column.
- Icon / List view: falls back to the current selection's parent, then the
  `current-path` GSettings value, then the home directory.

This is the same resolution already used by `open-terminal` and paste, so creation
lands where the user is looking.

### Creation flow

1. Resolve `dir = manager.focused_dir()`.
2. Compute a non-colliding default name with `untitled_name` (see below).
3. Create it on disk asynchronously:
   - Folder: `gio::File::make_directory_async`.
   - File: `gio::File::create_async` (creates an empty regular file; the returned
     stream is closed immediately).
4. On success, the **monitored** `DirectoryList` (built with `.monitored(true)`)
   surfaces the new entry automatically — no explicit refresh needed (this is how
   rename already works).
5. Open the **naming dialog** (the existing rename dialog, generalised) on the new
   path, pre-filled with the default name and with the entry text **pre-selected**
   so typing immediately replaces it.
   - `Enter` / "Create" → validate + rename on disk via `set_display_name_async`.
   - `Esc` / "Cancel" → keep the default name. The item **remains created**.
6. On creation failure (step 3) → toast the OS error; no dialog is shown.

### Default-name scheme

New items use an "untitled" numbering scheme, distinct from the copy scheme used by
duplicate/paste (`name (copy)`, `name (copy 2)`):

```
untitled folder        (none existing)
untitled folder 2      (untitled folder exists)
untitled folder 3      (both exist)
```

For files with an extension the number is inserted before the extension
(`untitled file` has no extension, so this is mostly relevant if the base is later
parameterised). The helper splits stem/extension with the existing `split_name`.

## Permission awareness

- The **context-menu** "New" section is gated on the target directory's
  `access::can-write` capability. `selection_caps()` is extended to also report
  whether `focused_dir()` is writable, and `build_context_menu` omits the section
  when it is not.
- The **header button** is always enabled. If creation fails because the directory
  is read-only (or any other reason), the async callback reports the GIO error as a
  toast. This guarantees feedback even when the gate is bypassed (e.g. a directory
  that became read-only after the menu was built).

## Architecture / components

Each unit has one responsibility and a clear interface.

### `file_ops.rs`

- `untitled_name(base: &str, ext: &str, exists: impl Fn(&str) -> bool) -> String`
  — **pure**, unit-tested. Returns `"{base}{ext}"` if free, else `"{base} 2{ext}"`,
  `"{base} 3{ext}"`, … Mirrors the existing `dedupe_file_name` signature style so it
  composes with `unique_destination`-like callers.
- `create_folder(manager: Rc<ColumnManager>, parent: gtk::Window, dir: PathBuf)`
  — computes the name, calls `make_directory_async`, then on success invokes the
  naming dialog; on error sends a toast.
- `create_file(manager: Rc<ColumnManager>, parent: gtk::Window, dir: PathBuf)`
  — same, using `create_async`.

These follow the established `fn op(manager, [parent,] …)` shape (cf. `transfer`,
`delete`, `symlink`).

### `utils.rs`

- `Caps` / `selection_caps()` are unchanged in shape, but the menu builder needs the
  target directory's writability. Add a thin helper used by `build_context_menu`:
  query `focused_dir()`'s `access::can-write` (single `query_info` on a local dir,
  consistent with the menu builder already calling `selection_caps()` synchronously).
- `build_context_menu(shift)` prepends a "New" section (`New Folder`,
  `New Empty File`) when the target dir is writable.

### `main.rs`

- Register `new-folder` and `new-file` `SimpleAction`s using the `ACTIVE_MANAGER`
  pattern; set accels `app.new-folder` → `<Shift><Primary>n`, `app.new-file` →
  `<Primary>n`.
- Add the header-bar "New" `MenuButton` with a `gio::Menu` model of both actions.
- Generalise `show_rename_dialog` into
  `show_name_dialog(parent, manager, heading: &str, body: &str, confirm_label: &str, initial: &str, path)`,
  adding `entry.grab_focus()` + `entry.select_region(0, -1)` so the name is
  pre-selected. The confirm response keeps a stable id (`"confirm"`) with a
  caller-supplied label. `show_rename_dialog` becomes a thin caller
  ("Rename File" / "Enter a new name for '…'" / "Rename"); creation calls it with
  "New Folder"/"New File" headings and a "Create" label.

## Data flow

```
[accel | context menu | header button]
        │  activates app.new-folder / app.new-file
        ▼
ACTIVE_MANAGER.with(...) ──► dir = manager.focused_dir()
        ▼
file_ops::create_folder/create_file(manager, parent, dir)
        │  name = untitled_name(base, ext, |n| dir.join(n).exists())
        ▼
File::make_directory_async / create_async
        ├─ Err ──► manager.send_toast(error)
        └─ Ok  ──► monitored DirectoryList shows the item
                   show_name_dialog(parent, manager, heading, body, "Create", name, new_path)
                          ├─ "confirm"/Enter ──► validate_filename → set_display_name_async (toast on error)
                          └─ "cancel"/Esc    ──► keep default name
```

## Error handling

- Directory creation / file creation failure → toast the GIO error string; no dialog.
- Naming dialog reuses `validate_filename` (empty, `/`, `.`/`..`, >255 chars) and
  `set_display_name_async` (collisions, permission errors) — both already toast.
- No `unwrap()` on the UI thread; `focused_dir()` already returns a sensible default,
  and async callbacks handle `Result` explicitly.

## Testing

### Unit (headless)

- `untitled_name`:
  - no collision → returns base unchanged (`"untitled folder"`).
  - base exists → `"untitled folder 2"`.
  - base and " 2" exist → `"untitled folder 3"`.
  - with extension → number inserted before ext (`"report"`, `".txt"` →
    `"report 2.txt"` when `report.txt` exists).
- `validate_filename` is already covered.

### Manual (Verification Checklist)

- `cargo check` / `cargo clippy` pass with zero warnings.
- New Folder via `Ctrl/Cmd+Shift+N`, context menu, and header button — all create
  `untitled folder` and open the naming dialog with text selected.
- New Empty File via `Ctrl/Cmd+N`, context menu, header button — creates
  `untitled file`.
- Typing a name + Enter renames; Esc keeps the default; both leave a valid item.
- Creating twice yields `untitled folder` then `untitled folder 2`.
- In a read-only directory: the context-menu "New" section is hidden; the header
  button produces a permission-denied toast and creates nothing.
- Works in Miller, Icon, and List views, and with Show Hidden on/off.
- New item appears in the focused column without a manual refresh.

## Out of scope (YAGNI)

- Templates / "New Document from template".
- True in-list inline editing (chvarkov renames via a dialog; this feature matches
  that pattern).
- Creating files with a chosen extension or content.
- Drag-to-create or duplicating selections.
