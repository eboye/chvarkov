mod column;
mod preview;

use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar};
use gtk4 as gtk;
use gtk::{Box, Orientation, ScrolledWindow};
use std::path::PathBuf;
use column::Column;
use preview::Preview;
use std::rc::Rc;
use std::cell::RefCell;

fn main() {
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
        .focused-column row:selected {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            border-radius: 6px;
        }

        /* Unfocused list selection (parent columns) - Subdued color */
        listview row:selected {
            background-color: alpha(@accent_bg_color, 0.2);
            color: @view_fg_color;
            border-radius: 6px;
        }

        /* Hover effect for rows */
        listview row:hover:not(:selected) {
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
    ");

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn setup_actions(app: &Application) {
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
    open_action.connect_activate(|_, _| println!("Open action triggered"));
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

    let show_hidden_action = gio::SimpleAction::new_stateful("show-hidden", None, &false.to_variant());
    let app_weak_h = app.downgrade();
    show_hidden_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            if let Some(app) = app_weak_h.upgrade() {
                app.activate();
            }
        }
    });
    app.add_action(&show_hidden_action);

    let show_meta_action = gio::SimpleAction::new_stateful("show-meta", None, &false.to_variant());
    let app_weak_m = app.downgrade();
    show_meta_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            if let Some(app) = app_weak_m.upgrade() {
                app.activate();
            }
        }
    });
    app.add_action(&show_meta_action);

    let view_type_action = gio::SimpleAction::new_stateful("view-type", Some(glib::VariantTy::new("s").unwrap()), &"miller".to_variant());
    let app_weak_v = app.downgrade();
    view_type_action.connect_change_state(move |action, state| {
        if let Some(state) = state {
            action.set_state(state);
            if let Some(app) = app_weak_v.upgrade() {
                app.activate();
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
            .default_width(1000)
            .default_height(600)
            .build()
    };

    let content = Box::builder()
        .orientation(Orientation::Vertical)
        .build();

    let header_bar = HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new("Arch-Finder", ""))
        .build();

    // Group 1: Display Toggles (Linked)
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

    // Separator
    let separator = gtk::Separator::new(Orientation::Vertical);
    header_bar.pack_start(&separator);

    let view_type = app.lookup_action("view-type")
        .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        .map(|a| a.state().unwrap().get::<String>().unwrap())
        .unwrap_or_else(|| "miller".to_string());

    // Group 2: View Options
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

    content.append(&header_bar);

    if view_type == "miller" {
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
        content.append(&scrolled_window);

        let show_hidden = app.lookup_action("show-hidden")
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            .map(|a| a.state().unwrap().get::<bool>().unwrap())
            .unwrap_or(false);

        let show_meta = app.lookup_action("show-meta")
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            .map(|a| a.state().unwrap().get::<bool>().unwrap())
            .unwrap_or(false);

        let manager = ColumnManager::new(columns_box, show_hidden, show_meta);
        let first_list_view = manager.add_column(glib::home_dir(), 0);

        window.set_content(Some(&content));
        window.present();

        if let Some(lv) = first_list_view {
            lv.add_css_class("focused-column");
            lv.grab_focus();
        }
    } else {
        let label = gtk::Label::new(Some(&format!("{} view is not yet implemented", view_type)));
        label.set_vexpand(true);
        content.append(&label);
        window.set_content(Some(&content));
        window.present();
    }
}

#[derive(Clone)]
struct ColumnEntry {
    container: gtk::Widget,
    focus_target: gtk::Widget, // The ListView or the Preview container
}

#[derive(Clone)]
struct ColumnManager {
    columns_box: Box,
    show_hidden: bool,
    show_meta: bool,
    entries: Rc<RefCell<Vec<ColumnEntry>>>,
}

impl ColumnManager {
    fn new(columns_box: Box, show_hidden: bool, show_meta: bool) -> Self {
        Self {
            columns_box,
            show_hidden,
            show_meta,
            entries: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn add_column(&self, path: PathBuf, index: usize) -> Option<gtk::ListView> {
        let column = Column::new(&path, self.show_hidden, self.show_meta);
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
            });
        }

        let self_clone = self.clone();
        let path_clone = path.clone();
        column.selection_model.connect_selection_changed(move |selection_model, _, _| {
            let selected_item = selection_model.selected_item();
            if let Some(item) = selected_item {
                let file_info = item.downcast_ref::<gio::FileInfo>().unwrap();
                let name = file_info.name();
                let mut new_path = path_clone.clone();
                new_path.push(name);

                if file_info.file_type() == gio::FileType::Directory {
                    self_clone.add_column(new_path, index + 1);
                } else {
                    let mut entries = self_clone.entries.borrow_mut();
                    while entries.len() > index + 1 {
                        let entry = entries.pop().unwrap();
                        self_clone.columns_box.remove(&entry.container);
                    }

                    let preview = Preview::new(file_info, &new_path);
                    let preview_widget = preview.widget.upcast::<gtk::Widget>();
                    self_clone.columns_box.append(&preview_widget);
                    entries.push(ColumnEntry {
                        container: preview_widget.clone(),
                        focus_target: preview_widget,
                    });
                }
            }
        });

        // Add Key Navigation
        let key_controller = gtk::EventControllerKey::new();
        let self_key_clone = self.clone();
        let list_view_focus = list_view.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Right {
                let entries = self_key_clone.entries.borrow();
                if index + 1 < entries.len() {
                    list_view_focus.remove_css_class("focused-column");
                    let target = &entries[index + 1].focus_target;
                    target.add_css_class("focused-column");
                    target.grab_focus();
                    return glib::Propagation::Stop;
                }
            } else if key == gtk::gdk::Key::Left {
                if index > 0 {
                    let entries = self_key_clone.entries.borrow();
                    list_view_focus.remove_css_class("focused-column");
                    let target = &entries[index - 1].focus_target;
                    target.add_css_class("focused-column");
                    target.grab_focus();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        list_view.add_controller(key_controller);

        Some(list_view)
    }
}
