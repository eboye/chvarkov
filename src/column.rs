use gtk4 as gtk;
use gtk::prelude::*;
use crate::utils;

pub struct Column {
    pub widget: gtk::Box,
    pub list_view: gtk::ListView,
    pub selection_model: gtk::MultiSelection,
}

impl Column {
    pub fn new(path: &std::path::Path, show_hidden: bool, show_meta: bool, zoom_level: i32, sort_type: &str) -> Self {
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
        
        let sorter = utils::create_sorter(sort_type);
        let sort_model = gtk::SortListModel::new(Some(filter_model), Some(sorter));
        
        let selection_model = gtk::MultiSelection::new(Some(sort_model));
        
        let factory = gtk::SignalListItemFactory::new();
        
        // Calculate sizes based on zoom level
        let icon_size = match zoom_level {
            0 => 16,
            1 => 24,
            2 => 32,
            3 => 48,
            4 => 64,
            _ => 96,
        };

        let font_size = match zoom_level {
            0 => 10,
            1 => 11,
            2 => 12,
            3 => 14,
            4 => 16,
            _ => 18,
        };

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

            let key_controller = gtk::EventControllerKey::new();
            let root_box_clone = root_box.clone();
            key_controller.connect_key_pressed(move |_, key, _, modifier| {
                if key == gtk::gdk::Key::Menu || (key == gtk::gdk::Key::F10 && modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK)) {
                    let widget = root_box_clone.clone().upcast::<gtk::Widget>();
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

            root_box.add_controller(gesture_right);
            root_box.add_controller(key_controller);
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

        let list_view = gtk::ListView::new(Some(selection_model.clone()), Some(factory));
        list_view.set_focusable(true);

        // Click-to-deselect on empty space
        let click_gesture = gtk::GestureClick::builder().button(1).build();
        let sel_model_clone = selection_model.clone();
        click_gesture.connect_pressed(move |gesture, _, _, _| {
            // Check if the click target was the list_view itself
            // We use the coordinates to see if a child was hit? 
            // Better: in GTK4, gestures on the parent only fire if children don't claim them
            sel_model_clone.unselect_all();
        });
        list_view.add_controller(click_gesture);
        
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
