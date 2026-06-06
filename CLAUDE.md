# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**chvarkov** is a cross-platform (Linux/GNOME + macOS) file manager built in **Rust** with **GTK4**, **Libadwaita**, and **GtkSourceView 5**. Its defining feature is **Miller Columns** navigation (macOS Finder-style side-by-side directory columns). It also offers Icon and List views.

App ID: `net.nocopypaste.chvarkov`. This is a Cargo workspace with two members: the app (`.`) and `xtask` (install/uninstall helper).

## Commands

```bash
cargo run --release            # build & run locally
cargo check                    # MUST pass with zero errors/warnings before finalizing any change
cargo watch -c -x run          # hot-reload dev loop (requires: cargo install cargo-watch)

cargo xtask install            # install to ~/.local (binary + schema + .desktop + icon, no sudo)
sudo cargo xtask install --system   # install to /usr/local
cargo xtask uninstall          # uninstall from ~/.local

./build.sh macos               # build Chvarkov.app bundle (macOS)
./build.sh appimage            # build Linux AppImage (runs in a Fedora 41 Docker container)
./build.sh flatpak             # build Flatpak bundle
```

There is no test suite. Verification is manual (see Verification Checklist below).

### System dependencies
- **macOS:** `brew install pkg-config gtk4 libadwaita adwaita-icon-theme gtksourceview5` plus `gstreamer gst-plugins-base gst-plugins-good gst-libav` (the GStreamer set is required for video previews).
- **Arch:** `gtk4 libadwaita gtksourceview5 gst-plugins-good`
- **Fedora:** `gtk4-devel libadwaita-devel gtksourceview5-devel gstreamer1-plugins-good`
- **Debian/Ubuntu:** `libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libgstreamer1.0-dev gstreamer1.0-plugins-good`

## Architecture

The app is a single GTK `Application`. Startup wiring lives in `main.rs`:
- `connect_startup` → `setup_actions(app)` (registers all `app.*` GActions + keyboard accelerators) and `setup_styles()` (one big inline CSS provider).
- `connect_activate` → `build_ui(app)` constructs the `ApplicationWindow`, `OverlaySplitView` (sidebar + content), `HeaderBar`, breadcrumb bar, and `ToastOverlay`.

### `ColumnManager` (the core) — `main.rs`
`ColumnManager` is the central controller for the content area. It owns the Miller column stack (`entries: Vec<ColumnEntry>`), the active main view, breadcrumbs, current selection, the Quick Look preview window, and the toast overlay. Most shared state inside it is `Rc<RefCell<T>>` because it is cloned into many GTK signal closures.

A single active `ColumnManager` is stored in a `thread_local!` `ACTIVE_MANAGER`. GActions in `setup_actions` reach the live manager via `ACTIVE_MANAGER.with(...)` (this avoids `unsafe` widget `set_data`). When adding a new action, follow that same pattern.

### Modules
- `main.rs` — application shell, `setup_actions` (every context-menu/keyboard action), `build_ui`, and `ColumnManager`.
- `column.rs` — a single Miller `Column` (a `ListView` over a filtered+sorted `DirectoryList` with a `MultiSelection`).
- `icon_view.rs` / `list_view.rs` — alternative `GridView` / `ColumnView` based views. All three view constructors share the same signature: `new(path, show_hidden, show_meta, zoom_level, sort_type, folders_first)`.
- `preview.rs` — `Preview` builds the file-info / Quick Look / properties layouts (images via `gtk::Picture`, video via `GtkVideo`, code via `GtkSourceView`).
- `sidebar.rs` — navigation sidebar (places/bookmarks).
- `utils.rs` — **shared logic lives here.** Context menus (`create_context_menu`, `create_context_menu_shift`), sorting (`create_sorter`), directory listing (`get_directory_list`), size/metadata formatting, icon+thumbnail resolution, zoom→size math, and shared view controllers. Centralize any reusable logic here rather than duplicating across views.

### State / persistence
All persistent settings go through **GSettings** (schema: `net.nocopypaste.chvarkov.gschema.xml`). Code reads/writes via `gio::Settings::new("net.nocopypaste.chvarkov")`. Keys include `show-hidden`, `show-meta`, `zoom-level`, `view-type` (`miller`/`icons`/`list`), `show-sidebar`, `current-path`, `default-path`, `sort-type` (`name`/`date`/`size`/`type`), `folders-first`. Changing settings is how views are reconfigured — actions write the key, then trigger a rebuild. The schema must be compiled for the app to run: `xtask` and `build.sh` handle this (and `main.rs` points `GSETTINGS_SCHEMA_DIR` at a local `compiled_schemas/` dir for dev/bundle runs). If you add or change a schema key, the schema must be recompiled.

## Project mandates (from GEMINI.md — apply these)

1. **Miller Columns first.** New view types must not break or degrade the Miller Column experience.
2. **Native GNOME aesthetic.** Use GTK4 + Libadwaita; avoid custom drawing / non-standard widgets unless required for Miller Column logic. Theme with Libadwaita named colors (`@accent_bg_color`, `@view_fg_color`, etc.) so dark/light/system themes work.
3. **Keyboard first.** Every feature must be fully keyboard-navigable: Left/Right arrows between columns, context menu via `Menu`/`Shift+F10`, initial focus on file lists (not the toolbar), `Space` for Quick Look. The preview window stays open and updates live as the user arrows through files.
4. **Multi-selection** everywhere (Shift+Arrow, Ctrl+Click, Ctrl+A).
5. **Performance & safety.** Avoid `unsafe` and avoid `unwrap()` on the UI thread. Use async GIO for filesystem operations so the UI never blocks.

### Conventions
- Centralize reusable logic in `utils.rs`; keep module responsibilities separate (see Modules above).
- Use `Rc<RefCell<T>>` for state shared across UI callbacks; clone widget handles before moving them into closures.
- Explicitly manage the `.focused-column` / `.focused-grid` / `.focused-list` CSS classes when shifting focus between views.
- Clicking empty space in a folder must clear the selection. Selecting a file must update the Preview pane and clear any stale child columns (use the `ColumnManager` pattern — never let stale sub-columns persist).
- All views must respect the global sort preference and the folders-first setting.
- When querying files via GIO, request these attributes: `standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified,standard::is-symlink-target-directory`.

## Verification checklist (manual, before finalizing)
- `cargo check` passes with zero errors/warnings.
- Keyboard nav works (arrows, Return, Delete, Ctrl+C/V).
- Works with "Show Hidden Files" both on and off.
- "Toggle Metadata" preserves current focus.
- Selecting a file triggers the Preview pane and clears subsequent columns.
