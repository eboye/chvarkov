# Arch-Finder Engineering Guidelines

This document establishes foundational mandates and technical standards for the **Arch-Finder** project. All future development and AI-assisted modifications MUST strictly adhere to these rules.

## 🎯 Core Mandates

1.  **Miller Column First:** The primary navigation paradigm is Miller Columns. Any new view types (Icons, List) must be implemented such that they do not break or degrade the core Miller Column experience.
2.  **Native GNOME Aesthetic:** The application MUST use **GTK4** and **Libadwaita**. Avoid custom drawing or non-standard widgets unless absolutely necessary for the Miller Column logic.
3.  **Keyboard First:** Every feature implemented MUST be fully navigable via keyboard. This includes:
    *   Left/Right arrow navigation between columns.
    *   Context menu access via `Menu` or `Shift+F10`.
    *   Initial focus on file lists, not toolbar elements.
    *   Spacebar for "Sushi-style" Quick Look content previews.
4.  **Multi-Selection Support:** All collection views (Miller Columns, Icon View) MUST support standard multi-selection (Shift+Arrow, Ctrl+Click, Ctrl+A).
5.  **Performance & Safety:** Leverage **Rust's** type system and memory safety. Avoid `unsafe` blocks and unnecessary `unwrap()` calls in the UI thread. Use asynchronous GIO APIs for all file system operations to prevent UI freezing.

## 🛠 Technical Standards

### UI Architecture
- **Focus Management:** Always explicitly manage the `.focused-column` or `.focused-grid` CSS class when shifting focus between views.
- **Dynamic Columns:** Use the `ColumnManager` pattern to handle the nesting and clearing of columns/previews. Never allow stale sub-columns to persist after a selection change.
- **Deselection:** Clicking on empty space in any folder MUST clear the current selection.
- **Sorting:** All views MUST respect the global sorting preference (Name, Date, Size, Type) defined in GSettings.

### Rust Coding Style
- **Reusability:** Common logic (Context Menus, Sorters, Directory Lists) MUST be centralized in `utils.rs`.
- **Modules:** Keep logic separated (`main.rs` for shell/actions, `column.rs` for directory lists, `icon_view.rs` for grid view, `preview.rs` for file info).
- **Clones:** Use `Rc<RefCell<T>>` for shared state between UI callbacks. Always clone handles (like `Box` or `ListView`) before moving them into closures.
- **GIO Attributes:** When querying files, always request: `standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified,standard::is-symlink-target-directory`.

### Styling & CSS
- **Theming:** Use Libadwaita named colors (e.g., `@accent_bg_color`, `@view_fg_color`) to ensure compatibility with dark/light mode and system themes.
- **Sidebar:** The active sidebar location MUST be highlighted using the system accent color with high-contrast text.
- **Visual Feedback:** 
    - Active view selection: Vibrant accent color.
    - Inactive view selection: Alpha-blended (dimmed) accent color.
    - Hover effects: Subtle background changes for non-selected rows.

## 🎞 Content Previews (Quick Look)
- **Images:** High-quality, scaling previews using `gtk::Picture`.
- **Videos:** Real-time playback with `GtkVideo` (looping/autoplay).
- **Code:** Syntax highlighting for common languages using `GtkSourceView`.
- **Navigation:** The preview window MUST remain open and update dynamically while the user navigates the file tree using arrow keys.

## 📋 Verification Checklist

Before finalizing any change:
- [ ] Run `cargo check` to ensure zero compilation errors/warnings.
- [ ] Verify keyboard navigation (Arrows, Return, Delete, Ctrl+C/V).
- [ ] Test with "Show Hidden Files" enabled and disabled.
- [ ] Ensure the "Toggle Metadata" state preserves the current focus.
- [ ] Verify that selecting a file correctly triggers the `Preview` pane and clears subsequent columns.
