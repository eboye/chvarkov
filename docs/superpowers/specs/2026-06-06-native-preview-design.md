# Richer Docked Preview + Native Space Preview — Design

**Date:** 2026-06-06
**Status:** Approved (pending implementation)

## Goal

Two related preview improvements:

1. **Richer docked pane** — the right-hand docked preview should render real file
   content (notably syntax-highlighted text), like the floating `Space` view, instead
   of falling back to a bare icon for text files.
2. **Native preview on `Space`** — pressing `Space` should invoke the OS-native
   previewer (macOS QuickLook via `qlmanage -p`; Linux GNOME Sushi if installed),
   falling back to our own GTK preview window when no native previewer is available.

## Part 1 — Richer docked pane

### Problem

`preview.rs::create_preview_layout(file_info, path, large)` is shared by the docked
pane (`large = false`) and the floating `Space` window (`large = true`). Today the
syntax-highlighted text branch is gated on `is_text && large` (line 45), so in the
docked pane a text file shows only a generic icon — never its contents. Images and
video already render in both sizes.

### Change

Change the gate from `is_text && large` to `is_text`. The `large` flag continues to
control **sizing only** (margins 40 vs 20, media height 400 vs 200, `title-1` vs
`title-2`). After the change:

- Docked pane (`large = false`): renders the source view for text files at the
  compact dock size; images/video/icon branches unchanged.
- Floating window (`large = true`): unchanged.

No new code paths, no new functions — a single behavioral change in the existing
`if/else` chain. `ColumnManager::update_dock` already calls
`create_preview_layout(..., false)`, so it picks this up automatically.

## Part 2 — Native preview on `Space`

### New module: `src/quicklook.rs`

A small, focused module with one pure (testable) function and two thin OS wrappers.

```rust
/// Resolve the native-previewer command to spawn for `path`, or None to signal
/// "no native previewer — fall back to our GTK window". Pure: availability is
/// passed in so it is testable on any host.
pub fn native_preview_command(
    path: &std::path::Path,
    has_qlmanage: bool,
    has_sushi: bool,
) -> Option<Vec<String>>
```

Behavior (selected by `cfg!`):
- **macOS:** `Some(vec!["qlmanage", "-p", <path>])` when `has_qlmanage`, else `None`.
- **Linux (and other unix):** `Some(vec!["sushi", <uri>])` when `has_sushi`, else
  `None`. The URI is `gio::File::for_path(path).uri()` (Sushi expects a URI).
- The function uses `cfg!(target_os = "macos")` to choose the branch; on non-macOS it
  takes the Sushi branch.

```rust
/// True if `name` is found in any $PATH entry and is a file.
pub fn binary_on_path(name: &str) -> bool
```

```rust
/// Try to open a native preview for `path`. Returns true if a native previewer was
/// launched, false if the caller should fall back to the GTK preview window.
pub fn open_native_preview(path: &std::path::Path) -> bool
```

`open_native_preview` resolves availability with `binary_on_path` (`"qlmanage"` on
macOS, `"sushi"` on Linux), calls `native_preview_command`, and if it returns `Some`,
spawns the command **detached and non-blocking** via `std::process::Command` with
stdout/stderr nulled and without waiting. It returns `true` on a successful spawn,
`false` if the command was `None` or the spawn errored.

### `Space` action change (`src/main.rs`)

The existing `preview` action currently always calls `manager.toggle_preview(&window)`
(our floating GTK window). Change it to:

1. Resolve the current single selection's path from `manager` (the
   `current_selection`'s `path`). If there is no selection, do nothing (current
   behavior — `toggle_preview` already no-ops without a selection).
2. Call `quicklook::open_native_preview(&path)`.
3. If it returns `false`, call the existing `manager.toggle_preview(&window)` (our GTK
   window — completely unchanged, with its `Esc` + close button).

Add a small accessor on `ColumnManager` to read the current selection path, e.g.
`fn current_selection_path(&self) -> Option<PathBuf>` returning
`self.current_selection.borrow().as_ref().map(|s| s.path.clone())`.

Outcome by platform:
- **macOS:** `qlmanage` is always present → `Space` always shows the real QuickLook
  panel. The GTK window becomes effectively the fallback that won't trigger here.
- **Linux:** Sushi installed → native; otherwise our GTK window (today's behavior).

### Interaction notes

- The native previewers own their own window and dismissal (their own `Esc`/controls).
  We do not try to position, embed, or close them.
- Pressing `Space` repeatedly spawns the native previewer again; this is acceptable and
  mirrors a "show preview" action. No child-process tracking.
- The docked pane (Part 1) is independent and always uses our GTK preview regardless of
  native availability.
- Module registration: add `mod quicklook;` to `main.rs`.

## Data flow

```
Space pressed -> preview action
  path = manager.current_selection_path()?    (no selection -> no-op)
  if quicklook::open_native_preview(&path):    (spawned native previewer)
        done
  else:
        manager.toggle_preview(&window)        (our GTK floating window, unchanged)

open_native_preview(path):
  has = binary_on_path("qlmanage" | "sushi")
  match native_preview_command(path, has_qlmanage, has_sushi):
        Some(argv) -> spawn detached; ok? true : false
        None       -> false
```

## Error handling

- Spawn failure (`Command::spawn` error) → `open_native_preview` returns `false` → GTK
  fallback. No panic.
- No `unwrap()` on the UI thread; selection access uses `Option`.
- Process is spawned detached with nulled stdio; we never block the UI waiting on it.

## Testing

### Unit (headless)

`native_preview_command` (the pure helper) — assert per the host's `cfg`:
- On macOS (`#[cfg(target_os = "macos")]` tests):
  - `(path, true, _)` → `Some(["qlmanage", "-p", path])`
  - `(path, false, _)` → `None`
- On Linux/other (`#[cfg(not(target_os = "macos"))]` tests):
  - `(path, _, true)` → `Some(["sushi", <uri>])` (first element `"sushi"`)
  - `(path, _, false)` → `None`

(`binary_on_path` and `open_native_preview` touch the environment/process table and are
covered by manual verification, not unit tests.)

### Manual (Verification Checklist)

- `cargo check` / `cargo clippy --all-targets` pass with zero warnings.
- **Part 1:** Select a text/source file → the docked pane shows its syntax-highlighted
  contents (not just an icon). Images and video still preview in the dock. The floating
  `Space` view is unchanged.
- **Part 2 (macOS):** Select a file, press `Space` → the macOS QuickLook panel opens
  with the real preview. Works for images, PDFs, text, etc.
- **Part 2 (Linux with Sushi):** `Space` opens Sushi for the selected file.
- **Part 2 (Linux without Sushi):** `Space` opens our GTK preview window (with `Esc` +
  close button), exactly as before.
- Pressing `Space` with no selection does nothing.

## Out of scope (YAGNI)

- Embedding the native preview inside our window (QLPreviewPanel / Sushi D-Bus
  embedding). We spawn the standalone previewers.
- A configurable list of Linux previewers beyond Sushi.
- Tracking/closing previously-spawned native preview processes.
- Changing the docked-pane visibility rules or the floating window's behavior.
