mod clipboard;
mod column;
mod file_ops;
mod preview;
mod quicklook;
mod sidebar;
mod icon_view;
mod list_view;
mod utils;

use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar, OverlaySplitView, ToastOverlay, Toast};
use gtk4 as gtk;
use gtk::{Box, Orientation, ScrolledWindow};
use std::path::PathBuf;
use column::Column;
use preview::Preview;
use sidebar::Sidebar;
use icon_view::IconView;
use list_view::ListView;
use std::rc::Rc;
use std::cell::RefCell;

// Use a thread-local for the active manager to avoid unsafe set_data and NonNull issues
thread_local! {
    static ACTIVE_MANAGER: RefCell<Option<Rc<ColumnManager>>> = const { RefCell::new(None) };
}

fn main() {
    // For development, point GSETTINGS_SCHEMA_DIR to our compiled schemas.
    // Check next to the binary first (installed / AppImage layout), then fall
    // back to the current working directory (cargo run / cargo watch).
    let schema_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("compiled_schemas")))
        .filter(|d| d.exists())
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|cwd| cwd.join("compiled_schemas"))
                .filter(|d| d.exists())
        });
    if let Some(dir) = schema_dir {
        unsafe { std::env::set_var("GSETTINGS_SCHEMA_DIR", &dir); }
    }

    let application = Application::builder()
        .application_id("net.nocopypaste.chvarkov")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_startup(|app| {
        setup_actions(app);
        setup_styles();
    });
    application.connect_activate(build_ui);

    // Handle being launched with a path argument — e.g. `xdg-open <dir>`, the
    // "Open With" menu, or when chvarkov is the default file manager. Open the
    // given directory (or the parent directory of a file), then build the UI.
    application.connect_open(|app, files, _hint| {
        if let Some(file) = files.first()
            && let Some(path) = file.path()
        {
            let dir = if path.is_dir() {
                path
            } else {
                path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
            };
            let settings = gio::Settings::new("net.nocopypaste.chvarkov");
            let _ = settings.set_string("current-path", &dir.to_string_lossy());
        }
        build_ui(app);
    });

    application.run();
}

fn setup_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data("
        /* Focused list selection - Vibrant accent color */
        .focused-column row:selected, .focused-grid row:selected, .focused-list row:selected {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            border-radius: 6px;
        }

        /* Unfocused list selection (parent columns) - Subdued color */
        listview row:selected, gridview row:selected, columnview row:selected {
            background-color: alpha(@accent_bg_color, 0.2);
            color: @view_fg_color;
            border-radius: 6px;
        }

        /* Hover effect for rows */
        listview row:hover:not(:selected), gridview row:hover:not(:selected), columnview row:hover:not(:selected) {
            background-color: alpha(@accent_bg_color, 0.05);
        }

        /* Make resizer more visible and interactive */
        separator.resizer {
            background-color: alpha(@borders, 0.3);
            min-width: 1px;
            margin: 0;
            padding: 0;
        }
        separator.resizer:hover {
            background-color: @accent_bg_color;
            min-width: 2px;
        }

        .navigation-sidebar {
            background-color: @window_bg_color;
            border-right: 1px solid alpha(@borders, 0.3);
            margin: 0;
            padding: 0;
        }

        /* Absolute Alignment Scrub */
        .sidebar-title-area, headerbar, .headerbar {
            background: none;
            background-color: @window_bg_color;
            /* We force the header background to the window color, so pair it with
               the window foreground — otherwise symbolic icons keep the default
               header fg and can render wrong (e.g. white in light theme). */
            color: @window_fg_color;
            border-bottom: 1px solid alpha(@borders, 0.3);
            padding: 0;
            margin: 0;
            min-height: 46px;
        }

        /* Ensure header symbolic icons follow the (window) foreground color. */
        headerbar image,
        .headerbar image,
        .sidebar-title-area image {
            color: @window_fg_color;
        }

        .sidebar-title-label {
            font-size: 1.1rem;
            font-weight: bold;
            margin: 0;
            padding: 0;
            /* No min-height here: the header already has min-height:46px. A 46px
               label would stack on top of the header's padding and make the
               sidebar header taller than the main header. */
        }

        .sidebar-footer-area, .breadcrumb-container-scrolled {
            background-color: @window_bg_color;
            border-top: 1px solid alpha(@borders, 0.3);
            padding: 0;
            margin: 0;
            min-height: 40px;
        }

        .sidebar-footer-label {
            font-size: 0.95rem;
            margin: 0;
            padding: 0;
        }

        /* Sidebar active highlighting */
        row.sidebar-active {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            font-weight: bold;
            border-radius: 6px;
        }
        row.sidebar-active image {
            color: @accent_fg_color;
        }

        .breadcrumb-bar {
            background-color: @window_bg_color;
            margin: 0;
            padding: 0;
        }

        .breadcrumb-bar button {
            padding: 2px 8px;
            font-size: 0.9rem;
            margin: 0;
        }
        .breadcrumb-bar button:hover {
            background-color: alpha(@accent_bg_color, 0.1);
            color: @accent_bg_color;
        }

        /* Adaptive labels */
        .adaptive-label {
            transition: all 200ms ease-in-out;
        }

        /* Thumbnail styling */
        image.thumbnail {
            border-radius: 4px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.2);
            background-color: @view_bg_color;
        }
    ");

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// (selected count, AND-combined capabilities) for the focused view's selection.
pub(crate) fn selection_caps() -> (usize, utils::Caps) {
    ACTIVE_MANAGER.with(|m| {
        if let Some(manager) = m.borrow().as_ref() {
            let sel = manager.collect_selection();
            let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
            (sel.len(), caps)
        } else {
            (0, utils::Caps::default())
        }
    })
}

/// Whether the directory new items would be created in is writable. Used to
/// gate the context-menu "New" section. Missing attribute defaults to writable.
pub(crate) fn target_dir_writable() -> bool {
    ACTIVE_MANAGER.with(|m| {
        if let Some(manager) = m.borrow().as_ref()
            && let Some(dir) = manager.focused_dir()
        {
            let file = gio::File::for_path(&dir);
            return match file.query_info(
                "access::can-write",
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            ) {
                Ok(info) => {
                    if info.has_attribute("access::can-write") {
                        info.boolean("access::can-write")
                    } else {
                        true
                    }
                }
                Err(_) => false,
            };
        }
        false
    })
}

fn get_selection_model(widget: &gtk::Widget) -> Option<gtk::MultiSelection> {
    if let Ok(lv) = widget.clone().downcast::<gtk::ListView>() {
        return lv.model().and_downcast::<gtk::MultiSelection>();
    }
    if let Ok(gv) = widget.clone().downcast::<gtk::GridView>() {
        return gv.model().and_downcast::<gtk::MultiSelection>();
    }
    if let Ok(cv) = widget.clone().downcast::<gtk::ColumnView>() {
        return cv.model().and_downcast::<gtk::MultiSelection>();
    }
    None
}

/// Resolve a FileInfo's real path. Uses the `standard::file` attribute (always
/// requested in our DirectoryLists), which is correct even for nested rows in
/// the tree-based List view. Falls back to `base/name` if the attribute is absent.
fn file_info_path(info: &gio::FileInfo, base: &std::path::Path) -> PathBuf {
    info.attribute_object("standard::file")
        .and_downcast::<gio::File>()
        .and_then(|f| f.path())
        .unwrap_or_else(|| base.join(info.name()))
}

