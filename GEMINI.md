# Arch-Finder Engineering Guidelines

This document establishes foundational mandates and technical standards for the **Arch-Finder** project. All future development and AI-assisted modifications MUST strictly adhere to these rules.

## 🎯 Core Mandates

1.  **Miller Column First:** The primary navigation paradigm is Miller Columns. Any new view types (Icons, List) must be implemented such that they do not break or degrade the core Miller Column experience.
2.  **Native GNOME Aesthetic:** The application MUST use **GTK4** and **Libadwaita**. Avoid custom drawing or non-standard widgets unless absolutely necessary for the Miller Column logic.
3.  **Keyboard First:** Every feature implemented MUST be fully navigable via keyboard. This includes:
    *   Left/Right arrow navigation between columns.
    *   Context menu access via `Menu` or `Shift+F10`.
    *   Initial focus on file lists, not toolbar elements.
4.  **Performance & Safety:** Leverage **Rust's** type system and memory safety. Avoid `unsafe` blocks and unnecessary `unwrap()` calls in the UI thread. Use asynchronous GIO APIs for all file system operations to prevent UI freezing.

## 🛠 Technical Standards

### UI Architecture
- **Focus Management:** Always explicitly manage the `.focused-column` CSS class when shifting focus between columns.
- **Dynamic Columns:** Use the `ColumnManager` pattern to handle the nesting and clearing of columns/previews. Never allow stale sub-columns to persist after a selection change.
- **Resizing:** Maintain the custom resizer-separator pattern with `GestureDrag` until a more stable native solution is identified. Minimum column width is **100px**.

### Rust Coding Style
- **Modules:** Keep logic separated (`main.rs` for shell/actions, `column.rs` for directory lists, `preview.rs` for file info).
- **Clones:** Use `Rc<RefCell<T>>` for shared state between UI callbacks. Always clone handles (like `Box` or `ListView`) before moving them into closures.
- **GIO Attributes:** When querying files, always request: `standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified`.

### Styling & CSS
- **Theming:** Use Libadwaita named colors (e.g., `@accent_bg_color`, `@view_fg_color`) to ensure compatibility with dark/light mode and system themes.
- **Visual Feedback:** 
    - Active column selection: Vibrant accent color.
    - Inactive column selection: Alpha-blended (dimmed) accent color.
    - Hover effects: Subtle background changes for non-selected rows.

## 📋 Verification Checklist

Before finalizing any change:
- [ ] Run `cargo check` to ensure zero compilation errors/warnings.
- [ ] Verify keyboard navigation (Arrows, Return, Delete, Ctrl+C/V).
- [ ] Test with "Show Hidden Files" enabled and disabled.
- [ ] Ensure the "Toggle Metadata" state preserves the current focus.
- [ ] Verify that selecting a file correctly triggers the `Preview` pane and clears subsequent columns.
