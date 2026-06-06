# SP1 — Robust Copy/Move Engine — Design

**Date:** 2026-06-07
**Status:** Approved (pending implementation)
**Roadmap:** First of three Nautilus-parity sub-projects (SP1 copy/move; SP2 conflict & naming; SP3 undo).

## Goal

Replace the current `transfer` engine (which copies each top-level item via a single
`copy_recursive` inside one `gio::spawn_blocking`, with no progress, cancellation, free-space
check, or per-file error recovery) with a scanned, cancellable, progress-reporting engine that
recovers gracefully from per-file errors. This kills the "frozen UI" perception on large copies,
prevents half-copies / out-of-space failures, and lets the user keep browsing while a copy runs.

## Behavior & UX

- A **non-modal progress bar** appears above the breadcrumb during a copy/move: file count +
  byte percentage, throttled to ~100 ms, with a **Cancel** button. It auto-hides when the
  operation finishes or is cancelled. The window stays interactive (browse while copying).
- The bar is shown **only when the pre-flight scan total exceeds a threshold** (≈16 MiB **or**
  >100 files); smaller operations run silently/instantly as today (no bar flash).
- **Cancel**: already-completed files are kept; the file being written is deleted (no partial
  left); for a cross-device move the current item's **source is left intact**; remaining items
  are untouched. Same-filesystem moves are instant renames and are effectively non-cancellable.
- **Out of space**: if the scanned total exceeds the destination's free space (for copies and
  cross-device moves), the operation is **refused up front** with a dialog; nothing is written.
- **Per-file error** (e.g. permission denied, vanished mid-copy): a **Skip / Skip All / Retry /
  Cancel** dialog. "Skip All" persists for the rest of the batch; "Retry" re-attempts that one
  file; "Cancel" stops (cleaning the partial). Skipped/failed files are tallied in the final toast.
- The existing **conflict resolution** (Replace / Skip / Keep Both / directory **Merge**, with
  "apply to all") is unchanged and still runs per top-level item.

## Architecture / components

All engine logic stays in `src/file_ops.rs`; a small progress widget is added in `src/main.rs`.

### 1. Pre-flight scan → plan (`file_ops.rs`)

```rust
/// One resolved top-level item (a file or a whole directory tree), grouped so a MOVE
/// can decide per-item whether its source may be deleted (only if every file copied).
pub struct ItemPlan {
    pub src_root: PathBuf,             // the original top-level source (for move-delete)
    pub dirs: Vec<PathBuf>,            // destination dirs to create (in order)
    pub files: Vec<(PathBuf, PathBuf)>, // (src_file, dst_file) regular files
    pub symlinks: Vec<(PathBuf, PathBuf)>, // (src_link, dst_link) to recreate
}

pub struct CopyPlan {
    pub items: Vec<ItemPlan>,
    pub total_bytes: u64,              // sum of all regular-file sizes across items
    pub total_files: usize,
}

/// Build one `ItemPlan` for a resolved top-level item rooted at `dst`, accumulating bytes.
/// Symlink-aware: a symlink is recorded for recreation and never descended (no loops).
fn plan_item(src: &Path, dst: &Path, total_bytes: &mut u64) -> std::io::Result<ItemPlan>;
```

The scan runs in `gio::spawn_blocking` (it can stat many files) and produces a per-item plan plus
global `total_bytes`/`total_files` (so progress is a single global percentage while move-deletes
stay per-item). The scan happens **after** conflict resolution decides each top-level item's
final `dst`. Per-file error/progress is exact because each item's `files` list is explicit.

### 2. Free-space check

`fn free_space(dir: &Path) -> Option<u64>` via `gio::File::for_path(dir).query_filesystem_info("filesystem::free", …)`
returning `filesystem::free`. If `plan.total_bytes > free`, show an `adw::AlertDialog`
("Not enough space on the destination — N needed, M free") and abort. Skipped for
same-filesystem moves (no extra space used).

### 3. Chunked, cancellable copy with progress

```rust
/// Copy one regular file in chunks, checking `cancel` between chunks and reporting
/// bytes written via `progress`. Returns Err on I/O failure; leaves a partial dst that
/// the caller deletes on error/cancel.
fn copy_file_chunked(
    src: &Path, dst: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &dyn Fn(u64),     // called with bytes-just-written
) -> std::io::Result<()>;
```

- Reads/writes in a fixed buffer (e.g. 256 KiB). Between chunks: if `cancel` is set, stop and
  return a sentinel `Cancelled` error; the caller removes the partial `dst`.
- `progress(n)` increments the shared byte counter; the counter is published to the UI through a
  channel (below), not on every chunk synchronously.

### 4. Cancellation + progress plumbing

