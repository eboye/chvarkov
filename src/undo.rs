//! Undo/redo: a bounded two-stack history plus the inverse/forward executors.
//! Pure stack logic and the freedesktop `.trashinfo` parser are unit-tested;
//! the filesystem work is kept thin and runs off the UI thread.
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::ColumnManager;

/// What kind of item a `Create` produced (drives how redo re-creates it).
// Variants are constructed by the recording hooks (Tasks 5/6); the executors
// here only match on them.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateKind {
    Folder,
    File,
}

/// One reversible operation. Each variant stores enough to invert AND re-apply.
// Variants are constructed by the recording hooks (Tasks 5/6); the executors
// here only match on them.
#[allow(dead_code)]
#[derive(Debug)]
pub enum UndoOp {
    /// `pairs` are forward `(from, to)`: undo moves `to`→`from`, redo `from`→`to`.
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    Rename { from: PathBuf, to: PathBuf },
    /// Absolute original paths; undo restores from trash, redo re-trashes.
    Trash { originals: Vec<PathBuf> },
    /// `created` are the copies; undo trashes them, redo re-copies `sources`→`created`.
    Copy { sources: Vec<PathBuf>, created: Vec<PathBuf> },
    Duplicate { sources: Vec<PathBuf>, created: Vec<PathBuf> },
    Create { kind: CreateKind, path: PathBuf },
}

impl UndoOp {
    /// Short human label for toast text.
    pub fn describe(&self) -> &'static str {
        match self {
            UndoOp::Move { .. } => "Move",
            UndoOp::Rename { .. } => "Rename",
            UndoOp::Trash { .. } => "Trash",
            UndoOp::Copy { .. } => "Copy",
            UndoOp::Duplicate { .. } => "Duplicate",
            UndoOp::Create { .. } => "Create",
        }
    }
}

/// Two bounded stacks. Pure logic — no I/O, no GTK.
pub struct UndoHistory {
    undo: Vec<UndoOp>,
    redo: Vec<UndoOp>,
    cap: usize,
}

impl UndoHistory {
    pub fn new(cap: usize) -> Self {
        UndoHistory { undo: Vec::new(), redo: Vec::new(), cap }
    }

    /// Record a freshly-performed op: clear redo, push to undo, drop the
    /// oldest undo entry if over `cap`.
    // Called by `ColumnManager::undo_record` from the recording hooks (Tasks 5/6).
    pub fn record(&mut self, op: UndoOp) {
        self.redo.clear();
        self.undo.push(op);
        if self.undo.len() > self.cap {
            self.undo.remove(0);
        }
    }