fn setup_actions(app: &Application) {
    let settings = gio::Settings::new("net.nocopypaste.chvarkov");

    let quit_action = gio::SimpleAction::new("quit", None);
    let app_weak = app.downgrade();
    quit_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit_action);
    app.set_accels_for_action("app.quit", &["<Control>q"]);

    let open_action = gio::SimpleAction::new("open", None);
    open_action.connect_activate(|_, _| {
       ACTIVE_MANAGER.with(|m| {
           if let Some(manager) = m.borrow().as_ref()
               && let Some(selection) = manager.current_selection.borrow().as_ref() {
                   let file_info = &selection.file_info;
                   let path = &selection.path;
                    let is_dir = file_info.file_type() == gio::FileType::Directory || path.is_dir();

                    if is_dir {
                        let settings = gio::Settings::new("net.nocopypaste.chvarkov");
                        let _ = settings.set_string("current-path", &path.to_string_lossy());

                        if let Some(app) = gio::Application::default() {
                             glib::idle_add_local(move || {
                                 app.activate();
                                 glib::ControlFlow::Break
                             });
                        }
                    } else {
                        let file = gio::File::for_path(path);
                        gio::AppInfo::launch_default_for_uri(&file.uri(), None::<&gio::AppLaunchContext>).ok();
                    }
                }
        });
    });
    app.add_action(&open_action);
    app.set_accels_for_action("app.open", &["Return"]);

    let cut_action = gio::SimpleAction::new("cut", None);
    cut_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !(caps.read && caps.delete) { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                clipboard::publish(&paths, clipboard::Mode::Cut);
                *manager.clipboard.borrow_mut() = Some(clipboard::ClipboardOp { mode: clipboard::Mode::Cut, paths });
                manager.send_toast("Cut");
            }
        });
    });
    app.add_action(&cut_action);
    app.set_accels_for_action("app.cut", &["<Control>x"]);

    let copy_action = gio::SimpleAction::new("copy", None);
    copy_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                clipboard::publish(&paths, clipboard::Mode::Copy);
                *manager.clipboard.borrow_mut() = Some(clipboard::ClipboardOp { mode: clipboard::Mode::Copy, paths });
                manager.send_toast("Copied");
            }
        });
    });
    app.add_action(&copy_action);
    app.set_accels_for_action("app.copy", &["<Control>c"]);

    let paste_action = gio::SimpleAction::new("paste", None);
    let paste_app_weak = app.downgrade();
    paste_action.connect_activate(move |_, _| {
        let Some(app) = paste_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let Some(dest) = manager.focused_dir() else { return };
                let internal = manager.clipboard.borrow().clone();
                if let Some(op) = internal {
                    let kind = match op.mode {
                        clipboard::Mode::Copy => file_ops::TransferKind::Copy,
                        clipboard::Mode::Cut => file_ops::TransferKind::Move,
                    };
                    file_ops::transfer(manager.clone(), window.clone().upcast(), op.paths, dest, kind);
                    if op.mode == clipboard::Mode::Cut {
                        *manager.clipboard.borrow_mut() = None;
                    }
                } else {
                    let manager_c = manager.clone();
                    let window_c = window.clone();
                    clipboard::read_external(move |op| {
                        if let Some(op) = op {
                            file_ops::transfer(manager_c.clone(), window_c.clone().upcast(), op.paths, dest.clone(), file_ops::TransferKind::Copy);
                        }
                    });
                }
            }
        });
    });
    app.add_action(&paste_action);
    app.set_accels_for_action("app.paste", &["<Control>v"]);

    let move_to_action = gio::SimpleAction::new("move-to", None);
    let move_app_weak = app.downgrade();
    move_to_action.connect_activate(move |_, _| {
        let Some(app) = move_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !(caps.read && caps.delete) { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                let manager_c = manager.clone();
                let win = window.clone();
                let dialog = gtk::FileDialog::builder().title("Move to Folder").build();
                dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |res| {
                    if let Ok(folder) = res
                        && let Some(dest) = folder.path() {
                            file_ops::transfer(manager_c.clone(), win.clone().upcast(), paths.clone(), dest, file_ops::TransferKind::Move);
                        }
                });
            }
        });
    });
    app.add_action(&move_to_action);

    let copy_to_action = gio::SimpleAction::new("copy-to", None);
    let copy_app_weak = app.downgrade();
    copy_to_action.connect_activate(move |_, _| {
        let Some(app) = copy_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                let manager_c = manager.clone();
                let win = window.clone();
                let dialog = gtk::FileDialog::builder().title("Copy to Folder").build();
                dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |res| {
                    if let Ok(folder) = res
                        && let Some(dest) = folder.path() {
                            file_ops::transfer(manager_c.clone(), win.clone().upcast(), paths.clone(), dest, file_ops::TransferKind::Copy);
                        }
                });
            }
        });
    });
    app.add_action(&copy_to_action);

    let rename_action = gio::SimpleAction::new("rename", None);
    let app_weak_rename = app.downgrade();
    rename_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_rename.upgrade() {
            ACTIVE_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref()
                    && let Some(selection) = manager.current_selection.borrow().as_ref()
                        && let Some(window) = app.active_window().and_then(|w| w.downcast::<ApplicationWindow>().ok()) {
                            show_rename_dialog(&window, manager.clone(), &selection.file_info.display_name(), selection.path.clone());
                        }
            });
        }
    });
    app.add_action(&rename_action);
    app.set_accels_for_action("app.rename", &["F2"]);

    let new_folder_action = gio::SimpleAction::new("new-folder", None);
    let new_folder_app_weak = app.downgrade();
    new_folder_action.connect_activate(move |_, _| {
        let Some(app) = new_folder_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let Some(dir) = manager.focused_dir() else { return };
                file_ops::create_folder(manager.clone(), window.clone().upcast(), dir);
            }
        });
    });
    app.add_action(&new_folder_action);
    app.set_accels_for_action("app.new-folder", &["<Shift><Control>n"]);

    let new_file_action = gio::SimpleAction::new("new-file", None);
    let new_file_app_weak = app.downgrade();
    new_file_action.connect_activate(move |_, _| {
        let Some(app) = new_file_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let Some(dir) = manager.focused_dir() else { return };
                file_ops::create_file(manager.clone(), window.clone().upcast(), dir);
            }
        });
    });
    app.add_action(&new_file_action);
    app.set_accels_for_action("app.new-file", &["<Control>n"]);

    let create_link_action = gio::SimpleAction::new("create-link", None);
    create_link_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::symlink(manager.clone(), paths);
            }
        });
    });
    app.add_action(&create_link_action);
    app.set_accels_for_action("app.create-link", &["<Shift><Control>m"]);

    let compress_action = gio::SimpleAction::new("compress", None);
    compress_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::compress(manager.clone(), paths);
            }
        });
    });
    app.add_action(&compress_action);

    let email_action = gio::SimpleAction::new("email", None);
    email_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().filter(|s| !s.path.is_dir()).map(|s| s.path).collect();
                if paths.is_empty() { manager.send_toast("Select file(s) to email"); return; }
                file_ops::email(manager.clone(), paths);
            }
        });
    });
    app.add_action(&email_action);

    let delete_action = gio::SimpleAction::new("delete", None);
    delete_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.trash { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::trash(manager.clone(), paths);
            }
        });
    });
    app.add_action(&delete_action);
    app.set_accels_for_action("app.delete", &["Delete"]);

    let permanent_delete_action = gio::SimpleAction::new("permanent-delete", None);
    let perm_del_app_weak = app.downgrade();
    permanent_delete_action.connect_activate(move |_, _| {
        let Some(app) = perm_del_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.delete { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::delete(manager.clone(), window.clone().upcast(), paths);
            }
        });
    });
    app.add_action(&permanent_delete_action);
    app.set_accels_for_action("app.permanent-delete", &["<Shift>Delete"]);

    let open_terminal_action = gio::SimpleAction::new("open-terminal", None);
    open_terminal_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let dir = manager.collect_selection().into_iter().next()
                    .and_then(|s| if s.path.is_dir() { Some(s.path) } else { s.path.parent().map(|p| p.to_path_buf()) })
                    .or_else(|| manager.focused_dir());
                if let Some(dir) = dir { file_ops::open_terminal(manager.clone(), dir); }
            }
        });
    });
    app.add_action(&open_terminal_action);

    let copy_path_action = gio::SimpleAction::new("copy-path", None);
    copy_path_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let text = manager.collect_selection().into_iter()
                    .map(|s| s.path.to_string_lossy().to_string())
                    .collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    if let Some(d) = gtk::gdk::Display::default() { d.clipboard().set_text(&text); }
                    manager.send_toast("Path copied");
                }
            }
        });
    });
    app.add_action(&copy_path_action);

    let copy_uri_action = gio::SimpleAction::new("copy-uri", None);
    copy_uri_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let text = manager.collect_selection().into_iter()
                    .map(|s| gio::File::for_path(&s.path).uri().to_string())
                    .collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    if let Some(d) = gtk::gdk::Display::default() { d.clipboard().set_text(&text); }
                    manager.send_toast("URI copied");
                }
            }
        });
    });
    app.add_action(&copy_uri_action);

    let copy_name_action = gio::SimpleAction::new("copy-name", None);
    copy_name_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let text = manager.collect_selection().into_iter()
                    .map(|s| s.file_info.display_name().to_string())
                    .collect::<Vec<_>>().join("\n");
                if !text.is_empty() {
                    if let Some(d) = gtk::gdk::Display::default() { d.clipboard().set_text(&text); }
                    manager.send_toast("Name copied");
                }
            }
        });
    });
    app.add_action(&copy_name_action);

    let sharing_options_action = gio::SimpleAction::new("sharing-options", None);
    let share_app_weak = app.downgrade();
    sharing_options_action.connect_activate(move |_, _| {
        let Some(app) = share_app_weak.upgrade() else { return };
        let Some(window) = app.active_window() else { return };
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                let sel = manager.collect_selection();
                if sel.is_empty() { return; }
                let caps = utils::combine_caps(sel.iter().map(|s| utils::caps_from_info(&s.file_info)));
                if !caps.read { return; }
                let paths: Vec<PathBuf> = sel.into_iter().map(|s| s.path).collect();
                file_ops::share(manager.clone(), window.clone().upcast(), paths);
            }
        });
    });
    app.add_action(&sharing_options_action);

    let properties_action = gio::SimpleAction::new("properties", None);
    let app_weak_prop = app.downgrade();
    properties_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_prop.upgrade() {
            ACTIVE_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref()
                    && let Some(selection) = manager.current_selection.borrow().as_ref() {
                        let file_info = &selection.file_info;
                        let path = &selection.path;

                        if let Some(parent_window) = app.active_window() {
                            let prop_window = adw::Window::builder()
                                .title(format!("Properties: {}", file_info.display_name()))
                                .transient_for(&parent_window)
                                .modal(true)
                                .default_width(420)
                                .default_height(600)
                                .build();

                            let content = Preview::create_properties_layout(file_info, path);

                            let scrolled = gtk::ScrolledWindow::builder()
                                .hscrollbar_policy(gtk::PolicyType::Never)
                                .vscrollbar_policy(gtk::PolicyType::Automatic)
                                .child(&content)
                                .build();

                            let toolbar_view = adw::ToolbarView::builder()
                                .content(&scrolled)
                                .build();

                            let header_bar = adw::HeaderBar::builder()
                                .build();

                            toolbar_view.add_top_bar(&header_bar);

                            prop_window.set_content(Some(&toolbar_view));
                            prop_window.present();
                        }
                    }
            });
        }
    });
    app.add_action(&properties_action);
    app.set_accels_for_action("app.properties", &["<Alt>Return"]);

    let preview_action = gio::SimpleAction::new("preview", None);
    let app_weak_p = app.downgrade();
    preview_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_p.upgrade() {
            ACTIVE_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref()
                    && let Some(window) = app.windows().into_iter().find_map(|w| w.downcast::<ApplicationWindow>().ok()) {
                        manager.toggle_preview(&window);
                    }
            });
        }
    });
    app.add_action(&preview_action);
    app.set_accels_for_action("app.preview", &["space"]);

    // Selection Actions
    let select_all_action = gio::SimpleAction::new("select-all", None);
    select_all_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref()
                && let Some(lv) = manager.get_focused_list_view()
                    && let Some(sm) = get_selection_model(&lv) {
                        sm.select_all();
                    }
        });
    });
    app.add_action(&select_all_action);
    app.set_accels_for_action("app.select-all", &["<Control>a"]);

    let select_none_action = gio::SimpleAction::new("select-none", None);
    select_none_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref()
                && let Some(lv) = manager.get_focused_list_view()
                    && let Some(sm) = get_selection_model(&lv) {
                        sm.unselect_all();
                    }
        });
    });
    app.add_action(&select_none_action);
    app.set_accels_for_action("app.select-none", &["<Control><Shift>a"]);

    let invert_selection_action = gio::SimpleAction::new("invert-selection", None);
    invert_selection_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref()
                && let Some(lv) = manager.get_focused_list_view()
                    && let Some(sm) = get_selection_model(&lv) {
                        let n_items = sm.n_items();
                        for i in 0..n_items {
                            if sm.is_selected(i) {
                                sm.unselect_item(i);
                            } else {
                                sm.select_item(i, false);
                            }
                        }
                    }
        });
    });
    app.add_action(&invert_selection_action);
    app.set_accels_for_action("app.invert-selection", &["<Control>i"]);

    // Use GSettings for persistent actions
    let toggle_sidebar_action = gio::SimpleAction::new_stateful("toggle-sidebar", None, &settings.value("show-sidebar"));
    let app_weak_s = app.downgrade();
    let settings_s = settings.clone();
    toggle_sidebar_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            let _ = settings_s.set_value("show-sidebar", state);
            if let Some(app) = app_weak_s.upgrade()
                 && let Some(window) = app.windows().into_iter().find_map(|w| w.downcast::<ApplicationWindow>().ok())
                     && let Some(split_view) = window.content().and_then(|w| w.downcast::<OverlaySplitView>().ok()) {
                         split_view.set_show_sidebar(state.get::<bool>().unwrap());
                     }
        }
    });
    app.add_action(&toggle_sidebar_action);
    app.set_accels_for_action("app.toggle-sidebar", &["F9"]);

    let show_hidden_action = gio::SimpleAction::new_stateful("show-hidden", None, &settings.value("show-hidden"));
    let app_weak_h = app.downgrade();
    let settings_h = settings.clone();
    show_hidden_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            let _ = settings_h.set_value("show-hidden", state);
            if let Some(app) = app_weak_h.upgrade() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        }
    });
    app.add_action(&show_hidden_action);
    app.set_accels_for_action("app.show-hidden", &["<Control>h"]);

    let show_meta_action = gio::SimpleAction::new_stateful("show-meta", None, &settings.value("show-meta"));
    let app_weak_m = app.downgrade();
    let settings_m = settings.clone();
    show_meta_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            let _ = settings_m.set_value("show-meta", state);
            if let Some(app) = app_weak_m.upgrade() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        }
    });
    app.add_action(&show_meta_action);
    app.set_accels_for_action("app.show-meta", &["<Control>m"]);

    let zoom_action = gio::SimpleAction::new_stateful("zoom-level", Some(glib::VariantTy::new("i").unwrap()), &settings.value("zoom-level"));
    let app_weak_z = app.downgrade();
    let settings_z = settings.clone();
    zoom_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            let val = state.get::<i32>().unwrap();
            if (0..=5).contains(&val) {
                action.set_state(&val.to_variant());
                let _ = settings_z.set_value("zoom-level", &val.to_variant());
                if let Some(app) = app_weak_z.upgrade() {
                    glib::idle_add_local(move || {
                        app.activate();
                        glib::ControlFlow::Break
                    });
                }
            }
        }
    });
    app.add_action(&zoom_action);

    let zoom_in_action = gio::SimpleAction::new("zoom-in", None);
    let app_weak_zi = app.downgrade();
    zoom_in_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_zi.upgrade()
            && let Some(action) = app.lookup_action("zoom-level")
                && let Some(current) = action.downcast::<gio::SimpleAction>().ok()
                    .and_then(|a| a.state())
                    .and_then(|s| s.get::<i32>()) {
                    app.activate_action("zoom-level", Some(&(current + 1).to_variant()));
                }
    });
    app.add_action(&zoom_in_action);
    app.set_accels_for_action("app.zoom-in", &["<Control>plus", "<Control>equal"]);

    let zoom_out_action = gio::SimpleAction::new("zoom-out", None);
    let app_weak_zo = app.downgrade();
    zoom_out_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_zo.upgrade()
            && let Some(action) = app.lookup_action("zoom-level")
                && let Some(current) = action.downcast::<gio::SimpleAction>().ok()
                    .and_then(|a| a.state())
                    .and_then(|s| s.get::<i32>()) {
                    app.activate_action("zoom-level", Some(&(current - 1).to_variant()));
                }
    });
    app.add_action(&zoom_out_action);
    app.set_accels_for_action("app.zoom-out", &["<Control>minus"]);

    let view_type_action = gio::SimpleAction::new_stateful("view-type", Some(glib::VariantTy::new("s").unwrap()), &settings.value("view-type"));
    let app_weak_v = app.downgrade();
    let settings_v = settings.clone();
    view_type_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            let _ = settings_v.set_value("view-type", state);
            if let Some(app) = app_weak_v.upgrade() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        }
    });
    app.add_action(&view_type_action);

    let sort_type_action = gio::SimpleAction::new_stateful("sort-type", Some(glib::VariantTy::new("s").unwrap()), &settings.value("sort-type"));
    let app_weak_st = app.downgrade();
    let settings_st = settings.clone();
    sort_type_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            let _ = settings_st.set_value("sort-type", state);
            if let Some(app) = app_weak_st.upgrade() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        }
    });
    app.add_action(&sort_type_action);

    let preferences_action = gio::SimpleAction::new("preferences", None);
    let app_weak_pref = app.downgrade();
    preferences_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_pref.upgrade() {
            show_preferences_window(&app);
        }
    });
    app.add_action(&preferences_action);
    app.set_accels_for_action("app.preferences", &["<Control>comma"]);
}

