# chvarkov 📂

**chvarkov** is a modern, high-performance file manager designed for the GNOME desktop and macOS. It brings the efficiency of macOS-style **Miller Columns** to both platforms, built from the ground up using **Rust**, **GTK4**, and **Libadwaita**.

<img width="1112" height="659" alt="image" src="https://github.com/user-attachments/assets/06c8a9dc-f8fb-4151-8fbe-7ac5a90f8724" />

## ✨ Features

### Navigation & UI
- **Miller Columns Navigation:** Navigate deep directory structures with ease using side-by-side columns (plus Icon and List views).
- **Cross-Platform:** Native support for both **Linux (GNOME)** and **macOS**.
- **Native Resizing:** Smoothly resize any column or preview pane using native handles.
- **Live Previews:** Selecting a file opens a preview docked to the right edge — file details, metadata, large icons, images, video, and syntax-highlighted text — staying pinned in place as you navigate. Press `Space` for a larger floating Quick Look.
- **Modern UI:** Adheres to Libadwaita standards for a clean, responsive interface, with the sidebar, header bar, breadcrumb, and footer aligned to a single consistent height.
- **Adaptive Sidebar:** Automatically collapses into an overlay on smaller screens.
- **Native Thumbnails:** Native support for file thumbnails in all view types.
- **Keyboard First & Multi-Selection:** Fully navigable via keyboard, with standard multi-selection (Shift, Ctrl, Ctrl+A).

### File Operations
- **Full Cut / Copy / Paste** with **system-clipboard interop** — copy in chvarkov and paste in Finder / GNOME Files (and vice-versa).
- **New Folder** and **New Empty File** (with inline naming), **Duplicate**, **Move to… / Copy to…**, **Rename**, **Create Link** (symlink), and **Compress to Zip**.
- **Move to Trash** and **Delete Permanently** (with a confirmation prompt), all operating on the whole selection.
- **Undo / Redo** (`Ctrl/Cmd+Z`, `Ctrl+Shift+Z`, plus an **Undo** button on the result toast) for move, rename, trash, copy, duplicate, and create — a bounded history of the last 16 operations. Undoing a copy/duplicate/create sends the new items to Trash so nothing is lost; undo never overwrites (a clashing original is restored as `name (2)`).
- **Open in Terminal**, **Copy Path / URI / Name**, and **Email / Share** (native macOS Share sheet; `xdg-email` on Linux).

### Safe by design
- **Permission-aware:** actions you don't have rights to perform are hidden from the menu (driven by the filesystem's reported capabilities).
- **Conflict handling:** Replace / Skip / Keep Both on name collisions, with **directory Merge** instead of destructive overwrite, and an "apply to all" option for batches.
- **Data-loss guards** (modeled on GNOME Nautilus): refuses to copy/move a folder into itself, never silently replaces a directory, preserves symlinks instead of dereferencing them, and validates rename input.

