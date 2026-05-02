use gtk4 as gtk;
use gtk::prelude::*;
use std::path::PathBuf;

pub struct Sidebar {
    pub widget: gtk::ScrolledWindow,
    pub list_box: gtk::ListBox,
}

struct SidebarItem {
    name: &'static str,
    icon: &'static str,
    path: PathBuf,
}

impl Sidebar {
    pub fn new() -> Self {
        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .width_request(200)
            .child(&list_box)
            .build();

        let items = vec![
            SidebarItem { name: "Home", icon: "user-home-symbolic", path: glib::home_dir() },
            SidebarItem { name: "Documents", icon: "folder-documents-symbolic", path: glib::user_special_dir(glib::UserDirectory::Documents).unwrap_or_else(glib::home_dir) },
            SidebarItem { name: "Downloads", icon: "folder-download-symbolic", path: glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(glib::home_dir) },
            SidebarItem { name: "Music", icon: "folder-music-symbolic", path: glib::user_special_dir(glib::UserDirectory::Music).unwrap_or_else(glib::home_dir) },
            SidebarItem { name: "Pictures", icon: "folder-pictures-symbolic", path: glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir) },
            SidebarItem { name: "Videos", icon: "folder-videos-symbolic", path: glib::user_special_dir(glib::UserDirectory::Videos).unwrap_or_else(glib::home_dir) },
            SidebarItem { name: "Trash", icon: "user-trash-symbolic", path: glib::home_dir().join(".local/share/Trash/files") },
        ];

        for item in items {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .margin_start(12)
                .margin_end(12)
                .margin_top(8)
                .margin_bottom(8)
                .build();

            let icon = gtk::Image::builder()
                .icon_name(item.icon)
                .build();
            
            let label = gtk::Label::builder()
                .label(item.name)
                .halign(gtk::Align::Start)
                .build();

            row.append(&icon);
            row.append(&label);

            let list_row = gtk::ListBoxRow::new();
            list_row.set_child(Some(&row));
            
            // Store the path in the widget so we can retrieve it on activation
            let path_string = item.path.to_string_lossy().to_string();
            list_row.set_widget_name(&path_string);
            
            list_box.append(&list_row);
        }

        Self {
            widget: scrolled_window,
            list_box,
        }
    }
}
