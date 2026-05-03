# chvarkov 📂

**chvarkov** is a modern, high-performance file manager designed for the GNOME desktop. It brings the efficiency of macOS-style **Miller Columns** to Linux, built from the ground up using **Rust**, **GTK4**, and **Libadwaita**.

<img width="1112" height="659" alt="image" src="https://github.com/user-attachments/assets/06c8a9dc-f8fb-4151-8fbe-7ac5a90f8724" />

## ✨ Features

- **Miller Columns Navigation:** Navigate deep directory structures with ease using side-by-side columns.
- **Native Resizing:** Smoothly resize any column or preview pane using native GTK handles.
- **Live Previews:** Instantly view file details, metadata, and large icons when a file is selected.
- **Modern GNOME UI:** Adheres to the latest GNOME Human Interface Guidelines (HIG) with a clean Libadwaita interface.
- **Metadata Toggle:** Quickly switch between a compact view and a detailed view showing "Last Modified" dates.
- **Hidden Files Toggle:** Easily hide or show dotfiles with a single click.

## ⌨️ Keyboard Shortcuts

| Action | Shortcut |
| :--- | :--- |
| **Quit Application** | `Ctrl` + `Q` |
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

## 🚀 Getting Started

### Prerequisites

To build chvarkov, you need the Rust toolchain and the GTK4/Libadwaita development libraries:

- **Rust:** [Install Rust](https://www.rust-lang.org/tools/install)
- **GTK4 & Libadwaita:** 
  - Fedora: `sudo dnf install gtk4-devel libadwaita-devel`
  - Ubuntu/Debian: `sudo apt install libgtk-4-dev libadwaita-1-dev`
  - Arch Linux: `sudo pacman -S gtk4 libadwaita`

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/arch-finder.git
   cd arch-finder
   ```

2. Build and run:
   ```bash
   cargo run
   ```

## 🛠 Development

### Automatic Recompilation (Hot Reload)

For a development experience similar to `npm watch`, we recommend using `cargo-watch`. It will automatically recompile and restart the app whenever you save a file.

1. Install `cargo-watch`:
   ```bash
   cargo install cargo-watch
   ```

2. Run the watch command:
   ```bash
   cargo watch -c -x run
   ```
   *The `-c` flag clears the terminal on each restart, and `-x run` executes the app.*

### Project Structure

- `src/main.rs`: Application shell, action handling, and column management logic.
- `src/column.rs`: The directory list widget and resizer logic.
- `src/preview.rs`: The file information and preview pane.

## 🤝 Contributing

Contributions are welcome! Feel free to open issues or submit pull requests to improve the navigation, add new view types (Icons/List), or enhance file previews.

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
