use std::path::{Path, PathBuf};
use std::rc::Rc;
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use crate::ColumnManager;

/// Spawn `cmd`, reaping the child on a detached thread so it doesn't become a
/// zombie. Returns Ok(()) if it launched.
fn spawn_reaped(cmd: &mut std::process::Command) -> std::io::Result<()> {
    let child = cmd.spawn()?;
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}

/// Split a file name into (stem, extension-with-dot). Leading-dot files
/// (".bashrc") have no extension. A compound `.tar.*` is kept whole
/// ("a.tar.gz" -> ("a", ".tar.gz")).
fn split_name(file_name: &str) -> (String, String) {
    if let Some(idx) = file_name.rfind('.')
        && idx > 0 {
            let mut stem = file_name[..idx].to_string();
            let mut ext = file_name[idx..].to_string();
            if let Some(tidx) = stem.rfind('.')
                && tidx > 0
                && &stem[tidx..] == ".tar" {
                    ext = format!(".tar{ext}");
                    stem.truncate(tidx);
                }
            return (stem, ext);
        }
    (file_name.to_string(), String::new())
}

/// Name for an explicit Duplicate: always adds the appendix, even if the bare
/// name is free. "x.txt" -> "x (Copy).txt" -> "x (Copy 2).txt" ...
pub fn copy_dup_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_name(file_name);
    let first = format!("{stem} (Copy){ext}");
    if !exists(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let cand = format!("{stem} (Copy {n}){ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// A non-colliding name: the bare name if free, else numbered. "x.txt" (taken)
/// -> "x (2).txt" -> "x (3).txt" ... Used for conflict Keep-Both, same-path
/// copy, compress, and symlink dedup.
pub fn conflict_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(file_name) {
        return file_name.to_string();
    }
    let (stem, ext) = split_name(file_name);
    let mut n = 2;
    loop {
        let cand = format!("{stem} ({n}){ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// Return a name for a brand-new item that does not collide, per `exists`.
/// Uses an "untitled" numbering scheme ("base", "base 2", "base 3", ...),
/// distinct from the " (copy)" scheme used for duplicates. The number is
/// inserted before `ext` (pass "" for no extension). `base` and `ext` are
/// expected to be pre-split (ext starts with `.` or is empty).
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

/// Compute a non-colliding destination path inside `dir` for `file_name`.
pub fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let name = conflict_name(file_name, |n| dir.join(n).exists());
    dir.join(name)
}

/// Recursively copy `src` to `dst` (file, directory, or symlink). `dst` is the
/// full target path. Symlinks are recreated (not dereferenced). Copying a
/// directory onto an existing directory MERGES (existing files are kept; only
/// colliding leaf files are overwritten).
pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        if let Some(parent) = dst.parent() { std::fs::create_dir_all(parent)?; }
        let _ = std::fs::remove_file(dst);
        std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)?;
        Ok(())
    } else if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::copy(src, dst)?;
        Ok(())
    }
}

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
    if let Ok(meta) = std::fs::metadata(src) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dst, std::fs::Permissions::from_mode(meta.permissions().mode()));
    }
    Ok(())
}

/// True if `path` equals `ancestor` or is nested under it. Inputs should be
/// canonicalized by the caller.
pub fn is_within(path: &Path, ancestor: &Path) -> bool {
    path.starts_with(ancestor)
}

/// Validate a name for rename/create. Returns Err(reason) if invalid.
pub fn validate_filename(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Name cannot be empty".into());
    }
    if name.contains('/') {
        return Err("Name cannot contain \u{201c}/\u{201d}".into());
    }
    if name == "." || name == ".." {
        return Err("Name cannot be \u{201c}.\u{201d} or \u{201c}..\u{201d}".into());
    }
    if name.len() > 255 {
        return Err("Name is too long".into());
    }
    Ok(())
}

/// True if a rename failed because source and destination are on different filesystems.
fn is_cross_device(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18) // EXDEV
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransferKind { Copy, Move }

