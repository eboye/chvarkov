use std::path::PathBuf;
use gtk4 as gtk;
use gtk::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Copy, Cut }

#[derive(Clone)]
pub struct ClipboardOp {
    pub mode: Mode,
    pub paths: Vec<PathBuf>,
}

// --- Pure helpers (unit-tested; no display needed) ---

/// `file://` URIs for each path.
fn uris_for(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| gio::File::for_path(p).uri().to_string()).collect()
}

/// `text/uri-list` payload (CRLF-separated).
fn uri_list(paths: &[PathBuf]) -> String {
    uris_for(paths).join("\r\n")
}

/// `x-special/gnome-copied-files` payload: a `cut`/`copy` verb then the URIs.
fn gnome_copied(paths: &[PathBuf], mode: Mode) -> String {
    let verb = if mode == Mode::Cut { "cut" } else { "copy" };
    format!("{verb}\n{}", uris_for(paths).join("\n"))
}

/// Parse `file://` lines out of clipboard text into paths (ignores other lines).
fn parse_uri_lines(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter(|l| l.starts_with("file://"))
        .filter_map(|l| gio::File::for_uri(l.trim()).path())
        .collect()
}

// --- GDK clipboard I/O (not unit-tested) ---

/// Publish a cut/copy of `paths` to the system clipboard (uri-list +
/// GNOME convention) so Finder / GNOME Files interoperate.
pub fn publish(paths: &[PathBuf], mode: Mode) {
    let Some(display) = gtk::gdk::Display::default() else { return };
    let clipboard = display.clipboard();

    let p_gnome = gtk::gdk::ContentProvider::for_bytes(
        "x-special/gnome-copied-files",
        &glib::Bytes::from(gnome_copied(paths, mode).as_bytes()),
    );
    let p_uris = gtk::gdk::ContentProvider::for_bytes(
        "text/uri-list",
        &glib::Bytes::from(uri_list(paths).as_bytes()),
    );
    let provider = gtk::gdk::ContentProvider::new_union(&[p_gnome, p_uris]);
    let _ = clipboard.set_content(Some(&provider));
}

/// First line `cut` => Mode::Cut; anything else (incl. `copy` or no verb) => Copy.
fn clipboard_mode(text: &str) -> Mode {
    match text.lines().next().map(str::trim) {
        Some("cut") => Mode::Cut,
        _ => Mode::Copy,
    }
}

/// Best-effort read of file paths from the system clipboard (when our internal
/// state is empty, e.g. copied from another app). Reads the GNOME
/// `x-special/gnome-copied-files` MIME first so the cut/copy verb is available.
pub fn read_external<F: Fn(Option<ClipboardOp>) + 'static>(callback: F) {
    let Some(display) = gtk::gdk::Display::default() else { callback(None); return };
    let clipboard = display.clipboard();
    clipboard.read_async(
        &["x-special/gnome-copied-files", "text/uri-list", "text/plain;charset=utf-8"],
        glib::Priority::DEFAULT,
        gio::Cancellable::NONE,
        move |res| {
            let Ok((stream, _mime)) = res else { callback(None); return };
            // 1 MiB is far more than any realistic clipboard URI list.
            stream.read_bytes_async(
                1 << 20,
                glib::Priority::DEFAULT,
                gio::Cancellable::NONE,
                move |res| {
                    let Ok(bytes) = res else { callback(None); return };
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let paths = parse_uri_lines(&text);
                    let op = if paths.is_empty() {
                        None
                    } else {
                        Some(ClipboardOp { mode: clipboard_mode(&text), paths })
                    };
                    callback(op);
                },
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_and_parse_roundtrip() {
        let paths = vec![PathBuf::from("/tmp/a b.txt"), PathBuf::from("/tmp/c.txt")];
        let list = uri_list(&paths);
        assert!(list.contains("file:///tmp/"));
        assert!(list.contains("\r\n"));
        assert_eq!(parse_uri_lines(&list), paths);
    }

    #[test]
    fn gnome_copied_prefixes_mode() {
        let p = [PathBuf::from("/tmp/a")];
        assert!(gnome_copied(&p, Mode::Cut).starts_with("cut\nfile:///tmp/a"));
        assert!(gnome_copied(&p, Mode::Copy).starts_with("copy\nfile:///tmp/a"));
    }

    #[test]
    fn parse_ignores_non_file_lines() {
        let text = "copy\nfile:///tmp/x\nhttp://example.com/y\n";
        assert_eq!(parse_uri_lines(text), vec![PathBuf::from("/tmp/x")]);
    }

    #[test]
    fn clipboard_mode_reads_verb() {
        assert_eq!(clipboard_mode("cut\nfile:///tmp/a"), Mode::Cut);
        assert_eq!(clipboard_mode("copy\nfile:///tmp/a"), Mode::Copy);
        assert_eq!(clipboard_mode("file:///tmp/a"), Mode::Copy); // no verb -> Copy
    }
}
