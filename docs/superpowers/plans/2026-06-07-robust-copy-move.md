# SP1 — Robust Copy/Move Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make copy/move scanned, cancellable, and progress-reporting with per-file Skip/Retry, replacing the silent all-or-nothing `transfer`.

**Architecture:** Up-front recursive scan gives total bytes/files → free-space gate + a non-modal bottom progress bar (shown past a threshold). Each top-level item is copied via a new async `transfer_item` that copies files in cancellable chunks, reports bytes through a shared `Arc<AtomicU64>` polled by a 100 ms glib timer, and prompts Skip/Skip All/Retry on per-file errors. The existing conflict-resolution loop (guards, Replace/Skip/Keep Both/Merge) is preserved.

**Tech Stack:** Rust 2024, gtk4 0.11, libadwaita 0.9, gio/glib 0.22, std (atomics/threads). No new crates (progress via atomics + glib timer, not a channel).

**Spec:** `docs/superpowers/specs/2026-06-07-robust-copy-move-design.md`

---

## File Structure
- `src/file_ops.rs` — pure scan/plan helpers + `copy_file_chunked` (Task 1); `transfer` rewrite + `transfer_item` (Task 3).
- `src/main.rs` — bottom progress bar widget + `ColumnManager` progress/cancel API (Task 2).
- `CHANGELOG.md` — document (Task 4).

---

## Task 1: Scan/plan helpers + chunked copy (pure, TDD)

**Files:** Modify `src/file_ops.rs` (add helpers near the top, after the existing `copy_recursive`; add tests to the `mod tests` block).

- [ ] **Step 1: Write failing tests** — append to the `#[cfg(test)] mod tests` block in `src/file_ops.rs`:

```rust
    #[test]
    fn needs_space_compares() {
        assert!(needs_space(100, 50));
        assert!(!needs_space(50, 100));
        assert!(!needs_space(50, 50));
    }

    #[test]
    fn show_progress_threshold() {
        assert!(!show_progress_for(1024, 3));               // tiny
        assert!(show_progress_for(20 * 1024 * 1024, 1));    // > 16 MiB
        assert!(show_progress_for(1024, 200));              // > 100 files
    }

    #[test]
    fn scan_item_counts_tree() {
        let tmp = std::env::temp_dir().join(format!("chv_scan_{}", std::process::id()));
        let src = tmp.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();        // 5 bytes
        std::fs::write(src.join("sub").join("b.txt"), b"hi").unwrap(); // 2 bytes
        let mut total = 0u64;
        let item = scan_item(&src, &tmp.join("dst"), &mut total).unwrap();
        assert_eq!(total, 7);
        assert_eq!(item.files.len(), 2);
        assert!(item.dirs.iter().any(|d| d.ends_with("dst")));
        assert!(item.dirs.iter().any(|d| d.ends_with("sub")));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn copy_file_chunked_copies_and_cancels() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let tmp = std::env::temp_dir().join(format!("chv_chunk_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("s"); let dst = tmp.join("d");
        std::fs::write(&src, vec![7u8; 1000]).unwrap();
        let cancel = AtomicBool::new(false);
        let bytes = AtomicU64::new(0);
        copy_file_chunked(&src, &dst, &cancel, &bytes).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap().len(), 1000);
        assert_eq!(bytes.load(Ordering::Relaxed), 1000);
        // pre-cancelled: returns Err, no full copy
        let dst2 = tmp.join("d2");
        cancel.store(true, Ordering::Relaxed);
        let bytes2 = AtomicU64::new(0);
        assert!(copy_file_chunked(&src, &dst2, &cancel, &bytes2).is_err());
        std::fs::remove_dir_all(&tmp).ok();
    }
```

Run: `cargo test scan_item 2>&1 | tail -15` → expect FAIL (functions missing).

- [ ] **Step 2: Implement the helpers** — add to `src/file_ops.rs` (after `copy_recursive`):

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Destination free space exceeded by `total`?
pub fn needs_space(total: u64, free: u64) -> bool { total > free }

/// Show the progress bar only for non-trivial operations.
pub fn show_progress_for(total_bytes: u64, total_files: usize) -> bool {
    total_bytes >= 16 * 1024 * 1024 || total_files > 100
}