/// Move or copy a batch of source paths into `dest_dir`, prompting on collisions.
/// Reports via the manager's toast overlay and refreshes the UI when done.
pub fn transfer(
    manager: Rc<ColumnManager>,
    parent: gtk::Window,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    kind: TransferKind,
) {
    glib::spawn_future_local(async move {
        let mut apply_to_all: Option<&'static str> = None;
        let mut skipped = 0usize;
        let mut failed = 0usize;

        let dest_canon = dest_dir.canonicalize().ok();

        let mut work: Vec<(PathBuf, PathBuf, bool)> = Vec::new();
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
                let suggested = conflict_name(&name, |n| dest_dir.join(n).exists());
                let (choice, keep_name) = match apply_to_all {
                    Some(c) => (c, String::new()),
                    None => {
                        let (c, all, kn) = ask_conflict(&parent, &name, both_dirs, &suggested).await;
                        if all { apply_to_all = Some(c); }
                        (c, kn)
                    }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => {
                        // Use the (possibly edited) name when it's valid and free;
                        // otherwise auto-generate. Apply-to-all always auto-names.
                        let edited_ok = apply_to_all.is_none()
                            && !keep_name.trim().is_empty()
                            && validate_filename(&keep_name).is_ok()
                            && !dest_dir.join(&keep_name).exists();
                        target = if edited_ok { dest_dir.join(&keep_name) } else { unique_destination(&dest_dir, &name) };
                    }
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

        // Free-space gate for copies and cross-device moves (a same-filesystem
        // move is an instant rename that uses no extra space, so it's exempt).
        let cross_device_move = kind == TransferKind::Move
            && work.iter().any(|(s, _, _)| !same_device(s, &dest_dir));
        if (kind == TransferKind::Copy || cross_device_move)
            && let Some(free) = free_space(&dest_dir)
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

        let show_bar = show_progress_for(total_bytes, total_files);
        let verb_ing = if kind == TransferKind::Move { "Moving" } else { "Copying" };
        let cancel = if show_bar {
            manager.show_progress(&format!("{verb_ing}\u{2026}"))
        } else {
            std::sync::Arc::new(AtomicBool::new(false))
        };
        let bytes_done = std::sync::Arc::new(AtomicU64::new(0));
        let files_done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let timer = if show_bar {
            let m = manager.clone();
            let bd = bytes_done.clone(); let fd = files_done.clone();
            Some(glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                let done = bd.load(Ordering::Relaxed);
                let frac = if total_bytes > 0 { done as f64 / total_bytes as f64 } else { 0.0 };
                m.update_progress(frac, &format!("{verb_ing} {} of {} files", fd.load(Ordering::Relaxed), total_files));
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
        if show_bar { manager.hide_progress(); }

        let verb = if kind == TransferKind::Move { "Moved" } else { "Copied" };
        let mut extra = String::new();
        if skipped > 0 { extra.push_str(&format!(", skipped {skipped}")); }
        if failed > 0 { extra.push_str(&format!(", {failed} failed")); }
        if cancelled { extra.push_str(", cancelled"); }
        manager.send_toast(&format!("{verb} {done} item(s){extra}"));
        manager.refresh();
    });
}

/// Whether `a` and `b` live on the same filesystem (device). Unknown → false
/// (conservative: treat as cross-device so the free-space gate still applies).
fn same_device(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) => ma.dev() == mb.dev(),
        _ => false,
    }
}

/// Free bytes on the filesystem holding `dir`, if queryable.
fn free_space(dir: &Path) -> Option<u64> {
    let info = gio::File::for_path(dir)
        .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
        .ok()?;
    Some(info.attribute_uint64("filesystem::free"))
}

enum ItemOutcome { Ok, Cancelled, Failed }

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
    if kind == TransferKind::Move && !merge_dirs && !target.exists() {
        let s = src.clone(); let t = target.clone();
        match gio::spawn_blocking(move || std::fs::rename(&s, &t)).await {
            Ok(Ok(())) => return ItemOutcome::Ok,
            Ok(Err(e)) if !is_cross_device(&e) => return ItemOutcome::Failed,
            _ => {}
        }
    }

    let s = src.clone(); let t = target.clone();
    let plan = match gio::spawn_blocking(move || { let mut n = 0u64; scan_item(&s, &t, &mut n) }).await {
        Ok(Ok(p)) => p,
        _ => return ItemOutcome::Failed,
    };

    // Create the dir skeleton off the UI thread (deep trees can have many dirs).
    let dirs = plan.dirs.clone();
    let _ = gio::spawn_blocking(move || {
        for d in &dirs { let _ = std::fs::create_dir_all(d); }
    }).await;

    let mut item_ok = true;
    for (fsrc, fdst) in &plan.files {
        loop {
            let (fs, fd, c, b) = (fsrc.clone(), fdst.clone(), cancel.clone(), bytes_done.clone());
            let res = gio::spawn_blocking(move || copy_file_chunked(&fs, &fd, &c, &b)).await;
            if cancel.load(Ordering::Relaxed) {
                let fd = fdst.clone();
                let _ = gio::spawn_blocking(move || { let _ = std::fs::remove_file(&fd); }).await;
                return ItemOutcome::Cancelled;
            }
            match res {
                Ok(Ok(())) => { files_done.fetch_add(1, Ordering::Relaxed); break; }
                _ => {
                    let fd = fdst.clone();
                    let _ = gio::spawn_blocking(move || { let _ = std::fs::remove_file(&fd); }).await;
                    if *skip_all { item_ok = false; break; }
                    let name = fsrc.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
                    match ask_file_error(parent, &name).await {
                        "retry" => continue,
                        "skip-all" => { *skip_all = true; item_ok = false; break; }
                        "cancel" => { cancel.store(true, Ordering::Relaxed); return ItemOutcome::Cancelled; }
                        _ => { item_ok = false; break; }
                    }
                }
            }
        }
    }

    // Recreate symlinks off the UI thread.
    let links = plan.symlinks.clone();
    let _ = gio::spawn_blocking(move || {
        for (lsrc, ldst) in &links {
            if let Ok(tgt) = std::fs::read_link(lsrc) {
                let _ = std::fs::remove_file(ldst);
                let _ = std::os::unix::fs::symlink(tgt, ldst);
            }
        }
    }).await;

    if kind == TransferKind::Move && item_ok {
        let s = src.clone();
        let _ = gio::spawn_blocking(move || {
            if s.is_dir() { std::fs::remove_dir_all(&s) } else { std::fs::remove_file(&s) }
        }).await;
    }

    if item_ok { ItemOutcome::Ok } else { ItemOutcome::Failed }
}

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
    match dialog.choose_future(Some(parent)).await.as_str() {
        "skip" => "skip",
        "skip-all" => "skip-all",
        "retry" => "retry",
        _ => "cancel",
    }
}

