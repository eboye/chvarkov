use std::path::{Path, PathBuf};
use std::rc::Rc;
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use crate::ColumnManager;

/// Split a file name into (stem, extension-with-dot). Leading-dot files
/// (".bashrc") are treated as having no extension.
fn split_name(file_name: &str) -> (String, String) {
    if let Some(idx) = file_name.rfind('.')
        && idx > 0 {
            return (file_name[..idx].to_string(), file_name[idx..].to_string());
        }
    (file_name.to_string(), String::new())
}

/// Return a file name that does not collide, per the `exists` predicate.
/// First tries "<stem> (copy)<ext>", then "<stem> (copy N)<ext>".
pub fn dedupe_file_name(file_name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(file_name) {
        return file_name.to_string();
    }
    let (stem, ext) = split_name(file_name);
    let first = format!("{stem} (copy){ext}");
    if !exists(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let cand = format!("{stem} (copy {n}){ext}");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// Return a name for a brand-new item that does not collide, per `exists`.
/// Uses an "untitled" numbering scheme ("base", "base 2", "base 3", ...),
/// distinct from the " (copy)" scheme used for duplicates. The number is
/// inserted before `ext` (pass "" for no extension).
#[allow(dead_code)]
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
    let name = dedupe_file_name(file_name, |n| dir.join(n).exists());
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
        let mut done = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;

        let dest_canon = dest_dir.canonicalize().ok();

        for src in sources {
            let Some(name) = src.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let src_canon = src.canonicalize().ok();

            // Guard: don't place a directory inside itself or a descendant.
            if src.is_dir()
                && let (Some(sc), Some(dc)) = (&src_canon, &dest_canon)
                    && is_within(dc, sc) {
                        manager.send_toast(&format!("Can't place \u{201c}{name}\u{201d} inside itself"));
                        failed += 1;
                        continue;
                    }

            let mut target = dest_dir.join(&name);

            // Guard: source and target resolve to the same path.
            let same_path = match (&src_canon, target.canonicalize().ok()) {
                (Some(a), Some(b)) => *a == b,
                _ => false,
            };
            if same_path {
                match kind {
                    TransferKind::Copy => { target = unique_destination(&dest_dir, &name); }
                    TransferKind::Move => { skipped += 1; continue; }
                }
            }

            // Conflict resolution.
            let mut merge_dirs = false;
            if target.exists() {
                let target_is_symlink = target.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);
                let both_dirs = src.is_dir() && target.is_dir() && !target_is_symlink;
                let choice = match apply_to_all {
                    Some(c) => c,
                    None => {
                        let (c, all) = ask_conflict(&parent, &name, both_dirs).await;
                        if all { apply_to_all = Some(c); }
                        c
                    }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => { target = unique_destination(&dest_dir, &name); }
                    _ => {
                        if both_dirs {
                            // Directory-into-directory is always merged, never destructively
                            // replaced (even under "apply to all"), to avoid losing unique
                            // destination files.
                            merge_dirs = true;
                        } else {
                            let t = target.clone();
                            let removed = gio::spawn_blocking(move || {
                                if t.is_dir() { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) }
                            }).await;
                            if !matches!(removed, Ok(Ok(()))) {
                                manager.send_toast(&format!("Could not replace {name}"));
                                failed += 1;
                                continue;
                            }
                        }
                    }
                }
            }

            let src_c = src.clone();
            let target_c = target.clone();
            let res = gio::spawn_blocking(move || -> std::io::Result<()> {
                match kind {
                    TransferKind::Copy => copy_recursive(&src_c, &target_c),
                    TransferKind::Move => {
                        if merge_dirs || target_c.exists() {
                            copy_recursive(&src_c, &target_c)?;
                            if src_c.is_dir() { std::fs::remove_dir_all(&src_c) } else { std::fs::remove_file(&src_c) }
                        } else {
                            match std::fs::rename(&src_c, &target_c) {
                                Ok(()) => Ok(()),
                                Err(e) if is_cross_device(&e) => {
                                    copy_recursive(&src_c, &target_c)?;
                                    if src_c.is_dir() { std::fs::remove_dir_all(&src_c) } else { std::fs::remove_file(&src_c) }
                                }
                                Err(e) => Err(e),
                            }
                        }
                    }
                }
            }).await;

            match res {
                Ok(Ok(())) => done += 1,
                _ => { failed += 1; manager.send_toast(&format!("Failed to transfer {name}")); }
            }
        }

        let verb = if kind == TransferKind::Move { "Moved" } else { "Copied" };
        let mut extra = String::new();
        if skipped > 0 { extra.push_str(&format!(", skipped {skipped}")); }
        if failed > 0 { extra.push_str(&format!(", {failed} failed")); }
        manager.send_toast(&format!("{verb} {done} item(s){extra}"));
        manager.refresh();
    });
}