/// One resolved top-level item's copy plan, grouped so a move can delete the
/// source only if every file copied.
pub struct ItemPlan {
    pub dirs: Vec<PathBuf>,
    pub files: Vec<(PathBuf, PathBuf)>,
    pub symlinks: Vec<(PathBuf, PathBuf)>,
}

/// Build the plan for copying `src` to `dst`, adding regular-file sizes to `total`.
/// Symlink-aware: links are recorded for recreation and never descended.
pub fn scan_item(src: &Path, dst: &Path, total: &mut u64) -> std::io::Result<ItemPlan> {
    let mut plan = ItemPlan { dirs: Vec::new(), files: Vec::new(), symlinks: Vec::new() };
    fn walk(src: &Path, dst: &Path, total: &mut u64, plan: &mut ItemPlan) -> std::io::Result<()> {
        let meta = std::fs::symlink_metadata(src)?;
        if meta.file_type().is_symlink() {
            plan.symlinks.push((src.to_path_buf(), dst.to_path_buf()));
        } else if meta.is_dir() {
            plan.dirs.push(dst.to_path_buf());
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                walk(&entry.path(), &dst.join(entry.file_name()), total, plan)?;
            }
        } else {
            *total += meta.len();
            plan.files.push((src.to_path_buf(), dst.to_path_buf()));
        }
        Ok(())
    }
    walk(src, dst, total, &mut plan)?;
    Ok(plan)
}

/// Copy one regular file in chunks, adding written bytes to `bytes_done` and
/// aborting (Err) if `cancel` is set between chunks. Leaves a partial `dst` on
/// abort/error for the caller to delete.
pub fn copy_file_chunked(src: &Path, dst: &Path, cancel: &AtomicBool, bytes_done: &AtomicU64) -> std::io::Result<()> {
    use std::io::{Read, Write};
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
    }
    if let Some(parent) = dst.parent() { std::fs::create_dir_all(parent)?; }
    let mut reader = std::fs::File::open(src)?;
    let mut writer = std::fs::File::create(dst)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        writer.write_all(&buf[..n])?;
        bytes_done.fetch_add(n as u64, Ordering::Relaxed);
    }
    writer.flush()?;
    // Preserve mode (best-effort).
    if let Ok(meta) = std::fs::metadata(src) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dst, std::fs::Permissions::from_mode(meta.permissions().mode()));
    }
    Ok(())
}
```

(`Path`, `PathBuf` are already imported at the top of `file_ops.rs`.)

- [ ] **Step 3: Run tests** — `cargo test 2>&1 | grep "test result"` → all pass (incl. the 4 new ones).
- [ ] **Step 4: Clippy** — `cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head` (expect none; `ItemPlan`/`scan_item` etc. may be unused until Task 3 — if clippy flags dead_code, add `#[allow(dead_code)]` with a `// TODO: used in Task 3` note, to be removed in Task 3).
- [ ] **Step 5: Commit** — `git add src/file_ops.rs && git commit -m "feat(file-ops): scan/plan + chunked cancellable copy helpers"`

---

## Task 2: Progress bar widget + ColumnManager API

**Files:** Modify `src/main.rs` — add a hidden progress bar to `build_ui` (above the breadcrumb), fields + methods on `ColumnManager`.

- [ ] **Step 1: Add fields to `ColumnManager`** (after the `preview_dock_content` field):

```rust
    progress_box: Rc<RefCell<Option<Box>>>,
    progress_bar: Rc<RefCell<Option<gtk::ProgressBar>>>,
    progress_label: Rc<RefCell<Option<gtk::Label>>>,
    cancel_flag: Rc<RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>>,
```

And initialise them in `ColumnManager::new` (after `preview_dock_content`):

```rust
            progress_box: Rc::new(RefCell::new(None)),
            progress_bar: Rc::new(RefCell::new(None)),
            progress_label: Rc::new(RefCell::new(None)),
            cancel_flag: Rc::new(RefCell::new(None)),
```

- [ ] **Step 2: Add the API methods** (in `impl ColumnManager`, near `update_dock`):

