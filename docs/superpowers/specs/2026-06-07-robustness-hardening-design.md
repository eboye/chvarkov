# Robustness Hardening Pass — Design

**Date:** 2026-06-07
**Status:** Approved (pending implementation)

## Goal

Fix a curated set of bugs, edge cases, and pitfalls found in a full read of `src/`
(plus the user-reported "CSS class used as logic" delete-bug class). Scope: all High
and Medium severity items, plus the cheap/safe Low-severity correctness items. No new
features; this is correctness/robustness only.

## Grouped fixes

### Group A — Focus / state source-of-truth (the data-loss-adjacent class)

**A1. Remove the stale CSS-class fallback in `get_focused_list_view`** (`main.rs`).
The function already uses real keyboard focus as primary, but falls back to reading
`has_css_class("focused-column"/"focused-grid"/"focused-list")`. That CSS class is only
maintained by arrow-key navigation, so the fallback can mis-target destructive actions
(the same class of bug as the original delete-the-parent-folder data loss). Fix: delete
the CSS-class fallback. Rebuilds already call `grab_focus`, so real focus is live in
normal use; if focus is genuinely unknown, return `None` so destructive actions **no-op
rather than act on the wrong view**. Keep the `focused-*` CSS classes for styling only
(they are still added/removed for appearance; nothing reads them for logic).

