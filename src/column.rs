use gtk4 as gtk;
use gtk::prelude::*;

pub struct Column {
    pub widget: gtk::Box,
    pub selection_model: gtk::SingleSelection,
}

fn create_context_menu() -> gio::Menu {
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

impl Column {
    pub fn new(path: &std::path::Path, show_hidden: bool, show_meta: bool) -> Self {
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
        factory.connect_setup(move |_, list_item| {
            let root_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(4)
                .margin_end(4)
                .focusable(true)
                .build();

            let image = gtk::Image::builder()
                .icon_size(gtk::IconSize::Normal)
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
            
            text_box.append(&label);

            if show_meta {
                let meta_label = gtk::Label::builder()
                    .halign(gtk::Align::Start)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .xalign(0.0)
                    .css_classes(["caption", "dim-label"])
                    .build();
                text_box.append(&meta_label);
            }

            root_box.append(&image);
            root_box.append(&text_box);

            // Context menu gesture (Right Click)
            let gesture = gtk::GestureClick::builder()
                .button(3)
                .build();
            
            gesture.connect_pressed(move |gesture, _, x, y| {
                let widget = gesture.widget().unwrap();
                let menu = create_context_menu();
                let popover = gtk::PopoverMenu::from_model(Some(&menu));
                popover.set_parent(&widget);
                popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            });

            // Keyboard shortcut for Context Menu (Menu key or Shift+F10)
            let key_controller = gtk::EventControllerKey::new();
            let root_box_clone = root_box.clone();
            key_controller.connect_key_pressed(move |_, key, _, modifier| {
                if key == gtk::gdk::Key::Menu || (key == gtk::gdk::Key::F10 && modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK)) {
                    let widget = root_box_clone.clone().upcast::<gtk::Widget>();
                    let menu = create_context_menu();
                    let popover = gtk::PopoverMenu::from_model(Some(&menu));
                    popover.set_parent(&widget);
                    // Point to the middle of the widget
                    let width = widget.width();
                    let height = widget.height();
                    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(width / 2, height / 2, 1, 1)));
                    popover.popup();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });

            root_box.add_controller(gesture);
            root_box.add_controller(key_controller);
            list_item.set_child(Some(&root_box));
        });

        factory.connect_bind(move |_, list_item| {
            let file_info = list_item
                .item()
                .and_downcast::<gio::FileInfo>()
                .expect("Item must be FileInfo");
            
            let root_box = list_item
                .child()
                .and_downcast::<gtk::Box>()
                .expect("Child must be Box");
            
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
