mod column;
mod preview;
mod sidebar;
mod icon_view;
mod list_view;
mod utils;

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
use list_view::ListView;
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
            border-top: 1px solid alpha(@borders, 0.3);
        }
        .breadcrumb-bar button {
            padding: 4px 8px;
            font-size: 0.9rem;
        }
        .breadcrumb-bar button:hover {
            background-color: alpha(@accent_bg_color, 0.1);
            color: @accent_bg_color;
        }

        /* Adaptive labels */
        .adaptive-label {
            transition: all 200ms ease-in-out;
        }
    ");

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
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

    // Selection Actions
    let select_all_action = gio::SimpleAction::new("select-all", None);
    select_all_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                if let Some(lv) = manager.get_focused_list_view() {
                    if let Some(sm) = get_selection_model(&lv) {
                        sm.select_all();
                    }
                }
            }
        });
    });
    app.add_action(&select_all_action);
    app.set_accels_for_action("app.select-all", &["<Control>a"]);

    let select_none_action = gio::SimpleAction::new("select-none", None);
    select_none_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                if let Some(lv) = manager.get_focused_list_view() {
                    if let Some(sm) = get_selection_model(&lv) {
                        sm.unselect_all();
                    }
                }
            }
        });
    });
    app.add_action(&select_none_action);
    app.set_accels_for_action("app.select-none", &["<Control><Shift>a"]);

    let invert_selection_action = gio::SimpleAction::new("invert-selection", None);
    invert_selection_action.connect_activate(|_, _| {
        ACTIVE_MANAGER.with(|m| {
            if let Some(manager) = m.borrow().as_ref() {
                if let Some(lv) = manager.get_focused_list_view() {
                    if let Some(sm) = get_selection_model(&lv) {
                        let n_items = sm.n_items();
                        for i in 0..n_items {
                            if sm.is_selected(i) {
                                sm.unselect_item(i);
                            } else {
                                sm.select_item(i, false);
                            }
                        }
                    }
                }
            }
        });
    });
    app.add_action(&invert_selection_action);
    app.set_accels_for_action("app.invert-selection", &["<Control>i"]);

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
    app.set_accels_for_action("app.toggle-sidebar", &["F9"]);

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

    let sort_type_action = gio::SimpleAction::new_stateful("sort-type", Some(glib::VariantTy::new("s").unwrap()), &settings.value("sort-type"));
    let app_weak_st = app.downgrade();
    let settings_st = settings.clone();
    sort_type_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(&state);
            let _ = settings_st.set_value("sort-type", &state);
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
    let window = app.active_window().unwrap();
    let settings = gio::Settings::new("com.example.ArchFinder");

    let pref_window = adw::PreferencesWindow::builder()
        .transient_for(&window)
        .modal(true)
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
    let pref_window_weak = pref_window.downgrade();
    pick_button.connect_clicked(move |_| {
        if let Some(pref_window) = pref_window_weak.upgrade() {
            let dialog = gtk::FileDialog::builder()
                .title("Select Default Startup Directory")
                .build();
            
            let settings_c = settings_path.clone();
            let label_c = path_label.clone();
            dialog.select_folder(Some(&pref_window), gio::Cancellable::NONE, move |res| {
                if let Ok(folder) = res {
                    if let Some(path) = folder.path() {
                        let path_str = path.to_string_lossy().to_string();
                        let _ = settings_c.set_string("default-path", &path_str);
                        label_c.set_label(&path_str);
                    }
                }
            });
        }
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

    pref_window.present();
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
        .action_name("app.toggle-sidebar")
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
        _ => "view-column-symbolic",
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

    let preferences_btn = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Preferences")
        .action_name("app.preferences")
        .build();
    header_bar.pack_end(&preferences_btn);

    main_content.append(&header_bar);

    let settings = gio::Settings::new("com.example.ArchFinder");
    
    // Responsive labels logic
    let view_label_weak = view_btn_label.downgrade();
    let sort_label_weak = sort_btn_label.downgrade();
    
    // Poll for width changes as a robust workaround in GTK4
    let win_weak = window.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        if let Some(win) = win_weak.upgrade() {
            let width = win.width();
            let show = width > 900;
            if let Some(l) = view_label_weak.upgrade() { l.set_visible(show); }
            if let Some(l) = sort_label_weak.upgrade() { l.set_visible(show); }
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
    
    // Initial check
    let initial_width = window.width();
    view_btn_label.set_visible(initial_width > 900);
    sort_btn_label.set_visible(initial_width > 900);

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
    ACTIVE_MANAGER.with(|m| *m.borrow_mut() = Some(manager.clone()));

    // Breadcrumb Bar
    let breadcrumb_bar = Box::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["linked", "breadcrumb-bar"])
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    
    let breadcrumb_scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&breadcrumb_bar)
        .visible(false)
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

    if show_sidebar {
        let sidebar = Sidebar::new();
        root_layout.append(&sidebar.widget);
        
        let manager_sidebar_clone = manager.clone();
        sidebar.list_box.connect_row_activated(move |list_box, list_row| {
            let path_string = list_row.widget_name();
            let path = PathBuf::from(path_string.as_str());
            let settings = gio::Settings::new("com.example.ArchFinder");
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

        let sep = gtk::Separator::new(Orientation::Vertical);
        root_layout.append(&sep);
    }

    if view_type == "miller" {
        main_content.append(&manager.scrolled_window);
        root_layout.append(&main_content);

        let first_list_view = manager.add_column(initial_path.clone(), 0);

        window.set_content(Some(&root_layout));
        window.present();

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

        main_content.append(&icon_view.widget);
        root_layout.append(&main_content);
        window.set_content(Some(&root_layout));
        window.present();
        
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

        main_content.append(&list_view_widget.widget);
        root_layout.append(&main_content);
        window.set_content(Some(&root_layout));
        window.present();
        
        list_view_widget.column_view.add_css_class("focused-list");
        list_view_widget.column_view.grab_focus();
        manager.set_main_view(list_view_widget.column_view.clone().upcast::<gtk::Widget>());
    } else {
        let label = gtk::Label::new(Some(&format!("{} view is not yet implemented", view_type)));
        label.set_vexpand(true);
        main_content.append(&label);
        root_layout.append(&main_content);
        window.set_content(Some(&root_layout));
        window.present();
    }

    main_content.append(&breadcrumb_scrolled);
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
        }
    }

    fn set_main_view(&self, view: gtk::Widget) {
        *self.main_view.borrow_mut() = Some(view);
    }

    fn set_breadcrumb_container(&self, container: Box, parent: ScrolledWindow) {
        *self.breadcrumb_container.borrow_mut() = Some(container);
        *self.breadcrumb_parent.borrow_mut() = Some(parent);
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
                    let settings = gio::Settings::new("com.example.ArchFinder");
                    let _ = settings.set_string("current-path", &path.to_string_lossy());
                    
                    let view_type: String = settings.get("view-type");
                    if view_type == "icons" || view_type == "list" {
                        if let Some(app) = gio::Application::default() {
                            app.activate();
                        }
                    } else {
                        manager_clone.add_column(path, 0);
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
                        if let Some(sm) = get_selection_model(&lv) {
                            let selection = sm.selection();
                            if !selection.is_empty() {
                                 let current = selection.nth(0);
                                 if key == gtk::gdk::Key::Up && current > 0 {
                                     sm.select_item(current - 1, true);
                                 } else if key == gtk::gdk::Key::Down {
                                     sm.select_item(current + 1, true);
                                 }
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
        // 1. Check Miller Columns
        let entries = self.entries.borrow();
        for entry in entries.iter() {
            if entry.focus_target.has_css_class("focused-column") {
                return Some(entry.focus_target.clone());
            }
        }
        // 2. Check active Icon/List view
        if let Some(main) = self.main_view.borrow().as_ref() {
            if main.has_css_class("focused-grid") || main.has_css_class("focused-list") {
                return Some(main.clone());
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
            } else if key == gtk::gdk::Key::Left {
                if index > 0 {
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
            }
            glib::Propagation::Proceed
        });
        list_view.add_controller(key_controller);

        Some(list_view)
    }

    fn handle_selection_change_multi(&self, selection_model: &gtk::MultiSelection, base_path: &PathBuf, index: usize) {
        let selection = selection_model.selection();
        if selection.is_empty() {
            println!("Selection cleared in Column {}", index);
            *self.current_selection.borrow_mut() = None;
            self.update_breadcrumbs(base_path);
            
            // Clear subsequent columns
            let mut entries = self.entries.borrow_mut();
            while entries.len() > index + 1 {
                let entry = entries.pop().unwrap();
                self.columns_box.remove(&entry.container);
            }
            
            self.update_preview_if_open();
            return;
        }

        // For previews and navigation, we use the first selected item
        let first_idx = selection.nth(0);
        let selected_item = selection_model.model().unwrap().item(first_idx);
        
        if let Some(item) = selected_item {
            // Handle TreeListRow wrapping if it's a List View
            let file_info = if let Ok(tree_row) = item.clone().downcast::<gtk::TreeListRow>() {
                tree_row.item().and_downcast::<gio::FileInfo>().unwrap()
            } else {
                item.downcast_ref::<gio::FileInfo>().unwrap().clone()
            };

            let name = file_info.name();
            let mut new_path = base_path.clone();
            new_path.push(&name);

            println!("Selection [Column {}]: {:?} | Type: {:?} | FS is_dir: {}", 
                     index, new_path, file_info.file_type(), new_path.is_dir());

            *self.current_selection.borrow_mut() = Some(SelectionInfo {
                file_info: file_info.clone(),
                path: new_path.clone(),
            });
            
            self.update_breadcrumbs(&new_path);
            self.update_preview_if_open();

            // In List View, we don't necessarily want to jump columns unless it's Miller
            let settings = gio::Settings::new("com.example.ArchFinder");
            let view_type: String = settings.get("view-type");
            
            if view_type == "miller" {
                if file_info.file_type() == gio::FileType::Directory || new_path.is_dir() {
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

                        let preview = Preview::new(&file_info, &new_path);
                        let preview_widget = preview.widget.upcast::<gtk::Widget>();
                        self.columns_box.append(&preview_widget);
                        entries.push(ColumnEntry {
                            container: preview_widget.clone(),
                            focus_target: preview_widget,
                            path: new_path,
                        });
                    }
                }
            }
        }
    }
}
