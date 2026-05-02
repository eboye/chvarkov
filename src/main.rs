mod column;
mod preview;
mod sidebar;
mod icon_view;

use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar};
use gtk4 as gtk;
use gtk::{Box, Orientation, ScrolledWindow};
use std::path::PathBuf;
use column::Column;
use preview::Preview;
use sidebar::Sidebar;
use icon_view::IconView;
use std::rc::Rc;
use std::cell::RefCell;

// Use a thread-local for the active manager to avoid unsafe set_data and NonNull issues
thread_local! {
    static ACTIVE_MANAGER: RefCell<Option<Rc<ColumnManager>>> = const { RefCell::new(None) };
}

fn main() {
    // For development, point GSETTINGS_SCHEMA_DIR to our compiled schemas
    unsafe {
        std::env::set_var("GSETTINGS_SCHEMA_DIR", "./compiled_schemas");
    }

    let application = Application::builder()
        .application_id("com.example.ArchFinder")
        .build();

    application.connect_startup(|app| {
        setup_actions(app);
        setup_styles();
    });
    application.connect_activate(build_ui);

    application.run();
}

fn setup_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data("
        /* Focused list selection - Vibrant accent color */
        .focused-column row:selected, .focused-grid row:selected {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            border-radius: 6px;
        }

        /* Unfocused list selection (parent columns) - Subdued color */
        listview row:selected, gridview row:selected {
            background-color: alpha(@accent_bg_color, 0.2);
            color: @view_fg_color;
            border-radius: 6px;
        }

        /* Hover effect for rows */
        listview row:hover:not(:selected), gridview row:hover:not(:selected) {
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
        }
    ");

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn setup_actions(app: &Application) {
    let settings = gio::Settings::new("com.example.ArchFinder");

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
            if let Some(manager) = m.borrow().as_ref() {
                if let Some(selection) = manager.current_selection.borrow().as_ref() {
                    let file_info = &selection.file_info;
                    let path = &selection.path;
                    
                    let is_dir = file_info.file_type() == gio::FileType::Directory || path.is_dir();
                    
                    if is_dir {
                        let settings = gio::Settings::new("com.example.ArchFinder");
                        let view_type: String = settings.get("view-type");
                        if view_type == "icons" {
                             if let Some(app) = gio::Application::default() {
                                 glib::idle_add_local(move || {
                                     app.activate();
                                     glib::ControlFlow::Break
                                 });
                             }
                        }
                    } else {
                        let file = gio::File::for_path(path);
                        gio::AppInfo::launch_default_for_uri(&file.uri(), None::<&gio::AppLaunchContext>).ok();
                    }
                }
            }
        });
    });
    app.add_action(&open_action);
    app.set_accels_for_action("app.open", &["Return"]);

    let cut_action = gio::SimpleAction::new("cut", None);
    cut_action.connect_activate(|_, _| println!("Cut action triggered"));
    app.add_action(&cut_action);
    app.set_accels_for_action("app.cut", &["<Control>x"]);

    let copy_action = gio::SimpleAction::new("copy", None);
    copy_action.connect_activate(|_, _| println!("Copy action triggered"));
    app.add_action(&copy_action);
    app.set_accels_for_action("app.copy", &["<Control>c"]);

    let paste_action = gio::SimpleAction::new("paste", None);
    paste_action.connect_activate(|_, _| println!("Paste action triggered"));
    app.add_action(&paste_action);
    app.set_accels_for_action("app.paste", &["<Control>v"]);

    let move_to_action = gio::SimpleAction::new("move-to", None);
    move_to_action.connect_activate(|_, _| println!("Move to action triggered"));
    app.add_action(&move_to_action);

    let copy_to_action = gio::SimpleAction::new("copy-to", None);
    copy_to_action.connect_activate(|_, _| println!("Copy to action triggered"));
    app.add_action(&copy_to_action);

    let rename_action = gio::SimpleAction::new("rename", None);
    rename_action.connect_activate(|_, _| println!("Rename action triggered"));
    app.add_action(&rename_action);
    app.set_accels_for_action("app.rename", &["F2"]);

    let create_link_action = gio::SimpleAction::new("create-link", None);
    create_link_action.connect_activate(|_, _| println!("Create link action triggered"));
    app.add_action(&create_link_action);
    app.set_accels_for_action("app.create-link", &["<Shift><Control>m"]);

    let compress_action = gio::SimpleAction::new("compress", None);
    compress_action.connect_activate(|_, _| println!("Compress action triggered"));
    app.add_action(&compress_action);

    let email_action = gio::SimpleAction::new("email", None);
    email_action.connect_activate(|_, _| println!("Email action triggered"));
    app.add_action(&email_action);

    let delete_action = gio::SimpleAction::new("delete", None);
    delete_action.connect_activate(|_, _| println!("Delete action triggered"));
    app.add_action(&delete_action);
    app.set_accels_for_action("app.delete", &["Delete"]);

    let open_terminal_action = gio::SimpleAction::new("open-terminal", None);
    open_terminal_action.connect_activate(|_, _| println!("Open in Terminal triggered"));
    app.add_action(&open_terminal_action);

    let copy_path_action = gio::SimpleAction::new("copy-path", None);
    copy_path_action.connect_activate(|_, _| println!("Copy Path triggered"));
    app.add_action(&copy_path_action);

    let copy_uri_action = gio::SimpleAction::new("copy-uri", None);
    copy_uri_action.connect_activate(|_, _| println!("Copy URI triggered"));
    app.add_action(&copy_uri_action);

    let copy_name_action = gio::SimpleAction::new("copy-name", None);
    copy_name_action.connect_activate(|_, _| println!("Copy Name triggered"));
    app.add_action(&copy_name_action);

    let sharing_options_action = gio::SimpleAction::new("sharing-options", None);
    sharing_options_action.connect_activate(|_, _| println!("Sharing Options triggered"));
    app.add_action(&sharing_options_action);

    let properties_action = gio::SimpleAction::new("properties", None);
    properties_action.connect_activate(|_, _| println!("Properties triggered"));
    app.add_action(&properties_action);
    app.set_accels_for_action("app.properties", &["<Alt>Return"]);

    let preview_action = gio::SimpleAction::new("preview", None);
    let app_weak_p = app.downgrade();
    preview_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_p.upgrade() {
            ACTIVE_MANAGER.with(|m| {
                if let Some(manager) = m.borrow().as_ref() {
                    if let Some(window) = app.active_window() {
                        let win = window.downcast::<ApplicationWindow>().unwrap();
                        manager.toggle_preview(&win);
                    }
                }
            });
        }
    });
    app.add_action(&preview_action);
    app.set_accels_for_action("app.preview", &["space"]);

    // Use GSettings for persistent actions
    let toggle_sidebar_action = gio::SimpleAction::new_stateful("show-sidebar", None, &settings.value("show-sidebar"));
    let app_weak_s = app.downgrade();
    let settings_s = settings.clone();
    toggle_sidebar_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(&state);
            let _ = settings_s.set_value("show-sidebar", &state);
            if let Some(app) = app_weak_s.upgrade() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        }
    });
    app.add_action(&toggle_sidebar_action);
    app.set_accels_for_action("app.show-sidebar", &["F9"]);

    let show_hidden_action = gio::SimpleAction::new_stateful("show-hidden", None, &settings.value("show-hidden"));
    let app_weak_h = app.downgrade();
    let settings_h = settings.clone();
    show_hidden_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(&state);
            let _ = settings_h.set_value("show-hidden", &state);
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
            action.set_state(&state);
            let _ = settings_m.set_value("show-meta", &state);
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
            if val >= 0 && val <= 5 {
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
        if let Some(app) = app_weak_zi.upgrade() {
            if let Some(action) = app.lookup_action("zoom-level") {
                let current = action.downcast::<gio::SimpleAction>().unwrap().state().unwrap().get::<i32>().unwrap();
                app.activate_action("zoom-level", Some(&(current + 1).to_variant()));
            }
        }
    });
    app.add_action(&zoom_in_action);
    app.set_accels_for_action("app.zoom-in", &["<Control>plus", "<Control>equal"]);

    let zoom_out_action = gio::SimpleAction::new("zoom-out", None);
    let app_weak_zo = app.downgrade();
    zoom_out_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak_zo.upgrade() {
            if let Some(action) = app.lookup_action("zoom-level") {
                let current = action.downcast::<gio::SimpleAction>().unwrap().state().unwrap().get::<i32>().unwrap();
                app.activate_action("zoom-level", Some(&(current - 1).to_variant()));
            }
        }
    });
    app.add_action(&zoom_out_action);
    app.set_accels_for_action("app.zoom-out", &["<Control>minus"]);

    let view_type_action = gio::SimpleAction::new_stateful("view-type", Some(glib::VariantTy::new("s").unwrap()), &settings.value("view-type"));
    let app_weak_v = app.downgrade();
    let settings_v = settings.clone();
    view_type_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(&state);
            let _ = settings_v.set_value("view-type", &state);
            if let Some(app) = app_weak_v.upgrade() {
                glib::idle_add_local(move || {
                    app.activate();
                    glib::ControlFlow::Break
                });
            }
        }
    });
    app.add_action(&view_type_action);
}