/// Conflict dialog. Returns (choice, apply_to_all, keep_both_name). The entry is
/// pre-filled with `suggested` and disabled when "apply to all" is ticked (a typed
/// name can't apply to a whole batch).
async fn ask_conflict(parent: &gtk::Window, name: &str, merge: bool, suggested: &str) -> (&'static str, bool, String) {
    let dialog = adw::AlertDialog::builder()
        .heading("Item already exists")
        .body(format!("\u{201c}{name}\u{201d} already exists in the destination. What do you want to do?"))
        .build();
    dialog.add_response("skip", "Skip");
    dialog.add_response("keep", "Keep Both");
    dialog.add_response("replace", if merge { "Merge" } else { "Replace" });
    if !merge {
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
    }
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("skip");

    let entry = gtk::Entry::builder()
        .text(suggested)
        .activates_default(true)
        .margin_top(8).margin_start(12).margin_end(12)
        .build();
    let check = gtk::CheckButton::with_label("Apply to all remaining");
    check.set_margin_top(8);
    check.set_margin_start(12);
    check.set_margin_end(12);
    check.set_margin_bottom(8);
    {
        let entry_c = entry.clone();
        check.connect_toggled(move |c| entry_c.set_sensitive(!c.is_active()));
    }
    let extra = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    extra.append(&entry);
    extra.append(&check);
    dialog.set_extra_child(Some(&extra));

    let entry_sel = entry.clone();
    glib::idle_add_local_once(move || {
        entry_sel.grab_focus();
        entry_sel.select_region(0, -1);
    });

    let response = dialog.choose_future(Some(parent)).await;
    let choice: &'static str = match response.as_str() {
        "replace" => "replace",
        "keep" => "keep",
        _ => "skip",
    };
    (choice, check.is_active(), entry.text().to_string())
}

/// Move a batch of paths to the trash. One summary toast; collapses columns per success.
pub fn trash(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    glib::spawn_future_local(async move {
        let mut done = 0usize;
        let mut failed = 0usize;
        for path in &paths {
            let file = gio::File::for_path(path);
            match file.trash_future(glib::Priority::DEFAULT).await {
                Ok(_) => done += 1,
                Err(_) => failed += 1,
            }
        }
        if done > 0 {
            manager.on_files_deleted(&paths);
        }
        manager.send_toast(&format!(
            "Moved {done} item(s) to Trash{}",
            if failed > 0 { format!(", {failed} failed") } else { String::new() }
        ));
    });
}