    pub fn pop_undo(&mut self) -> Option<UndoOp> {
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<UndoOp> {
        self.redo.pop()
    }

    /// Return an op to the undo stack after a successful redo.
    pub fn push_undo(&mut self, op: UndoOp) {
        self.undo.push(op);
    }

    /// Move an op to the redo stack after a successful undo.
    pub fn push_redo(&mut self, op: UndoOp) {
        self.redo.push(op);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Percent-decode a freedesktop trashinfo `Path=` value (RFC2396; `/` is literal).
// Used only by the Linux `restore_from_trash` (and tests); dead on macOS.
#[allow(dead_code)]
fn url_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse a `.trashinfo` body → (decoded original path, deletion-date string).
/// Returns `None` if there is no `[Trash Info]` section with a `Path=` line.
// Used only by the Linux `restore_from_trash` (and tests); dead on macOS.
#[allow(dead_code)]
pub fn parse_trashinfo(body: &str) -> Option<(PathBuf, String)> {
    let mut in_section = false;
    let mut path: Option<PathBuf> = None;
    let mut date = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.eq_ignore_ascii_case("[Trash Info]") {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(v) = line.strip_prefix("Path=") {
            path = Some(PathBuf::from(url_decode(v)));
        } else if let Some(v) = line.strip_prefix("DeletionDate=") {
            date = v.to_string();
        }
    }
    path.map(|p| (p, date))
}

/// Among `(orig, deletion_date, info_file)` entries, the one whose `orig`
/// equals `target` with the lexicographically-greatest ISO-8601 date (newest).
// Used only by the Linux `restore_from_trash` (and tests); dead on macOS.
#[allow(dead_code)]
fn pick_newest<'a>(
    entries: &'a [(PathBuf, String, PathBuf)],
    target: &Path,
) -> Option<&'a (PathBuf, String, PathBuf)> {
    entries
        .iter()
        .filter(|(orig, _, _)| orig == target)
        .max_by(|a, b| a.1.cmp(&b.1))
}

/// A destination that never overwrites: `desired` if free, else a `conflict_name`
/// sibling (`name (2).ext`, …).
fn safe_target(desired: &Path) -> PathBuf {
    if !desired.exists() {
        return desired.to_path_buf();
    }
    match (desired.parent(), desired.file_name().and_then(|n| n.to_str())) {
        (Some(dir), Some(name)) => {
            dir.join(crate::file_ops::conflict_name(name, |n| dir.join(n).exists()))
        }
        _ => desired.to_path_buf(),
    }
}

/// Move `from`→`to`, falling back to copy+remove across devices. Blocking.
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            crate::file_ops::copy_recursive(from, to)?;
            if from.is_dir() {
                std::fs::remove_dir_all(from)
            } else {
                std::fs::remove_file(from)
            }
        }
    }
}

/// Restore `original` from the trash to its place (or a `conflict_name` sibling
/// if occupied). Best-effort, platform-specific. Blocking. Returns the path it
/// landed at on success.
#[cfg(not(target_os = "macos"))]
pub fn restore_from_trash(original: &Path) -> std::io::Result<PathBuf> {
    let trash = trash_dir();
    let info_dir = trash.join("info");
    let files_dir = trash.join("files");

    let mut entries: Vec<(PathBuf, String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&info_dir) {
        for ent in rd.flatten() {
            let info_path = ent.path();
            if info_path.extension().and_then(|e| e.to_str()) != Some("trashinfo") {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(&info_path)
                && let Some((orig, date)) = parse_trashinfo(&body)
            {
                entries.push((orig, date, info_path));
            }
        }
    }

    let Some((_, _, info_path)) = pick_newest(&entries, original) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found in trash",
        ));
    };
    // The data file shares the info file's stem (drops the `.trashinfo`).
    let stem = info_path
        .file_stem()
        .ok_or_else(|| std::io::Error::other("bad trashinfo name"))?;
    let data_file = files_dir.join(stem);
    let dest = safe_target(original);
    move_path(&data_file, &dest)?;
    let _ = std::fs::remove_file(info_path);
    Ok(dest)
}

#[cfg(not(target_os = "macos"))]
fn trash_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("Trash");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".local/share/Trash")
}

/// macOS best-effort: items land in `~/.Trash/<basename>`; move it back if present.
#[cfg(target_os = "macos")]
pub fn restore_from_trash(original: &Path) -> std::io::Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let name = original
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;
    let trashed = PathBuf::from(home).join(".Trash").join(name);
    if !trashed.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found in Trash",
        ));
    }
    let dest = safe_target(original);
    move_path(&trashed, &dest)?;
    Ok(dest)
}

/// Build the result-toast text, e.g. "Undone Move: 2 item(s)" or
/// "Undone Move: 1 item(s), 1 failed".
fn summary(verb: &str, what: &str, ok: usize, fail: usize) -> String {
    if fail > 0 {
        format!("{verb} {what}: {ok} item(s), {fail} failed")
    } else {
        format!("{verb} {what}: {ok} item(s)")
    }
}

/// Move `from`→`to` off the UI thread, never overwriting (`safe_target`).
async fn move_back(to: PathBuf, desired_from: PathBuf) -> bool {
    let dest = safe_target(&desired_from);
    matches!(
        gio::spawn_blocking(move || move_path(&to, &dest)).await,
        Ok(Ok(()))
    )
}

