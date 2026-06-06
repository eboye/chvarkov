# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `Space` now opens the **OS-native previewer** when available — macOS QuickLook (`qlmanage`) or GNOME Sushi on Linux — and falls back to the built-in GTK preview window otherwise.

### Changed
- The docked preview pane now renders file **contents** (e.g. syntax-highlighted text), matching the `Space` view, instead of showing only an icon for text files.

### Fixed
- The "Columns" view-type button icon no longer renders as a broken/white glyph in light theme (`view-columns-symbolic` doesn't exist in the Adwaita theme; now uses `view-dual-symbolic`).
- Destructive actions (delete/trash) no longer rely on a styling-only CSS class to pick the target view, eliminating a stale-state mis-targeting hazard; when no view is focused they now safely do nothing.
- Previewing a special file (FIFO/socket/device) or a file on a slow mount no longer blocks the UI — preview rendering is gated to regular files and text is read off the UI thread.
- The docked preview no longer autoplays video (only the `Space` view does), and video playback stops when the preview is hidden or swapped (dock and Quick Look window).
- Pasting from GNOME Files now honors cut vs copy (cut moves) by reading the `x-special/gnome-copied-files` clipboard payload.
- Replacing a symlinked target removes the link rather than its target tree; a folder move/copy is refused if its path can't be verified; non-UTF-8 names are skipped in zip/email instead of being mangled.
- Reaped spawned helper processes (preview/terminal/email) and stopped the responsive-label timer from accumulating across view rebuilds.
- Replaced reachable UI-thread `unwrap()`s in navigation and list rendering with graceful guards.

## [0.4.1] - 2026-06-06

### Fixed
- **macOS "app is damaged" error** — the `.app` is now ad-hoc code-signed, so Apple Silicon no longer rejects it outright. (It's still not notarized; clear quarantine on first launch — see the README.)
- **Breadcrumb navigation** — clicking a breadcrumb now walks the existing Miller column chain (scrolls/focuses that column, keeping the deeper columns) instead of collapsing the whole history to a single column.

### Internal
- CI: bumped `actions/checkout`@v5, `upload-artifact`@v7, `download-artifact`@v8, and `softprops/action-gh-release`@v3 to Node 24-native versions.
- CI: macOS builds target Apple Silicon only (GitHub retired the Intel `macos-13` hosted runner); Intel users can build from source with `./build.sh macos`.

[0.4.1]: https://github.com/eboye/chvarkov/releases/tag/v0.4.1

## [0.4.0] - 2026-06-06

### Changed
- The file preview is now a **fixed pane docked to the right edge** of the window, shown whenever a single file is selected (in Miller, Icon, and List views). It stays pinned in place instead of scrolling away as an inline Miller column. Selecting a folder, multiple items, or empty space hides it. The pane is resizable; `Space` still opens the larger floating Quick Look.

### Fixed
- Restored unit tests for capability, size-formatting, and zoom-level logic that had been dropped from the test suite.

### Internal
- CI: bumped `actions/checkout`, `upload-artifact`, and `download-artifact` to v5 (Node 24).

[0.4.0]: https://github.com/eboye/chvarkov/releases/tag/v0.4.0

## [0.3.0] - 2026-06-06

### Added
- **New Folder** and **New Empty File** creation — from the right-click menu, a "New" header-bar button, and keyboard shortcuts (`Ctrl/Cmd+Shift+N` and `Ctrl/Cmd+N`). New items get a non-colliding "untitled" name and open a naming dialog (text pre-selected) so you can rename immediately; the menu entries are hidden in read-only directories.

[0.3.0]: https://github.com/eboye/chvarkov/releases/tag/v0.3.0

## [0.2.2] - 2026-06-06

### Fixed
- Sidebar header now lines up with the main header bar. The OverlaySplitView insets the sidebar pane vertically; the titlebars are the same height but the sidebar one sat a few pixels lower — its position is now measured at runtime and corrected.
- Sidebar "Preferences" footer now aligns with the breadcrumb bar. GTK's built-in `.navigation-sidebar` padding was insetting the sidebar pane, leaving the footer ending a few pixels above the window bottom while the breadcrumb sat flush; the padding is now zeroed so both bars share the same top and bottom edges.

[0.2.2]: https://github.com/eboye/chvarkov/releases/tag/v0.2.2

## [0.2.1] - 2026-06-06

### Fixed
- **Critical:** deleting or trashing a file could act on its **parent folder** when navigating by mouse. Destructive actions now resolve the target from the column that actually holds the keyboard focus, instead of a CSS class only updated by arrow-key navigation.
- The view-type button icon rendered as a white/broken glyph in light theme — it used a non-existent icon name (`view-column-symbolic`); now uses the correct `view-columns-symbolic`.

### Added
- A visible close button on the Quick Look (`Space`) preview window (`Esc`/`Space` still close it).

[0.2.1]: https://github.com/eboye/chvarkov/releases/tag/v0.2.1

## [0.2.0] - 2026-06-06

### Added
- **Cut / Copy / Paste** with **system-clipboard interop** — copy in chvarkov and paste in Finder / GNOME Files, and vice-versa.
- **Move to… / Copy to…** via a folder picker.
- **Create Link** (symlink) and **Compress to Zip**.
- **Move to Trash** and **Delete Permanently** (with a confirmation prompt), all operating on the full multi-selection.
- **Open in Terminal**, **Copy Path / Copy URI / Copy Name**.
- **Email** attachments and **Share** — native macOS Share sheet (`NSSharingServicePicker`); `xdg-email` on Linux.
- **Permission-aware context menus** — actions you don't have rights to perform are hidden, based on the filesystem's reported capabilities.
- **Name-collision handling** — Replace / Skip / Keep Both, with directory **Merge** instead of destructive overwrite and an "apply to all" option for batches.
- **Data-loss safety guards** (modeled on GNOME Nautilus) — refuses to copy/move a folder into itself, never silently replaces a directory, preserves symlinks instead of dereferencing them, and validates rename input.
- **Linux:** opens a folder passed on the command line, so chvarkov works as an `xdg-open` / default file-manager target.
- Headless unit-test suite covering the file-operation, clipboard, capability, and formatting logic.

### Fixed
- Smooth Miller-column and preview-pane resizing — the dragged edge no longer jumps left/right.
- List view: operations now target the correct file for items in expanded subfolders (previously resolved to the wrong path).
- Quick Look preview: Down-arrow no longer selects past the last item.
- Hardened a panic path when opening Preferences with no active window.
- Sidebar "Trash" now points to the correct location per platform (`~/.Trash` on macOS).
- Prevented AppleScript injection when emailing files with unusual names (macOS).
- Permanent delete now removes non-empty directories recursively.
- Cleaned up GTK CSS parser warnings from the inline stylesheet.

### Changed
- Removed debug logging; the build is free of compiler and Clippy warnings.
- Consolidated project guidance into `CLAUDE.md`.

[0.2.0]: https://github.com/eboye/chvarkov/releases/tag/v0.2.0