/// Permanently delete a batch of paths after a confirmation dialog.
pub fn delete(manager: Rc<ColumnManager>, parent: gtk::Window, paths: Vec<PathBuf>) {
    if paths.is_empty() { return; }
    glib::spawn_future_local(async move {
        let n = paths.len();
        let dialog = adw::AlertDialog::builder()
            .heading("Delete permanently?")
            .body(format!(
                "{} will be permanently deleted. This cannot be undone.",
                if n == 1 { "1 item".to_string() } else { format!("{n} items") }
            ))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        if dialog.choose_future(Some(&parent)).await != "delete" {
            return;
        }

        let mut done = 0usize;
        let mut failed = 0usize;
        for path in &paths {
            let p = path.clone();
            let res = gio::spawn_blocking(move || {
                if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) }
            }).await;
            match res {
                Ok(Ok(())) => done += 1,
                _ => failed += 1,
            }
        }
        if done > 0 {
            manager.on_files_deleted(&paths);
        }
        manager.send_toast(&format!(
            "Deleted {done} item(s){}",
            if failed > 0 { format!(", {failed} failed") } else { String::new() }
        ));
    });
}

/// Open a terminal at `dir`.
pub fn open_terminal(manager: Rc<ColumnManager>, dir: PathBuf) {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    let spawned = spawn_reaped(Command::new("open").arg("-a").arg("Terminal").arg(&dir)).is_ok();

    #[cfg(not(target_os = "macos"))]
    let spawned = {
        let mut ok = false;
        let candidates: Vec<(String, Vec<String>)> = {
            let mut v = Vec::new();
            if let Ok(t) = std::env::var("TERMINAL") { v.push((t, vec![])); }
            v.push(("gnome-terminal".into(), vec!["--working-directory".into(), dir.to_string_lossy().to_string()]));
            v.push(("konsole".into(), vec!["--workdir".into(), dir.to_string_lossy().to_string()]));
            v.push(("kitty".into(), vec!["--directory".into(), dir.to_string_lossy().to_string()]));
            v.push(("alacritty".into(), vec!["--working-directory".into(), dir.to_string_lossy().to_string()]));
            v.push(("foot".into(), vec![]));
            v.push(("xterm".into(), vec![]));
            v
        };
        for (cmd, args) in candidates {
            if spawn_reaped(Command::new(&cmd).args(&args).current_dir(&dir)).is_ok() { ok = true; break; }
        }
        ok
    };

    if !spawned { manager.send_toast("No terminal found"); }
}

/// Choose the archive file name for a selection.
pub fn archive_name(paths: &[PathBuf]) -> String {
    if paths.len() == 1
        && let Some(name) = paths[0].file_name().and_then(|n| n.to_str()) {
            return format!("{name}.zip");
        }
    "Archive.zip".to_string()
}

/// Zip the selected paths into a .zip in their parent directory.
pub fn compress(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    let Some(first) = paths.first() else { return };
    let Some(dir) = first.parent().map(|p| p.to_path_buf()) else { return };
    let name = conflict_name(&archive_name(&paths), |n| dir.join(n).exists());
    let out = dir.join(name);

    glib::spawn_future_local(async move {
        let res = gio::spawn_blocking(move || zip_paths(&paths, &out)).await;
        match res {
            Ok(Ok(())) => { manager.send_toast("Compressed"); manager.refresh(); }
            Ok(Err(e)) => manager.send_toast(&format!("Compression failed: {e}")),
            Err(_) => manager.send_toast("Compression failed"),
        }
    });
}

fn zip_paths(paths: &[PathBuf], out: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    fn add(zip: &mut zip::ZipWriter<std::fs::File>, base: &Path, path: &Path, opts: &zip::write::SimpleFileOptions) -> std::io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Ok(()); // skip symlinks: avoids loops and out-of-tree path leakage
        }
        let rel = path.strip_prefix(base.parent().unwrap_or(base)).unwrap_or(path);
        let Some(name) = rel.to_str() else { return Ok(()); }; // skip non-UTF-8 names
        let name = name.trim_start_matches('/').to_string();
        if meta.is_dir() {
            zip.add_directory(format!("{name}/"), *opts).map_err(std::io::Error::from)?;
            for entry in std::fs::read_dir(path)? {
                add(zip, base, &entry?.path(), opts)?;
            }
        } else {
            zip.start_file(name, *opts).map_err(std::io::Error::from)?;
            let data = std::fs::read(path)?;
            zip.write_all(&data)?;
        }
        Ok(())
    }

    for p in paths {
        add(&mut zip, p, p, &opts)?;
    }
    zip.finish().map_err(std::io::Error::from)?;
    Ok(())
}

