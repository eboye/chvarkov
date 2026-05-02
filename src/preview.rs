use gtk4 as gtk;
use gtk::prelude::*;
use sourceview5 as sourceview;
use sourceview::prelude::*;

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

        if is_image && large {
            let picture = gtk::Picture::for_filename(path);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_hexpand(true);
            picture.set_vexpand(true);
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
            
            if let Some(icon) = file_info.icon() {
                image.set_from_gicon(&icon);
            }
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
