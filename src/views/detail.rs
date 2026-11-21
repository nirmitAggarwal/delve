//! Detail view: a virtualised, sortable, multi-column table.

use super::{entry_icon, format_size, tag_summary};
use crate::app::App;
use crate::fs::{Entry, SortField};
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

pub struct DetailView {
    pub scroller: gtk::ScrolledWindow,
    pub column_view: gtk::ColumnView,
    pub store: gio::ListStore,
    pub selection: gtk::MultiSelection,
    entries: Rc<RefCell<Vec<Entry>>>,
    index: Rc<RefCell<HashMap<PathBuf, u32>>>,
    syncing: Rc<Cell<bool>>,
    headers: Rc<RefCell<HashMap<SortField, gtk::Button>>>,
    pub columns: Rc<RefCell<Vec<(SortField, gtk::ColumnViewColumn)>>>,
}

const COLUMNS: &[(&str, SortField, i32)] = &[
    ("name", SortField::Name, 260),
    ("type", SortField::Type, 130),
    ("size", SortField::Size, 100),
    ("modified", SortField::Modified, 150),
    ("created", SortField::Created, 150),
    ("permissions", SortField::Permissions, 110),
    ("owner", SortField::Owner, 100),
    ("tags", SortField::Tags, 160),
];

impl DetailView {
    pub fn new(app: &Rc<App>, tab_id: u64) -> DetailView {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::MultiSelection::new(Some(store.clone()));
        let column_view = gtk::ColumnView::builder()
            .model(&selection)
            .show_row_separators(true)
            .show_column_separators(false)
            .reorderable(true)
            .build();
        column_view.add_css_class("delve-detail");
        column_view.set_single_click_activate(false);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();
        scroller.add_css_class("delve-canvas");

        let syncing = Rc::new(Cell::new(false));
        let entries: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(Vec::new()));
        let index: Rc<RefCell<HashMap<PathBuf, u32>>> = Rc::new(RefCell::new(HashMap::new()));
        let headers: Rc<RefCell<HashMap<SortField, gtk::Button>>> = Rc::new(RefCell::new(HashMap::new()));
        let columns: Rc<RefCell<Vec<(SortField, gtk::ColumnViewColumn)>>> = Rc::new(RefCell::new(Vec::new()));

        for (key, field, width) in COLUMNS {
            let column = gtk::ColumnViewColumn::new(Some(field.label()), None::<gtk::ListItemFactory>);
            column.set_resizable(true);
            column.set_fixed_width(*width);
            column.set_expand(*key == "name");
            column.set_title(Some(field.label()));

            match *key {
                "name" => column.set_factory(Some(&name_factory(app, tab_id))),
                "type" => column.set_factory(Some(&text_factory(app, tab_id, |app, e| {
                    e.type_label()
                }))),
                "size" => column.set_factory(Some(&text_factory(app, tab_id, |app, e| {
                    if e.is_dir {
                        "—".to_string()
                    } else {
                        format_size(app, e.size)
                    }
                }))),
                "modified" => column.set_factory(Some(&text_factory(app, tab_id, |_, e| {
                    crate::fs::format_time(e.modified)
                }))),
                "created" => column.set_factory(Some(&text_factory(app, tab_id, |_, e| {
                    crate::fs::format_time(e.created)
                }))),
                "permissions" => column.set_factory(Some(&text_factory(app, tab_id, |_, e| {
                    e.permissions_string()
                }))),
                "owner" => column.set_factory(Some(&text_factory(app, tab_id, |_, e| e.owner_name()))),
                "tags" => column.set_factory(Some(&text_factory(app, tab_id, |app, e| {
                    tag_summary(app, &e.path)
                }))),
                _ => {}
            }
            column_view.append_column(&column);
            columns.borrow_mut().push((*field, column));
        }

        // Selection -> tab state
        {
            let strong = Rc::downgrade(app);
            let syncing = syncing.clone();
            let store_ref = store.clone();
            selection.connect_selection_changed(move |sel, _, _| {
                if syncing.get() {
                    return;
                }
                let Some(app) = strong.upgrade() else { return };
                let bitset = sel.selection();
                let mut paths = Vec::new();
                for idx in super::bitset_indices(&bitset) {
                    if let Some(obj) = store_ref.item(idx) {
                        if let Ok(boxed) = obj.downcast::<glib::BoxedAnyObject>() {
                            let entry = boxed.borrow::<Entry>();
                            paths.push(entry.path.clone());
                        }
                    }
                }
                let anchor = paths.last().cloned();
                syncing.set(true);
                app.set_selection(tab_id, paths, anchor);
                let syncing2 = syncing.clone();
                glib::idle_add_local_once(move || syncing2.set(false));
            });
        }

