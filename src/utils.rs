use gtk4 as gtk;
use gtk::prelude::*;

/// Attaches a right-click (`button 3`) gesture to `widget` that opens the context menu.
/// Respects the Shift modifier: shows `create_context_menu_shift()` when Shift is held.
pub fn attach_context_menu_gesture(widget: &impl gtk::prelude::IsA<gtk::Widget>) {
    let gesture = gtk::GestureClick::builder().button(3).build();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let w = gesture.widget().unwrap();
        let shift_held = gesture.current_event_state()
            .contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let menu = if shift_held { create_context_menu_shift() } else { create_context_menu() };
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&w);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    widget.add_controller(gesture);
}

/// Attaches a keyboard controller to `widget` that opens the context menu on the
/// `Menu` key or `Shift+F10`. The popover is anchored to the centre of the widget.
pub fn attach_context_menu_key_controller(widget: &impl gtk::prelude::IsA<gtk::Widget>) {
    let key_controller = gtk::EventControllerKey::new();
    let widget_clone = widget.clone().upcast::<gtk::Widget>();
    key_controller.connect_key_pressed(move |_, key, _, modifier| {
        let is_menu_key = key == gtk::gdk::Key::Menu
            || (key == gtk::gdk::Key::F10 && modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK));
        if !is_menu_key {
            return glib::Propagation::Proceed;
        }
        let menu = create_context_menu();
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&widget_clone);
        let w = widget_clone.width();
        let h = widget_clone.height();
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(w / 2, h / 2, 1, 1)));
        popover.popup();
        glib::Propagation::Stop
    });
    widget.add_controller(key_controller);
}

/// Opens the context menu anchored to the centre of `widget`.
/// Used by keyboard handlers that already have a widget reference.
pub fn open_context_menu_at_center(widget: &impl gtk::prelude::IsA<gtk::Widget>) {
    let menu = create_context_menu();
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    let w = widget.clone().upcast::<gtk::Widget>();
    popover.set_parent(&w);
    let width = w.width();
    let height = w.height();
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(width / 2, height / 2, 1, 1)));
    popover.popup();
}

pub fn create_context_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let section1 = gio::Menu::new();
    section1.append(Some("Open"), Some("app.open"));
    menu.append_section(None, &section1);

    let section2 = gio::Menu::new();
    section2.append(Some("Cut"), Some("app.cut"));
    section2.append(Some("Copy"), Some("app.copy"));
    section2.append(Some("Move to..."), Some("app.move-to"));
    section2.append(Some("Copy to..."), Some("app.copy-to"));
    menu.append_section(None, &section2);

    let section3 = gio::Menu::new();
    section3.append(Some("Rename..."), Some("app.rename"));
    section3.append(Some("Create Link"), Some("app.create-link"));
    section3.append(Some("Compress..."), Some("app.compress"));
    section3.append(Some("Email..."), Some("app.email"));
    section3.append(Some("Move to Trash"), Some("app.delete"));
    section3.append(Some("Delete Permanently"), Some("app.permanent-delete"));
    menu.append_section(None, &section3);

    let section4 = gio::Menu::new();
    section4.append(Some("Open in Terminal"), Some("app.open-terminal"));
    section4.append(Some("Copy Path"), Some("app.copy-path"));
    section4.append(Some("Copy URI"), Some("app.copy-uri"));
    section4.append(Some("Copy Name"), Some("app.copy-name"));
    section4.append(Some("Sharing Options"), Some("app.sharing-options"));
    menu.append_section(None, &section4);

    let section5 = gio::Menu::new();
    section5.append(Some("Properties"), Some("app.properties"));
    menu.append_section(None, &section5);

    menu
}