/// Show the collision dialog; returns (choice, apply_to_all).
/// `merge` relabels the overwrite action "Merge" (directory-into-directory).
async fn ask_conflict(parent: &gtk::Window, name: &str, merge: bool) -> (&'static str, bool) {
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

    let check = gtk::CheckButton::with_label("Apply to all remaining");
    check.set_margin_top(8);
    check.set_margin_start(12);
    check.set_margin_end(12);
    dialog.set_extra_child(Some(&check));

    let response = dialog.choose_future(Some(parent)).await;
    let choice: &'static str = match response.as_str() {
        "replace" => "replace",
        "keep" => "keep",
        _ => "skip",
    };
    (choice, check.is_active())
}

/// Move a batch of paths to the trash. One summary toast; collapses columns per success.
pub fn trash(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    glib::spawn_future_local(async move {
        let mut done = 0usize;
        let mut failed = 0usize;
        for path in &paths {
            let file = gio::File::for_path(path);
            match file.trash_future(glib::Priority::DEFAULT).await {
                Ok(_) => { done += 1; manager.on_file_deleted(path); }
                Err(_) => failed += 1,
            }
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
                Ok(Ok(())) => { done += 1; manager.on_file_deleted(path); }
                _ => failed += 1,
            }
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
    let spawned = Command::new("open").arg("-a").arg("Terminal").arg(&dir).spawn().is_ok();

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
            if Command::new(&cmd).args(&args).current_dir(&dir).spawn().is_ok() { ok = true; break; }
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
    let name = dedupe_file_name(&archive_name(&paths), |n| dir.join(n).exists());
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
        let name = rel.to_string_lossy().trim_start_matches('/').to_string();
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
    let paths: Vec<PathBuf> = paths.into_iter().filter(|p| p.is_file()).collect();
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
        if cmd.spawn().is_err() { manager.send_toast("Could not open Mail"); }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = Command::new("xdg-email");
        for p in &paths { cmd.arg("--attach").arg(p); }
        if cmd.spawn().is_err() { manager.send_toast("xdg-email not available"); }
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
        let link_name = dedupe_file_name(&format!("{name} link"), |n| dir.join(n).exists());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_no_collision_returns_original() {
        assert_eq!(dedupe_file_name("a.txt", |_| false), "a.txt");
    }

    #[test]
    fn dedupe_first_collision_adds_copy() {
        assert_eq!(dedupe_file_name("a.txt", |n| n == "a.txt"), "a (copy).txt");
    }

    #[test]
    fn dedupe_second_collision_numbers() {
        let taken = |n: &str| n == "a.txt" || n == "a (copy).txt";
        assert_eq!(dedupe_file_name("a.txt", taken), "a (copy 2).txt");
    }

    #[test]
    fn dedupe_dotfile_has_no_extension() {
        assert_eq!(dedupe_file_name(".bashrc", |n| n == ".bashrc"), ".bashrc (copy)");
    }

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
        assert_eq!(split_name("a.tar.gz"), ("a.tar".to_string(), ".gz".to_string()));
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
        assert_eq!(unique_destination(&tmp, "x.txt"), tmp.join("x (copy).txt"));
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
}