/// Trash a path off the UI thread.
async fn trash_path(p: PathBuf) -> bool {
    gio::File::for_path(&p)
        .trash_future(glib::Priority::DEFAULT)
        .await
        .is_ok()
}

/// Copy `src`→`dst` off the UI thread (symlink-preserving), never overwriting.
async fn copy_path(src: PathBuf, dst: PathBuf) -> bool {
    let dest = safe_target(&dst);
    matches!(
        gio::spawn_blocking(move || crate::file_ops::copy_recursive(&src, &dest)).await,
        Ok(Ok(()))
    )
}

/// Pop the most recent undoable op and execute its inverse. Public entry point
/// for the `app.undo` action and the toast "Undo" button.
pub fn trigger_undo(manager: Rc<ColumnManager>) {
    let op = manager.undo_history.borrow_mut().pop_undo();
    match op {
        None => manager.send_toast("Nothing to undo"),
        Some(op) => perform_undo(manager, op),
    }
}

/// Pop the most recent redoable op and re-apply it.
pub fn trigger_redo(manager: Rc<ColumnManager>) {
    let op = manager.undo_history.borrow_mut().pop_redo();
    match op {
        None => manager.send_toast("Nothing to redo"),
        Some(op) => perform_redo(manager, op),
    }
}

/// Execute the inverse of `op`. On success the op moves to the redo stack and a
/// "Redo" toast is shown.
pub fn perform_undo(manager: Rc<ColumnManager>, op: UndoOp) {
    glib::spawn_future_local(async move {
        let what = op.describe();
        let (mut ok, mut fail) = (0usize, 0usize);
        match &op {
            UndoOp::Move { pairs } => {
                for (from, to) in pairs {
                    if move_back(to.clone(), from.clone()).await { ok += 1 } else { fail += 1 }
                }
            }
            UndoOp::Rename { from, to } => {
                if move_back(to.clone(), from.clone()).await { ok += 1 } else { fail += 1 }
            }
            UndoOp::Trash { originals } => {
                for p in originals {
                    let p = p.clone();
                    match gio::spawn_blocking(move || restore_from_trash(&p)).await {
                        Ok(Ok(_)) => ok += 1,
                        _ => fail += 1,
                    }
                }
            }
            UndoOp::Copy { created, .. } | UndoOp::Duplicate { created, .. } => {
                for p in created {
                    if trash_path(p.clone()).await { ok += 1 } else { fail += 1 }
                }
            }
            UndoOp::Create { path, .. } => {
                if trash_path(path.clone()).await { ok += 1 } else { fail += 1 }
            }
        }
        if ok > 0 {
            manager.undo_history.borrow_mut().push_redo(op);
        }
        manager.refresh_undo_actions();
        // Show the toast before refreshing: refresh() defers an app.activate()
        // that rebuilds the window content (and its ToastOverlay), so a toast
        // added after it would be lost. Same order as the file_ops callers.
        let m = manager.clone();
        manager.send_toast_action(&summary("Undone", what, ok, fail), "Redo", move || {
            trigger_redo(m.clone())
        });
        manager.refresh();
    });
}

