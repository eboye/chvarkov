use gtk4 as gtk;
use gtk::prelude::*;
use crate::utils;

pub struct ListView {
    pub widget: gtk::ScrolledWindow,
    pub column_view: gtk::ColumnView,
}

impl ListView {
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

        let sort_type_owned = sort_type.to_string();
        // Tree Model for nested expansion
        let tree_model = gtk::TreeListModel::new(sort_model, false, false, move |item| {
            let file_info = item.downcast_ref::<gio::FileInfo>().unwrap();
            if file_info.file_type() == gio::FileType::Directory {
                if let Some(file) = file_info.attribute_object("standard::file").and_downcast::<gio::File>() {
                    let child_dir_list = gtk::DirectoryList::builder()
                        .attributes("standard::name,standard::display-name,standard::icon,standard::type,standard::is-hidden,standard::size,standard::content-type,time::modified,standard::is-symlink-target-directory,standard::n-children,standard::file")
                        .file(&file)
                        .monitored(true)
                        .build();

                    let child_filter = gtk::CustomFilter::new(move |item| {
                        let info = item.downcast_ref::<gio::FileInfo>().unwrap();
                        if !show_hidden {
                            if info.is_hidden() || info.name().to_string_lossy().starts_with('.') {
                                return false;
                            }
                        }
                        true
                    });

                    let child_filter_model = gtk::FilterListModel::new(Some(child_dir_list), Some(child_filter));
                    let child_sorter = utils::create_sorter(&sort_type_owned, folders_first);
                    let child_sort_model = gtk::SortListModel::new(Some(child_filter_model), Some(child_sorter));

                    return Some(child_sort_model.upcast());
                }
            }
            None
        });

        let selection_model = gtk::MultiSelection::new(Some(tree_model));

        let column_view = gtk::ColumnView::builder()
            .model(&selection_model)
            .reorderable(true)
            .show_column_separators(true)
            .focusable(true)
            .build();

        let icon_size = utils::get_list_icon_size(zoom_level);

        // 1. Name Column (with TreeExpander)
        let name_factory = gtk::SignalListItemFactory::new();
        name_factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let expander = gtk::TreeExpander::new();

            let container = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();

            let image = gtk::Image::builder()
                .pixel_size(icon_size)
                .build();
            let label = gtk::Label::builder()
                .halign(gtk::Align::Start)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();

            container.append(&image);
            container.append(&label);

            // Context menu right-click gesture still works on the row content
            utils::attach_context_menu_gesture(&container);

