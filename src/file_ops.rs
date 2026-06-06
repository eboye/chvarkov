use std::path::{Path, PathBuf};

/// Split a file name into (stem, extension-with-dot). Leading-dot files
/// (".bashrc") are treated as having no extension.
fn split_name(file_name: &str) -> (String, String) {
    if let Some(idx) = file_name.rfind('.') {
        if idx > 0 {
            return (file_name[..idx].to_string(), file_name[idx..].to_string());
        }
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

/// Compute a non-colliding destination path inside `dir` for `file_name`.
#[allow(dead_code)]
pub fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let name = dedupe_file_name(file_name, |n| dir.join(n).exists());
    dir.join(name)
}

/// Recursively copy `src` to `dst` (file or directory). `dst` is the full
/// target path (not a parent directory).
#[allow(dead_code)]
pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(())
    }
}

use std::rc::Rc;
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use crate::ColumnManager;

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TransferKind { Copy, Move }

/// Move or copy a batch of source paths into `dest_dir`, prompting on collisions.
/// Reports via the manager's toast overlay and refreshes the UI when done.
#[allow(dead_code)]
pub fn transfer(
    manager: Rc<ColumnManager>,
    parent: gtk::Window,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    kind: TransferKind,
) {
    glib::spawn_future_local(async move {
        let mut apply_to_all: Option<&'static str> = None; // "replace" | "skip" | "keep"
        let mut done = 0usize;
        let mut skipped = 0usize;

        for src in sources {
            let Some(name) = src.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let mut target = dest_dir.join(&name);

            if target.exists() {
                let choice = match apply_to_all {
                    Some(c) => c,
                    None => {
                        let (c, all) = ask_conflict(&parent, &name).await;
                        if all { apply_to_all = Some(c); }
                        c
                    }
                };
                match choice {
                    "skip" => { skipped += 1; continue; }
                    "keep" => { target = unique_destination(&dest_dir, &name); }
                    _ => { /* replace: remove existing first */
                        let t = target.clone();
                        let _ = gio::spawn_blocking(move || {
                            if t.is_dir() { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) }
                        }).await;
                    }
                }
            }

            let src_c = src.clone();
            let target_c = target.clone();
            let res = gio::spawn_blocking(move || -> std::io::Result<()> {
                match kind {
                    TransferKind::Copy => copy_recursive(&src_c, &target_c),
                    TransferKind::Move => {
                        match std::fs::rename(&src_c, &target_c) {
                            Ok(()) => Ok(()),
                            Err(_) => {
                                copy_recursive(&src_c, &target_c)?;
                                if src_c.is_dir() { std::fs::remove_dir_all(&src_c) } else { std::fs::remove_file(&src_c) }
                            }
                        }
                    }
                }
            }).await;

            match res {
                Ok(Ok(())) => done += 1,
                _ => manager.send_toast(&format!("Failed to transfer {name}")),
            }
        }

        let verb = if kind == TransferKind::Move { "Moved" } else { "Copied" };
        manager.send_toast(&format!("{verb} {done} item(s){}", if skipped > 0 { format!(", skipped {skipped}") } else { String::new() }));
        manager.refresh();
    });
}

/// Show the collision dialog; returns (choice, apply_to_all).
#[allow(dead_code)]
async fn ask_conflict(parent: &gtk::Window, name: &str) -> (&'static str, bool) {
    let dialog = adw::AlertDialog::builder()
        .heading("Item already exists")
        .body(format!("\u{201c}{name}\u{201d} already exists in the destination. What do you want to do?"))
        .build();
    dialog.add_response("skip", "Skip");
    dialog.add_response("keep", "Keep Both");
    dialog.add_response("replace", "Replace");
    dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
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

/// Move a batch of paths to the trash.
pub fn trash(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    for path in paths {
        let file = gio::File::for_path(&path);
        let manager_c = manager.clone();
        file.trash_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, move |res| {
            match res {
                Ok(_) => { manager_c.send_toast("Moved to Trash"); manager_c.on_file_deleted(&path); }
                Err(e) => manager_c.send_toast(&format!("Error moving to trash: {e}")),
            }
        });
    }
}

/// Permanently delete a batch of paths.
pub fn delete(manager: Rc<ColumnManager>, paths: Vec<PathBuf>) {
    for path in paths {
        let file = gio::File::for_path(&path);
        let manager_c = manager.clone();
        file.delete_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, move |res| {
            match res {
                Ok(_) => { manager_c.send_toast("Deleted permanently"); manager_c.on_file_deleted(&path); }
                Err(e) => manager_c.send_toast(&format!("Error deleting: {e}")),
            }
        });
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
}
