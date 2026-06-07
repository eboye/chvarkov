//! Undo/redo: a bounded two-stack history plus the inverse/forward executors.
//! Pure stack logic and the freedesktop `.trashinfo` parser are unit-tested;
//! the filesystem work is kept thin and runs off the UI thread.
#![allow(dead_code)]
#![allow(unused_imports)]

use std::path::{Path, PathBuf};

/// What kind of item a `Create` produced (drives how redo re-creates it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateKind {
    Folder,
    File,
}

/// One reversible operation. Each variant stores enough to invert AND re-apply.
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
}