**A2. `focused_dir` stale fallback** (`main.rs`). Today, when the focus/entry lookup
misses, it falls back to the `current-path` GSetting, so New Folder / Paste can target
the wrong directory after a delete/rebuild. Fix: derive the directory from the
authoritative focused view via `get_focused_list_view` (match it to its `entries` row,
or the selection's parent), and only fall back to `current-path` when no focused view
exists at all.

**A3. Multi-file delete leaves stale selection / racy column reconciliation**
(`file_ops.rs` `trash`/`delete`, `main.rs` `on_file_deleted`). A batch delete calls
`on_file_deleted` per path, each recomputing `keep_count` against an already-mutated
`entries`, and the view's `MultiSelection` still shows the deleted rows highlighted.
Fix: after a batch completes, clear the focused view's `MultiSelection` once and let the
monitored `DirectoryList` refresh; reconcile columns a single time rather than per-path.

### Group B — Preview safety (UI-thread blocking)

**B1. Gate preview rendering on regular files** (`preview.rs`). `create_preview_layout`
points `Picture::for_filename` / `gtk::Video` / a sync `std::fs` read at the selected
path with no file-type check. A FIFO/socket/device (or a path that became one) can block
the UI thread indefinitely. Fix: only attempt image/video/text rendering when
`file_info.file_type() == gio::FileType::Regular`; otherwise show the info layout
(icon + metadata). Symlinks resolve to their target type via the existing FileInfo.

**B2. Read text content off the UI thread** (`preview.rs`). The text branch does a
synchronous `std::fs::File::open` + `read` (first 10 KB) on the main thread. Fix: load
asynchronously — read the first 10 KB via `gio::File::load_contents_async` (or
`gio::spawn_blocking`) and set the buffer text in the async callback, so a slow/stale
mount never blocks the UI. The view is created immediately; text fills in when ready.

**B3. Docked video: no autoplay, explicit stop** (`preview.rs` + `main.rs`
`update_dock`). The docked pane builds a `GtkVideo` with `autoplay(true).loop_(true)`,
so arrowing through files spins up an autoplaying+looping pipeline per selection, and
hiding the dock relies on widget finalization to stop playback. Fix: in the **docked**
(non-`large`) layout, do not autoplay (show the video widget paused / first frame); and
in `update_dock`, before swapping/clearing the child, explicitly stop the media stream
(e.g. `video.set_media_stream(None)` or pause) so playback ends deterministically. The
floating `Space`/large preview keeps autoplay.

### Group C — Lifecycle / leaks

**C1. `build_ui` timer/handler accumulation** (`main.rs`). Settings toggles call
`app.activate()` → `build_ui`, which re-creates the perpetual 500 ms responsive-label
timer and re-adds `hadjustment` `connect_upper_notify`/`connect_page_size_notify`
handlers every time, accumulating over a session. Fix (targeted): store the 500 ms
timer's `glib::SourceId` (e.g. in a `thread_local!` or on `ColumnManager`) and remove
the previous one before adding a new one; guard the one-time scroll-snap handler so it
is connected once. No change to the rebuild model.

**C2. Reap spawned child processes** (`quicklook.rs`, `file_ops.rs`). `qlmanage`,
`sushi`, the terminal launcher, `xdg-email`/`osascript` are spawned fire-and-forget and
never waited on, leaving zombies over a long session (Linux). Fix: after spawning, hand
the `Child` to `glib::child_watch_add` (or equivalent) so it is reaped on exit. Keep the
fire-and-forget UX (we still don't block).

### Group D — Clipboard / file-op correctness (cheap Low items)

**D1. Honor external cut vs copy on paste** (`clipboard.rs`). `read_external` always
treats a system paste as Copy, ignoring the `cut`/`copy` verb in
`x-special/gnome-copied-files`. Fix: parse the verb line and return the correct `Mode`
so cutting in GNOME Files + pasting in chvarkov moves. Unit-test the parser.

**D2. Non-UTF-8 path guards** (`file_ops.rs` email/zip, `main.rs` copy-path/uri/name).
Several paths use `to_string_lossy()`, silently corrupting non-UTF-8 names. Fix: for
email and zip-entry names, skip or `to_str()`-guard non-UTF-8 paths (matching the share
sheet which already filters); document copy-path as best-effort. No panics.

**D3. Symlink-aware replace in `transfer`** (`file_ops.rs`). The conflict "replace"
branch uses `target.is_dir()` (follows symlinks) to choose `remove_dir_all` vs
`remove_file`, which is wrong for a symlink-to-dir. Fix: decide via `symlink_metadata`
(as the `both_dirs` check already does) so a symlinked target is removed as a link.

**D4. Don't skip the into-itself guard when `canonicalize` fails** (`file_ops.rs`
`transfer`). The directory-into-its-own-descendant guard is inside
`if let (Some(sc), Some(dc)) = (src_canon, dest_canon)`, so a `canonicalize` failure
(permission denied, vanished) silently skips the guard. Fix: treat a canonicalize
failure on a directory source as "cannot verify → refuse the operation" for that item
(report via toast) rather than proceeding unguarded.

### Group E — Defensive unwraps

**E1. Replace UI-thread `unwrap()`s with graceful guards** in the reachable hot paths:
the arrow-key handlers in `add_column` (`main.rs` — `model().unwrap()`,
`downcast().unwrap()`), the idle selection closure, and the list factories
(`column.rs`, `icon_view.rs`, `list_view.rs` — `downcast_ref().unwrap()`,
`first_child().unwrap()`, `next_sibling().unwrap()`). Convert to
`let-else { return }` / `and_downcast` so an unexpected widget-tree/model shape degrades
gracefully instead of aborting the process (per the project's no-unwrap-on-UI-thread
mandate). Out of scope: construction-time `unwrap`s that only run once at startup are
left unless trivially convertible.

## Out of scope (deferred)

- Nautilus-style enhancements (cancellation/progress, pre-flight free-space scan,
  per-file skip/retry, undo, naming parity, conflict "apply to all") — these are
  features for a later roadmap, not this correctness pass.
- The `build_ui` "reconfigure in place instead of full rebuild" refactor (a larger,
  riskier change; the targeted C1 fix removes the leak without it).
- Sidebar `widget_name`-as-path-storage (functional, set-once/read-back; not stale).
- Overstated audit items with negligible real risk: the epoch-`0`
  `DateTime::from_unix_local(0).unwrap()` (always valid for 0) and the action-`state()`
  startup unwraps — left as-is.

## Testing

### Unit (headless)
- `clipboard`: verb parser (D1) — `cut` line → `Mode::Cut`, `copy`/absent → `Mode::Copy`.
- Any pure helper extracted during D2/D3 (e.g. a "should this name be skipped"/path
  guard) gets a focused test.
- Existing tests must keep passing.

### Manual (Verification Checklist)
- `cargo check` / `cargo clippy --all-targets` zero warnings; `cargo test` passes.
- Delete with the mouse-focused column targets that column; with no clear focus it
  no-ops (never the wrong column). New Folder / Paste land in the focused directory.
- Selecting a FIFO/socket/device (e.g. `/dev/null`, a named pipe) shows info, does not
  hang the UI. Selecting a large text file on a slow path doesn't freeze.
- Arrowing through videos doesn't leave audio/playback running; hiding the dock stops it.
- Toggling view/sort/zoom/hidden repeatedly doesn't accumulate timers (verify the stored
  SourceId is replaced; no growth in a long session).
- Cut in GNOME Files → paste in chvarkov moves (Linux).
- Replacing a symlinked target removes the link, not its target tree.
