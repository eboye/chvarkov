use gtk4 as gtk;
use gtk::prelude::*;
use crate::utils;

pub struct IconView {
    pub widget: gtk::ScrolledWindow,
    pub grid_view: gtk::GridView,
}

impl IconView {
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

        let icon_size = utils::get_grid_icon_size(zoom_level);

        let item_width = icon_size + 24;

        factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let container = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(4)
                .margin_end(4)
                .width_request(item_width)
                .focusable(true)
                .build();

            let image = gtk::Image::builder()
                .pixel_size(icon_size)
                .halign(gtk::Align::Center)
                .build();

            let label = gtk::Label::builder()
                .halign(gtk::Align::Center)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .lines(1)
                .max_width_chars(10)
                .justify(gtk::Justification::Center)
                .build();

            container.append(&image);
            container.append(&label);

            if show_meta {
                let meta_label = gtk::Label::builder()
                    .halign(gtk::Align::Center)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .lines(1)
                    .max_width_chars(10)
                    .css_classes(["caption", "dim-label"])
                    .justify(gtk::Justification::Center)
                    .build();

                let attrs = gtk::pango::AttrList::new();
                let font_attr = gtk::pango::AttrSize::new(8 * gtk::pango::SCALE);
                attrs.insert(font_attr);
                meta_label.set_attributes(Some(&attrs));

                container.append(&meta_label);
            }

            utils::attach_context_menu_gesture(&container);
            list_item.set_child(Some(&container));
        });

        factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let file_info = list_item.item().and_downcast::<gio::FileInfo>().unwrap();
            let container = list_item.child().and_downcast::<gtk::Box>().unwrap();

            let image = container.first_child().unwrap().downcast::<gtk::Image>().unwrap();
            let label = image.next_sibling().unwrap().downcast::<gtk::Label>().unwrap();

            label.set_text(&file_info.display_name());

            utils::set_icon_and_thumbnail(&image, &file_info);

            if show_meta {
                if let Some(meta_label) = label.next_sibling().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                    meta_label.set_text(&utils::format_metadata(&file_info));
                }
            }
        });

        let grid_view = gtk::GridView::builder()
            .model(&selection_model)
            .factory(&factory)
            .max_columns(100)
            .min_columns(1)
            .enable_rubberband(true)
            .focusable(true)
            .build();

        grid_view.connect_activate(move |_, _| {
            utils::trigger_open_action();
        });

        utils::setup_view_common_controllers(&grid_view, &selection_model);

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&grid_view)
            .build();

        Self {
            widget: scrolled_window,
            grid_view,
        }
    }
}
