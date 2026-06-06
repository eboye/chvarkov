# Design: Implement placeholder file-manager actions

Date: 2026-06-06
Status: Approved

## Summary

The context menu and keyboard shortcuts in chvarkov expose 13 file-manager
actions that are currently stubs (they only `println!`). This work implements
them for real, plus four correctness fixes surfaced during a code review. All
operations act on the **full multi-selection** of the focused view, use the
**system clipboard** for interop with Finder / GNOME Files, and **prompt on
name collisions**.

## Goals

- Implement every stubbed action: cut, copy, paste, move-to, copy-to,
  create-link, compress, email, open-terminal, copy-path, copy-uri, copy-name,
  sharing-options.
- Operations act on all selected items in the focused view, not just one.
- Cut/Copy/Paste interoperate with the system file manager via the clipboard.
- Paste / Move / Copy-to prompt on name collisions (Replace / Skip / Keep both,
  with "apply to all" for batches).
- Fold in the reviewed correctness fixes (see "Bundled bug fixes").

## Non-goals

- A background job queue / progress-bar UI for very large transfers. Operations
  report start/finish via toasts; a full progress UI is a later enhancement.
- Undo/redo of file operations.
- Network/remote locations beyond what `gio::File` already supports.

## Key decisions

| Decision | Choice |
| --- | --- |
| Clipboard | System-clipboard interop (file URIs + GNOME cut/copy convention; best-effort on macOS) |
| Operation scope | All selected items in the focused view |
| Name collisions | Prompt: Replace / Skip / Keep both (Rename), with "apply to all" |
| Email / Share | Native, best-effort: macOS Share sheet; Linux `xdg-email --attach`, hide Share on Linux |
| Compress format | `.zip` (cross-platform), via the `zip` crate on a worker thread |
| Open in Terminal | macOS `open -a Terminal <dir>`; Linux `$TERMINAL` then a known list |
| Create Link | Symlink `"<name> link"` in the same folder (numbered on collision) |
| Permissions | Actions the user lacks rights for are **hidden** from the context menu and their shortcuts no-op, driven by GIO `access::*` attributes |

## Architecture

Dedicated modules with thin action wrappers, consistent with the project's
"separate modules / centralize reusable logic in `utils.rs`" mandate.

### New module: `src/file_ops.rs`
The async operation engine. Public functions for: `copy`, `move_` (move/rename
across dirs), `trash`, `delete`, `symlink`, `compress`. Each takes the list of
source paths and (where relevant) a destination directory plus an
`Rc<ColumnManager>` handle for toasts and refresh.

- Runs on the GLib main loop using gio async file APIs, iterating the selection.
- Conflict resolution: before writing each item, check for an existing target.
  On collision show an `adw::AlertDialog` with responses Replace / Skip /
  Keep both, and a "Apply to all" check button that short-circuits subsequent
  prompts in the same batch. "Keep both" derives a non-colliding name
  (`"<stem> (copy)<.ext>"`, then `"<stem> (copy N)<.ext>"`).
- Reporting: success/failure summarized via the existing `ToastOverlay`
  (`ColumnManager::send_toast`). Errors never panic.
- Refresh: after a batch completes, refresh affected columns. Reuse the existing
  rebuild path (`app.activate()` / column re-add) rather than inventing a new
  refresh mechanism.
- Compress runs the zip creation on a worker thread (`gio::spawn_blocking` or a
  std thread + channel) and reports completion on the main loop.

### New module: `src/clipboard.rs`
System-clipboard interop plus authoritative internal state.

- Internal state: `Rc<RefCell<Option<ClipboardOp>>>` holding the operation mode
  (`Cut` | `Copy`) and the list of source paths, owned by `ColumnManager`.
- Copy/Cut: set the internal state AND publish to the GDK clipboard:
  `text/uri-list` (newline-joined `file://` URIs) and
  `x-special/gnome-copied-files` (`"cut\n<uri>\n..."` / `"copy\n<uri>\n..."`)
  so GNOME Files interoperates. macOS publishes file URIs best-effort.
- Paste: prefer the internal state if present; otherwise read the clipboard
  (`x-special/gnome-copied-files`, falling back to `text/uri-list`). Resolve to
  the destination = the directory of the currently focused column / view. Cut
  clears the internal state after a successful move.

### Helper in `src/utils.rs`: `collect_selection`
`collect_selection(view) -> Vec<SelectionInfo>` gathers every selected item from
the focused view (Miller `ListView`, Icon `GridView`, List `ColumnView`),
reading each item's real path from the `standard::file` attribute. This both
enables multi-item operations and fixes the nested List-view path bug (items in
expanded subfolders currently resolve to `root/leaf`).

`SelectionInfo` (currently in `main.rs`) is reused; `current_selection` remains
the *primary* item for the preview pane.

### Action wrappers in `src/main.rs`
Each stubbed `SimpleAction` closure becomes a thin wrapper: resolve the active
`ColumnManager`, gather the selection via `collect_selection`, obtain any
destination, and call into `file_ops` / `clipboard`. No operation logic lives in
the closures.

## Action specifications

- **Copy / Cut** — set clipboard (internal + system) from the selection. Cut is
  visually identical to copy until paste.
- **Paste** — destination is the focused directory. Copy or move each clipboard
  source into it via the engine (with conflict prompts). Cut clears the
  clipboard on success.
- **Move to… / Copy to…** — `gtk::FileDialog::select_folder` picks a destination,
  then the engine moves/copies the selection there.