```rust
    fn set_progress_widgets(&self, container: Box, bar: gtk::ProgressBar, label: gtk::Label) {
        *self.progress_box.borrow_mut() = Some(container);
        *self.progress_bar.borrow_mut() = Some(bar);
        *self.progress_label.borrow_mut() = Some(label);
    }

    /// Show the progress bar for a new operation and return its cancel flag.
    fn show_progress(&self, initial_label: &str) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        *self.cancel_flag.borrow_mut() = Some(flag.clone());
        if let Some(label) = self.progress_label.borrow().as_ref() { label.set_text(initial_label); }
        if let Some(bar) = self.progress_bar.borrow().as_ref() { bar.set_fraction(0.0); }
        if let Some(b) = self.progress_box.borrow().as_ref() { b.set_visible(true); }
        flag
    }

    fn update_progress(&self, fraction: f64, label: &str) {
        if let Some(bar) = self.progress_bar.borrow().as_ref() { bar.set_fraction(fraction.clamp(0.0, 1.0)); }
        if let Some(l) = self.progress_label.borrow().as_ref() { l.set_text(label); }
    }

    fn hide_progress(&self) {
        if let Some(b) = self.progress_box.borrow().as_ref() { b.set_visible(false); }
        *self.cancel_flag.borrow_mut() = None;
    }
```

- [ ] **Step 3: Build the widget in `build_ui` and wire Cancel** — immediately BEFORE `main_content.append(&breadcrumb_scrolled);`, add:

```rust
    // Non-modal copy/move progress bar (hidden until an operation runs).
    let progress_label = gtk::Label::builder().halign(gtk::Align::Start).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::Middle).build();
    let progress_bar = gtk::ProgressBar::builder().hexpand(true).valign(gtk::Align::Center).build();
    let cancel_btn = gtk::Button::builder().icon_name("process-stop-symbolic").tooltip_text("Cancel").has_frame(false).build();
    let progress_box = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(4).margin_bottom(4).margin_start(12).margin_end(12)
        .build();
    progress_box.append(&progress_label);
    progress_box.append(&progress_bar);
    progress_box.append(&cancel_btn);
    progress_box.set_visible(false);
    {
        let manager_cancel = manager.clone();
        cancel_btn.connect_clicked(move |_| {
            if let Some(flag) = manager_cancel.cancel_flag.borrow().as_ref() {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }
    manager.set_progress_widgets(progress_box.clone(), progress_bar, progress_label);
    main_content.append(&progress_box);
    main_content.append(&breadcrumb_scrolled);
```

- [ ] **Step 4: Build + clippy + test** — `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`. The progress methods may be unused until Task 3 — if dead_code warnings appear, add `#[allow(dead_code)]` to those methods with a `// TODO: Task 3` note (removed in Task 3).
- [ ] **Step 5: Commit** — `git add src/main.rs && git commit -m "feat(ui): non-modal copy/move progress bar + ColumnManager progress API"`

---

## Task 3: Rewrite `transfer` to scan, gate, progress, and recover

**Files:** Modify `src/file_ops.rs` — replace the execution part of `transfer` and add `transfer_item`; remove any Task 1/2 `#[allow(dead_code)]`.

- [ ] **Step 1: Add `free_space` + the per-item executor** — add to `src/file_ops.rs`:

```rust
/// Free bytes on the filesystem holding `dir`, if queryable.
fn free_space(dir: &Path) -> Option<u64> {
    let info = gio::File::for_path(dir)
        .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
        .ok()?;
    Some(info.attribute_uint64("filesystem::free"))
}

enum ItemOutcome { Ok, Cancelled, Failed }

/// Copy/move one already-conflict-resolved item, reporting bytes to `bytes_done`,
/// honoring `cancel`, and prompting Skip/Skip All/Retry on per-file errors.
/// `skip_all` is shared across the whole batch.
#[allow(clippy::too_many_arguments)]
async fn transfer_item(
    parent: &gtk::Window,
    src: PathBuf,
    target: PathBuf,
    kind: TransferKind,
    merge_dirs: bool,
    cancel: std::sync::Arc<AtomicBool>,
    bytes_done: std::sync::Arc<AtomicU64>,
    files_done: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    skip_all: &mut bool,
) -> ItemOutcome {
    // Fast path: same-filesystem move with no merge/replace = instant rename.
    if kind == TransferKind::Move && !merge_dirs && !target.exists() {
        let s = src.clone(); let t = target.clone();
        match gio::spawn_blocking(move || std::fs::rename(&s, &t)).await {
            Ok(Ok(())) => return ItemOutcome::Ok,
            Ok(Err(e)) if !is_cross_device(&e) => return ItemOutcome::Failed,
            _ => {} // cross-device (or join error) -> fall through to copy
        }
    }

    // Scan this item.
    let s = src.clone(); let t = target.clone();
    let plan = match gio::spawn_blocking(move || { let mut n = 0u64; scan_item(&s, &t, &mut n) }).await {
        Ok(Ok(p)) => p,
        _ => return ItemOutcome::Failed,
    };

    // Create dirs.
    for d in &plan.dirs { let _ = std::fs::create_dir_all(d); }

    // Copy files (per-file Skip/Skip All/Retry).
    let mut item_ok = true;
    for (fsrc, fdst) in &plan.files {
        loop {
            let (fs, fd, c, b) = (fsrc.clone(), fdst.clone(), cancel.clone(), bytes_done.clone());
            let res = gio::spawn_blocking(move || copy_file_chunked(&fs, &fd, &c, &b)).await;
            if cancel.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(fdst); // partial
                return ItemOutcome::Cancelled;
            }
            match res {
                Ok(Ok(())) => { files_done.fetch_add(1, Ordering::Relaxed); break; }
                _ => {
                    let _ = std::fs::remove_file(fdst); // partial
                    if *skip_all { item_ok = false; break; }
                    let name = fsrc.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
                    match ask_file_error(parent, &name).await {
                        "retry" => continue,
                        "skip-all" => { *skip_all = true; item_ok = false; break; }
                        "cancel" => { cancel.store(true, Ordering::Relaxed); return ItemOutcome::Cancelled; }
                        _ => { item_ok = false; break; } // skip
                    }
                }
            }
        }
    }

    // Recreate symlinks.
    for (lsrc, ldst) in &plan.symlinks {
        if let Ok(tgt) = std::fs::read_link(lsrc) {
            let _ = std::fs::remove_file(ldst);
            let _ = std::os::unix::fs::symlink(tgt, ldst);
        }
    }

    if kind == TransferKind::Move && item_ok {
        let s = src.clone();
        let _ = gio::spawn_blocking(move || {
            if s.is_dir() { std::fs::remove_dir_all(&s) } else { std::fs::remove_file(&s) }
        }).await;
    }

    if item_ok { ItemOutcome::Ok } else { ItemOutcome::Failed }
}

/// Skip / Skip All / Retry / Cancel dialog for a per-file error.
async fn ask_file_error(parent: &gtk::Window, name: &str) -> &'static str {
    let dialog = adw::AlertDialog::builder()
        .heading("Couldn't copy file")
        .body(format!("\u{201c}{name}\u{201d} could not be copied."))
        .build();
    dialog.add_response("skip", "Skip");
    dialog.add_response("skip-all", "Skip All");
    dialog.add_response("retry", "Retry");
    dialog.add_response("cancel", "Cancel");
    dialog.set_default_response(Some("retry"));
    dialog.set_close_response("cancel");
    match dialog.choose_future(parent).await.as_str() {
        "skip" => "skip",
        "skip-all" => "skip-all",
        "retry" => "retry",
        _ => "cancel",
    }
}
```

- [ ] **Step 2: Rewrite the execution part of `transfer`** — keep the existing conflict-resolution loop, but instead of executing each item inline, collect resolved items, then scan/gate/execute. Replace the body of the `glib::spawn_future_local(async move { ... })` in `transfer` with:

```rust
    glib::spawn_future_local(async move {
        let mut apply_to_all: Option<&'static str> = None;
        let mut skipped = 0usize;
        let mut failed = 0usize;

        let dest_canon = dest_dir.canonicalize().ok();

        // Phase 1: resolve conflicts/guards into a work list (unchanged logic).
        let mut work: Vec<(PathBuf, PathBuf, bool)> = Vec::new(); // (src, target, merge_dirs)
        for src in sources {
            let Some(name) = src.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let src_canon = src.canonicalize().ok();
            if src.is_dir() && src_canon.is_none() {
                manager.send_toast(&format!("Can't verify \u{201c}{name}\u{201d}; skipped for safety"));
                failed += 1; continue;
            }
            if src.is_dir()
                && let (Some(sc), Some(dc)) = (&src_canon, &dest_canon)
                    && is_within(dc, sc) {
                        manager.send_toast(&format!("Can't place \u{201c}{name}\u{201d} inside itself"));
                        failed += 1; continue;
                    }
            let mut target = dest_dir.join(&name);
            let same_path = matches!((&src_canon, target.canonicalize().ok()), (Some(a), Some(b)) if *a == b);
            if same_path {
                match kind {
                    TransferKind::Copy => { target = unique_destination(&dest_dir, &name); }
                    TransferKind::Move => { skipped += 1; continue; }
                }
            }
            let mut merge_dirs = false;
            if target.exists() {
                let target_is_symlink = target.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);
                let both_dirs = src.is_dir() && target.is_dir() && !target_is_symlink;
                let choice = match apply_to_all {
                    Some(c) => c,
                    None => { let (c, all) = ask_conflict(&parent, &name, both_dirs).await; if all { apply_to_all = Some(c); } c }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => { target = unique_destination(&dest_dir, &name); }
                    _ => {
                        if both_dirs { merge_dirs = true; }
                        else {
                            let t = target.clone();
                            let removed = gio::spawn_blocking(move || {
                                let is_real_dir = std::fs::symlink_metadata(&t).map(|m| m.file_type().is_dir()).unwrap_or(false);
                                if is_real_dir { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) }
                            }).await;
                            if !matches!(removed, Ok(Ok(()))) { manager.send_toast(&format!("Could not replace {name}")); failed += 1; continue; }
                        }
                    }
                }
            }
            work.push((src, target, merge_dirs));
        }

        // Phase 2: scan totals for free-space + progress.
        let scan_srcs: Vec<PathBuf> = work.iter().map(|(s, _, _)| s.clone()).collect();
        let (total_bytes, total_files) = gio::spawn_blocking(move || {
            let mut bytes = 0u64; let mut files = 0usize;
            for s in &scan_srcs {
                let mut n = 0u64;
                if let Ok(p) = scan_item(s, std::path::Path::new("/x"), &mut n) { files += p.files.len(); }
                bytes += n;
            }
            (bytes, files)
        }).await.unwrap_or((0, 0));

        // Free-space gate (copies + cross-device moves; same-fs move is a rename).
        let cross_device = match (dest_canon.as_ref().and_then(|d| free_space(d)), ()) { _ => true };
        if kind == TransferKind::Copy || cross_device {
            if let Some(free) = free_space(&dest_dir)
                && needs_space(total_bytes, free) {
                    let dialog = adw::AlertDialog::builder()
                        .heading("Not enough space")
                        .body(format!("This needs {} but only {} is free on the destination.",
                            glib::format_size(total_bytes), glib::format_size(free)))
                        .build();
                    dialog.add_response("ok", "OK");
                    dialog.present(Some(&parent));
                    return;
                }
        }

        // Phase 3: execute with progress.
        let cancel = manager.show_progress("Preparing\u{2026}");
        let show_bar = show_progress_for(total_bytes, total_files);
        if !show_bar { manager.hide_progress(); }
        let bytes_done = std::sync::Arc::new(AtomicU64::new(0));
        let files_done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // 100ms throttled UI updater.
        let timer = if show_bar {
            let m = manager.clone();
            let bd = bytes_done.clone(); let fd = files_done.clone();
            let verb = if kind == TransferKind::Move { "Moving" } else { "Copying" };
            Some(glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                let done = bd.load(Ordering::Relaxed);
                let frac = if total_bytes > 0 { done as f64 / total_bytes as f64 } else { 0.0 };
                m.update_progress(frac, &format!("{verb} {} of {} files", fd.load(Ordering::Relaxed), total_files));
                glib::ControlFlow::Continue
            }))
        } else { None };

        let mut done = 0usize;
        let mut skip_all = false;
        let mut cancelled = false;
        for (src, target, merge_dirs) in work {
            match transfer_item(&parent, src, target, kind, merge_dirs, cancel.clone(), bytes_done.clone(), files_done.clone(), &mut skip_all).await {
                ItemOutcome::Ok => done += 1,
                ItemOutcome::Failed => failed += 1,
                ItemOutcome::Cancelled => { cancelled = true; break; }
            }
        }

        if let Some(t) = timer { t.remove(); }
        manager.hide_progress();

        let verb = if kind == TransferKind::Move { "Moved" } else { "Copied" };
        let mut extra = String::new();
        if skipped > 0 { extra.push_str(&format!(", skipped {skipped}")); }
        if failed > 0 { extra.push_str(&format!(", {failed} failed")); }
        if cancelled { extra.push_str(", cancelled"); }
        manager.send_toast(&format!("{verb} {done} item(s){extra}"));
        manager.refresh();
    });
```