fn show_preferences_window(app: &Application) {
    let Some(window) = app.active_window() else { return; };
    let settings = gio::Settings::new("net.nocopypaste.chvarkov");

    let pref_window = adw::PreferencesDialog::builder()
        .build();

    let page = adw::PreferencesPage::new();
    pref_window.add(&page);

    let group = adw::PreferencesGroup::builder()
        .title("General")
        .build();
    page.add(&group);

    // Default Path
    let path_str = settings.string("default-path");
    let display_path = if path_str.is_empty() { "Not set (Home)".to_string() } else { path_str.to_string() };

    let path_label = gtk::Label::builder()
        .label(display_path)
        .valign(gtk::Align::Center)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .max_width_chars(30)
        .css_classes(["dim-label"])
        .build();

    let pick_button = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .valign(gtk::Align::Center)
        .build();

    let default_path_row = adw::ActionRow::builder()
        .title("Default Startup Path")
        .build();
    default_path_row.add_suffix(&path_label);
    default_path_row.add_suffix(&pick_button);

    let settings_path = settings.clone();
    let window_c = window.clone();
    pick_button.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::builder()
            .title("Select Default Startup Directory")
            .build();

        let settings_c = settings_path.clone();
        let label_c = path_label.clone();
        dialog.select_folder(Some(&window_c), gio::Cancellable::NONE, move |res| {
            if let Ok(folder) = res
                    && let Some(path) = folder.path() {
                        let path_str = path.to_string_lossy().to_string();
                        let _ = settings_c.set_string("default-path", &path_str);
                        label_c.set_label(&path_str);
                    }
            });
    });
    group.add(&default_path_row);

    // Folders First
    let folders_first_switch = gtk::Switch::builder()
        .active(settings.boolean("folders-first"))
        .valign(gtk::Align::Center)
        .build();

    let folders_first_row = adw::ActionRow::builder()
        .title("List Folders First")
        .activatable_widget(&folders_first_switch)
        .build();
    folders_first_row.add_suffix(&folders_first_switch);

    let settings_folders = settings.clone();
    folders_first_switch.connect_active_notify(move |sw| {
        let _ = settings_folders.set_boolean("folders-first", sw.is_active());
        if let Some(app) = gio::Application::default() {
            app.activate();
        }
    });
    group.add(&folders_first_row);

    pref_window.present(Some(&window));
}

