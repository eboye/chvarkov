use gtk4 as gtk;
use gtk::prelude::*;

pub struct IconView {
    pub widget: gtk::ScrolledWindow,
    pub grid_view: gtk::GridView,
}

impl IconView {
    pub fn new(path: &std::path::Path, show_hidden: bool, zoom_level: i32) -> Self {
        let file = gio::File::for_path(path);
        let directory_list = gtk::DirectoryList::builder()
            .attributes("standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden")
            .file(&file)
            .monitored(true)
            .build();

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
        let selection_model = gtk::SingleSelection::builder()
            .model(&filter_model)
            .autoselect(false)
            .build();

        let factory = gtk::SignalListItemFactory::new();
        
        let icon_size = match zoom_level {
            0 => 48,
            1 => 64,
            2 => 80,
            3 => 96,
            4 => 112,
            _ => 128,
        };

        // Item width is icon_size + horizontal padding
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
                .build();

            let image = gtk::Image::builder()
                .pixel_size(icon_size)
                .halign(gtk::Align::Center)
                .build();

            let label = gtk::Label::builder()
                .halign(gtk::Align::Center)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .lines(2)
                .max_width_chars(10) // More aggressive truncation
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .justify(gtk::Justification::Center)
                .build();

            container.append(&image);
            container.append(&label);
            list_item.set_child(Some(&container));
        });

        factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let file_info = list_item.item().and_downcast::<gio::FileInfo>().unwrap();
            let container = list_item.child().and_downcast::<gtk::Box>().unwrap();
            
            let image = container.first_child().unwrap().downcast::<gtk::Image>().unwrap();
            let label = image.next_sibling().unwrap().downcast::<gtk::Label>().unwrap();

            label.set_text(&file_info.display_name());
            if let Some(icon) = file_info.icon() {
                image.set_from_gicon(&icon);
            }
        });

        // To make it adapt to window width, we use max_columns(0) which means "as many as fit"
        // in newer GTK, or just rely on the ScrolledWindow + GridView default behavior.
        // We set max_columns to a high number to allow wrapping.
        let grid_view = gtk::GridView::builder()
            .model(&selection_model)
            .factory(&factory)
            .max_columns(100) 
            .min_columns(1)
            .enable_rubberband(true)
            .build();

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
