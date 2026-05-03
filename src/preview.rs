use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use sourceview5 as sourceview;
use sourceview::prelude::*;
use crate::utils;

pub struct Preview {
    pub widget: gtk::Box,
}

impl Preview {
    pub fn new(file_info: &gio::FileInfo, path: &std::path::Path) -> Self {
        let container = Self::create_preview_layout(file_info, path, false);

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .width_request(200)
            .child(&container)
            .build();

        // Add Resizer to make it match Directory Columns
        let resizer = gtk::Separator::new(gtk::Orientation::Vertical);
        resizer.set_cursor_from_name(Some("col-resize"));
        resizer.add_css_class("resizer");
        
        let drag_gesture = gtk::GestureDrag::new();
        let sw_weak = scrolled_window.downgrade();
        
        let start_width = std::rc::Rc::new(std::cell::Cell::new(200));
        let start_width_clone = start_width.clone();
        
        drag_gesture.connect_drag_begin(move |_, _, _| {
            if let Some(sw) = sw_weak.upgrade() {
                start_width_clone.set(sw.width_request());
            }
        });

        let sw_weak_update = scrolled_window.downgrade();
        drag_gesture.connect_drag_update(move |_, offset_x, _| {
            if let Some(sw) = sw_weak_update.upgrade() {
                let new_width = (start_width.get() as f64 + offset_x).max(100.0) as i32;
                sw.set_width_request(new_width);
            }
        });

        resizer.add_controller(drag_gesture);

        let wrapper = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build();
        
        wrapper.append(&scrolled_window);
        wrapper.append(&resizer);

        Self {
            widget: wrapper,
        }
    }

    pub fn create_preview_layout(file_info: &gio::FileInfo, path: &std::path::Path, large: bool) -> gtk::Box {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(if large { 40 } else { 20 })
            .margin_bottom(if large { 40 } else { 20 })
            .margin_start(if large { 40 } else { 20 })
            .margin_end(if large { 40 } else { 20 })
            .build();

        let content_type = file_info.content_type();
        let is_image = content_type.as_ref().map(|ct| gio::content_type_is_a(ct, "image/*")).unwrap_or(false);
        let is_video = content_type.as_ref().map(|ct| gio::content_type_is_a(ct, "video/*")).unwrap_or(false);
        let is_text = content_type.as_ref().map(|ct| gio::content_type_is_a(ct, "text/*")).unwrap_or(false);

        if is_image {
            let picture = gtk::Picture::for_filename(path);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_hexpand(true);
            picture.set_vexpand(true);
            picture.set_height_request(if large { 400 } else { 200 });
            container.append(&picture);
        } else if is_video && large {
            let file = gio::File::for_path(path);
            let video = gtk::Video::builder()
                .file(&file)
                .autoplay(true)
                .loop_(true)
                .hexpand(true)
                .vexpand(true)
                .build();
            container.append(&video);
        } else if is_text && large {
            let buffer = sourceview::Buffer::new(None);
            let view = sourceview::View::with_buffer(&buffer);
            view.set_editable(false);
            view.set_hexpand(true);
            view.set_vexpand(true);
            view.set_monospace(true);
            view.set_show_line_numbers(true);

            let lang_manager = sourceview::LanguageManager::default();
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let lang = lang_manager.guess_language(Some(filename), content_type.as_deref());
            buffer.set_language(lang.as_ref());

            if let Ok(file) = std::fs::File::open(path) {
                use std::io::Read;
                let mut content = Vec::new();
                file.take(10000).read_to_end(&mut content).ok(); // Read first 10KB

                let text = String::from_utf8_lossy(&content);
                buffer.set_text(&text);
            }

            let scrolled = gtk::ScrolledWindow::builder()
                .hexpand(true)
                .vexpand(true)
                .child(&view)
                .build();
            container.append(&scrolled);
        } else {
            let image = gtk::Image::builder()
                .pixel_size(if large { 256 } else { 128 })
                .halign(gtk::Align::Center)
                .build();
            
            utils::set_icon_and_thumbnail(&image, file_info);
            container.append(&image);
        }

        let name_label = gtk::Label::builder()
            .label(file_info.display_name().as_str())
            .css_classes([if large { "title-1" } else { "title-2" }])
            .halign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        container.append(&name_label);

        let grid = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(8)
            .halign(gtk::Align::Center)
            .build();

        let mut row = 0;

        if let Some(ct) = content_type {
            let type_desc = gio::content_type_get_description(&ct);
            add_info_row(&grid, "Type", &type_desc, &mut row);
        }

        let size = file_info.size();
        let size_str = glib::format_size(size as u64);
        add_info_row(&grid, "Size", &size_str, &mut row);

        if let Some(date_time) = file_info.modification_date_time() {
            if let Ok(formatted) = date_time.format("%Y-%m-%d %H:%M") {
                add_info_row(&grid, "Modified", &formatted, &mut row);
            }
        }

        container.append(&grid);
        container
    }