fn build_ui(app: &Application) {
    let window = if let Some(window) = app.windows().into_iter().find_map(|w| w.downcast::<ApplicationWindow>().ok()) {
        window
    } else {
        ApplicationWindow::builder()
            .application(app)
            .default_width(1200)
            .default_height(800)
            .build()
    };

    window.set_title(None);

    let split_view = OverlaySplitView::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    // Responsive Breakpoint for Mobile/Narrow widths
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        600.0,
        adw::LengthUnit::Px,
    ));

    breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);

    let main_content = Box::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let header_bar = HeaderBar::builder()
        .title_widget(&gtk::Box::new(Orientation::Horizontal, 0))
        .build();

    let toggle_sidebar_btn = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Sidebar (F9)")
        .action_name("app.toggle-sidebar")
        .build();
    header_bar.pack_start(&toggle_sidebar_btn);

    // Sync toggle button state with split view
    split_view.bind_property("show-sidebar", &toggle_sidebar_btn, "active")
        .sync_create()
        .bidirectional()
        .build();

    let display_group = Box::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["linked"])
        .build();

    let show_hidden_btn = gtk::ToggleButton::builder()
        .icon_name("view-conceal-symbolic")
        .tooltip_text("Show Hidden Files")
        .action_name("app.show-hidden")
        .build();
    display_group.append(&show_hidden_btn);

    let show_meta_btn = gtk::ToggleButton::builder()
        .icon_name("view-list-bullet-symbolic")
        .tooltip_text("Toggle Metadata")
        .action_name("app.show-meta")
        .build();
    display_group.append(&show_meta_btn);

    header_bar.pack_start(&display_group);

    let separator = gtk::Separator::new(Orientation::Vertical);
    header_bar.pack_start(&separator);

    let view_type = app.lookup_action("view-type")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<String>().unwrap())
        .unwrap_or_else(|| "miller".to_string());

    let sort_type = app.lookup_action("sort-type")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<String>().unwrap())
        .unwrap_or_else(|| "name".to_string());

    let view_menu = gio::Menu::new();
    view_menu.append(Some("Miller Columns"), Some("app.view-type::miller"));
    view_menu.append(Some("Icons View"), Some("app.view-type::icons"));
    view_menu.append(Some("List View"), Some("app.view-type::list"));

    let view_icon = match view_type.as_str() {
        "icons" => "view-grid-symbolic",
        "list" => "view-list-symbolic",
        _ => "view-columns-symbolic",
    };

    let view_label_text = match view_type.as_str() {
        "icons" => "Icons",
        "list" => "List",
        _ => "Columns",
    };

    let view_btn_content = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    let view_btn_image = gtk::Image::from_icon_name(view_icon);
    let view_btn_label = gtk::Label::new(Some(view_label_text));
    view_btn_label.add_css_class("adaptive-label");
    view_btn_content.append(&view_btn_image);
    view_btn_content.append(&view_btn_label);

    let view_type_btn = gtk::MenuButton::builder()
        .child(&view_btn_content)
        .tooltip_text("View Options")
        .menu_model(&view_menu)
        .build();
    header_bar.pack_start(&view_type_btn);

    // Sort Menu
    let sort_menu = gio::Menu::new();
    sort_menu.append(Some("Sort by Name"), Some("app.sort-type::name"));
    sort_menu.append(Some("Sort by Date"), Some("app.sort-type::date"));
    sort_menu.append(Some("Sort by Size"), Some("app.sort-type::size"));
    sort_menu.append(Some("Sort by Type"), Some("app.sort-type::type"));

    let sort_label_text = match sort_type.as_str() {
        "date" => "Date",
        "size" => "Size",
        "type" => "Type",
        _ => "Name",
    };

    let sort_btn_content = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    let sort_btn_image = gtk::Image::from_icon_name("view-sort-ascending-symbolic");
    let sort_btn_label = gtk::Label::new(Some(sort_label_text));
    sort_btn_label.add_css_class("adaptive-label");
    sort_btn_content.append(&sort_btn_image);
    sort_btn_content.append(&sort_btn_label);

    let sort_type_btn = gtk::MenuButton::builder()
        .child(&sort_btn_content)
        .tooltip_text("Sort Options")
        .menu_model(&sort_menu)
        .build();
    header_bar.pack_start(&sort_type_btn);

    // New (create) Menu
    let new_menu = gio::Menu::new();
    new_menu.append(Some("New Folder"), Some("app.new-folder"));
    new_menu.append(Some("New Empty File"), Some("app.new-file"));

    let new_btn_content = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    new_btn_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    let new_btn_label = gtk::Label::new(Some("New"));
    new_btn_label.add_css_class("adaptive-label");
    new_btn_content.append(&new_btn_label);

    let new_type_btn = gtk::MenuButton::builder()
        .child(&new_btn_content)
        .tooltip_text("Create New")
        .menu_model(&new_menu)
        .build();
    header_bar.pack_start(&new_type_btn);

    // Zoom Controls
    let zoom_group = Box::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["linked"])
        .build();

    let zoom_out_btn = gtk::Button::builder()
        .icon_name("zoom-out-symbolic")
        .tooltip_text("Zoom Out (Ctrl+-)")
        .action_name("app.zoom-out")
        .build();
    zoom_group.append(&zoom_out_btn);

    let zoom_in_btn = gtk::Button::builder()
        .icon_name("zoom-in-symbolic")
        .tooltip_text("Zoom In (Ctrl++)")
        .action_name("app.zoom-in")
        .build();
    zoom_group.append(&zoom_in_btn);

    header_bar.pack_end(&zoom_group);

    main_content.append(&header_bar);

    let toast_overlay = ToastOverlay::new();

    let settings = gio::Settings::new("net.nocopypaste.chvarkov");

    // Responsive labels logic
    let new_label_weak = new_btn_label.downgrade();
    let view_label_weak = view_btn_label.downgrade();
    let sort_label_weak = sort_btn_label.downgrade();

    // Poll for width changes as a robust workaround in GTK4
    let win_weak = window.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        if let Some(win) = win_weak.upgrade() {
            let width = win.width();
            let show = width > 900;
            if let Some(l) = new_label_weak.upgrade() { l.set_visible(show); }
            if let Some(l) = view_label_weak.upgrade() { l.set_visible(show); }
            if let Some(l) = sort_label_weak.upgrade() { l.set_visible(show); }
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });

    // Initial check
    let initial_width = window.width();
    new_btn_label.set_visible(initial_width > 900);
    view_btn_label.set_visible(initial_width > 900);
    sort_btn_label.set_visible(initial_width > 900);

    let show_sidebar = app.lookup_action("toggle-sidebar")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<bool>().unwrap())
        .unwrap_or(true);

    let show_hidden = app.lookup_action("show-hidden")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<bool>().unwrap())
        .unwrap_or(false);

    let show_meta = app.lookup_action("show-meta")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<bool>().unwrap())
        .unwrap_or(false);

    let zoom_level = app.lookup_action("zoom-level")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<i32>().unwrap())
        .unwrap_or(0);

    let folders_first = settings.boolean("folders-first");

    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .build();

    let columns_box = Box::builder()
        .orientation(Orientation::Horizontal)
        .build();

    scrolled_window.set_child(Some(&columns_box));

    // Align horizontal scroll to the right during resize and column addition
    let adj = scrolled_window.hadjustment();
    let snap_to_right = |a: &gtk::Adjustment| {
        let a = a.clone();
        glib::idle_add_local(move || {
            a.set_value(a.upper() - a.page_size());
            glib::ControlFlow::Break
        });
    };

    adj.connect_upper_notify(snap_to_right);
    adj.connect_page_size_notify(snap_to_right);

    let manager = Rc::new(ColumnManager::new(columns_box, scrolled_window, show_hidden, show_meta, zoom_level, sort_type, folders_first));
    *manager.toast_overlay.borrow_mut() = Some(toast_overlay.clone());
    ACTIVE_MANAGER.with(|m| *m.borrow_mut() = Some(manager.clone()));

    // Breadcrumb Bar - Zero margins for absolute alignment
    let breadcrumb_bar = Box::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["linked", "breadcrumb-bar"])
        .margin_top(0)
        .margin_bottom(0)
        .margin_start(12)
        .margin_end(12)
        .build();

    let breadcrumb_scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&breadcrumb_bar)
        .visible(false)
        .css_classes(["breadcrumb-container-scrolled"])
        .build();

    manager.set_breadcrumb_container(breadcrumb_bar.clone(), breadcrumb_scrolled.clone());

    let current_path_str: String = settings.get("current-path");
    let default_path_str: String = settings.get("default-path");
    let initial_path = if !current_path_str.is_empty() {
        PathBuf::from(&current_path_str)
    } else if !default_path_str.is_empty() {
        PathBuf::from(&default_path_str)
    } else {
        glib::home_dir()
    };

    let sidebar = Sidebar::new();

    // The OverlaySplitView insets the sidebar pane a few px at the top, so the
    // sidebar header sits lower than the main header even though they're the same
    // height. The exact inset is platform-dependent, so once both are laid out we
    // measure it and nudge the higher-positioned header down to match, lining up
    // the two top bars pixel-for-pixel.
    {
        let th = sidebar.title_header.clone().upcast::<gtk::Widget>();
        let mh = header_bar.clone().upcast::<gtk::Widget>();
        let win = window.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            if th.height() <= 1 || mh.height() <= 1 {
                return glib::ControlFlow::Continue; // not laid out yet
            }
            let top = |w: &gtk::Widget| w.translate_coordinates(&win, 0.0, 0.0).map(|(_, y)| y);
            let (Some(ty), Some(my)) = (top(&th), top(&mh)) else {
                return glib::ControlFlow::Continue;
            };
            let delta = (ty - my).round() as i32;
            if delta > 0 {
                mh.set_margin_top(mh.margin_top() + delta);
            } else if delta < 0 {
                th.set_margin_top(th.margin_top() - delta);
            }
            glib::ControlFlow::Break
        });
    }

    split_view.set_sidebar(Some(&sidebar.widget));
    split_view.set_show_sidebar(show_sidebar);

    // macOS window controls are on the start (left) side.
    // In a split view, they should be in the sidebar's header.
    // If the sidebar is hidden, they should move to the main header.
    sidebar.title_header.set_show_end_title_buttons(false);
    header_bar.set_show_start_title_buttons(!show_sidebar);

    let header_bar_limit = header_bar.clone();
    split_view.connect_notify_local(Some("show-sidebar"), move |sv, _| {
        if let Ok(sv) = sv.clone().downcast::<OverlaySplitView>() {
            header_bar_limit.set_show_start_title_buttons(!sv.shows_sidebar());
        }
    });

    let manager_sidebar_clone = manager.clone();
    let split_view_row_clone = split_view.clone();
    sidebar.list_box.connect_row_activated(move |list_box, list_row| {
        let path_string = list_row.widget_name();
        let path = PathBuf::from(path_string.as_str());
        let settings = gio::Settings::new("net.nocopypaste.chvarkov");
        let _ = settings.set_string("current-path", &path.to_string_lossy());

        // Update highlighting
        let mut current = list_box.first_child();
        while let Some(child) = current {
            child.remove_css_class("sidebar-active");
            current = child.next_sibling();
        }
        let list_row_clone = list_row.clone();
        glib::idle_add_local(move || {
            list_row_clone.add_css_class("sidebar-active");
            glib::ControlFlow::Break
        });

        // Hide sidebar in collapsed/mobile mode after activation
        if split_view_row_clone.is_collapsed() {
            split_view_row_clone.set_show_sidebar(false);
        }

        let view_type: String = settings.get("view-type");
        if view_type == "icons" || view_type == "list" {
            if let Some(app) = gio::Application::default() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        } else {
            let manager_clone = manager_sidebar_clone.clone();
            glib::idle_add_local(move || {
                let path = path.clone();
                manager_clone.add_column(path, 0);
                glib::ControlFlow::Break
            });
        }
    });

    // Initial highlight
    let current_path_norm = initial_path.to_string_lossy().to_string();
    let mut current = sidebar.list_box.first_child();
    while let Some(child) = current {
        if child.widget_name() == current_path_norm {
            child.add_css_class("sidebar-active");
            break;
        }
        current = child.next_sibling();
    }

    // Content row: the active view (expands) plus the docked preview pane (right).
    let content_row = Box::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .vexpand(true)
        .build();
    let (dock_container, dock_content) = build_preview_dock();
    manager.set_preview_dock(dock_container.clone(), dock_content);

    if view_type == "miller" {
        content_row.append(&manager.scrolled_window);
        let first_list_view = manager.add_column(initial_path.clone(), 0);

        if let Some(lv) = first_list_view {
            lv.add_css_class("focused-column");
            lv.grab_focus();
        }
    } else if view_type == "icons" {
        let icon_view = IconView::new(&initial_path, show_hidden, show_meta, zoom_level, &manager.sort_type, folders_first);

        let manager_icon_clone = manager.clone();
        let path_icon_clone = initial_path.clone();
        icon_view.grid_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::MultiSelection>().unwrap();
            manager_icon_clone.handle_selection_change_multi(selection_model, &path_icon_clone, 0);
        });

        icon_view.widget.set_hexpand(true);
        content_row.append(&icon_view.widget);
        icon_view.grid_view.add_css_class("focused-grid");
        icon_view.grid_view.grab_focus();
        manager.set_main_view(icon_view.grid_view.clone().upcast::<gtk::Widget>());
    } else if view_type == "list" {
        let list_view_widget = ListView::new(&initial_path, show_hidden, show_meta, zoom_level, &manager.sort_type, folders_first);

        let manager_list_clone = manager.clone();
        let path_list_clone = initial_path.clone();
        list_view_widget.column_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::MultiSelection>().unwrap();
            manager_list_clone.handle_selection_change_multi(selection_model, &path_list_clone, 0);
        });

        list_view_widget.widget.set_hexpand(true);
        content_row.append(&list_view_widget.widget);
        list_view_widget.column_view.add_css_class("focused-list");
        list_view_widget.column_view.grab_focus();
        manager.set_main_view(list_view_widget.column_view.clone().upcast::<gtk::Widget>());
    } else {
        let label = gtk::Label::new(Some(&format!("{} view is not yet implemented", view_type)));
        label.set_vexpand(true);
        label.set_hexpand(true);
        content_row.append(&label);
    }

    content_row.append(&dock_container);
    main_content.append(&content_row);
    main_content.append(&breadcrumb_scrolled);

    toast_overlay.set_child(Some(&main_content));
    split_view.set_content(Some(&toast_overlay));

    window.set_content(Some(&split_view));
    window.present();
}

