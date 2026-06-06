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
