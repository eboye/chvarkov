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
        
        let icon_size = match zoom_level {
            0 => 48,
            1 => 64,
            2 => 80,
            3 => 96,
            4 => 112,
            _ => 128,
        };

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

            // Context menu gesture
            let gesture_right = gtk::GestureClick::builder()
                .button(3)
                .build();
            
            gesture_right.connect_pressed(move |gesture, _, x, y| {
                let widget = gesture.widget().unwrap();
                let menu = utils::create_context_menu();
                let popover = gtk::PopoverMenu::from_model(Some(&menu));
                popover.set_parent(&widget);
                popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            });

            // Keyboard shortcut for Context Menu
            let key_controller = gtk::EventControllerKey::new();
            let container_clone = container.clone();
            key_controller.connect_key_pressed(move |_, key, _, modifier| {
                if key == gtk::gdk::Key::Menu || (key == gtk::gdk::Key::F10 && modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK)) {
                    let widget = container_clone.clone().upcast::<gtk::Widget>();
                    let menu = utils::create_context_menu();
                    let popover = gtk::PopoverMenu::from_model(Some(&menu));
                    popover.set_parent(&widget);
                    let width = widget.width();
                    let height = widget.height();
                    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(width / 2, height / 2, 1, 1)));
                    popover.popup();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });

            container.add_controller(gesture_right);
            container.add_controller(key_controller);
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

            if show_meta {
                if let Some(meta_label) = label.next_sibling().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                    if let Some(date_time) = file_info.modification_date_time() {
                        if let Ok(formatted) = date_time.format("%Y-%m-%d") {
                            meta_label.set_text(&formatted);
                        }
                    }
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
            if let Some(app) = gio::Application::default() {
                app.activate_action("open", None);
            }
        });

        // Click-to-deselect on empty space
        let click_gesture = gtk::GestureClick::builder().button(1).build();
        let sel_model_clone = selection_model.clone();
        click_gesture.connect_pressed(move |_, _, _, _| {
             sel_model_clone.unselect_all();
        });
        grid_view.add_controller(click_gesture);

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