- **Cancel flag:** `Arc<AtomicBool>`, shared with the progress bar's Cancel button.
- **Progress channel:** `async_channel::unbounded::<Progress>()` where
  `Progress { bytes_done: u64, files_done: usize, total_bytes: u64, total_files: usize }` (or a
  lighter "delta" message). The execute loop runs file copies in `spawn_blocking`, sending
  progress; a `glib::spawn_future_local` receiver updates the bar **throttled to ~100 ms**
  (coalesce: only redraw if ≥100 ms since last redraw). When execution ends, the sender is
  dropped and the receiver loop exits and hides the bar.
  - If `async_channel` is not already a dependency, add it (it integrates with the glib main
    loop); otherwise use a `glib`-native channel equivalent. Pick whichever is cleanest to
    compile in this workspace.

### 5. Execute loop (`transfer` rewrite)

Per top-level source (after the existing conflict resolution picks `dst`):
- **Same-filesystem move** → `std::fs::rename(src, dst)` (instant; counts toward "done", no
  byte progress needed).
- Otherwise build the item's plan (or scan all items up front — see Data flow), then:
  create dirs, copy each file with `copy_file_chunked` (per-file error → Skip/Skip All/Retry/
  Cancel; cancel → delete partial + stop), recreate symlinks, and for a **move** delete the
  source item after all its files copied successfully.

The whole operation runs inside one `glib::spawn_future_local` so the per-file error dialogs
(`AlertDialog::choose_future`) can be awaited on the main task between `spawn_blocking` file copies.

### 6. Progress bar widget (`main.rs`)

A `gtk::Box` (hidden by default) packed into `main_content` just above the breadcrumb,
containing a `gtk::Label` (status), a `gtk::ProgressBar`, and a Cancel `gtk::Button`. Exposed via
`ColumnManager` (like the dock): `set_progress_widgets(...)`, `show_progress(total)`,
`update_progress(fraction, label)`, `hide_progress()`. Cancel sets the shared `AtomicBool`.

## Data flow

```
transfer(sources, dest, kind):
  spawn_future_local:
    resolve conflicts per top-level item -> Vec<(src, dst, action)>   (existing logic)
    if move && all same filesystem: rename each; done (no bar)
    plan = spawn_blocking(scan all resolved items)                    (items, total_bytes, total_files)
    if (copy || cross-device) && total_bytes > free_space(dest): dialog "no space"; abort
    if show_progress_for(total_bytes, total_files): manager.show_progress(total)
    'items: for item in plan.items:
        let mut item_ok = true;
        create item.dirs
        for (src,dst) in item.files:
            loop {
              res = spawn_blocking(copy_file_chunked(src,dst,cancel,progress_sender))
              match res:
                Ok -> break
                Cancelled -> remove partial dst; manager.hide_progress(); return (keep done)
                Err -> match skip_all? Skip : choose_future(Skip/SkipAll/Retry/Cancel)
                         Skip -> failed+=1; item_ok=false; break
                         SkipAll -> skip_all=true; failed+=1; item_ok=false; break
                         Retry -> continue
                         Cancel -> remove partial dst; hide; return
            }
        recreate item.symlinks
        if move && item_ok: delete item.src_root   // only fully-copied items
    manager.hide_progress(); summary toast (done / skipped / failed)
  receiver task: drain progress channel -> manager.update_progress(...) throttled 100ms
```

## Error handling

- Scan I/O error → abort with toast; nothing written.
- Free-space query failure → proceed (don't block on an unknown; better than refusing a valid op),
  but log/ignore silently.
- Per-file copy error → Skip/Skip All/Retry/Cancel (never aborts the whole batch unless the user
  chooses Cancel).
- Cancel → partial current dst removed; completed kept; move sources for incomplete items kept.
- No `unwrap()` on the UI thread; `spawn_blocking` join handled (`unwrap_or` an error result).

## Testing

### Unit (headless)
- `plan_item` / scan on a temp tree: correct `total_bytes`, file/dir/symlink lists, symlinks not
  descended.
- Free-space comparison helper: `needs_space(total, free) -> bool` (e.g. `total > free`).
- Threshold decision helper: `show_progress_for(total_bytes, total_files) -> bool` (≥16 MiB or
  >100 files).
- `copy_file_chunked` on a temp file: copies bytes correctly; with a pre-set cancel flag, stops
  and the partial is removable; reports the right total via the progress callback.

### Manual (Verification Checklist)
- Copy a large folder → bottom bar shows %, file count, ETA-ish; window stays interactive.
- Cancel mid-copy → bar hides; completed files present; the in-progress file is gone; (move)
  source intact for the unfinished item.
- Copy more than free space → refused up front with a dialog; nothing written.
- Copy a tree containing a permission-denied file → Skip/Skip All/Retry/Cancel; Skip All finishes
  the rest; final toast reports the skipped count.
- Small copy (a few KB) → no bar (silent), same as today.
- Same-filesystem move → instant, no bar; cross-device move → bar + source removed on success.
- Conflict resolution (Replace/Skip/Keep Both/Merge) still works for colliding names.

## Out of scope (later sub-projects / deferred)
- SP2: Nautilus naming parity and conflict "apply to all" + suggested name (separate spec).
- SP3: undo (separate spec).
- Pause/resume, multiple concurrent operations with a queue/stack of progress rows (single active
  operation bar for now), and per-second transfer-rate/ETA text (a simple percentage + counts is
  enough for v1; ETA may be added if cheap).
