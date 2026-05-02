use gtk4 as gtk;
use gtk::prelude::*;

pub struct Preview {
    pub widget: gtk::Box,
}

impl Preview {
    pub fn new(file_info: &gio::FileInfo, _path: &std::path::Path) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(20)
            .margin_end(20)
            .build();

        // Thumbnail / Icon
        let image = gtk::Image::builder()
            .pixel_size(128)
            .halign(gtk::Align::Center)
            .build();
        
        if let Some(icon) = file_info.icon() {
            image.set_from_gicon(&icon);
        }
        
        container.append(&image);

        // Filename
        let name_label = gtk::Label::builder()
            .label(file_info.display_name().as_str())
            .css_classes(["title-1"])
            .halign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        container.append(&name_label);

        // Info Grid
        let grid = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(8)
            .halign(gtk::Align::Center)
            .build();

        let mut row = 0;

        // Type
        if let Some(content_type) = file_info.content_type() {
            let type_desc = gio::content_type_get_description(&content_type);
            add_info_row(&grid, "Type", &type_desc, &mut row);
        }

        // Size
        let size = file_info.size();
        let size_str = glib::format_size(size as u64);
        add_info_row(&grid, "Size", &size_str, &mut row);

        // Modified
        if let Some(date_time) = file_info.modification_date_time() {
            if let Ok(formatted) = date_time.format("%Y-%m-%d %H:%M") {
                add_info_row(&grid, "Modified", &formatted, &mut row);
            }
        }

        container.append(&grid);

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