            expander.set_child(Some(&container));
            list_item.set_child(Some(&expander));
        });

        name_factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let tree_row = list_item.item().and_downcast::<gtk::TreeListRow>().unwrap();
            let file_info = tree_row.item().and_downcast::<gio::FileInfo>().unwrap();

            let expander = list_item.child().and_downcast::<gtk::TreeExpander>().unwrap();
            expander.set_list_row(Some(&tree_row));

            let container = expander.child().and_downcast::<gtk::Box>().unwrap();
            let image = container.first_child().unwrap().downcast::<gtk::Image>().unwrap();
            let label = image.next_sibling().unwrap().downcast::<gtk::Label>().unwrap();

            label.set_text(&file_info.display_name());

            utils::set_icon_and_thumbnail(&image, &file_info);
        });

        let name_col = gtk::ColumnViewColumn::builder()
            .title("Name")
            .factory(&name_factory)
            .expand(true)
            .build();
        column_view.append_column(&name_col);

        if show_meta {
            // 2. Type Column
            let type_factory = gtk::SignalListItemFactory::new();
            type_factory.connect_setup(|_, list_item| {
                let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
                let label = gtk::Label::builder()
                    .halign(gtk::Align::Start)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .css_classes(["dim-label", "caption"])
                    .margin_start(8)
                    .build();
                list_item.set_child(Some(&label));
            });
            type_factory.connect_bind(|_, list_item| {
                let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
                let tree_row = list_item.item().and_downcast::<gtk::TreeListRow>().unwrap();
                let file_info = tree_row.item().and_downcast::<gio::FileInfo>().unwrap();
                let label = list_item.child().and_downcast::<gtk::Label>().unwrap();

                let is_dir = file_info.file_type() == gio::FileType::Directory;
                let text = if is_dir {
                    "Folder".to_string()
                } else {
                    file_info.content_type()
                        .map(|ct| gio::content_type_get_description(&ct).to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                };
                label.set_text(&text);
            });
            let type_col = gtk::ColumnViewColumn::builder()
                .title("Type")
                .factory(&type_factory)
                .fixed_width(150)
                .build();
            column_view.append_column(&type_col);

            // 3. Date Column
            let date_factory = gtk::SignalListItemFactory::new();
            date_factory.connect_setup(|_, list_item| {
                let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
                let label = gtk::Label::builder()
                    .halign(gtk::Align::Start)
                    .css_classes(["dim-label", "caption"])
                    .margin_start(8)
                    .build();
                list_item.set_child(Some(&label));
            });
            date_factory.connect_bind(|_, list_item| {
                let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
                let tree_row = list_item.item().and_downcast::<gtk::TreeListRow>().unwrap();
                let file_info = tree_row.item().and_downcast::<gio::FileInfo>().unwrap();
                let label = list_item.child().and_downcast::<gtk::Label>().unwrap();

                let date = file_info.modification_date_time()
                    .and_then(|dt| dt.format("%Y-%m-%d").ok())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "----".to_string());
                label.set_text(&date);
            });
            let date_col = gtk::ColumnViewColumn::builder()
                .title("Date Modified")
                .factory(&date_factory)
                .fixed_width(120)
                .build();
            column_view.append_column(&date_col);

            // 4. Size Column
            let size_factory = gtk::SignalListItemFactory::new();
            size_factory.connect_setup(|_, list_item| {
                let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
                let label = gtk::Label::builder()
                    .halign(gtk::Align::End)
                    .css_classes(["dim-label", "caption"])
                    .margin_end(8)
                    .build();
                list_item.set_child(Some(&label));
            });
            size_factory.connect_bind(|_, list_item| {
                let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
                let tree_row = list_item.item().and_downcast::<gtk::TreeListRow>().unwrap();
                let file_info = tree_row.item().and_downcast::<gio::FileInfo>().unwrap();
                let label = list_item.child().and_downcast::<gtk::Label>().unwrap();

                let is_dir = file_info.file_type() == gio::FileType::Directory;
                if is_dir {
                    let count = file_info.attribute_uint32("standard::n-children");
                    label.set_text(&format!("{} items", count));
                } else {
                    label.set_text(&utils::format_size(file_info.size()));
                }
            });
            let size_col = gtk::ColumnViewColumn::builder()
                .title("Size")
                .factory(&size_factory)
                .fixed_width(100)
                .build();
            column_view.append_column(&size_col);
        }

        // activations
        column_view.connect_activate(move |_, _| {
            utils::trigger_open_action();
        });

        // Universal Keyboard Shortcut Controller on the ColumnView itself
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        let sel_model_shortcuts = selection_model.clone();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            let selection = sel_model_shortcuts.selection();
            if selection.is_empty() { return glib::Propagation::Proceed; }

            let first_idx = selection.minimum();
            let model = sel_model_shortcuts.model().unwrap();
            let item = model.item(first_idx);

            // Handle Tree expansion
            if let Some(tree_row) = item.and_downcast::<gtk::TreeListRow>() {
                if key == gtk::gdk::Key::Right {
                    if tree_row.is_expandable() && !tree_row.is_expanded() {
                        tree_row.set_expanded(true);
                        return glib::Propagation::Stop;
                    }
                } else if key == gtk::gdk::Key::Left {
                    if tree_row.is_expanded() {
                        tree_row.set_expanded(false);
                        return glib::Propagation::Stop;
                    }
                }
            }
            glib::Propagation::Proceed
        });
        column_view.add_controller(key_ctrl);

        utils::setup_view_common_controllers(&column_view, &selection_model);

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&column_view)
            .build();

        Self {
            widget: scrolled_window,
            column_view,
        }
    }
}
