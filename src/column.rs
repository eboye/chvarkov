use gtk4 as gtk;
use gtk::prelude::*;

pub struct Column {
    pub widget: gtk::Box,
    pub selection_model: gtk::SingleSelection,
}

impl Column {
    pub fn new(path: &std::path::Path, show_hidden: bool) -> Self {
        let file = gio::File::for_path(path);
        let directory_list = gtk::DirectoryList::builder()
            .attributes("standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified")
            .file(&file)
            .monitored(true)
            .build();
        
        let filter = gtk::CustomFilter::new(move |item| {
            let file_info = item.downcast_ref::<gio::FileInfo>().unwrap();
            
            if !show_hidden {
                if file_info.is_hidden() {
                    return false;
                }
                if file_info.name().to_string_lossy().starts_with('.') {
                    return false;
                }
            }
            true
        });

        let filter_model = gtk::FilterListModel::new(Some(directory_list), Some(filter));
        let selection_model = gtk::SingleSelection::new(Some(filter_model));
        
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, list_item| {
            let box_ = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(4)
                .margin_end(4)
                .build();

            let image = gtk::Image::builder()
                .icon_size(gtk::IconSize::Normal)
                .build();
            
            let label = gtk::Label::builder()
                .halign(gtk::Align::Start)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .xalign(0.0)
                .hexpand(true)
                .build();
            
            box_.append(&image);
            box_.append(&label);

            let gesture = gtk::GestureClick::builder()
                .button(3)
                .build();
            
            gesture.connect_pressed(move |gesture, _, x, y| {
                let widget = gesture.widget().unwrap();
                
                let menu = gio::Menu::new();
                menu.append(Some("Copy"), Some("app.copy"));
                menu.append(Some("Paste"), Some("app.paste"));
                menu.append(Some("Rename"), Some("app.rename"));
                menu.append(Some("Delete"), Some("app.delete"));

                let popover = gtk::PopoverMenu::from_model(Some(&menu));
                popover.set_parent(&widget);
                popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            });

            box_.add_controller(gesture);
            list_item.set_child(Some(&box_));
        });

        factory.connect_bind(|_, list_item| {
            let file_info = list_item
                .item()
                .and_downcast::<gio::FileInfo>()
                .expect("Item must be FileInfo");
            
            let box_ = list_item
                .child()
                .and_downcast::<gtk::Box>()
                .expect("Child must be Box");
            
            let image = box_.first_child().unwrap().downcast::<gtk::Image>().unwrap();
            let label = image.next_sibling().unwrap().downcast::<gtk::Label>().unwrap();
            
            label.set_text(&file_info.display_name());
            if let Some(icon) = file_info.icon() {
                image.set_from_gicon(&icon);
            }
        });

        let list_view = gtk::ListView::new(Some(selection_model.clone()), Some(factory));
        
        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .width_request(200)
            .build();
        
        scrolled_window.set_child(Some(&list_view));

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

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build();
        
        container.append(&scrolled_window);
        container.append(&resizer);

        Self {
            widget: container,
            selection_model,
        }
    }
}