        // Activation
        {
            let strong = Rc::downgrade(app);
            let store_ref = store.clone();
            column_view.connect_activate(move |_, position| {
                if let Some(obj) = store_ref.item(position) {
                    if let Ok(boxed) = obj.downcast::<glib::BoxedAnyObject>() {
                        let entry = boxed.borrow::<Entry>().clone();
                        drop(boxed);
                        if let Some(app) = strong.upgrade() {
                            app.open_entry(tab_id, &entry);
                        }
                    }
                }
            });
        }

        // Drag selected rows out of the view and onto tabs, crumbs or places.
        {
            let source = gtk::DragSource::new();
            source.set_actions(
                gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK,
            );
            let strong = Rc::downgrade(app);
            source.connect_prepare(move |_, _, _| {
                let app = strong.upgrade()?;
                let paths: Vec<PathBuf> = app.tab_selection(tab_id).into_iter().collect();
                if paths.is_empty() {
                    return None;
                }
                Some(app.begin_drag(tab_id, paths))
            });
            let strong = Rc::downgrade(app);
            source.connect_drag_end(move |_, _, _| {
                if let Some(app) = strong.upgrade() {
                    app.end_drag();
                }
            });
            column_view.add_controller(source);
        }

        // Background context menu
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let strong = Rc::downgrade(app);
            let view = column_view.clone();
            gesture.connect_pressed(move |_, _, x, y| {
                if let Some(app) = strong.upgrade() {
                    let widget = view.pick(x, y, gtk::PickFlags::DEFAULT);
                    let anchor: gtk::Widget = widget.unwrap_or_else(|| view.clone().upcast());
                    app.show_context_menu(tab_id, &anchor, None, x, y);
                }
            });
            column_view.add_controller(gesture);
        }

        // Clicking empty space below the rows clears the selection.
        {
            let strong = Rc::downgrade(app);
            let view = column_view.clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_released(move |_, _, x, y| {
                let Some(app) = strong.upgrade() else { return };
                let view_widget: gtk::Widget = view.clone().upcast();
                let empty = view
                    .pick(x, y, gtk::PickFlags::DEFAULT)
                    .map(|w| w == view_widget)
                    .unwrap_or(true);
                if empty {
                    app.set_selection(tab_id, Vec::new(), None);
                }
            });
            column_view.add_controller(gesture);
        }

        DetailView {
            scroller,
            column_view,
            store,
            selection,
            entries,
            index,
            syncing,
            headers,
            columns,
        }
    }

    pub fn sync(&self, app: &Rc<App>, tab_id: u64) {
        let (visible, selection_paths, settings) = {
            let tabs = app.tabs.borrow();
            let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else { return };
            let state = tab.state.borrow();
            (state.visible_entries(), state.selection.clone(), app.settings.borrow().clone())
        };
        self.syncing.set(true);
        self.entries.replace(visible.clone());

        let items: Vec<glib::BoxedAnyObject> = visible
            .iter()
            .map(|e| glib::BoxedAnyObject::new(e.clone()))
            .collect();
        self.store.splice(0, self.store.n_items(), &items);
        {
            let mut index = self.index.borrow_mut();
            index.clear();
            for (i, entry) in visible.iter().enumerate() {
                index.insert(entry.path.clone(), i as u32);
            }
        }
        self.selection.unselect_all();
        {
            let index = self.index.borrow();
            for path in &selection_paths {
                if let Some(pos) = index.get(path) {
                    self.selection.select_item(*pos, false);
                }
            }
        }
        self.apply_columns(&settings);
        self.update_headers(app, tab_id);
        self.syncing.set(false);
    }

    fn update_headers(&self, app: &Rc<App>, tab_id: u64) {
        let sort = {
            let tabs = app.tabs.borrow();
            tabs.iter()
                .find(|t| t.id == tab_id)
                .map(|t| t.state.borrow().sort)
                .unwrap_or_default()
        };
        for (field, button) in self.headers.borrow().iter() {
            let arrow = if *field == sort.field {
                if sort.ascending {
                    "  ▲"
                } else {
                    "  ▼"
                }
            } else {
                ""
            };
            button.set_label(&format!("{}{}", field.label(), arrow));
            button.set_tooltip_text(Some(&format!("Sort by {}", field.label())));
        }
    }

    pub fn apply_columns(&self, settings: &crate::settings::Settings) {
        for (field, column) in self.columns.borrow().iter() {
            let key = match field {
                SortField::Name => "name",
                SortField::Type => "type",
                SortField::Size => "size",
                SortField::Modified => "modified",
                SortField::Created => "created",
                SortField::Permissions => "permissions",
                SortField::Owner => "owner",
                SortField::Tags => "tags",
            };
            let visible = key == "name" || settings.detail_columns.iter().any(|c| c == key);
            column.set_visible(visible);
        }
    }

    pub fn apply_selection(&self, selection: &HashSet<PathBuf>) {
        self.syncing.set(true);
        self.selection.unselect_all();
        let index = self.index.borrow();
        for path in selection {
            if let Some(pos) = index.get(path) {
                self.selection.select_item(*pos, false);
            }
        }
        self.syncing.set(false);
    }

    pub fn scroll_to_first_selected(&self) {}

    pub fn selected_anchor_widget(&self) -> gtk::Widget {
        self.column_view.clone().upcast()
    }

    pub fn entry_count(&self) -> u32 {
        self.entries.borrow().len() as u32
    }
}