/// Build the docked preview pane: a fixed-width, resizable container (hidden by
/// default) plus the scrolled window whose child is swapped to the current preview.
/// Returns (container, content_scrolled). The resizer is on the dock's left edge.
fn build_preview_dock() -> (Box, ScrolledWindow) {
    let content = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .width_request(300)
        .vexpand(true)
        .build();

    let resizer = gtk::Separator::new(Orientation::Vertical);
    resizer.set_cursor_from_name(Some("col-resize"));
    resizer.add_css_class("resizer");

    let drag = gtk::GestureDrag::new();
    let content_weak = content.downgrade();
    let resizer_weak = resizer.downgrade();
    drag.connect_drag_update(move |g, off_x, off_y| {
        if let (Some(sw), Some(rz)) = (content_weak.upgrade(), resizer_weak.upgrade())
            && let Some((sx, sy)) = g.start_point() {
                let (cx, cy) = (sx + off_x, sy + off_y);
                // The dock content is to the RIGHT of the resizer; dragging the
                // handle left should widen it. Pointer x in the content's frame goes
                // negative as it moves left of the content edge, so width - dx grows.
                if let Some((dx, _)) = rz.translate_coordinates(&sw, cx, cy) {
                    let new_w = (sw.width() as f64 - dx).round() as i32;
                    sw.set_width_request(new_w.max(150));
                }
            }
    });
    resizer.add_controller(drag);

    let container = Box::builder().orientation(Orientation::Horizontal).build();
    container.append(&resizer);
    container.append(&content);
    container.set_visible(false);
    (container, content)
}

