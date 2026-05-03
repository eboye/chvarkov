# chvarkov 📂

**chvarkov** is a modern, high-performance file manager designed for the GNOME desktop. It brings the efficiency of macOS-style **Miller Columns** to Linux, built from the ground up using **Rust**, **GTK4**, and **Libadwaita**.

<img width="1112" height="659" alt="image" src="https://github.com/user-attachments/assets/06c8a9dc-f8fb-4151-8fbe-7ac5a90f8724" />

## ✨ Features

- **Miller Columns Navigation:** Navigate deep directory structures with ease using side-by-side columns.
- **Native Resizing:** Smoothly resize any column or preview pane using native GTK handles.
- **Live Previews:** Instantly view file details, metadata, and large icons when a file is selected.
- **Modern GNOME UI:** Adheres to the latest GNOME Human Interface Guidelines (HIG) with a clean Libadwaita interface.
- **Adaptive Sidebar:** Automatically collapses into an overlay on smaller screens.
- **GNOME Thumbnails:** Native support for file thumbnails in all view types.
- **Keyboard First:** Fully navigable via keyboard with standard shortcuts.

## ⌨️ Keyboard Shortcuts

| Action | Shortcut |
| :--- | :--- |
| **Quit Application** | `Ctrl` + `Q` |
| **Quick Look Preview** | `Space` |
| **Toggle Sidebar** | `F9` |
| **Toggle Hidden Files** | `Ctrl` + `H` |
| **Toggle Metadata** | `Ctrl` + `M` |
| **Open File/Folder** | `Return` (Enter) |
| **Copy** | `Ctrl` + `C` |
| **Cut** | `Ctrl` + `X` |
| **Paste** | `Ctrl` + `V` |
| **Rename** | `F2` |
| **Create Link** | `Ctrl` + `Shift` + `M` |
| **Move to Trash** | `Delete` |
| **View Properties** | `Alt` + `Return` |
| **Trigger Context Menu** | `Menu` key or `Shift` + `F10` |

## 🚀 Installation

### Option A — AppImage (no dependencies, runs anywhere)

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
  - **Arch Linux:** `sudo pacman -S gtk4 libadwaita gtksourceview5`
  - **Fedora:** `sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel`
  - **Ubuntu/Debian:** `sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev`

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

## 🤝 Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