/// Attach the given files to a new email via the platform's mechanism.
pub fn email(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    use std::process::Command;
    let paths: Vec<PathBuf> = paths.into_iter().filter(|p| p.is_file() && p.to_str().is_some()).collect();
    if paths.is_empty() { manager.send_toast("Select file(s) to share"); return; }

    #[cfg(target_os = "macos")]
    {
        // Pass paths as argv (data, not interpolated into the script) to avoid injection.
        let mut cmd = Command::new("osascript");
        cmd.arg("-e").arg("on run argv")
            .arg("-e").arg("tell application \"Mail\"")
            .arg("-e").arg("set m to make new outgoing message")
            .arg("-e").arg("tell m")
            .arg("-e").arg("repeat with p in argv")
            .arg("-e").arg("make new attachment with properties {file name:POSIX file (p as text)}")
            .arg("-e").arg("end repeat")
            .arg("-e").arg("end tell")
            .arg("-e").arg("set visible of m to true")
            .arg("-e").arg("activate")
            .arg("-e").arg("end tell")
            .arg("-e").arg("end run");
        for p in &paths { cmd.arg(p); }
        if spawn_reaped(&mut cmd).is_err() { manager.send_toast("Could not open Mail"); }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = Command::new("xdg-email");
        for p in &paths { cmd.arg("--attach").arg(p); }
        if spawn_reaped(&mut cmd).is_err() { manager.send_toast("xdg-email not available"); }
    }
}

/// Share files. On macOS, presents the native `NSSharingServicePicker` anchored
/// to the app window; if that cannot be presented it falls back to `email`. On
/// other platforms it delegates straight to `email`.
#[cfg(target_os = "macos")]
pub fn share(manager: Rc<ColumnManager>, parent: gtk::Window, paths: Vec<PathBuf>) {
    if !show_share_sheet(&parent, &paths) {
        email(manager, paths);
    }
}

/// Share files (non-macOS): delegate to the email mechanism.
#[cfg(not(target_os = "macos"))]
pub fn share(manager: Rc<ColumnManager>, parent: gtk::Window, paths: Vec<PathBuf>) {
    let _ = &parent;
    email(manager, paths);
}

/// Present the native macOS Share sheet for the given files anchored to the
/// content view of `parent`'s `NSWindow`. Returns `false` (without presenting)
/// if there are no existing files or the native window cannot be resolved, so
/// the caller can fall back to `email`. This is `unsafe` Obj-C FFI; every step
/// is guarded so it can never panic.
#[cfg(target_os = "macos")]
fn show_share_sheet(parent: &gtk::Window, paths: &[PathBuf]) -> bool {
    use gtk::prelude::NativeExt;
    use gdk4_macos::MacosSurface;
    use objc2::AnyThread;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSSharingServicePicker, NSWindow};
    use objc2_foundation::{NSArray, NSRectEdge, NSString, NSURL};

    // Only share files that actually exist on disk.
    let files: Vec<&PathBuf> = paths.iter().filter(|p| p.is_file()).collect();
    if files.is_empty() {
        return false;
    }

    // Build NSURL file URLs from the paths. Skip any path that is not valid UTF-8.
    let mut urls: Vec<Retained<NSURL>> = Vec::with_capacity(files.len());
    for p in &files {
        let Some(s) = p.to_str() else { continue };
        let ns = NSString::from_str(s);
        let url = NSURL::fileURLWithPath(&ns);
        urls.push(url);
    }
    if urls.is_empty() {
        return false;
    }

    // Resolve the underlying NSWindow* from the GTK window's GDK surface.
    let surface = parent.surface();
    let Some(surface) = surface else { return false };
    let Ok(macos_surface) = surface.downcast::<MacosSurface>() else {
        return false;
    };
    let win_ptr = macos_surface.native();
    if win_ptr.is_null() {
        return false;
    }

    // SAFETY: `native()` hands us a borrowed `NSWindow*`. We take a retained
    // reference so the object stays alive for the duration of this call.
    let window: Retained<NSWindow> = match unsafe { Retained::retain(win_ptr.cast()) } {
        Some(w) => w,
        None => return false,
    };

    // `contentView` returns the window's content view (or nil).
    let Some(view) = window.contentView() else {
        return false;
    };

    // Build the picker over the file URLs (each NSURL is a shareable item).
    // `initWithItems:` wants an untyped `NSArray` (of `AnyObject`), so collect
    // the URLs as type-erased object references.
    let objects: Vec<&AnyObject> = urls.iter().map(|u| AsRef::<AnyObject>::as_ref(&**u)).collect();
    let items: Retained<NSArray<AnyObject>> = NSArray::from_slice(&objects);
    // SAFETY: `initWithItems:` takes ownership of the allocated picker and reads
    // the items array; both are valid here.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };

    let bounds = view.bounds();
    // Present the share sheet relative to the content view's bounds.
    picker.showRelativeToRect_ofView_preferredEdge(bounds, &view, NSRectEdge::MinY);

    true
}