NOTE: simplify the `cross_device` line above — the intent is "do the free-space check for copies always, and for moves too (a move may be cross-device)". A correct, simple form: always check free space when `kind == Copy`, and for `Move` also check (a same-fs move that's a rename uses no extra space, but checking is harmless because rename needs ~0 and `total_bytes` would only matter for the cross-device case; to avoid false 'no space' on a huge same-fs move, gate the move check on the destination being a different device — if you can't cheaply tell, SKIP the free-space check for moves and only enforce it for copies). Implement the pragmatic version: **enforce free-space only for `kind == Copy`** (moves are usually same-fs renames; a cross-device move that runs out of space will surface as a per-file error → Skip/Retry). Replace the `cross_device`/`if kind == Copy || cross_device` block with simply `if kind == TransferKind::Copy { if let Some(free) = free_space(&dest_dir) && needs_space(total_bytes, free) { …dialog…; return; } }`.

- [ ] **Step 3: Remove any temporary `#[allow(dead_code)]`** added in Tasks 1-2 (the helpers/methods now have callers). `grep -rn "allow(dead_code)" src/file_ops.rs src/main.rs` and remove the ones added for this feature.

- [ ] **Step 4: Build + clippy + test**

Run: `cargo build 2>&1 | tail -8 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"`
Expected: `Finished`, no clippy output, all tests pass. (`copy_recursive` may become unused if `transfer_item` fully replaces it — if so, check whether `compress`/other callers still use it via `grep -rn copy_recursive src/`; keep it if used, else remove it.)

- [ ] **Step 5: Commit** — `git add src/file_ops.rs && git commit -m "feat(file-ops): scanned, cancellable copy/move with progress and per-file Skip/Retry"`

---

## Task 4: Documentation + verification

**Files:** Modify `CHANGELOG.md`.

- [ ] **Step 1: CHANGELOG** — under `## [Unreleased]` (create it at the top if absent, before the latest released heading) add:

```markdown
### Added
- Copy/move now shows a non-modal **progress bar** (with Cancel) for large operations, checks destination **free space** up front, and offers **Skip / Skip All / Retry** when an individual file can't be copied. Cancelling removes the partial file and keeps already-copied items.
```

- [ ] **Step 2: Commit docs** — `git add CHANGELOG.md && git commit -m "docs: changelog for robust copy/move"`

- [ ] **Step 3: Full build + lint + test** — `cargo build 2>&1 | tail -3 && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head && cargo test 2>&1 | grep "test result"` → clean, all pass.

- [ ] **Step 4: Manual verification (`cargo run`)**
  - Copy a large folder (e.g. a few hundred MB) → bottom bar shows %, file count; window stays interactive; bar hides at the end.
  - Click Cancel mid-copy → bar hides; completed files present; in-progress file gone; (move) source intact for the unfinished item.
  - Copy more than the destination's free space → refused up front with a dialog; nothing written.
  - Copy a tree containing an unreadable file (e.g. `chmod 000` a file) → Skip / Skip All / Retry; Skip All finishes the rest; final toast reports the count.
  - Small copy (a few KB) → no bar (instant), as before.
  - Same-filesystem move → instant; cross-device move (e.g. to a USB/another mount) → bar + source removed on success.
  - Name conflicts still prompt Replace / Skip / Keep Both / Merge.

- [ ] **Step 5: Finish the branch** — use `superpowers:finishing-a-development-branch`.

---

## Notes for the implementer
- `gio::spawn_blocking(f).await` returns `std::thread::Result<T>`; match `Ok(Ok(_))` etc.; treat a join `Err(_)` as failure.
- `ask_conflict`, `unique_destination`, `is_within`, `is_cross_device`, `copy_recursive` already exist in `file_ops.rs`.
- `glib::format_size(u64)` renders human sizes; `glib::timeout_add_local` returns a `SourceId` with `.remove()`.
- Keep the conflict-resolution logic byte-for-byte equivalent to today's (only its *structure* moves into "Phase 1"); do not weaken any guard.
- Do not add new crates. Progress is via `Arc<AtomicU64>`/`AtomicUsize` + a 100 ms glib timer, cancel via `Arc<AtomicBool>`.
- No `unwrap()` on the UI thread (the `.unwrap_or((0,0))` is on a join handle).