/// Context menu variant shown when Shift is held — moves "Delete Permanently" to the top of the
/// delete section to make the destructive action explicit.
pub fn create_context_menu_shift() -> gio::Menu {
    let menu = gio::Menu::new();

    let section1 = gio::Menu::new();
    section1.append(Some("Open"), Some("app.open"));
    menu.append_section(None, &section1);

    let section2 = gio::Menu::new();
    section2.append(Some("Cut"), Some("app.cut"));
    section2.append(Some("Copy"), Some("app.copy"));
    section2.append(Some("Move to..."), Some("app.move-to"));
    section2.append(Some("Copy to..."), Some("app.copy-to"));
    menu.append_section(None, &section2);

    let section3 = gio::Menu::new();
    section3.append(Some("Rename..."), Some("app.rename"));
    section3.append(Some("Create Link"), Some("app.create-link"));
    section3.append(Some("Compress..."), Some("app.compress"));
    section3.append(Some("Email..."), Some("app.email"));
    section3.append(Some("Delete Permanently"), Some("app.permanent-delete"));
    section3.append(Some("Move to Trash"), Some("app.delete"));
    menu.append_section(None, &section3);

    let section4 = gio::Menu::new();
    section4.append(Some("Open in Terminal"), Some("app.open-terminal"));
    section4.append(Some("Copy Path"), Some("app.copy-path"));
    section4.append(Some("Copy URI"), Some("app.copy-uri"));
    section4.append(Some("Copy Name"), Some("app.copy-name"));
    section4.append(Some("Sharing Options"), Some("app.sharing-options"));
    menu.append_section(None, &section4);

    let section5 = gio::Menu::new();
    section5.append(Some("Properties"), Some("app.properties"));
    menu.append_section(None, &section5);

    menu
}

pub fn get_directory_list(path: &std::path::Path) -> gtk::DirectoryList {
    let file = gio::File::for_path(path);
    gtk::DirectoryList::builder()
        .attributes("standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified,time::access,time::created,unix::mode,unix::uid,unix::gid,unix::user,unix::group,standard::n-children,standard::file,thumbnail::path,thumbnail::is-valid")
        .file(&file)
        .monitored(true)
        .io_priority(glib::Priority::DEFAULT)
        .build()
}

pub fn format_size(size: i64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_metadata(info: &gio::FileInfo) -> String {
    let date = info.modification_date_time()
        .and_then(|dt| dt.format("%Y-%m-%d").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "----".to_string());

    let is_dir = info.file_type() == gio::FileType::Directory;

    if is_dir {
        let count = info.attribute_uint32("standard::n-children");
        if count > 0 {
            format!("{} | Folder ({} items)", date, count)
        } else {
            format!("{} | Folder", date)
        }
    } else {
        let type_desc = info.content_type()
            .map(|ct| gio::content_type_get_description(&ct).to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let size = format_size(info.size());
        format!("{} | {} | {}", date, type_desc, size)
    }
}

pub fn create_sorter(sort_type: &str, folders_first: bool) -> gtk::Sorter {
    let multi_sorter = gtk::MultiSorter::new();

    if folders_first {
        let folders_sorter = gtk::CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<gio::FileInfo>().unwrap();
            let b = b.downcast_ref::<gio::FileInfo>().unwrap();
            let a_is_dir = a.file_type() == gio::FileType::Directory;
            let b_is_dir = b.file_type() == gio::FileType::Directory;

            if a_is_dir && !b_is_dir {
                gtk::Ordering::Smaller
            } else if !a_is_dir && b_is_dir {
                gtk::Ordering::Larger
            } else {
                gtk::Ordering::Equal
            }
        });
        multi_sorter.append(folders_sorter);
    }

    let primary_sorter: gtk::Sorter = match sort_type {
        "date" => {
            let sorter = gtk::CustomSorter::new(|a, b| {
                let a = a.downcast_ref::<gio::FileInfo>().unwrap();
                let b = b.downcast_ref::<gio::FileInfo>().unwrap();
                let a_time = a.modification_date_time().unwrap_or_else(|| glib::DateTime::from_unix_local(0).unwrap());
                let b_time = b.modification_date_time().unwrap_or_else(|| glib::DateTime::from_unix_local(0).unwrap());

                if b_time > a_time {
                    gtk::Ordering::Larger
                } else if b_time < a_time {
                    gtk::Ordering::Smaller
                } else {
                    gtk::Ordering::Equal
                }
            });
            sorter.upcast()
        },
        "size" => {
            let sorter = gtk::CustomSorter::new(|a, b| {
                let a = a.downcast_ref::<gio::FileInfo>().unwrap();
                let b = b.downcast_ref::<gio::FileInfo>().unwrap();
                b.size().cmp(&a.size()).into() // Largest first
            });
            sorter.upcast()
        },
        "type" => {
            let sorter = gtk::CustomSorter::new(|a, b| {
                let a = a.downcast_ref::<gio::FileInfo>().unwrap();
                let b = b.downcast_ref::<gio::FileInfo>().unwrap();
                let a_type = a.content_type().unwrap_or_else(|| "".into());
                let b_type = b.content_type().unwrap_or_else(|| "".into());
                a_type.cmp(&b_type).into()
            });
            sorter.upcast()
        },
        _ => {
            let sorter = gtk::CustomSorter::new(|a, b| {
                let a = a.downcast_ref::<gio::FileInfo>().unwrap();
                let b = b.downcast_ref::<gio::FileInfo>().unwrap();
                a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()).into()
            });
            sorter.upcast()
        }
    };
    multi_sorter.append(primary_sorter);

    if sort_type != "name" && !sort_type.is_empty() {
        let fallback_sorter = gtk::CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<gio::FileInfo>().unwrap();
            let b = b.downcast_ref::<gio::FileInfo>().unwrap();
            a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()).into()
        });
        multi_sorter.append(fallback_sorter);
    }

    multi_sorter.upcast()
}