/// Re-apply `op` (forward). On success the op moves back to the undo stack.
pub fn perform_redo(manager: Rc<ColumnManager>, op: UndoOp) {
    glib::spawn_future_local(async move {
        let what = op.describe();
        let (mut ok, mut fail) = (0usize, 0usize);
        match &op {
            UndoOp::Move { pairs } => {
                for (from, to) in pairs {
                    if move_back(from.clone(), to.clone()).await { ok += 1 } else { fail += 1 }
                }
            }
            UndoOp::Rename { from, to } => {
                if move_back(from.clone(), to.clone()).await { ok += 1 } else { fail += 1 }
            }
            UndoOp::Trash { originals } => {
                for p in originals {
                    if trash_path(p.clone()).await { ok += 1 } else { fail += 1 }
                }
            }
            UndoOp::Copy { sources, created } | UndoOp::Duplicate { sources, created } => {
                for (s, d) in sources.iter().zip(created.iter()) {
                    if copy_path(s.clone(), d.clone()).await { ok += 1 } else { fail += 1 }
                }
            }
            UndoOp::Create { kind, path } => {
                let path = path.clone();
                let kind = *kind;
                let res = gio::spawn_blocking(move || match kind {
                    CreateKind::Folder => std::fs::create_dir(&path),
                    CreateKind::File => std::fs::File::create(&path).map(|_| ()),
                })
                .await;
                if matches!(res, Ok(Ok(()))) { ok += 1 } else { fail += 1 }
            }
        }
        if ok > 0 {
            manager.undo_history.borrow_mut().push_undo(op);
        }
        manager.refresh_undo_actions();
        // Show the toast before refreshing (see perform_undo for why).
        let m = manager.clone();
        manager.send_toast_action(&summary("Redone", what, ok, fail), "Undo", move || {
            trigger_undo(m.clone())
        });
        manager.refresh();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ren(a: &str, b: &str) -> UndoOp {
        UndoOp::Rename { from: PathBuf::from(a), to: PathBuf::from(b) }
    }

    #[test]
    fn record_pushes_and_bounds() {
        let mut h = UndoHistory::new(2);
        h.record(ren("a", "b"));
        h.record(ren("c", "d"));
        h.record(ren("e", "f")); // over cap 2 → oldest dropped
        assert!(h.pop_undo().is_some());
        assert!(h.pop_undo().is_some());
        assert!(h.pop_undo().is_none());
    }

    #[test]
    fn undo_then_redo_roundtrip() {
        let mut h = UndoHistory::new(8);
        h.record(ren("a", "b"));
        let op = h.pop_undo().unwrap();
        assert!(!h.can_undo());
        h.push_redo(op);
        assert!(h.can_redo());
        let op = h.pop_redo().unwrap();
        h.push_undo(op);
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn record_clears_redo() {
        let mut h = UndoHistory::new(8);
        h.record(ren("a", "b"));
        let op = h.pop_undo().unwrap();
        h.push_redo(op);
        assert!(h.can_redo());
        h.record(ren("x", "y"));
        assert!(!h.can_redo());
    }

    #[test]
    fn parse_trashinfo_decodes_path() {
        let body = "[Trash Info]\nPath=/home/u/My%20Docs/a.txt\nDeletionDate=2026-06-07T10:00:00\n";
        let (p, d) = parse_trashinfo(body).unwrap();
        assert_eq!(p, PathBuf::from("/home/u/My Docs/a.txt"));
        assert_eq!(d, "2026-06-07T10:00:00");
    }

    #[test]
    fn parse_trashinfo_rejects_malformed() {
        assert!(parse_trashinfo("garbage\nno path here\n").is_none());
    }

    #[test]
    fn pick_newest_selects_latest_match() {
        let entries = vec![
            (PathBuf::from("/a"), "2026-01-01T00:00:00".to_string(), PathBuf::from("i1")),
            (PathBuf::from("/a"), "2026-06-01T00:00:00".to_string(), PathBuf::from("i2")),
            (PathBuf::from("/b"), "2026-09-01T00:00:00".to_string(), PathBuf::from("i3")),
        ];
        let best = pick_newest(&entries, Path::new("/a")).unwrap();
        assert_eq!(best.2, PathBuf::from("i2"));
    }

    #[test]
    fn safe_target_avoids_overwrite() {
        let tmp = std::env::temp_dir().join(format!("chv_undo_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let free = tmp.join("free.txt");
        assert_eq!(safe_target(&free), free); // unoccupied → unchanged
        let taken = tmp.join("taken.txt");
        std::fs::write(&taken, b"x").unwrap();
        let alt = safe_target(&taken); // occupied → conflict_name sibling
        assert_ne!(alt, taken);
        assert!(alt.file_name().unwrap().to_str().unwrap().contains("(2)"));
        std::fs::remove_dir_all(&tmp).ok();
    }
}