### Linux integration
- **Default file manager:** can be registered to open folders via `xdg-open` and "Open With" (see [Set as default file manager](#-set-as-default-file-manager-linux)).

## ⌨️ Keyboard Shortcuts

| Action | Shortcut |
| :--- | :--- |
| **Quit Application** | `Ctrl` + `Q` (Linux) / `Cmd` + `Q` (macOS) |
| **Quick Look Preview** | `Space` (macOS QuickLook / GNOME Sushi when available, else built-in preview) |
| **Toggle Sidebar** | `F9` |
| **Toggle Hidden Files** | `Ctrl` + `H` (Linux) / `Cmd` + `H` (macOS) |
| **Toggle Metadata** | `Ctrl` + `M` (Linux) / `Cmd` + `M` (macOS) |
| **Open File/Folder** | `Return` (Enter) |
| **Copy** | `Ctrl` + `C` (Linux) / `Cmd` + `C` (macOS) |
| **Cut** | `Ctrl` + `X` (Linux) / `Cmd` + `X` (macOS) |
| **Paste** | `Ctrl` + `V` (Linux) / `Cmd` + `V` (macOS) |
| **Rename** | `F2` |
| **Duplicate** | `Ctrl` + `D` (Linux) / `Cmd` + `D` (macOS) |
| **Undo** | `Ctrl` + `Z` (Linux) / `Cmd` + `Z` (macOS) |
| **Redo** | `Ctrl` + `Shift` + `Z` (Linux) / `Cmd` + `Shift` + `Z` (macOS) |
| **New Folder** | `Ctrl` + `Shift` + `N` |
| **New Empty File** | `Ctrl` + `N` |
| **Create Link** | `Ctrl` + `Shift` + `M` |
| **Move to Trash** | `Delete` (Linux) / `Cmd` + `Delete` (macOS) |
| **Delete Permanently** | `Shift` + `Delete` |
| **Select All** | `Ctrl` + `A` |
| **Preferences** | `Ctrl` + `,` |
| **Zoom In / Out** | `Ctrl` + `+` / `Ctrl` + `-` |
| **View Properties** | `Alt` + `Return` |
| **Trigger Context Menu** | `Menu` key or `Shift` + `F10` |

> Additional operations — Move to…, Copy to…, Compress, Open in Terminal, Copy Path/URI/Name, Email, Share — are available from the right-click context menu (only the entries you have permission to perform are shown).

## 🚀 Installation

### macOS (Homebrew)

Download `Chvarkov-macos-aarch64.zip` (Apple Silicon / M-series) from the [Releases Page](https://github.com/eboye/chvarkov/releases), extract it, and move `Chvarkov.app` to your `/Applications` folder.

> **Intel Mac?** GitHub no longer offers Intel macOS CI runners, so prebuilt Intel binaries aren't published. On an Intel Mac you can build one yourself with `./build.sh macos` (see [Building from Source](#-building-from-source)).

> **"Chvarkov.app is damaged and can't be opened"?** The app is ad-hoc signed but not notarized (no paid Apple Developer account), so macOS quarantines it on download. Clear the quarantine flag once after moving it to `/Applications`:
> ```bash
> xattr -dr com.apple.quarantine /Applications/Chvarkov.app
> ```
> Then open it normally.

**Note:** You must have the system dependencies installed:
```bash
brew install pkg-config gtk4 libadwaita adwaita-icon-theme gtksourceview5
```

For **video previews** on macOS, GStreamer is also required:
```bash
brew install gstreamer gst-plugins-base gst-plugins-good gst-libav
```

---

### Option A — AppImage (Linux, no dependencies)

Download `chvarkov-linux-amd64.AppImage` from the [Releases Page](https://github.com/eboye/chvarkov/releases), then:

```bash
chmod +x chvarkov-linux-amd64.AppImage
./chvarkov-linux-amd64.AppImage
```

All GTK4/Adwaita/GtkSourceView libraries are bundled inside the AppImage.

---

### Option B — Flatpak

Download `chvarkov-linux-amd64.flatpak` from the [Releases Page](https://github.com/eboye/chvarkov/releases), then:

```bash
flatpak install --user chvarkov-linux-amd64.flatpak
flatpak run net.nocopypaste.chvarkov
```

### Option B — macOS .app (Local)

Requires Homebrew dependencies listed in Prerequisites.

```bash
./build.sh macos
```

This creates `Chvarkov.app` in the project root.

---

### Option C — Binary tarball

Requires GTK4, Libadwaita and GtkSourceView 5 to be installed on your system:

#### **Arch Linux**
```bash
sudo pacman -S gtk4 libadwaita gtksourceview5
```

#### **Fedora**
```bash
sudo dnf install gtk4 libadwaita gtksourceview5
```

#### **Debian / Ubuntu**
```bash
sudo apt install libgtk-4-1 libadwaita-1-0 libgtksourceview-5-0
```

Then download `chvarkov-linux-amd64.tar.gz`, extract and run:

```bash
tar -xzf chvarkov-linux-amd64.tar.gz
./chvarkov
```

---

## 🛠 Building from Source

### Prerequisites

- **Rust:** [Install Rust](https://www.rust-lang.org/tools/install)
- **Development headers:**
  - **macOS (Homebrew):** `brew install pkg-config gtk4 libadwaita adwaita-icon-theme gtksourceview5 gstreamer gst-plugins-base gst-plugins-good gst-libav`
  - **Arch Linux:** `sudo pacman -S gtk4 libadwaita gtksourceview5 gst-plugins-good`
  - **Fedora:** `sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel gstreamer1-plugins-good`
  - **Ubuntu/Debian:** `sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libgstreamer1.0-dev gstreamer1.0-plugins-good`

### Install system-wide (no sudo — goes to `~/.local`)

```bash
git clone https://github.com/eboye/chvarkov.git
cd chvarkov
cargo xtask install
```

This builds the binary, installs it to `~/.local/bin/chvarkov`, registers the GSettings schema, `.desktop` entry and icon. Make sure `~/.local/bin` is in your `$PATH`.

To install to `/usr/local` instead (requires sudo):

```bash
sudo cargo xtask install --system
```

To uninstall:

```bash
cargo xtask uninstall          # from ~/.local
sudo cargo xtask uninstall --system
```

### Build and run without installing

```bash
cargo run --release
```

## 🛠 Development

### Automatic Recompilation (Hot Reload)

For a development experience similar to `npm watch`, we recommend using `cargo-watch`.

1. Install `cargo-watch`:
   ```bash
   cargo install cargo-watch
   ```

2. Run the watch command:
   ```bash
   cargo watch -c -x run
   ```

## 🗂 Set as default file manager (Linux)

After installing (`cargo xtask install`), register chvarkov as the handler for folders so it opens from `xdg-open`, file links, and the "Open With" menu:

```bash
xdg-mime default net.nocopypaste.chvarkov.desktop inode/directory
```

Then opening a folder launches chvarkov at that path:

```bash
xdg-open ~/Downloads
```

## 🤝 Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
