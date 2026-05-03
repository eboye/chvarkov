use gtk4 as gtk;
use gtk::prelude::*;
use crate::utils;

pub struct Column {
    pub widget: gtk::Box,
    pub list_view: gtk::ListView,
    pub selection_model: gtk::MultiSelection,
}

impl Column {
    pub fn new(path: &std::path::Path, show_hidden: bool, show_meta: bool, zoom_level: i32, sort_type: &str, folders_first: bool) -> Self {
        let directory_list = utils::get_directory_list(path);

        let filter = gtk::CustomFilter::new(move |item| {
            let file_info = item.downcast_ref::<gio::FileInfo>().unwrap();
            if !show_hidden {
                if file_info.is_hidden() || file_info.name().to_string_lossy().starts_with('.') {
                    return false;
                }
            }
            true
        });

        let filter_model = gtk::FilterListModel::new(Some(directory_list), Some(filter));

        let sorter = utils::create_sorter(sort_type, folders_first);
        let sort_model = gtk::SortListModel::new(Some(filter_model), Some(sorter));

        let selection_model = gtk::MultiSelection::new(Some(sort_model));

        let factory = gtk::SignalListItemFactory::new();

        // Calculate sizes based on zoom level
        let icon_size = utils::get_list_icon_size(zoom_level);
        let font_size = utils::get_font_size(zoom_level);

        factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let root_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(4)
                .margin_end(4)
                .focusable(true)
                .build();

            let image = gtk::Image::builder()
                .pixel_size(icon_size)
                .valign(gtk::Align::Center)
                .build();

            let text_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .valign(gtk::Align::Center)
                .build();

            let label = gtk::Label::builder()
                .halign(gtk::Align::Start)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .xalign(0.0)
                .build();

            let attrs = gtk::pango::AttrList::new();
            let mut font_attr = gtk::pango::AttrSize::new(font_size * gtk::pango::SCALE);
            font_attr.set_start_index(0);
            font_attr.set_end_index(u32::MAX);
            attrs.insert(font_attr);
            label.set_attributes(Some(&attrs));

            text_box.append(&label);

            if show_meta {
                let meta_label = gtk::Label::builder()
                    .halign(gtk::Align::Start)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .xalign(0.0)
                    .css_classes(["caption", "dim-label"])
                    .build();

                let meta_attrs = gtk::pango::AttrList::new();
                let mut meta_font_attr = gtk::pango::AttrSize::new((font_size - 2).max(8) * gtk::pango::SCALE);
                meta_font_attr.set_start_index(0);
                meta_font_attr.set_end_index(u32::MAX);
                meta_attrs.insert(meta_font_attr);
                meta_label.set_attributes(Some(&meta_attrs));

                text_box.append(&meta_label);
            }

            root_box.append(&image);
            root_box.append(&text_box);

            utils::attach_context_menu_gesture(&root_box);
            list_item.set_child(Some(&root_box));
        });

        factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let file_info = list_item.item().and_downcast::<gio::FileInfo>().unwrap();
            let root_box = list_item.child().and_downcast::<gtk::Box>().unwrap();

            let image = root_box.first_child().unwrap().downcast::<gtk::Image>().unwrap();
            let text_box = image.next_sibling().unwrap().downcast::<gtk::Box>().unwrap();
            let label = text_box.first_child().unwrap().downcast::<gtk::Label>().unwrap();

            label.set_text(&file_info.display_name());

            utils::set_icon_and_thumbnail(&image, &file_info);

            if show_meta {
                if let Some(meta_label) = label.next_sibling().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                    meta_label.set_text(&utils::format_metadata(&file_info));
                }
            }
        });

        let list_view = gtk::ListView::new(Some(selection_model.clone()), Some(factory));
        list_view.set_focusable(true);

        list_view.connect_activate(move |_, _| {
            utils::trigger_open_action();
        });

        utils::setup_view_common_controllers(&list_view, &selection_model);

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .width_request(200 + (zoom_level * 40))
            .build();

        scrolled_window.set_child(Some(&list_view));

        let resizer = gtk::Separator::new(gtk::Orientation::Vertical);
        resizer.set_cursor_from_name(Some("col-resize"));
        resizer.add_css_class("resizer");

        let drag_gesture = gtk::GestureDrag::new();
        let sw_weak = scrolled_window.downgrade();
        let start_width = std::rc::Rc::new(std::cell::Cell::new(200 + (zoom_level * 40)));
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

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build();

        container.append(&scrolled_window);
        container.append(&resizer);

        Self {
            widget: container,
            list_view,
            selection_model,
        }
    }
}
