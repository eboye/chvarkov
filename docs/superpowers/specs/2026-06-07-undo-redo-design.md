# SP3 — Undo / Redo — Design

**Date:** 2026-06-07
**Status:** Approved (pending implementation)
**Roadmap:** Third of the Nautilus-parity sub-projects (SP1 copy/move shipped as v0.5.0; SP2 conflict & naming shipped as v0.6.0).

## Goal

A Nautilus-style **undo / redo** manager. `Ctrl/Cmd+Z` reverses the last file operation, `Ctrl+Shift+Z` re-applies it, and successful operations show an **Undo** button in their result toast.

Covered operations: **Move, Rename, Trash, Copy, Duplicate, Create** (new folder / new empty file).

Explicitly *not* a single-level undo — a **bounded stack** (default cap 16) with **redo**.

## Current state (for reference)

- `src/file_ops.rs` holds every operation: `transfer` (Copy/Move), `show_rename_dialog`'s `set_display_name_async` (rename, in `main.rs`), `trash`, `delete`, `duplicate`, `create_folder`, `create_file`.
- Each op runs in `glib::spawn_future_local`, performs its I/O (often via `gio::spawn_blocking`), then calls `manager.send_toast(msg)` and refreshes.
- `ColumnManager` is reachable everywhere: it is passed as `Rc<ColumnManager>` to file-op functions, and the live instance is in the `ACTIVE_MANAGER` thread-local for GActions.
- `send_toast` (`main.rs:1707`) builds a plain `adw::Toast` with no button.
- Naming helpers `conflict_name`, `copy_recursive`, `unique_destination`, `validate_filename` already exist in `file_ops.rs`.
- There is **no undo of any kind** today.

## Components

### 1. Undo history + op model (`src/undo.rs`, new)

```rust
/// One reversible operation. Each variant stores enough to invert AND re-apply.
pub enum UndoOp {
    Move { pairs: Vec<(PathBuf, PathBuf)> },        // (from, to)
    Rename { from: PathBuf, to: PathBuf },
    Trash { originals: Vec<PathBuf> },              // absolute original paths
    Copy { sources: Vec<PathBuf>, created: Vec<PathBuf> },
    Duplicate { sources: Vec<PathBuf>, created: Vec<PathBuf> },
    Create { kind: CreateKind, path: PathBuf },     // Folder | File
}

pub enum CreateKind { Folder, File }
```

```rust
/// Two bounded stacks. Pure logic — no I/O, no GTK. Unit-tested.
pub struct UndoHistory {
    undo: Vec<UndoOp>,
    redo: Vec<UndoOp>,
    cap: usize,            // default 16
}

impl UndoHistory {
    pub fn new(cap: usize) -> Self;
    /// Record a freshly-performed op: push to `undo`, clear `redo`,
    /// drop the oldest `undo` entry if over `cap`.
    pub fn record(&mut self, op: UndoOp);
    /// Pop the most recent undoable op (caller executes the inverse,
    /// then calls `push_redo` on success).
    pub fn pop_undo(&mut self) -> Option<UndoOp>;
    pub fn pop_redo(&mut self) -> Option<UndoOp>;
    pub fn push_undo(&mut self, op: UndoOp);   // after a successful redo
    pub fn push_redo(&mut self, op: UndoOp);   // after a successful undo
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
}
```

A short human description per op (`UndoOp::describe()` → `"Move"`, `"Rename"`, …) feeds the toast text (`"Undone: Move"`).

### 2. Inverse / forward executors (`src/undo.rs`)

Two async free functions driven by the manager:

```rust
pub fn perform_undo(manager: Rc<ColumnManager>, op: UndoOp);  // execute the inverse
pub fn perform_redo(manager: Rc<ColumnManager>, op: UndoOp);  // re-apply forward
```

Each spawns `glib::spawn_future_local`, does I/O off the UI thread, tallies
per-item results, refreshes the manager, sends the result toast, and on success
hands the op back to the history (redo→ for undo, undo→ for redo) and updates the
undo/redo action enabled state.

**Inverse semantics (undo):**