fn build_ui(app: &Application) {
    let window = if let Some(window) = app.active_window() {
        window.downcast::<ApplicationWindow>().unwrap()
    } else {
        ApplicationWindow::builder()
            .application(app)
            .default_width(1200)
            .default_height(800)
            .build()
    };

    let root_layout = Box::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .vexpand(true)
        .build();

    let main_content = Box::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let header_bar = HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new("Arch-Finder", ""))
        .build();

    let toggle_sidebar_btn = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Sidebar (F9)")
        .action_name("app.show-sidebar")
        .build();
    header_bar.pack_start(&toggle_sidebar_btn);

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

    let view_menu = gio::Menu::new();
    view_menu.append(Some("Miller Columns"), Some("app.view-type::miller"));
    view_menu.append(Some("Icons View"), Some("app.view-type::icons"));
    view_menu.append(Some("List View"), Some("app.view-type::list"));

    let view_icon = match view_type.as_str() {
        "icons" => "view-grid-symbolic",
        "list" => "view-list-symbolic",
        _ => "view-column-symbolic",
    };

    let view_type_btn = gtk::MenuButton::builder()
        .icon_name(view_icon)
        .tooltip_text("View Options")
        .menu_model(&view_menu)
        .build();
    header_bar.pack_start(&view_type_btn);

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

    let show_sidebar = app.lookup_action("show-sidebar")
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

    let manager = Rc::new(ColumnManager::new(columns_box, scrolled_window, show_hidden, show_meta, zoom_level));
    ACTIVE_MANAGER.with(|m| *m.borrow_mut() = Some(manager.clone()));

    if show_sidebar {
        let sidebar = Sidebar::new();
        root_layout.append(&sidebar.widget);
        
        let manager_sidebar_clone = manager.clone();
        sidebar.list_box.connect_row_activated(move |_, list_row| {
            let path_string = list_row.widget_name();
            let path = PathBuf::from(path_string.as_str());
            let manager_clone = manager_sidebar_clone.clone();
            glib::idle_add_local(move || {
                let path = path.clone();
                manager_clone.add_column(path, 0);
                glib::ControlFlow::Break
            });
        });

        let sep = gtk::Separator::new(Orientation::Vertical);
        root_layout.append(&sep);
    }

    if view_type == "miller" {
        main_content.append(&manager.scrolled_window);
        root_layout.append(&main_content);

        let first_list_view = manager.add_column(glib::home_dir(), 0);

        window.set_content(Some(&root_layout));
        window.present();

        if let Some(lv) = first_list_view {
            lv.add_css_class("focused-column");
            lv.grab_focus();
        }
    } else if view_type == "icons" {
        let current_path = manager.current_selection.borrow().as_ref()
            .map(|s| {
                if s.path.is_dir() {
                    s.path.clone()
                } else {
                    s.path.parent().unwrap_or(&s.path).to_path_buf()
                }
            })
            .unwrap_or_else(glib::home_dir);
            
        let icon_view = IconView::new(&current_path, show_hidden, zoom_level);
        
        let manager_icon_clone = manager.clone();
        let path_icon_clone = current_path.clone();
        icon_view.grid_view.model().unwrap().connect_selection_changed(move |selection_model, _, _| {
            let selection_model = selection_model.downcast_ref::<gtk::SingleSelection>().unwrap();
            manager_icon_clone.handle_selection_change(selection_model, &path_icon_clone, 0);
        });

        main_content.append(&icon_view.widget);
        root_layout.append(&main_content);
        window.set_content(Some(&root_layout));
        window.present();
        
        icon_view.grid_view.add_css_class("focused-grid");
        icon_view.grid_view.grab_focus();
    } else {
        let label = gtk::Label::new(Some(&format!("{} view is not yet implemented", view_type)));
        label.set_vexpand(true);
        main_content.append(&label);
        root_layout.append(&main_content);
        window.set_content(Some(&root_layout));
        window.present();
    }
}

