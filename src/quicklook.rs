use gio::prelude::FileExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// Resolve the native-previewer command (argv) to spawn for `path`, or None to
/// signal "no native previewer — fall back to our GTK window". Availability is
/// passed in so this is testable on any host.
pub fn native_preview_command(path: &Path, has_qlmanage: bool, has_sushi: bool) -> Option<Vec<String>> {
    if cfg!(target_os = "macos") {
        // Non-UTF-8 paths return None → caller falls back to the GTK preview
        // (avoids lossy-converting and handing qlmanage a corrupted path).
        if has_qlmanage {
            path.to_str()
                .map(|s| vec!["qlmanage".to_string(), "-p".to_string(), s.to_string()])
        } else {
            None
        }
    } else if has_sushi {
        let uri = gio::File::for_path(path).uri().to_string();
        Some(vec!["sushi".to_string(), uri])
    } else {
        None
    }
}

/// True if `name` is found in any $PATH entry as a file.
#[allow(dead_code)]
pub fn binary_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

/// Try to open a native preview for `path`. Returns true if a native previewer was
/// launched, false if the caller should fall back to the GTK preview window.
#[allow(dead_code)]
pub fn open_native_preview(path: &Path) -> bool {
    let (has_qlmanage, has_sushi) = if cfg!(target_os = "macos") {
        (binary_on_path("qlmanage"), false)
    } else {
        (false, binary_on_path("sushi"))
    };

    let Some(argv) = native_preview_command(path, has_qlmanage, has_sushi) else {
        return false;
    };

    let mut it = argv.into_iter();
    let Some(cmd) = it.next() else { return false };
    let args: Vec<String> = it.collect();

    // Fire-and-forget: the Child handle is intentionally dropped (the native
    // previewer owns its own window/lifecycle); we don't track or wait on it.
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_qlmanage_when_available() {
        assert_eq!(
            native_preview_command(Path::new("/tmp/x.txt"), true, false),
            Some(vec!["qlmanage".to_string(), "-p".to_string(), "/tmp/x.txt".to_string()])
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_none_without_qlmanage() {
        assert_eq!(native_preview_command(Path::new("/tmp/x.txt"), false, true), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_uses_sushi_when_available() {
        let cmd = native_preview_command(Path::new("/tmp/x.txt"), false, true).unwrap();
        assert_eq!(cmd[0], "sushi");
        assert!(cmd[1].starts_with("file://"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_none_without_sushi() {
        assert_eq!(native_preview_command(Path::new("/tmp/x.txt"), true, false), None);
    }
}