- **Create Link** — create a symlink per selected item in its own directory named
  `"<name> link"`, numbered on collision. `std::os::unix::fs::symlink`.
- **Copy Path / Copy URI / Copy Name** — write newline-joined absolute paths /
  `file://` URIs / file names to the clipboard as text.
- **Open in Terminal** — open a terminal at the selection's directory (parent dir
  if a file is selected). macOS `open -a Terminal <dir>`; Linux `$TERMINAL` then
  gnome-terminal/konsole/kitty/alacritty/foot/xterm.
- **Compress** — create a `.zip` in the selection's directory, named after the
  single item (`<name>.zip`) or `Archive.zip` for multiples, conflict-safe.
- **Email** — attach the selected files: macOS via the Share sheet (Mail target)
  / Linux `xdg-email --attach`.
- **Sharing Options** — macOS native Share sheet (NSSharingServicePicker bridge);
  hidden/disabled on Linux.

## Permission awareness

Actions are gated by the user's actual filesystem rights so that, e.g., Delete
does not appear for a file the user cannot delete.

- **Source of truth:** GIO `access::*` boolean attributes, requested in every
  `DirectoryList` query: `access::can-read`, `access::can-write`,
  `access::can-execute`, `access::can-delete`, `access::can-trash`,
  `access::can-rename`. GIO computes these accounting for parent-directory
  permissions and ownership, cross-platform. If an attribute is absent it is
  treated as permitted (true), to avoid hiding actions spuriously.
- **A `Caps` struct** (`utils.rs`) reads these from a `FileInfo`. For a
  multi-selection, capabilities are AND-combined: an action is offered only if
  **every** selected item supports it (so a batch never partially fails on the
  unsupported members). An empty selection yields no capabilities.
- **Two surfaces are gated:**
  1. The context menu is rebuilt per popup, omitting items the current selection
     can't perform (hidden, not greyed — matches the requirement).
  2. Each action handler re-checks the relevant capability, so the keyboard
     shortcut also no-ops when not permitted (no toast spam).
- **Per-action capability gate:**
  | Action | Shown / allowed when |
  | --- | --- |
  | Open | ≥1 selected |
  | Cut, Move to… | all selected `can-read` and `can-delete` |
  | Copy, Copy to…, Compress, Email, Sharing | all selected `can-read` |
  | Rename | exactly 1 selected and `can-rename` |
  | Create Link | all selected `can-read` (parent-dir write enforced by the engine; toast on failure) |
  | Move to Trash | all selected `can-trash` |
  | Delete Permanently | all selected `can-delete` |
  | Copy Path / URI / Name, Open in Terminal, Properties | ≥1 selected (read-only metadata) |
  | Paste | keyboard-only; engine reports destination-permission errors via toast |
- Operations that depend on parent-directory writability but have no direct file
  attribute (Create Link, Compress output, Paste destination) rely on the engine
  surfacing a permission error as a toast, rather than predictive hiding.

## Bundled bug fixes (from the review)

- **#3** Sidebar "Trash" path: use `~/.Trash` on macOS, `~/.local/share/Trash/files`
  on Linux (`#[cfg(target_os = ...)]`).
- **#4** Preview Down-arrow: bound-check `current + 1 < n_items()` before
  `select_item`.
- **#5** Harden `show_preferences_window`'s `active_window().unwrap()` (and
  similar UI-thread unwraps touched while wiring actions).
- Remove the debug `println!`s in the action closures and
  `handle_selection_change_multi`.

## Error handling

- All gio operations are async and their `Result` is matched; failures produce a
  toast, never a panic.
- The conflict dialog is the only modal interruption; "apply to all" prevents
  prompt storms on large batches.
- Platform integrations (terminal, email, share) that can't find a backend
  report a clear toast instead of failing silently.

## Testing / verification

**Unit tests cover all logic that can run headless** (no display / GTK main loop):
file-name dedup, `split_name`, recursive copy (temp-dir), `unique_destination`,
cross-device detection, archive naming, `caps_from_info` / `combine_caps`,
`format_permissions`, `format_size`, the zoom→size functions, and
`file_info_path`. These live in `#[cfg(test)] mod tests` within each module and
run via `cargo test`. `caps_from_info` and `file_info_path` build `gio::FileInfo`
objects directly (GObject works without a display).

**Not unit-tested** (require a display, GTK main loop, or external programs):
GUI widget construction, the async transfer/conflict dialog flow, the GDK
clipboard, file-chooser dialogs, terminal/email/share spawning. These remain
manually verified per phase:
- `cargo build` is warning-free after each phase.
- Each action verified on the primary platform (macOS) with single and
  multi-selection, including a name-collision case for paste/move/copy.
- Trash/Delete still collapse child columns correctly.
- Clipboard interop spot-checked against Finder (copy in chvarkov → paste in
  Finder, and vice-versa).

## Phased build order

1. `collect_selection` helper + nested-path fix.
1b. Permission model (`Caps` + `access::*` attributes) + permission-aware menus.
2. `file_ops` engine + conflict dialog.
3. Switch Trash/Delete onto the engine (multi-item) + the four bundled bug fixes.
4. `clipboard` module → Copy / Cut / Paste.
5. Move to… / Copy to…
6. Copy Path / Copy URI / Copy Name.
7. Create Link.
8. Open in Terminal.
9. Compress.
10. Email + Sharing (native, best-effort) — highest risk, done last.

Each phase ends warning-free and is independently testable.