    pub fn create_properties_layout(file_info: &gio::FileInfo, path: &std::path::Path) -> gtk::Box {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        // 1. Header with Thumbnail/Icon
        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_bottom(24)
            .build();

        let content_type = file_info.content_type();
        let is_image = content_type.as_ref().map(|ct| gio::content_type_is_a(ct, "image/*")).unwrap_or(false);

        if is_image {
            let picture = gtk::Picture::for_filename(path);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_height_request(128);
            header_box.append(&picture);
        } else {
            let image = gtk::Image::builder()
                .pixel_size(96)
                .halign(gtk::Align::Center)
                .build();
            
            utils::set_icon_and_thumbnail(&image, file_info);
            header_box.append(&image);
        }

        let name_label = gtk::Label::builder()
            .label(file_info.display_name().as_str())
            .css_classes(["title-1"])
            .halign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        header_box.append(&name_label);

        let type_desc = content_type
            .map(|ct| gio::content_type_get_description(&ct).to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        
        let sub_label = gtk::Label::builder()
            .label(&format!("{} | {}", type_desc, glib::format_size(file_info.size() as u64)))
            .halign(gtk::Align::Center)
            .css_classes(["dim-label"])
            .build();
        header_box.append(&sub_label);

        container.append(&header_box);

        // 2. Details Group
        let details_group = adw::PreferencesGroup::new();
        
        let parent_folder = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "/".to_string());
        let parent_row = adw::ActionRow::builder()
            .title("Parent Folder")
            .subtitle(parent_folder)
            .build();
        parent_row.add_suffix(&gtk::Image::from_icon_name("folder-symbolic"));
        details_group.add(&parent_row);

        let access_time = file_info.attribute_uint64("time::access");
        if access_time > 0 {
            if let Ok(dt) = glib::DateTime::from_unix_local(access_time as i64) {
                if let Ok(formatted) = dt.format("%d/%m/%Y, %H:%M:%S") {
                    let row = adw::ActionRow::builder().title("Accessed").subtitle(formatted).build();
                    details_group.add(&row);
                }
            }
        }

        if let Some(dt) = file_info.modification_date_time() {
            if let Ok(formatted) = dt.format("%d/%m/%Y, %H:%M:%S") {
                let row = adw::ActionRow::builder().title("Modified").subtitle(formatted).build();
                details_group.add(&row);
            }
        }

        let create_time = file_info.attribute_uint64("time::created");
        if create_time > 0 {
            if let Ok(dt) = glib::DateTime::from_unix_local(create_time as i64) {
                if let Ok(formatted) = dt.format("%d/%m/%Y, %H:%M:%S") {
                    let row = adw::ActionRow::builder().title("Created").subtitle(formatted).build();
                    details_group.add(&row);
                }
            }
        }

        container.append(&details_group);

        // 3. Permissions Group
        let perm_group = adw::PreferencesGroup::builder()
            .margin_top(24)
            .build();
        
        let mode = file_info.attribute_uint32("unix::mode");
        let perm_str = if mode != 0 {
            format_permissions(mode)
        } else {
            "Unknown".to_string()
        };

        let perm_row = adw::ExpanderRow::builder()
            .title("Permissions")
            .subtitle(perm_str)
            .build();
        
        let owner_uid = file_info.attribute_uint32("unix::uid");
        let owner_user = file_info.attribute_string("unix::user").unwrap_or_else(|| "".into());
        let owner_subtitle = if !owner_user.is_empty() {
            format!("{} (UID: {})", owner_user, owner_uid)
        } else {
            format!("UID: {}", owner_uid)
        };

        let owner_row = adw::ActionRow::builder()
            .title("Owner")
            .subtitle(owner_subtitle)
            .build();
        perm_row.add_row(&owner_row);

        let group_gid = file_info.attribute_uint32("unix::gid");
        let group_name = file_info.attribute_string("unix::group").unwrap_or_else(|| "".into());
        let group_subtitle = if !group_name.is_empty() {
            format!("{} (GID: {})", group_name, group_gid)
        } else {
            format!("GID: {}", group_gid)
        };

        let group_row = adw::ActionRow::builder()
            .title("Group")
            .subtitle(group_subtitle)
            .build();
        perm_row.add_row(&group_row);

        perm_group.add(&perm_row);

        container.append(&perm_group);

        container
    }
}

fn format_permissions(mode: u32) -> String {
    let mut s = String::new();
    let r = mode & 0o400 != 0;
    let w = mode & 0o200 != 0;
    let x = mode & 0o100 != 0;

    if r && w {
        s.push_str("Read and Write");
    } else if r {
        s.push_str("Read-only");
    } else if w {
        s.push_str("Write-only");
    } else {
        s.push_str("No access");
    }

    if x {
        s.push_str(" (Executable)");
    }
    s
}

fn add_info_row(grid: &gtk::Grid, label_text: &str, value_text: &str, row: &mut i32) {
    let l = gtk::Label::builder()
        .label(label_text)
        .halign(gtk::Align::End)
        .css_classes(["dim-label"])
        .build();
    let v = gtk::Label::builder()
        .label(value_text)
        .halign(gtk::Align::Start)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    
    grid.attach(&l, 0, *row, 1, 1);
    grid.attach(&v, 1, *row, 1, 1);
    *row += 1;
}