| Op | Inverse |
|----|---------|
| `Move{pairs}` | move each `to` → `from` |
| `Rename{from,to}` | rename `to` → `from` |
| `Trash{originals}` | restore each original from trash (see §3) |
| `Copy{created}` / `Duplicate{created}` | **trash** each `created` path |
| `Create{path}` | **trash** `path` |

**Forward semantics (redo):**

| Op | Forward |
|----|---------|
| `Move{pairs}` | move each `from` → `to` |
| `Rename{from,to}` | rename `from` → `to` |
| `Trash{originals}` | trash each original again |
| `Copy`/`Duplicate{sources,created}` | re-copy each `source` → matching `created` via `copy_recursive` |
| `Create{kind,path}` | re-create empty folder / file at `path` |

**Move / restore primitive (shared):** a move-back uses rename when same device, else
copy+remove (reuse `copy_recursive` then remove source), matching the existing engine.
All recreate/copy/remove run in `gio::spawn_blocking`; trash uses `gio::File::trash_future`.

### 3. Trash restore (`src/undo.rs`, `#[cfg]`-split, best-effort)

`gio` trashing is one-way, so `Trash` records original absolute paths and restore
locates the trashed item and moves it back.

**Linux (freedesktop spec, parsed directly):**
- Home trash = `$XDG_DATA_HOME/Trash` (default `~/.local/share/Trash`), with
  `files/<name>` (the item) and `info/<name>.trashinfo` (INI: `Path=<url-encoded
  original>`, `DeletionDate=`).
- To restore original `P`: scan `info/*.trashinfo`, URL-decode each `Path`, select the
  entry whose decoded path == `P` with the **newest** `DeletionDate`; move
  `files/<name>` → `P`; delete the matching `.trashinfo`.
- A **pure** helper parses a `.trashinfo` body → `(decoded_path, deletion_date)` and a
  pure selector picks the newest match for `P`. Both unit-tested with fixture strings.
- Files trashed from another mount (`$mount/.Trash-$uid/`) are **out of scope**; a miss
  toasts "couldn't restore … from Trash".

**macOS (best-effort):**
- Items go to `~/.Trash/<basename>`. If `~/.Trash/<basename>` exists, move it back to the
  original path; otherwise toast "couldn't restore <name> from Trash". No guessing at
  Finder's collision-mangled names.

### 4. Recording hooks (`src/file_ops.rs`, `src/main.rs`)

Each op records **on success**, capturing the *actual* final paths so Keep-Both renames
and partial-success batches are recorded correctly:

- `transfer` (already loops per item computing `target`): collect successful
  `(src, target)` pairs; at the end record `Move{pairs}` or `Copy{sources,created}`
  per `TransferKind`. Record nothing if zero items succeeded.
- rename (`main.rs`, in the `set_display_name_async` success arm): record
  `Rename{ from: path, to: parent.join(new_name) }`.
- `trash`: collect successfully-trashed originals → `Trash{originals}`.
- `duplicate`: collect `(src, dst)` actually created → `Duplicate{sources,created}`.
- `create_folder` / `create_file`: on `Ok` → `Create{kind,path}`.
- `delete` (permanent) records **nothing** — irreversible.

Recording is `manager.undo_record(op)` (a thin `ColumnManager` method delegating to the
history). The success toast switches from `send_toast` to `send_toast_action(msg, "Undo",
on_undo)`.

### 5. `ColumnManager` integration (`src/main.rs`)

- New field: `undo_history: Rc<RefCell<UndoHistory>>` (constructed with cap 16).
- Methods: `undo_record(&self, op)`, `undo(&self)`, `redo(&self)` (the latter two clone
  `Rc<ColumnManager>` and call `perform_undo`/`perform_redo`), and
  `refresh_undo_actions(&self)` to toggle action `enabled`.
- New `send_toast_action(&self, message: &str, label: &str, action: impl Fn() + 'static)`:
  builds an `adw::Toast`, `set_button_label(label)`, `connect_button_clicked` → runs the
  closure. `send_toast` stays for plain messages.

