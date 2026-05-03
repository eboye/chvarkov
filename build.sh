#!/usr/bin/env bash
# build.sh — local build helper for chvarkov
#
# Usage:
#   ./build.sh appimage        Build AppImage only
#   ./build.sh flatpak         Build Flatpak bundle only
#   ./build.sh both            Build both
#   ./build.sh                 Interactive menu

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ID="net.nocopypaste.chvarkov"
APPIMAGE_OUT="chvarkov-linux-amd64.AppImage"
FLATPAK_OUT="chvarkov-linux-amd64.flatpak"

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()    { echo -e "${CYAN}==> $*${NC}"; }
success() { echo -e "${GREEN}✔ $*${NC}"; }
warn()    { echo -e "${YELLOW}⚠ $*${NC}"; }
die()     { echo -e "${RED}✖ $*${NC}" >&2; exit 1; }

# ── Dependency checks ─────────────────────────────────────────────────────────
require() {
    command -v "$1" &>/dev/null || die "'$1' not found. $2"
}

# ── AppImage build ────────────────────────────────────────────────────────────
build_appimage() {
    info "Building AppImage inside Fedora 41 container..."
    require docker "Install Docker: https://docs.docker.com/engine/install/"

    docker run --rm --privileged \
        -v "${REPO_ROOT}:/workspace" -w /workspace \
        fedora:41 bash -c '
set -euo pipefail

info()    { echo "==> $*"; }
info "Installing system dependencies..."
dnf install -y -q \
    gtk4-devel libadwaita-devel gtksourceview5-devel \
    glib2-devel pkg-config gcc curl wget fuse \
    desktop-file-utils librsvg2 gdk-pixbuf2 patchelf file

info "Installing Rust toolchain..."
curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable -q
source "$HOME/.cargo/env"

info "Building release binary..."
cargo build --release

info "Downloading appimagetool..."
wget -qO /usr/local/bin/appimagetool \
    https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x /usr/local/bin/appimagetool

info "Assembling AppDir..."
rm -rf AppDir
mkdir -p AppDir/usr/bin \
         AppDir/usr/lib \
         AppDir/usr/share/glib-2.0/schemas \
         AppDir/usr/share/applications \
         AppDir/usr/share/icons/hicolor/scalable/apps \
         AppDir/usr/share/metainfo

cp target/release/chvarkov       AppDir/usr/bin/chvarkov
cp AppRun                         AppDir/AppRun && chmod +x AppDir/AppRun
cp net.nocopypaste.chvarkov.gschema.xml AppDir/usr/share/glib-2.0/schemas/
glib-compile-schemas AppDir/usr/share/glib-2.0/schemas/
cp data/net.nocopypaste.chvarkov.desktop     AppDir/usr/share/applications/
cp data/net.nocopypaste.chvarkov.desktop     AppDir/
cp data/net.nocopypaste.chvarkov.svg         AppDir/usr/share/icons/hicolor/scalable/apps/
cp data/net.nocopypaste.chvarkov.svg         AppDir/
cp data/net.nocopypaste.chvarkov.metainfo.xml AppDir/usr/share/metainfo/
ln -sf net.nocopypaste.chvarkov.svg AppDir/.DirIcon

info "Bundling shared libraries..."
EXCLUDE="^lib(c|m|dl|rt|pthread|resolv|nss_|nsl|util|mvec|ld-linux|ld-musl|BrokenLocale|anl|cidn|crypt|nss|z)\."
bundle_libs() {
    ldd "$1" 2>/dev/null | awk "/=> \// {print \$3}" | grep -v "^$" | while read -r lib; do
        base="$(basename "$lib")"
        echo "$base" | grep -qE "$EXCLUDE" && continue
        cp -n "$lib" AppDir/usr/lib/ 2>/dev/null || true
    done
}
bundle_libs AppDir/usr/bin/chvarkov
find AppDir/usr/lib -name "*.so*" -type f | while read -r f; do bundle_libs "$f"; done

info "Packaging AppImage..."
/usr/local/bin/appimagetool --appimage-extract-and-run AppDir chvarkov-linux-amd64.AppImage
chmod 755 chvarkov-linux-amd64.AppImage
echo "✔ AppImage built successfully"
'

    success "AppImage ready: ${REPO_ROOT}/${APPIMAGE_OUT}"
}