fn name_factory(app: &Rc<App>, tab_id: u64) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let weak = Rc::downgrade(app);
    factory.connect_setup(move |_, item| {
        let Some(list_item) = item.downcast_ref::<gtk::ListItem>() else { return };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_start(6);
        row.set_margin_end(4);
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.set_size_request(18, 18);
        holder.set_halign(gtk::Align::Center);
        holder.set_valign(gtk::Align::Center);
        holder.set_overflow(gtk::Overflow::Hidden);
        row.append(&holder);
        let label = gtk::Label::new(None);
        label.add_css_class("cell-name");
        label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        row.append(&label);
        let tagdot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tagdot.add_css_class("color-dot");
        tagdot.set_size_request(6, 6);
        tagdot.set_valign(gtk::Align::Center);
        tagdot.set_visible(false);
        row.append(&tagdot);
        list_item.set_child(Some(&row));
        if let Some(app) = weak.upgrade() {
            super::install_row_drop(&row, &app, tab_id);
        }

        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let weak2 = weak.clone();
        gesture.connect_pressed(move |g, _, _, _| {
            let Some(app) = weak2.upgrade() else { return };
            let Some(row) = g.widget() else { return };
            let Some(path) = row.tooltip_text().map(PathBuf::from) else { return };
            if path.as_os_str().is_empty() {
                return;
            }
            if !app.tab_selection(tab_id).contains(&path) {
                app.set_selection(tab_id, vec![path.clone()], Some(path.clone()));
            }
            app.show_context_menu_at_pointer(tab_id, &path);
        });
        row.add_controller(gesture);
    });
    let bind_weak = Rc::downgrade(app);
    factory.connect_bind(move |_, item| {
        let (Some(list_item), Some(app)) = (item.downcast_ref::<gtk::ListItem>(), bind_weak.upgrade())
        else {
            return;
        };
        let Some(obj) = list_item.item().and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok()) else {
            return;
        };
        let entry = obj.borrow::<Entry>().clone();
        drop(obj);
        let Some(row) = list_item.child().and_then(|c| c.downcast::<gtk::Box>().ok()) else {
            return;
        };
        if let Some(holder) = row.first_child().and_then(|c| c.downcast::<gtk::Box>().ok()) {
            while let Some(child) = holder.first_child() {
                holder.remove(&child);
            }
            holder.append(&entry_icon(&app, &entry, 18));
        }
        if let Some(label) = row.first_child().and_then(|c| c.next_sibling()).and_then(|c| c.downcast::<gtk::Label>().ok()) {
            label.set_label(&entry.name);
        }
        if let Some(tagdot) = row.last_child().and_then(|c| c.downcast::<gtk::Box>().ok()) {
            if tagdot.has_css_class("color-dot") {
                let color = app
                    .meta
                    .borrow()
                    .item(&entry.path)
                    .and_then(|i| i.tags.first().cloned())
                    .and_then(|id| app.meta.borrow().tag(&id).map(|t| t.color.clone()));
                tagdot.set_visible(color.is_some());
                if let Some(color) = color {
                    for class in tagdot.css_classes() {
                        if class.starts_with('c') && class.len() == 17 {
                            tagdot.remove_css_class(&class);
                        }
                    }
                    tagdot.add_css_class(&format!("swatch-{}", crate::theme::color_class(&color)));
                }
            }
        }
        if let Some(child) = list_item.child() {
            child.set_tooltip_text(Some(&entry.path.display().to_string()));
        }
    });
    factory
}

fn text_factory<F>(app: &Rc<App>, _tab_id: u64, render: F) -> gtk::SignalListItemFactory
where
    F: Fn(&App, &Entry) -> String + 'static,
{
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(list_item) = item.downcast_ref::<gtk::ListItem>() else { return };
        let label = gtk::Label::new(None);
        label.add_css_class("cell-meta");
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_halign(gtk::Align::Start);
        label.set_margin_start(6);
        label.set_margin_end(4);
        list_item.set_child(Some(&label));
    });
    let render = Rc::new(render);
    let bind_weak = Rc::downgrade(app);
    factory.connect_bind(move |_, item| {
        let (Some(list_item), Some(app)) = (item.downcast_ref::<gtk::ListItem>(), bind_weak.upgrade())
        else {
            return;
        };
        let Some(obj) = list_item.item().and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok()) else {
            return;
        };
        let entry = obj.borrow::<Entry>().clone();
        drop(obj);
        let text = render(&app, &entry);
        if let Some(label) = list_item.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
            label.set_label(&text);
            if text.is_empty() {
                label.set_label("—");
                label.add_css_class("cell-muted");
            }
        }
        if let Some(child) = list_item.child() {
            child.set_tooltip_text(Some(&text));
        }
    });
    factory
}