/// Unified logic to set either a thumbnail or a standard icon for a file.
pub fn set_icon_and_thumbnail(image: &gtk::Image, file_info: &gio::FileInfo) {
    let mut icon_set = false;
    if let Some(thumb_path) = file_info.attribute_byte_string("thumbnail::path") {
        use std::os::unix::ffi::OsStrExt;
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(thumb_path.as_bytes()));
        let file = gio::File::for_path(path);
        let thumb_icon = gio::FileIcon::new(&file);
        image.set_from_gicon(&thumb_icon);
        image.add_css_class("thumbnail");
        icon_set = true;
    }

    if !icon_set {
        if let Some(icon) = file_info.icon() {
            image.set_from_gicon(&icon);
            image.remove_css_class("thumbnail");
        }
    }
}

/// Sets up common view controllers:
/// 1. Click-to-deselect on empty space
/// 2. Keyboard-triggered context menu (Menu key / Shift+F10)
pub fn setup_view_common_controllers(view: &impl gtk::prelude::IsA<gtk::Widget>, selection_model: &gtk::MultiSelection) {
    // Click-to-deselect
    let click_gesture = gtk::GestureClick::builder().button(1).build();
    let sel_model_clone = selection_model.clone();
    click_gesture.connect_pressed(move |_, _, _, _| {
        sel_model_clone.unselect_all();
    });
    view.add_controller(click_gesture);

    // Keyboard context menu
    attach_context_menu_key_controller(view);
}

pub fn get_list_icon_size(zoom_level: i32) -> i32 {
    match zoom_level {
        0 => 16,
        1 => 24,
        2 => 32,
        3 => 48,
        4 => 64,
        _ => 96,
    }
}

pub fn get_grid_icon_size(zoom_level: i32) -> i32 {
    match zoom_level {
        0 => 48,
        1 => 64,
        2 => 80,
        3 => 96,
        4 => 112,
        _ => 128,
    }
}

pub fn get_font_size(zoom_level: i32) -> i32 {
    match zoom_level {
        0 => 10,
        1 => 11,
        2 => 12,
        3 => 14,
        4 => 16,
        _ => 18,
    }
}

pub fn trigger_open_action() {
    if let Some(app) = gio::Application::default() {
        app.activate_action("open", None);
    }
}
