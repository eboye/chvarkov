/// xtask — local install/uninstall helper for chvarkov.
///
/// Usage:
///   cargo xtask install          — install to ~/.local (user, no sudo)
///   cargo xtask install --system — install to /usr/local (requires sudo)
///   cargo xtask uninstall
///   cargo xtask uninstall --system

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, exit},
};

const APP_ID: &str = "net.nocopypaste.chvarkov";
const BINARY_NAME: &str = "chvarkov";

fn main() {
    let args: Vec<String> = env::args().collect();
    let task = args.get(1).map(String::as_str).unwrap_or("help");
    let system = args.iter().any(|a| a == "--system");

    match task {
        "install" => install(system),
        "uninstall" => uninstall(system),
        _ => print_help(),
    }
}

/// Print usage help.
fn print_help() {
    eprintln!("Usage:");
    eprintln!("  cargo xtask install           — install to ~/.local (no sudo)");
    eprintln!("  cargo xtask install --system  — install to /usr/local (sudo required)");
    eprintln!("  cargo xtask uninstall");
    eprintln!("  cargo xtask uninstall --system");
}

/// Install the app: binary, schema, .desktop, icon, metainfo.
fn install(system: bool) {
    let prefix = prefix(system);
    println!("Installing {} to {}", BINARY_NAME, prefix.display());

    // 1. Build release binary
    run("cargo", &["build", "--release"]);

    // 2. Copy binary
    let bin_dir = prefix.join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    let src_bin = workspace_root().join("target/release").join(BINARY_NAME);
    let dst_bin = bin_dir.join(BINARY_NAME);
    fs::copy(&src_bin, &dst_bin).expect("copy binary");
    make_executable(&dst_bin);
    println!("  ✓ binary → {}", dst_bin.display());

    // 3. Compile and install GSettings schema
    let schema_dir = prefix.join("share/glib-2.0/schemas");
    fs::create_dir_all(&schema_dir).expect("create schema dir");
    let src_schema = workspace_root().join(format!("{}.gschema.xml", APP_ID));
    let dst_schema = schema_dir.join(format!("{}.gschema.xml", APP_ID));
    fs::copy(&src_schema, &dst_schema).expect("copy schema");
    run("glib-compile-schemas", &[schema_dir.to_str().unwrap()]);
    println!("  ✓ schema → {}", dst_schema.display());

    // 4. Install .desktop file
    let app_dir = prefix.join("share/applications");
    fs::create_dir_all(&app_dir).expect("create applications dir");
    let src_desktop = data_dir().join(format!("{}.desktop", APP_ID));
    let dst_desktop = app_dir.join(format!("{}.desktop", APP_ID));
    fs::copy(&src_desktop, &dst_desktop).expect("copy .desktop");
    println!("  ✓ .desktop → {}", dst_desktop.display());

    // 5. Install SVG icon
    let icon_dir = prefix.join("share/icons/hicolor/scalable/apps");
    fs::create_dir_all(&icon_dir).expect("create icon dir");
    let src_icon = data_dir().join(format!("{}.svg", APP_ID));
    let dst_icon = icon_dir.join(format!("{}.svg", APP_ID));
    fs::copy(&src_icon, &dst_icon).expect("copy icon");
    println!("  ✓ icon → {}", dst_icon.display());

    // 6. Install metainfo
    let meta_dir = prefix.join("share/metainfo");
    fs::create_dir_all(&meta_dir).expect("create metainfo dir");
    let src_meta = data_dir().join(format!("{}.metainfo.xml", APP_ID));
    let dst_meta = meta_dir.join(format!("{}.metainfo.xml", APP_ID));
    fs::copy(&src_meta, &dst_meta).expect("copy metainfo");
    println!("  ✓ metainfo → {}", dst_meta.display());

    // 7. Update icon cache
    let hicolor = prefix.join("share/icons/hicolor");
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t", hicolor.to_str().unwrap()])
        .status();

    // 8. Update desktop database
    let _ = Command::new("update-desktop-database")
        .arg(app_dir.to_str().unwrap())
        .status();

    println!("\nDone! Run `{}` to launch.", BINARY_NAME);
    if !system {
        println!("(Make sure ~/.local/bin is in your $PATH)");
    }
}

/// Uninstall all installed files.
fn uninstall(system: bool) {
    let prefix = prefix(system);
    println!("Uninstalling {} from {}", BINARY_NAME, prefix.display());

    let paths = [
        prefix.join("bin").join(BINARY_NAME),
        prefix.join(format!("share/glib-2.0/schemas/{}.gschema.xml", APP_ID)),
        prefix.join(format!("share/applications/{}.desktop", APP_ID)),
        prefix.join(format!("share/icons/hicolor/scalable/apps/{}.svg", APP_ID)),
        prefix.join(format!("share/metainfo/{}.metainfo.xml", APP_ID)),
    ];

    for path in &paths {
        if path.exists() {
            fs::remove_file(path).expect("remove file");
            println!("  ✓ removed {}", path.display());
        }
    }

    // Recompile schemas after removing ours
    let schema_dir = prefix.join("share/glib-2.0/schemas");
    if schema_dir.exists() {
        run("glib-compile-schemas", &[schema_dir.to_str().unwrap()]);
    }

    println!("Done.");
}

/// Returns the install prefix depending on --system flag.
fn prefix(system: bool) -> PathBuf {
    if system {
        PathBuf::from("/usr/local")
    } else {
        dirs_home().join(".local")
    }
}

/// Returns the user's home directory.
fn dirs_home() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .expect("$HOME not set")
}

/// Returns the workspace root (parent of xtask/).
fn workspace_root() -> PathBuf {
    // When run as `cargo xtask`, CARGO_MANIFEST_DIR is xtask/
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().expect("workspace root").to_path_buf()
}

/// Returns the data/ directory in the workspace root.
fn data_dir() -> PathBuf {
    workspace_root().join("data")
}

/// Run a command, exit on failure.
fn run(cmd: &str, args: &[&str]) {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));
    if !status.success() {
        eprintln!("Command failed: {} {:?}", cmd, args);
        exit(1);
    }
}

/// Make a file executable (chmod +x).
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(path, perms).expect("chmod");
}
