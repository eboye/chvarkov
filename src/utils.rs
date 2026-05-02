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
        .attributes("standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified,standard::is-symlink-target-directory,standard::n-children")
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
