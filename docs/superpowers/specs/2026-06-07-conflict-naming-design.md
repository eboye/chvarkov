# SP2 — Conflict & Naming Polish — Design

**Date:** 2026-06-07
**Status:** Approved (pending implementation)
**Roadmap:** Second of the Nautilus-parity sub-projects (SP1 copy/move shipped as v0.5.0; SP3 undo is next).

## Goal

Bring chvarkov's file naming and conflict handling in line with GNOME Files:
- A new **Duplicate** command that names copies `name (Copy).ext`, `name (Copy 2).ext`, …
- **Numeric** conflict naming (`name (2).ext`, `(3)`, …) for Keep-Both and the other
  "make a non-colliding name" paths (currently a lowercase `(copy)` scheme).
- **Compound `.tar.*` extensions** preserved when inserting a counter.
- An **editable suggested name** in the conflict dialog's Keep-Both flow (the
  "Apply to all" checkbox already exists).

## Current state (for reference)

- `split_name` splits at the last `.`, so `archive.tar.gz` → (`archive.tar`, `.gz`).
- `dedupe_file_name` → `name (copy)` / `name (copy 2)`. Used by `unique_destination`
  (called for conflict **Keep-Both** and same-path **Copy**) and directly by **compress**
  (`archive_name` dedup) and **symlink** (`"X link"` dedup).
- `untitled_name` → numeric `base`, `base 2`, … (new-folder/file). **Unchanged by SP2.**
- `ask_conflict` already shows an **"Apply to all remaining"** checkbox and returns
  `(choice, apply_all)`. No editable name field.
- There is **no Duplicate command**.

## Components

### 1. Naming helpers (`src/file_ops.rs`)

**Compound-extension split.** Replace `split_name` so a `.tar` immediately before the
final extension is folded into the extension:
- `archive.tar.gz` → (`archive`, `.tar.gz`)
- `archive.tar` → (`archive`, `.tar`)
- `report.txt` → (`report`, `.txt`)
- `.bashrc` → (`.bashrc`, ``)  (leading-dot, no extension — unchanged)
- `noext` → (`noext`, ``)