### 6. Actions + accelerators (`src/main.rs setup_actions`)

- `app.undo` → `ACTIVE_MANAGER.with(|m| m.undo())`; accel `<Primary>z`.
- `app.redo` → `m.redo()`; accel `<Primary><Shift>z`.
- Both `SimpleAction`s start disabled; `refresh_undo_actions` sets `enabled` from
  `can_undo` / `can_redo` after every record/undo/redo.
- Pressing undo/redo with an empty stack is impossible via accel (disabled), but
  `undo()`/`redo()` still guard with a "Nothing to undo/redo" toast for the
  programmatic/toast path.

## Data flow

```
perform op → on success: collect actual paths → manager.undo_record(op)
           → send_toast_action("…", "Undo", || ACTIVE_MANAGER.undo())
           → refresh_undo_actions()

Ctrl/Cmd+Z (or toast Undo button): manager.undo()
  → history.pop_undo() → None ⇒ toast "Nothing to undo"
                       → Some(op) ⇒ perform_undo(op):
        execute inverse off UI thread, tally
        on success ⇒ history.push_redo(op); refresh view; refresh_undo_actions();
                      send_toast_action("Undone: <desc>", "Redo", || manager.redo())

Ctrl+Shift+Z: manager.redo() ⇒ pop_redo → perform_redo → push_undo → refresh
```

## Error handling

- **Inverse-target occupied** — never overwrite. Move-back / restore to a
  `conflict_name` `(2)` and toast that it was renamed.
- **Recorded path missing** (user already changed it) — skip, tally, toast a count.
- **Partial batch** — undo/redo what it can; toast `Undone N item(s), M failed`.
- **Destructive undo always to Trash** — undoing copy / duplicate / create trashes the
  created items, so an edited copy is still recoverable.
- All inverse I/O via `gio::spawn_blocking` / gio futures; no UI-thread blocking; no
  `unwrap()` on the UI thread.
- **Toast-button staleness (accepted, minor):** the Undo button acts on the top of the
  stack. Two undoable ops back-to-back ⇒ the older toast's button targets the newer op.
  Same single-toast model as Nautilus; the accelerator is the canonical path.

## Testing

### Unit (headless, pure logic — in `src/undo.rs`)
- `UndoHistory`: `record` pushes to undo and clears redo; bounded drop-oldest at `cap`;
  `pop_undo` + `push_redo` move an entry undo→redo; `pop_redo` + `push_undo` move it back;
  empty `pop_undo`/`pop_redo` return `None`; `can_undo`/`can_redo` track contents.
- Inverse-target computation: occupied target → `conflict_name` `(2)`; free target →
  original path.
- Linux `.trashinfo` parsing: URL-decoded `Path`; newest-`DeletionDate` selection among
  duplicate originals; tolerance of a malformed file (skipped, not panicking).

### Manual (Verification Checklist)
- `cargo check` / `cargo clippy --all-targets` zero warnings; `cargo test` passes.
- Move a file, `Ctrl+Z` → returns to origin; `Ctrl+Shift+Z` → moves again.
- Rename, undo → old name; redo → new name.
- Trash a file (Linux), undo → restored to original dir; redo → back in trash.
- Copy / Duplicate, undo → copies go to Trash (recoverable), originals untouched;
  redo → re-created.
- New Folder / New File, undo → trashed; redo → re-created empty.
- Undo into an occupied original path → restored as `name (2)` with a toast (no overwrite).
- Undo button appears on the success toast and works; "Nothing to undo" when empty.
- Keyboard-first: `Ctrl/Cmd+Z` and `Ctrl+Shift+Z` work with focus on the file list.
- "Show Hidden Files" on/off unaffected; selecting a file still drives the Preview pane.

## Out of scope (v1 / deferred)
- Cross-mount trash dirs (`$mount/.Trash-$uid`) on Linux; macOS collision-mangled-name
  restore.
- Persisting undo history across sessions.
- Undoing permanent delete (irreversible) or compress / symlink.
- A menu/headerbar Undo entry (accel + toast button only; action `enabled` state is
  already wired so a menu item is a trivial later add).