/// Generic "enter a name" dialog. Shows an entry pre-filled with `initial` (text
/// pre-selected so typing replaces it); on confirm it renames `path` to the entered
/// name via `set_display_name_async`. Used both for renaming and for naming a
/// freshly-created item. Confirm response id is "confirm".
pub(crate) fn show_name_dialog(
    parent: &impl IsA<gtk::Widget>,
    manager: Rc<ColumnManager>,
    heading: &str,
    body: &str,
    confirm_label: &str,
    initial: &str,
    path: PathBuf,
) {
    let initial = initial.to_string();
    let entry = gtk::Entry::builder()
        .text(&initial)
        .activates_default(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    // Clone for the post-present selection closure (the response closure moves `entry`).
    let entry_sel = entry.clone();

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .extra_child(&entry)
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("confirm", confirm_label);
    dialog.set_default_response(Some("confirm"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);

    let path_clone = path.clone();
    let manager_c = manager.clone();
    let initial_c = initial.clone();
    dialog.connect_response(None, move |_d, response| {
        if response == "confirm" {
            let new_name = entry.text().to_string();
            if new_name != initial_c {
                if let Err(reason) = file_ops::validate_filename(&new_name) {
                    manager_c.send_toast(&reason);
                    return;
                }
                let file = gio::File::for_path(&path_clone);
                let manager_inner = manager_c.clone();
                let name_to_report = new_name.clone();
                file.set_display_name_async(
                    &new_name,
                    glib::Priority::DEFAULT,
                    gio::Cancellable::NONE,
                    move |res| match res {
                        Ok(_) => manager_inner.send_toast(&format!("Renamed to {}", name_to_report)),
                        Err(e) => manager_inner.send_toast(&format!("Error: {}", e)),
                    },
                );
            }
        }
    });

    dialog.present(Some(parent));

    // Pre-select the name after the dialog is mapped so typing replaces it.
    glib::idle_add_local_once(move || {
        if entry_sel.is_realized() {
            entry_sel.grab_focus();
            entry_sel.select_region(0, -1);
        }
    });
}

fn show_rename_dialog(parent: &ApplicationWindow, manager: Rc<ColumnManager>, old_name_str: &str, path: PathBuf) {
    show_name_dialog(
        parent,
        manager,
        "Rename File",
        &format!("Enter a new name for '{}':", old_name_str),
        "Rename",
        old_name_str,
        path,
    );
}


#[derive(Clone)]
struct SelectionInfo {
    file_info: gio::FileInfo,
    path: PathBuf,
}

#[derive(Clone)]
struct ColumnEntry {
    container: gtk::Widget,
    focus_target: gtk::Widget,
    path: PathBuf,
}

#[derive(Clone)]
struct ColumnManager {
    columns_box: Box,
    scrolled_window: ScrolledWindow,
    breadcrumb_container: Rc<RefCell<Option<Box>>>,
    breadcrumb_parent: Rc<RefCell<Option<ScrolledWindow>>>,
    main_view: Rc<RefCell<Option<gtk::Widget>>>,
    show_hidden: bool,
    show_meta: bool,
    zoom_level: i32,
    sort_type: String,
    folders_first: bool,
    entries: Rc<RefCell<Vec<ColumnEntry>>>,
    current_selection: Rc<RefCell<Option<SelectionInfo>>>,
    preview_window: Rc<RefCell<Option<adw::Window>>>,
    preview_dock: Rc<RefCell<Option<Box>>>,
    preview_dock_content: Rc<RefCell<Option<ScrolledWindow>>>,
    toast_overlay: Rc<RefCell<Option<ToastOverlay>>>,
    clipboard: Rc<RefCell<Option<clipboard::ClipboardOp>>>,
}

impl ColumnManager {
    fn new(columns_box: Box, scrolled_window: ScrolledWindow, show_hidden: bool, show_meta: bool, zoom_level: i32, sort_type: String, folders_first: bool) -> Self {
        Self {
            columns_box,
            scrolled_window,
            breadcrumb_container: Rc::new(RefCell::new(None)),
            breadcrumb_parent: Rc::new(RefCell::new(None)),
            main_view: Rc::new(RefCell::new(None)),
            show_hidden,
            show_meta,
            zoom_level,
            sort_type,
            folders_first,
            entries: Rc::new(RefCell::new(Vec::new())),
            current_selection: Rc::new(RefCell::new(None)),
            preview_window: Rc::new(RefCell::new(None)),
            preview_dock: Rc::new(RefCell::new(None)),
            preview_dock_content: Rc::new(RefCell::new(None)),
            toast_overlay: Rc::new(RefCell::new(None)),
            clipboard: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn send_toast(&self, message: &str) {
        if let Some(overlay) = self.toast_overlay.borrow().as_ref() {
            overlay.add_toast(Toast::new(message));
        }
    }

    /// The directory currently shown by the focused column/view (paste target).
    pub(crate) fn focused_dir(&self) -> Option<PathBuf> {
        let view = self.get_focused_list_view()?;
        for entry in self.entries.borrow().iter() {
            if entry.focus_target == view {
                return Some(entry.path.clone());
            }
        }
        if let Some(sel) = self.current_selection.borrow().as_ref()
            && let Some(parent) = sel.path.parent() { return Some(parent.to_path_buf()); }
        let settings = gio::Settings::new("net.nocopypaste.chvarkov");
        let p: String = settings.get("current-path");
        if p.is_empty() { Some(glib::home_dir()) } else { Some(PathBuf::from(p)) }
    }

    /// Rebuild the UI from the current path (reuses the existing window).
    pub(crate) fn refresh(&self) {
        if let Some(app) = gio::Application::default() {
            glib::idle_add_local(move || { app.activate(); glib::ControlFlow::Break });
        }
    }

    /// Called after a file is trashed or permanently deleted.
    /// Clears the current selection and collapses any child columns that were
    /// opened from the deleted path, keeping only the parent column focused.
    pub(crate) fn on_file_deleted(&self, deleted_path: &std::path::Path) {
        *self.current_selection.borrow_mut() = None;

        let parent = deleted_path.parent().map(|p| p.to_path_buf());
        let keep_count = if let Some(parent_path) = &parent {
            let entries = self.entries.borrow();
            entries.iter().position(|e| &e.path == parent_path)
                .map(|i| i + 1)
                .unwrap_or(entries.len())
        } else {
            self.entries.borrow().len()
        };

        let mut entries = self.entries.borrow_mut();
        while entries.len() > keep_count {
            let entry = entries.pop().unwrap();
            self.columns_box.remove(&entry.container);
        }

        self.update_preview_if_open();
        self.update_dock(false); // hide the docked preview for the deleted selection
    }

    fn set_main_view(&self, view: gtk::Widget) {
        *self.main_view.borrow_mut() = Some(view);
    }

    fn set_breadcrumb_container(&self, container: Box, parent: ScrolledWindow) {
        *self.breadcrumb_container.borrow_mut() = Some(container);
        *self.breadcrumb_parent.borrow_mut() = Some(parent);
    }

    /// Navigate to `path` within the Miller chain. If a column already shows that
    /// path, scroll/focus it and KEEP the deeper columns (walk the history instead of
    /// collapsing it). Otherwise (e.g. an ancestor above the root column), rebuild
    /// from there.
    fn navigate_to_column(&self, path: &std::path::Path) {
        let target = self
            .entries
            .borrow()
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.focus_target.clone());

        match target {
            Some(focus_target) => {
                for entry in self.entries.borrow().iter() {
                    entry.focus_target.remove_css_class("focused-column");
                }
                focus_target.add_css_class("focused-column");
                focus_target.grab_focus(); // scrolls the column into view
            }
            None => {
                // Only a directory can open as a column (skip a selected file's own
                // leaf breadcrumb).
                if path.is_dir() {
                    self.add_column(path.to_path_buf(), 0);
                }
            }
        }
    }

    fn update_breadcrumbs(&self, path: &std::path::Path) {
        if let Some(container) = self.breadcrumb_container.borrow().as_ref() {
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            let mut parts = Vec::new();
            let mut current = Some(path);
            while let Some(p) = current {
                if let Some(name) = p.file_name() {
                    parts.push((name.to_string_lossy().to_string(), p.to_path_buf()));
                } else if p.to_string_lossy() == "/" {
                    parts.push(("/".to_string(), p.to_path_buf()));
                }
                current = p.parent();
            }
            parts.reverse();

            if let Some(parent) = self.breadcrumb_parent.borrow().as_ref() {
                parent.set_visible(!parts.is_empty());
            }

            for (i, (name, p)) in parts.iter().enumerate() {
                if i > 0 {
                    let sep = gtk::Label::new(Some(" / "));
                    sep.add_css_class("dim-label");
                    container.append(&sep);
                }

                let btn = gtk::Button::builder()
                    .label(name)
                    .has_frame(false)
                    .build();

                let p_clone = p.clone();
                let manager_clone = self.clone();
                btn.connect_clicked(move |_| {
                    let path = p_clone.clone();
                    let settings = gio::Settings::new("net.nocopypaste.chvarkov");
                    let _ = settings.set_string("current-path", &path.to_string_lossy());

                    let view_type: String = settings.get("view-type");
                    if view_type == "icons" || view_type == "list" {
                        if let Some(app) = gio::Application::default() {
                            app.activate();
                        }
                    } else {
                        // Walk the existing Miller chain instead of collapsing it.
                        manager_clone.navigate_to_column(&path);
                    }
                });
                container.append(&btn);
            }
        }
    }

    fn toggle_preview(&self, parent: &ApplicationWindow) {
        let mut window_slot = self.preview_window.borrow_mut();
        if let Some(window) = window_slot.take() {
            window.close();
        } else if let Some(selection) = self.current_selection.borrow().as_ref() {
            let window = self.create_preview_window(parent, selection);
            window.present();
            *window_slot = Some(window);
        }
    }

    fn create_preview_window(&self, parent: &ApplicationWindow, selection: &SelectionInfo) -> adw::Window {
        let preview_layout = Preview::create_preview_layout(&selection.file_info, &selection.path, true);
        // Wrap in a toolbar view + header bar so the window has a visible close button.
        let toolbar_view = adw::ToolbarView::builder().content(&preview_layout).build();
        toolbar_view.add_top_bar(&adw::HeaderBar::new());

        let window = adw::Window::builder()
            .transient_for(parent)
            .default_width(800)
            .default_height(600)
            .modal(true)
            .content(&toolbar_view)
            .build();

        let manager_clone = self.clone();
        let window_clone = window.clone();

        window.connect_close_request(move |_| {
            *manager_clone.preview_window.borrow_mut() = None;
            glib::Propagation::Proceed
        });

        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let manager_key_clone = self.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            match key {
                gtk::gdk::Key::Escape | gtk::gdk::Key::space => {
                    window_clone.close();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Up | gtk::gdk::Key::Down => {
                    if let Some(lv) = manager_key_clone.get_focused_list_view()
                        && let Some(sm) = get_selection_model(&lv) {
                            let selection = sm.selection();
                            if !selection.is_empty() {
                                 let current = selection.minimum();
                                 if key == gtk::gdk::Key::Up && current > 0 {
                                     sm.select_item(current - 1, true);
                                 } else if key == gtk::gdk::Key::Down && current + 1 < sm.n_items() {
                                     sm.select_item(current + 1, true);
                                 }
                            }
                        }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(key_controller);
        window
    }

    fn get_focused_list_view(&self) -> Option<gtk::Widget> {
        // PRIMARY: use the real keyboard focus. The `focused-column` CSS class is
        // only maintained by arrow-key navigation, so relying on it alone makes
        // mouse-driven selection target the wrong column — a data-loss hazard for
        // destructive actions (e.g. deleting a file could hit its parent folder).
        // The widget that actually contains the focus is the source of truth.
        let root = self
            .columns_box
            .root()
            .or_else(|| self.main_view.borrow().as_ref().and_then(|m| m.root()));
        if let Some(focus) = root.and_then(|r| r.focus()) {
            let entries = self.entries.borrow();
            for entry in entries.iter() {
                if focus == entry.focus_target || focus.is_ancestor(&entry.focus_target) {
                    return Some(entry.focus_target.clone());
                }
            }
            if let Some(main) = self.main_view.borrow().as_ref()
                && (focus == *main || focus.is_ancestor(main)) {
                    return Some(main.clone());
                }
        }

        // FALLBACK (no live focus, e.g. just after a rebuild): the CSS class set
        // by keyboard navigation.
        let entries = self.entries.borrow();
        for entry in entries.iter() {
            if entry.focus_target.has_css_class("focused-column") {
                return Some(entry.focus_target.clone());
            }
        }
        if let Some(main) = self.main_view.borrow().as_ref()
            && (main.has_css_class("focused-grid") || main.has_css_class("focused-list")) {
                return Some(main.clone());
            }
        None
    }

    /// Gather every selected item in the currently focused view as SelectionInfo.
    /// Empty if nothing is focused/selected. Paths resolve via `standard::file`,
    /// so nested List-view rows are correct.
    fn collect_selection(&self) -> Vec<SelectionInfo> {
        let Some(view) = self.get_focused_list_view() else { return Vec::new() };
        let Some(sm) = get_selection_model(&view) else { return Vec::new() };
        // Directory the focused view is listing. Used only as a fallback inside
        // file_info_path when a FileInfo lacks `standard::file` (our DirectoryList
        // queries always include it, so this is belt-and-suspenders).
        let base = self
            .entries
            .borrow()
            .iter()
            .find(|e| e.focus_target == view)
            .map(|e| e.path.clone())
            .or_else(|| {
                self.current_selection
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.path.parent().map(|p| p.to_path_buf()))
            })
            .unwrap_or_else(glib::home_dir);

        let selection = sm.selection();
        let Some(model) = sm.model() else { return Vec::new() };
        let mut out = Vec::new();
        for i in 0..selection.size() {
            let pos = selection.nth(i as u32);
            let Some(item) = model.item(pos) else { continue };
            let info = if let Ok(tree_row) = item.clone().downcast::<gtk::TreeListRow>() {
                tree_row.item().and_downcast::<gio::FileInfo>()
            } else {
                item.downcast::<gio::FileInfo>().ok()
            };
            if let Some(info) = info {
                let path = file_info_path(&info, &base);
                out.push(SelectionInfo { file_info: info, path });
            }
        }
        out
    }

    fn update_preview_if_open(&self) {
        if let Some(window) = self.preview_window.borrow().as_ref()
            && let Some(selection) = self.current_selection.borrow().as_ref() {
                let preview_layout = Preview::create_preview_layout(&selection.file_info, &selection.path, true);
                let toolbar_view = adw::ToolbarView::builder().content(&preview_layout).build();
                toolbar_view.add_top_bar(&adw::HeaderBar::new());
                window.set_content(Some(&toolbar_view));
            }
    }

    fn set_preview_dock(&self, container: Box, content: ScrolledWindow) {
        *self.preview_dock.borrow_mut() = Some(container);
        *self.preview_dock_content.borrow_mut() = Some(content);
    }

    /// Show the docked preview for the current single-file selection, or hide it.
    /// Clearing the content on hide drops the previous preview widget (so video
    /// playback stops).
    fn update_dock(&self, show: bool) {
        let container = self.preview_dock.borrow().clone();
        let content = self.preview_dock_content.borrow().clone();
        let (Some(container), Some(content)) = (container, content) else { return };
        if show
            && let Some(sel) = self.current_selection.borrow().as_ref() {
                let layout = Preview::create_preview_layout(&sel.file_info, &sel.path, false);
                content.set_child(Some(&layout));
                container.set_visible(true);
                return;
            }
        content.set_child(None::<&gtk::Widget>);
        container.set_visible(false);
    }

    fn add_column(&self, path: PathBuf, index: usize) -> Option<gtk::ListView> {
        {
            let entries = self.entries.borrow();
            if index < entries.len() && entries[index].path == path {
                return entries[index].focus_target.clone().downcast::<gtk::ListView>().ok();
            }
        }

        let column = Column::new(&path, self.show_hidden, self.show_meta, self.zoom_level, &self.sort_type, self.folders_first);
        let column_widget = column.widget.clone().upcast::<gtk::Widget>();
        let list_view = column.list_view.clone();

        {
            let mut entries = self.entries.borrow_mut();

            while entries.len() > index {
                let entry = entries.pop().unwrap();
                self.columns_box.remove(&entry.container);
            }

            self.columns_box.append(&column_widget);
            entries.push(ColumnEntry {
                container: column_widget.clone(),
                focus_target: list_view.clone().upcast::<gtk::Widget>(),
                path: path.clone(),
            });
        }

        let adj = self.scrolled_window.hadjustment();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            adj.set_value(adj.upper() - adj.page_size());
            glib::ControlFlow::Break
        });

        let self_clone = self.clone();
        let path_clone = path.clone();
        let index_clone = index;
        column.selection_model.connect_selection_changed(move |selection_model, _, _| {
            let self_idle = self_clone.clone();
            let selection_idle = selection_model.clone().downcast::<gtk::MultiSelection>().unwrap();
            let path_idle = path_clone.clone();
            glib::idle_add_local(move || {
                self_idle.handle_selection_change_multi(&selection_idle, &path_idle, index_clone);
                glib::ControlFlow::Break
            });
        });

        let key_controller = gtk::EventControllerKey::new();
        let self_key_clone = self.clone();
        let list_view_focus = list_view.clone();
        let path_key_clone = path.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Right {
                let selection_model = list_view_focus.model().unwrap().downcast::<gtk::MultiSelection>().unwrap();

                if selection_model.selection().is_empty() {
                    selection_model.select_item(0, true);
                    return glib::Propagation::Stop;
                }

                self_key_clone.handle_selection_change_multi(&selection_model, &path_key_clone, index);

                let entries = self_key_clone.entries.borrow();
                if index + 1 < entries.len() {
                    list_view_focus.remove_css_class("focused-column");
                    let target_entry = &entries[index + 1];
                    let target = &target_entry.focus_target;

                    if let Ok(lv) = target.clone().downcast::<gtk::ListView>() {
                        let sel = lv.model().unwrap().downcast::<gtk::MultiSelection>().unwrap();
                        if sel.selection().is_empty() {
                            sel.select_item(0, true);
                        }
                    }

                    target.add_css_class("focused-column");
                    target.grab_focus();
                    return glib::Propagation::Stop;
                }
            } else if key == gtk::gdk::Key::Left
                && index > 0 {
                    let entries = self_key_clone.entries.borrow();
                    list_view_focus.remove_css_class("focused-column");
                    let target_entry = &entries[index - 1];
                    let target = &target_entry.focus_target;
                    target.add_css_class("focused-column");
                    target.grab_focus();

                    if let Ok(lv) = target.clone().downcast::<gtk::ListView>() {
                        let sel = lv.model().unwrap().downcast::<gtk::MultiSelection>().unwrap();
                        self_key_clone.handle_selection_change_multi(&sel, &target_entry.path, index - 1);
                    }

                    return glib::Propagation::Stop;
                }
            glib::Propagation::Proceed
        });
        list_view.add_controller(key_controller);

        Some(list_view)
    }

    fn handle_selection_change_multi(&self, selection_model: &gtk::MultiSelection, base_path: &std::path::Path, index: usize) {
        let selection = selection_model.selection();
        if selection.is_empty() {
            *self.current_selection.borrow_mut() = None;
            self.update_breadcrumbs(base_path);

            // Clear subsequent columns
            let mut entries = self.entries.borrow_mut();
            while entries.len() > index + 1 {
                let entry = entries.pop().unwrap();
                self.columns_box.remove(&entry.container);
            }

            self.update_preview_if_open();
            self.update_dock(false);
            return;
        }

        // For previews and navigation, we use the first selected item
        let first_idx = selection.minimum();
        let Some(model) = selection_model.model() else { return };
        let selected_item = model.item(first_idx);

        if let Some(item) = selected_item {
            // Handle TreeListRow wrapping if it's a List View
            let file_info = if let Ok(tree_row) = item.clone().downcast::<gtk::TreeListRow>() {
                let Some(fi) = tree_row.item().and_downcast::<gio::FileInfo>() else { return };
                fi
            } else {
                let Some(fi) = item.downcast_ref::<gio::FileInfo>() else { return };
                fi.clone()
            };

            let new_path = file_info_path(&file_info, base_path);

            *self.current_selection.borrow_mut() = Some(SelectionInfo {
                file_info: file_info.clone(),
                path: new_path.clone(),
            });

            self.update_breadcrumbs(&new_path);
            self.update_preview_if_open();

            let is_dir = file_info.file_type() == gio::FileType::Directory || new_path.is_dir();
            self.update_dock(utils::should_dock_preview(selection.size() as usize, is_dir));

            // In List View, we don't necessarily want to jump columns unless it's Miller
            let settings = gio::Settings::new("net.nocopypaste.chvarkov");
            let view_type: String = settings.get("view-type");

            if view_type == "miller" {
                if is_dir {
                    self.add_column(new_path, index + 1);
                } else {
                    // Selecting a file collapses any deeper columns; the preview now
                    // lives in the docked pane rather than an appended column.
                    let mut entries = self.entries.borrow_mut();
                    while entries.len() > index + 1 {
                        let entry = entries.pop().unwrap();
                        self.columns_box.remove(&entry.container);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_info_path_uses_standard_file_attr() {
        let f = gio::File::for_path("/tmp/some/nested/file.txt");
        let info = gio::FileInfo::new();
        info.set_attribute_object("standard::file", &f);
        let p = file_info_path(&info, std::path::Path::new("/base"));
        assert_eq!(p, std::path::PathBuf::from("/tmp/some/nested/file.txt"));
    }

    #[test]
    fn file_info_path_falls_back_to_base_join_name() {
        let info = gio::FileInfo::new();
        info.set_name("leaf.txt");
        let p = file_info_path(&info, std::path::Path::new("/base/dir"));
        assert_eq!(p, std::path::PathBuf::from("/base/dir/leaf.txt"));
    }
}