Implementation: find the last dot (with `idx > 0` as today) → (stem, ext). Then if
`stem` ends with `.tar` (and the `.tar`'s dot is not the leading char), move `.tar` from
the stem into the front of `ext`. Keep it to the single, well-known `.tar` compound
(matches Nautilus's special-case; we don't try to model every multi-dot name).

**Two numbered-name functions**, each inserting the counter **before** the (compound)
extension, parameterised by an `exists` predicate (pure/testable):

```rust
/// `name (Copy).ext`, then `name (Copy 2).ext`, `(Copy 3)`, … (the Duplicate scheme).
pub fn copy_dup_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String;

/// `name (2).ext`, `(3)`, … (the conflict / keep-both / dedup scheme).
pub fn conflict_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String;
```

- `copy_dup_name`: if `file_name` free, returns it unchanged? No — Duplicate always makes
  a *copy*, so the first result is always `stem (Copy)ext` (even if free); then `(Copy 2)`,
  `(Copy 3)`, … until free.
- `conflict_name`: the input always already exists (it's a collision), so the first result
  is `stem (2)ext`, then `(3)`, … until free. (If somehow free, returns `stem (2)ext` —
  it is only ever called to make an alternative to an existing name.)

**Rewire callers:**
- `unique_destination(dir, name)` → use `conflict_name` (numeric). This serves conflict
  **Keep-Both** (transfer line ~254) and same-path **Copy** (line ~240).
- **compress** (`file_ops.rs` ~603) and **symlink** (~781) dedups → `conflict_name`.
- **Duplicate** (new) → `copy_dup_name`.
- Remove the now-unused `dedupe_file_name` (replaced by the two new functions), or keep
  only if still referenced — grep and delete if dead.

### 2. Duplicate command (`src/main.rs` + `src/file_ops.rs`)

- Register a `duplicate` `SimpleAction` (via the `ACTIVE_MANAGER` pattern) with accel
  `<Primary>d`; add a **"Duplicate"** entry to the context menu (in `utils.rs`
  `build_context_menu`), shown when `count >= 1 && caps.read` and the target dir is
  writable (reuse `crate::target_dir_writable()` like the New section).
- `pub fn duplicate(manager: Rc<ColumnManager>, paths: Vec<PathBuf>)`: for each path,
  resolve its parent dir, compute `copy_dup_name(original_name, |n| parent.join(n).exists())`,
  and copy via `gio::spawn_blocking(move || copy_recursive(&src, &dst))` (symlink-preserving).
  Tally done/failed; one summary toast; `manager.refresh()`. Duplicate never prompts.
- The action handler collects the selection (`collect_selection` → paths), like the other
  file-op actions, and calls `file_ops::duplicate`.

### 3. Conflict dialog: editable suggested name (`ask_conflict`)

Change signature to `async fn ask_conflict(parent, name, merge, suggested: &str) -> (choice, apply_all, keep_both_name: String)`.

- The dialog's `extra_child` becomes a vertical `gtk::Box` holding:
  - a `gtk::Entry` pre-filled with `suggested` (the `conflict_name` for this collision),
  - the existing **"Apply to all remaining"** `gtk::CheckButton`.
- Pre-select the counter portion of the entry text so typing tweaks the number
  (fallback: select-all). Use `entry.grab_focus()` + `select_region` after `present`,
  via `glib::idle_add_local_once` (same pattern as `show_name_dialog`).
- The checkbox `connect_toggled` disables the entry when checked
  (`entry.set_sensitive(false)`), since a hand-typed name can't apply to a whole batch.
- Return the entry's text as `keep_both_name`.

**Caller (`transfer`, the conflict block ~line 245):**
- Compute `suggested = conflict_name(&name, |n| dest_dir.join(n).exists())` before showing
  the dialog (only when prompting; under apply-to-all reuse the stored choice and auto-name).
- On `choice == "keep"`:
  - If apply-to-all is active (so no per-item name), or the returned name is blank /
    fails `validate_filename` / still collides → `target = unique_destination(&dest_dir, &name)`
    (auto numeric).
  - Else `target = dest_dir.join(keep_both_name)`.
- `apply_to_all` continues to persist the *choice* for the rest of the batch; the edited
  name only ever applies to the single prompted item.

## Data flow (conflict Keep-Both)

```
collision on `name`:
  suggested = conflict_name(name, exists-in-dest)
  if apply_to_all is set: choice = stored; keep_both_name = "" (auto)
  else: (choice, all, keep_both_name) = ask_conflict(parent, name, merge, &suggested)
        if all { store choice }
  on "keep":
     if all OR keep_both_name invalid/blank/collides -> target = unique_destination(dir, name)
     else -> target = dir.join(keep_both_name)
```

## Error handling
- An edited Keep-Both name that is invalid, empty, or still colliding silently falls back
  to the auto-generated unique numeric name — never an overwrite.
- Duplicate copy failure → per-item count + summary toast; partial multi-select duplicates
  report how many succeeded.
- No `unwrap()` on the UI thread; copies run in `spawn_blocking`.

## Testing

### Unit (headless)
- `split_name`: `archive.tar.gz` → (`archive`, `.tar.gz`); `archive.tar` → (`archive`, `.tar`);
  `report.txt` → (`report`, `.txt`); `.bashrc` → (`.bashrc`, ``); `noext` → (`noext`, ``);
  `a.tar.bz2` → (`a`, `.tar.bz2`).
- `copy_dup_name`: free name → `x (Copy).txt`; with `x (Copy).txt` taken → `x (Copy 2).txt`;
  compound: `a.tar.gz` → `a (Copy).tar.gz`.
- `conflict_name`: `x.txt` taken → `x (2).txt`; `x.txt`+`x (2).txt` taken → `x (3).txt`;
  compound: `a.tar.gz` → `a (2).tar.gz`.
- Update the existing `dedupe_*` naming tests to the new schemes (or replace them with the
  above). `untitled_name` tests unchanged.

### Manual (Verification Checklist)
- `cargo check` / `cargo clippy --all-targets` zero warnings; `cargo test` passes.
- Right-click → **Duplicate** (and `Cmd/Ctrl+D`): `report.txt` → `report (Copy).txt`; again →
  `report (Copy 2).txt`; a folder duplicates recursively; `data.tar.gz` → `data (Copy).tar.gz`.
- Duplicate is hidden when the directory isn't writable.
- Copy a colliding file, choose **Keep Both** → result is `name (2).ext`; the dialog's name
  field is pre-filled and editable; an edited valid name is used; a bad/edited-to-collide name
  falls back to auto `(2)`.
- Tick **Apply to all** on a conflict → the name field disables and remaining conflicts get
  auto numeric names.
- Compress when `Archive.zip` exists → `Archive (2).zip`; Create Link dedup uses `(2)`.

## Out of scope (later / deferred)
- SP3: undo (separate spec).
- Modeling arbitrary multi-dot "extensions" beyond the `.tar.*` special case.
- A separate "Rename on conflict" button distinct from Keep-Both (the editable field on
  Keep-Both covers the rename-to-resolve use case).