# ── Flatpak build ─────────────────────────────────────────────────────────────
build_flatpak() {
    info "Building Flatpak bundle..."
    require flatpak         "Install with: sudo pacman -S flatpak  (or your distro equivalent)"
    require flatpak-builder "Install with: sudo pacman -S flatpak-builder"

    # Ensure the GNOME SDK runtime is installed
    if ! flatpak info org.gnome.Sdk//47 &>/dev/null; then
        info "Installing GNOME SDK runtime 47..."
        flatpak install -y --noninteractive flathub org.gnome.Platform//47 org.gnome.Sdk//47 \
            || die "Could not install GNOME SDK. Run: flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo"
    fi

    local build_dir="${REPO_ROOT}/.flatpak-builder-local"
    info "Building (this may take a while on first run)..."
    flatpak-builder --force-clean --user \
        --state-dir="${build_dir}/state" \
        --repo="${build_dir}/repo" \
        "${build_dir}/build" \
        "${REPO_ROOT}/${APP_ID}.yml"

    info "Exporting bundle..."
    flatpak build-bundle \
        "${build_dir}/repo" \
        "${REPO_ROOT}/${FLATPAK_OUT}" \
        "${APP_ID}"

    success "Flatpak ready: ${REPO_ROOT}/${FLATPAK_OUT}"
}

# ── macOS build ───────────────────────────────────────────────────────────────
build_macos() {
    info "Building macOS .app bundle..."
    
    # Check for macOS
    if [[ "$OSTYPE" != "darwin"* ]]; then
        warn "This target is intended for macOS. Proceeding anyway..."
    fi

    require glib-compile-schemas "Install with: brew install glib"

    APP_NAME="Chvarkov"
    APP_DIR="${REPO_ROOT}/${APP_NAME}.app"
    CONTENTS_DIR="${APP_DIR}/Contents"
    MACOS_DIR="${CONTENTS_DIR}/MacOS"
    VERSION=$(grep "^version =" Cargo.toml | head -n1 | cut -d'"' -f2 || echo "0.1.0")

    info "Building release binary..."
    cargo build --release

    info "Assembling .app structure..."
    rm -rf "${APP_DIR}"
    mkdir -p "${MACOS_DIR}/compiled_schemas"

    cp "target/release/chvarkov" "${MACOS_DIR}/chvarkov"
    
    # Compile schemas directly into the bundle
    glib-compile-schemas . --targetdir="${MACOS_DIR}/compiled_schemas"

    # Create Info.plist
    cat > "${CONTENTS_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>chvarkov</string>
    <key>CFBundleIdentifier</key>
    <string>${APP_ID}</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

    success "macOS .app ready: ${APP_DIR}"
}

# ── Menu / argument dispatch ──────────────────────────────────────────────────
run_menu() {
    echo ""
    echo -e "${CYAN}chvarkov local build${NC}"
    echo "────────────────────"
    echo "  1) AppImage (Linux)"
    echo "  2) Flatpak (Linux)"
    echo "  3) macOS .app"
    echo "  4) All"
    echo "  q) Quit"
    echo ""
    read -rp "Choice: " choice
    case "$choice" in
        1) build_appimage ;;
        2) build_flatpak ;;
        3) build_macos ;;
        4) build_appimage; build_flatpak; build_macos ;;
        q|Q) exit 0 ;;
        *) die "Invalid choice" ;;
    esac
}

case "${1:-}" in
    appimage) build_appimage ;;
    flatpak)  build_flatpak ;;
    macos)    build_macos ;;
    both)     build_appimage; build_flatpak ;;
    all)      build_appimage; build_flatpak; build_macos ;;
    "")       run_menu ;;
    *)        die "Unknown target '${1}'. Use: appimage | flatpak | macos | all" ;;
esac
