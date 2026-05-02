use gtk4 as gtk;
use gtk::prelude::*;

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
        .attributes("standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified,standard::is-symlink-target-directory")
        .file(&file)
        .monitored(true)
        .io_priority(glib::Priority::DEFAULT)
        .build()
}

pub fn create_sorter(sort_type: &str) -> gtk::Sorter {
    match sort_type {
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
        _ => {
            // Use CustomSorter for name to avoid PropertyExpression crash (display-name is not a GObject property)
            let sorter = gtk::CustomSorter::new(|a, b| {
                let a = a.downcast_ref::<gio::FileInfo>().unwrap();
                let b = b.downcast_ref::<gio::FileInfo>().unwrap();
                a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()).into()
            });
            sorter.upcast()
        }
    }
}
