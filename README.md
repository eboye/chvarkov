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

### 1. Install Dependencies

Before running the pre-built binary or building from source, ensure your system has the required libraries installed:

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
sudo apt update
sudo apt install libgtk-4-1 libadwaita-1-0 libgtksourceview-5-0
```

### 2. Download and Run

1.  Download the latest release from the [Releases Page](https://github.com/eboye/chvarkov/releases).
2.  Extract the archive:
    ```bash
    tar -xzf chvarkov-linux-amd64.tar.gz
    ```
3.  Run the application:
    ```bash
    ./chvarkov
    ```

---

## 🛠 Building from Source

If you prefer to build chvarkov yourself, follow these steps:

### Prerequisites

You will need the Rust toolchain and development headers for the dependencies:

- **Rust:** [Install Rust](https://www.rust-lang.org/tools/install)
- **Development Headers:**
  - **Fedora:** `sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel`
  - **Ubuntu/Debian:** `sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev`
  - **Arch Linux:** `sudo pacman -S gtk4 libadwaita gtksourceview5`

### Build Steps

1. Clone the repository:
   ```bash
   git clone https://github.com/eboye/chvarkov.git
   cd chvarkov
   ```

2. Build and run:
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