fn show_preview_popup(parent: &ApplicationWindow, selection: &SelectionInfo) {
    let preview_layout = Preview::create_preview_layout(&selection.file_info, &selection.path, true);
    
    let popup = adw::Window::builder()
        .transient_for(parent)
        .default_width(800)
        .default_height(600)
        .modal(true)
        .content(&preview_layout)
        .build();
    
    let key_controller = gtk::EventControllerKey::new();
    let popup_clone = popup.clone();
    key_controller.connect_key_pressed(move |_, _, _, _| {
        popup_clone.close();
        glib::Propagation::Stop
    });
    popup.add_controller(key_controller);

    popup.present();
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
    show_hidden: bool,
    show_meta: bool,
    zoom_level: i32,
    entries: Rc<RefCell<Vec<ColumnEntry>>>,
    current_selection: Rc<RefCell<Option<SelectionInfo>>>,
    preview_window: Rc<RefCell<Option<adw::Window>>>,
}

impl ColumnManager {
    fn new(columns_box: Box, scrolled_window: ScrolledWindow, show_hidden: bool, show_meta: bool, zoom_level: i32) -> Self {
        Self {
            columns_box,
            scrolled_window,
            show_hidden,
            show_meta,
            zoom_level,
            entries: Rc::new(RefCell::new(Vec::new())),
            current_selection: Rc::new(RefCell::new(None)),
            preview_window: Rc::new(RefCell::new(None)),
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
        
        let window = adw::Window::builder()
            .transient_for(parent)
            .default_width(800)
            .default_height(600)
            .modal(true)
            .content(&preview_layout)
            .build();
        
        let manager_clone = self.clone();
        let window_clone = window.clone();
        
        window.connect_close_request(move |_| {
            *manager_clone.preview_window.borrow_mut() = None;
            glib::Propagation::Proceed
        });

        let key_controller = gtk::EventControllerKey::new();
        let manager_key_clone = self.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            match key {
                gtk::gdk::Key::Escape | gtk::gdk::Key::space => {
                    window_clone.close();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Up | gtk::gdk::Key::Down => {
                    if let Some(lv) = manager_key_clone.get_focused_list_view() {
                        let selection_model = lv.model().unwrap().downcast::<gtk::SingleSelection>().unwrap();
                        let current = selection_model.selected();
                        if key == gtk::gdk::Key::Up && current > 0 {
                            selection_model.set_selected(current - 1);
                        } else if key == gtk::gdk::Key::Down {
                            selection_model.set_selected(current + 1);
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

    fn get_focused_list_view(&self) -> Option<gtk::ListView> {
        let entries = self.entries.borrow();
        for entry in entries.iter() {
            if entry.focus_target.has_css_class("focused-column") || entry.focus_target.has_css_class("focused-grid") {
                return entry.focus_target.clone().downcast::<gtk::ListView>().ok();
            }
        }
        None
    }

    fn update_preview_if_open(&self) {
        if let Some(window) = self.preview_window.borrow().as_ref() {
            if let Some(selection) = self.current_selection.borrow().as_ref() {
                let preview_layout = Preview::create_preview_layout(&selection.file_info, &selection.path, true);
                window.set_content(Some(&preview_layout));
            }
        }
    }

    fn add_column(&self, path: PathBuf, index: usize) -> Option<gtk::ListView> {
        {
            let entries = self.entries.borrow();
            if index < entries.len() && entries[index].path == path {
                return entries[index].focus_target.clone().downcast::<gtk::ListView>().ok();
            }
        }

        let column = Column::new(&path, self.show_hidden, self.show_meta, self.zoom_level);
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
            let selection_idle = selection_model.clone();
            let path_idle = path_clone.clone();
            glib::idle_add_local(move || {
                self_idle.handle_selection_change(&selection_idle, &path_idle, index_clone);
                glib::ControlFlow::Break
            });
        });

        let key_controller = gtk::EventControllerKey::new();
        let self_key_clone = self.clone();
        let list_view_focus = list_view.clone();
        let path_key_clone = path.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Right {
                let selection_model = list_view_focus.model().unwrap().downcast::<gtk::SingleSelection>().unwrap();
                
                if selection_model.selected() == gtk::INVALID_LIST_POSITION {
                    selection_model.set_selected(0);
                    return glib::Propagation::Stop;
                }

                self_key_clone.handle_selection_change(&selection_model, &path_key_clone, index);

                let entries = self_key_clone.entries.borrow();
                if index + 1 < entries.len() {
                    list_view_focus.remove_css_class("focused-column");
                    let target_entry = &entries[index + 1];
                    let target = &target_entry.focus_target;
                    
                    if let Ok(lv) = target.clone().downcast::<gtk::ListView>() {
                        let sel = lv.model().unwrap().downcast::<gtk::SingleSelection>().unwrap();
                        if sel.selected() == gtk::INVALID_LIST_POSITION {
                            sel.set_selected(0);
                        }
                    }

                    target.add_css_class("focused-column");
                    target.grab_focus();
                    return glib::Propagation::Stop;
                }
            } else if key == gtk::gdk::Key::Left {
                if index > 0 {
                    let entries = self_key_clone.entries.borrow();
                    list_view_focus.remove_css_class("focused-column");
                    let target_entry = &entries[index - 1];
                    let target = &target_entry.focus_target;
                    target.add_css_class("focused-column");
                    target.grab_focus();

                    if let Ok(lv) = target.clone().downcast::<gtk::ListView>() {
                        let sel = lv.model().unwrap().downcast::<gtk::SingleSelection>().unwrap();
                        self_key_clone.handle_selection_change(&sel, &target_entry.path, index - 1);
                    }
                    
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        list_view.add_controller(key_controller);

        Some(list_view)
    }

    fn handle_selection_change(&self, selection_model: &gtk::SingleSelection, base_path: &PathBuf, index: usize) {
        let selected_item = selection_model.selected_item();
        if let Some(item) = selected_item {
            let file_info = item.downcast_ref::<gio::FileInfo>().unwrap();
            let name = file_info.name();
            let mut new_path = base_path.clone();
            new_path.push(&name);

            let ftype = file_info.file_type();
            let fs_is_dir = new_path.is_dir();
            
            println!("Selection [Column {}]: {:?} | Type: {:?} | FS is_dir: {}", 
                     index, new_path, ftype, fs_is_dir);

            *self.current_selection.borrow_mut() = Some(SelectionInfo {
                file_info: file_info.clone(),
                path: new_path.clone(),
            });
            
            self.update_preview_if_open();

            if ftype == gio::FileType::Directory || fs_is_dir {
                self.add_column(new_path, index + 1);
            } else {
                let mut entries = self.entries.borrow_mut();
                
                let should_replace = if index + 1 < entries.len() {
                    entries[index + 1].path != new_path
                } else {
                    true
                };

                if should_replace {
                    while entries.len() > index + 1 {
                        let entry = entries.pop().unwrap();
                        self.columns_box.remove(&entry.container);
                    }

                    let preview = Preview::new(file_info, &new_path);
                    let preview_widget = preview.widget.upcast::<gtk::Widget>();
                    self.columns_box.append(&preview_widget);
                    entries.push(ColumnEntry {
                        container: preview_widget.clone(),
                        focus_target: preview_widget,
                        path: new_path,
                    });
                }
            }
        } else {
            println!("Selection cleared in Column {}", index);
        }
    }
}