/// Create a symlink named "<name> link" beside each source (numbered on collision).
pub fn symlink(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    let mut made = 0usize;
    for src in &paths {
        let Some(dir) = src.parent() else { continue };
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
        let link_name = conflict_name(&format!("{name} link"), |n| dir.join(n).exists());
        let link_path = dir.join(link_name);
        match std::os::unix::fs::symlink(src, &link_path) {
            Ok(()) => made += 1,
            Err(e) => manager.send_toast(&format!("Link failed: {e}")),
        }
    }
    if made > 0 {
        manager.send_toast(&format!("Created {made} link(s)"));
        manager.refresh();
    }
}

/// Duplicate each path in place with a `(Copy)` name (symlink-preserving). Never
/// prompts: a fresh non-colliding name is always generated.
pub fn duplicate(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    glib::spawn_future_local(async move {
        let mut done = 0usize;
        let mut failed = 0usize;
        for src in &paths {
            let Some(dir) = src.parent().map(|p| p.to_path_buf()) else { failed += 1; continue };
            let Some(name) = src.file_name().and_then(|n| n.to_str()) else { failed += 1; continue };
            let dst = dir.join(copy_dup_name(name, |n| dir.join(n).exists()));
            let s = src.clone();
            match gio::spawn_blocking(move || copy_recursive(&s, &dst)).await {
                Ok(Ok(())) => done += 1,
                _ => failed += 1,
            }
        }
        let mut extra = String::new();
        if failed > 0 { extra.push_str(&format!(", {failed} failed")); }
        manager.send_toast(&format!("Duplicated {done} item(s){extra}"));
        manager.refresh();
    });
}

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
/// then open the naming dialog so the user can rename it. The monitored
/// DirectoryList surfaces the new file automatically.
pub fn create_file(manager: Rc<ColumnManager>, parent: gtk::Window, dir: PathBuf) {
    let name = untitled_name("untitled file", "", |n| dir.join(n).exists());
    let new_path = dir.join(&name);
    let file = gio::File::for_path(&new_path);
    file.create_async(
        gio::FileCreateFlags::NONE,
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |res| match res {
            // `_stream` is the new FileOutputStream; dropping it closes the fd via
            // GObject finalize on the built-in GLocalFileOutputStream.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_recursive_copies_tree() {
        let tmp = std::env::temp_dir().join(format!("chv_test_{}", std::process::id()));
        let src = tmp.join("src");
        let dst = tmp.join("dst");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("sub").join("f.txt"), b"hi").unwrap();
        copy_recursive(&src, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("sub").join("f.txt")).unwrap(), b"hi");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn split_name_cases() {
        assert_eq!(split_name("a.txt"), ("a".to_string(), ".txt".to_string()));
        assert_eq!(split_name("noext"), ("noext".to_string(), String::new()));
        assert_eq!(split_name(".bashrc"), (".bashrc".to_string(), String::new()));
        assert_eq!(split_name("a.tar.gz"), ("a".to_string(), ".tar.gz".to_string()));
        assert_eq!(split_name("a.tar"), ("a".to_string(), ".tar".to_string()));
        assert_eq!(split_name("a.tar.bz2"), ("a".to_string(), ".tar.bz2".to_string()));
    }

    #[test]
    fn copy_dup_name_scheme() {
        assert_eq!(copy_dup_name("x.txt", |_| false), "x (Copy).txt");
        assert_eq!(copy_dup_name("x.txt", |n| n == "x (Copy).txt"), "x (Copy 2).txt");
        assert_eq!(copy_dup_name("a.tar.gz", |_| false), "a (Copy).tar.gz");
        assert_eq!(copy_dup_name(".bashrc", |_| false), ".bashrc (Copy)");
    }

    #[test]
    fn conflict_name_scheme() {
        assert_eq!(conflict_name("x.txt", |_| false), "x.txt"); // free -> bare
        assert_eq!(conflict_name("x.txt", |n| n == "x.txt"), "x (2).txt");
        let taken = |n: &str| n == "x.txt" || n == "x (2).txt";
        assert_eq!(conflict_name("x.txt", taken), "x (3).txt");
        assert_eq!(conflict_name("a.tar.gz", |n| n == "a.tar.gz"), "a (2).tar.gz");
    }

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

    #[test]
    fn is_cross_device_detects_exdev() {
        assert!(is_cross_device(&std::io::Error::from_raw_os_error(18)));
        assert!(!is_cross_device(&std::io::Error::from(std::io::ErrorKind::NotFound)));
    }

    #[test]
    fn unique_destination_avoids_collisions() {
        let tmp = std::env::temp_dir().join(format!("chv_ud_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert_eq!(unique_destination(&tmp, "x.txt"), tmp.join("x.txt"));
        std::fs::write(tmp.join("x.txt"), b"").unwrap();
        assert_eq!(unique_destination(&tmp, "x.txt"), tmp.join("x (2).txt"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn is_within_cases() {
        use std::path::Path;
        assert!(is_within(Path::new("/a/b"), Path::new("/a")));
        assert!(is_within(Path::new("/a"), Path::new("/a")));
        assert!(!is_within(Path::new("/a"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/x/y"), Path::new("/a")));
    }

    #[test]
    fn copy_recursive_merges_into_existing_dir() {
        let tmp = std::env::temp_dir().join(format!("chv_merge_{}", std::process::id()));
        let src = tmp.join("src");
        let dst = tmp.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("b.txt"), b"b").unwrap();
        std::fs::write(dst.join("a.txt"), b"a").unwrap();
        // colliding file: present in both, different contents
        std::fs::write(src.join("c.txt"), b"from-src").unwrap();
        std::fs::write(dst.join("c.txt"), b"from-dst").unwrap();
        copy_recursive(&src, &dst).unwrap();
        assert!(dst.join("a.txt").exists(), "existing dest file preserved");
        assert!(dst.join("b.txt").exists(), "source file merged in");
        assert_eq!(std::fs::read(dst.join("c.txt")).unwrap(), b"from-src", "colliding file overwritten with source content");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn validate_filename_rules() {
        assert!(validate_filename("ok.txt").is_ok());
        assert!(validate_filename("").is_err());
        assert!(validate_filename("   ").is_err());
        assert!(validate_filename("a/b").is_err());
        assert!(validate_filename(".").is_err());
        assert!(validate_filename("..").is_err());
        assert!(validate_filename(&"x".repeat(256)).is_err());
        assert!(validate_filename(".hidden").is_ok()); // leading dot allowed (warning only)
    }

    #[test]
    fn archive_name_single_and_multi() {
        assert_eq!(archive_name(&[PathBuf::from("/a/b/photo.png")]), "photo.png.zip");
        assert_eq!(archive_name(&[PathBuf::from("/a/x"), PathBuf::from("/a/y")]), "Archive.zip");
    }

    #[test]
    fn copy_recursive_preserves_symlink() {
        let tmp = std::env::temp_dir().join(format!("chv_link_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let real = tmp.join("real.txt");
        std::fs::write(&real, b"hi").unwrap();
        let link = tmp.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let dst = tmp.join("copied");
        copy_recursive(&link, &dst).unwrap();
        assert!(dst.symlink_metadata().unwrap().file_type().is_symlink());
        std::fs::remove_dir_all(&tmp).ok();
    }

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
        std::fs::write(src.join("a.txt"), b"hello").unwrap();          // 5 bytes
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
        let dst2 = tmp.join("d2");
        cancel.store(true, Ordering::Relaxed);
        let bytes2 = AtomicU64::new(0);
        assert!(copy_file_chunked(&src, &dst2, &cancel, &bytes2).is_err());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
