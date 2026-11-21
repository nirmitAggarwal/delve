//! Recursive view: an asynchronous, cancellable, tree-shaped listing of
//! everything below the current folder.

use super::{entry_icon, format_size};
use crate::app::App;
use crate::fs::{Entry, ScanNode, SortField};
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone)]
struct RowItem {
    node: usize,
    depth: u32,
    has_children: bool,
    expanded: bool,
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
    children: usize,
    relative: String,
}

pub struct RecursiveView {
    pub root: gtk::Box,
    pub list: gtk::ListView,
    store: gio::ListStore,
    selection: gtk::MultiSelection,
    scroller: gtk::ScrolledWindow,
    rows: Rc<RefCell<Vec<RowItem>>>,
    syncing: Rc<Cell<bool>>,
    summary: gtk::Label,
    path_label: gtk::Label,
    progress: gtk::Box,
    progress_label: gtk::Label,
    cancel_btn: gtk::Button,
    empty: gtk::Label,
}

impl RecursiveView {
    pub fn new(app: &Rc<App>, tab_id: u64) -> RecursiveView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.add_css_class("delve-toolbar");
        bar.set_margin_start(10);
        bar.set_margin_end(10);
        bar.set_margin_top(4);
        bar.set_margin_bottom(4);
        // Folder navigation: the current root plus one-click expand/collapse.
        // Individual folders expand in place instead of a global depth setting.
        let root_icon = gtk::Image::from_icon_name("folder-open-symbolic");
        root_icon.set_pixel_size(15);
        bar.append(&root_icon);
        let path_label = gtk::Label::new(Some("This folder"));
        path_label.add_css_class("crumb");
        path_label.set_xalign(0.0);
        path_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        path_label.set_max_width_chars(42);
        path_label.set_hexpand(true);
        bar.append(&path_label);
        let expand_btn = gtk::Button::from_icon_name("zoom-in-symbolic");
        expand_btn.add_css_class("flat");
        expand_btn.set_tooltip_text(Some("Expand every folder"));
        {
            let strong = Rc::downgrade(app);
            expand_btn.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.set_all_expanded(tab_id, true);
                }
            });
        }
        bar.append(&expand_btn);
        let collapse_btn = gtk::Button::from_icon_name("zoom-out-symbolic");
        collapse_btn.add_css_class("flat");
        collapse_btn.set_tooltip_text(Some("Collapse to one level"));
        {
            let strong = Rc::downgrade(app);
            collapse_btn.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.set_all_expanded(tab_id, false);
                }
            });
        }
        bar.append(&collapse_btn);
        bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.add_css_class("flat");
        refresh.set_tooltip_text(Some("Rescan"));
        {
            let strong = Rc::downgrade(app);
            refresh.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    let depth = app.recursive_depth(tab_id);
                    app.start_scan(tab_id, depth, true);
                }
            });
        }
        bar.append(&refresh);

        let summary = gtk::Label::new(None);
        summary.add_css_class("status-text");
        summary.set_hexpand(true);
        summary.set_xalign(1.0);
        summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        bar.append(&summary);
        root.append(&bar);

        let progress = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        progress.add_css_class("banner");
        progress.set_margin_start(10);
        progress.set_margin_end(10);
        progress.set_margin_top(4);
        progress.set_visible(false);
        let spinner = gtk::Spinner::new();
        spinner.start();
        progress.append(&spinner);
        let progress_label = gtk::Label::new(Some("Scanning…"));
        progress_label.add_css_class("detail");
        progress_label.set_hexpand(true);
        progress_label.set_xalign(0.0);
        progress_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        progress.append(&progress_label);
        let cancel_btn = gtk::Button::with_label("Cancel");
        cancel_btn.add_css_class("pill-button");
        {
            let strong = Rc::downgrade(app);
            cancel_btn.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.cancel_scan(tab_id);
                }
            });
        }
        progress.append(&cancel_btn);
        root.append(&progress);

        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::MultiSelection::new(Some(store.clone()));
        let list = gtk::ListView::builder()
            .model(&selection)
            .single_click_activate(false)
            .show_separators(true)
            .build();
        list.add_css_class("delve-recursive");
        list.set_factory(Some(&row_factory(app, tab_id)));

        let empty = gtk::Label::new(Some("Nothing to show yet"));
        empty.add_css_class("delve-empty");
        empty.set_vexpand(true);
        empty.set_valign(gtk::Align::Center);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hexpand(true)
            .build();
        scroller.add_css_class("delve-canvas");

        let overlay = gtk::Overlay::builder().child(&scroller).build();
        overlay.add_overlay(&empty);
        overlay.set_vexpand(true);
        root.append(&overlay);

        let rows: Rc<RefCell<Vec<RowItem>>> = Rc::new(RefCell::new(Vec::new()));
        let syncing = Rc::new(Cell::new(false));

        {
            let strong = Rc::downgrade(app);
            let rows_ref = rows.clone();
            let syncing = syncing.clone();
            selection.connect_selection_changed(move |sel, _, _| {
                if syncing.get() {
                    return;
                }
                let Some(app) = strong.upgrade() else { return };
                let mut paths = Vec::new();
                for idx in super::bitset_indices(&sel.selection()) {
                    if let Some(row) = rows_ref.borrow().get(idx as usize) {
                        paths.push(row.path.clone());
                    }
                }
                syncing.set(true);
                app.set_selection(tab_id, paths, None);
                let s = syncing.clone();
                glib::idle_add_local_once(move || s.set(false));
            });
        }
        {
            let strong = Rc::downgrade(app);
            let rows_ref = rows.clone();
            list.connect_activate(move |_, position| {
                if let Some(row) = rows_ref.borrow().get(position as usize) {
                    if let Some(app) = strong.upgrade() {
                        let entry = Entry::from_path(row.path.clone(), row.name.clone());
                        app.open_entry(tab_id, &entry);
                    }
                }
            });
        }
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let strong = Rc::downgrade(app);
            let rows_ref = rows.clone();
            let list_ref = list.clone();
            gesture.connect_pressed(move |_, _, x, y| {
                if let Some(app) = strong.upgrade() {
                    let path = list_ref
                        .pick(x, y, gtk::PickFlags::DEFAULT)
                        .and_then(|w| {
                            let mut cur = Some(w);
                            while let Some(c) = cur {
                                if let Some(text) = c.tooltip_text() {
                                    return Some(PathBuf::from(text));
                                }
                                cur = c.parent();
                            }
                            None
                        })
                        .filter(|p| !p.as_os_str().is_empty());
                    if let Some(p) = &path {
                        if !app.tab_selection(tab_id).contains(p) {
                            app.set_selection(tab_id, vec![p.clone()], Some(p.clone()));
                        }
                    }
                    let _ = &rows_ref;
                    let anchor: gtk::Widget = list_ref.clone().upcast();
                    app.show_context_menu(tab_id, &anchor, path, x, y);
                }
            });
            list.add_controller(gesture);
        }

        RecursiveView {
            root,
            list,
            store,
            selection,
            scroller,
            rows,
            syncing,
            summary,
            path_label,
            progress,
            progress_label,
            cancel_btn,
            empty,
        }
    }

    pub fn sync(&self, app: &Rc<App>, tab_id: u64) {
        let (scan, running, progress_text, scan_error) = {
            let tabs = app.tabs.borrow();
            let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else { return };
            let s = tab.state.borrow();
            (
                s.scan.clone(),
                s.scan_running,
                s.scan_progress.clone(),
                s.error.clone(),
            )
        };
        self.progress.set_visible(running);
        self.progress_label.set_text(if progress_text.is_empty() {
            "Scanning…"
        } else {
            &progress_text
        });
        self.cancel_btn.set_visible(running);

        let Some(scan) = scan else {
            self.rows.borrow_mut().clear();
            self.store.splice(0, self.store.n_items(), &[] as &[glib::BoxedAnyObject]);
            self.empty.set_visible(true);
            self.empty.set_label(if let Some(err) = &scan_error {
                err
            } else if running {
                "Scanning…"
            } else {
                "Switch to this view to scan the current folder"
            });
            self.summary.set_text("");
            return;
        };

        self.path_label
            .set_text(&crate::paths::display_path(&scan.root));

        // First visit: show one level and let the user drill in by clicking
        // folders, instead of asking for a manual depth value.
        let seeded = app
            .tab_by_id(tab_id)
            .map(|t| t.state.borrow().recursive_seeded)
            .unwrap_or(true);
        if !seeded {
            let mut collapse: HashSet<PathBuf> = HashSet::new();
            for node in scan.nodes.iter().skip(1) {
                if node.entry.is_dir && node.child_count > 0 && node.depth >= 1 {
                    collapse.insert(node.entry.path.clone());
                }
            }
            if let Some(tab) = app.tab_by_id(tab_id) {
                let mut state = tab.state.borrow_mut();
                state.collapsed = collapse;
                state.recursive_seeded = true;
            }
        }
        let collapsed = app
            .tab_by_id(tab_id)
            .map(|t| t.state.borrow().collapsed.clone())
            .unwrap_or_default();

        // Build children map and flatten the visible tree.
        let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, node) in scan.nodes.iter().enumerate().skip(1) {
            if let Some(parent) = node.parent {
                children.entry(parent).or_default().push(i);
            }
        }
        let sort = {
            let tabs = app.tabs.borrow();
            tabs.iter()
                .find(|t| t.id == tab_id)
                .map(|t| t.state.borrow().sort)
                .unwrap_or_default()
        };
        for kids in children.values_mut() {
            kids.sort_by(|a, b| {
                let na = &scan.nodes[*a];
                let nb = &scan.nodes[*b];
                let ord = match sort.field {
                    SortField::Size => na.total.cmp(&nb.total),
                    SortField::Modified => na.entry.modified.cmp(&nb.entry.modified),
                    SortField::Type => na
                        .entry
                        .type_label()
                        .to_lowercase()
                        .cmp(&nb.entry.type_label().to_lowercase()),
                    _ => crate::fs::compare_names(&na.entry.name, &nb.entry.name),
                };
                if sort.ascending {
                    ord
                } else {
                    ord.reverse()
                }
            });
        }
        let root_path = &scan.nodes[0].entry.path;
        let mut rows = Vec::new();
        fn walk(
            idx: usize,
            scan: &crate::fs::ScanResult,
            children: &HashMap<usize, Vec<usize>>,
            collapsed: &HashSet<PathBuf>,
            rows: &mut Vec<RowItem>,
            root_path: &std::path::Path,
        ) {
            let node = &scan.nodes[idx];
            let rel = node
                .entry
                .path
                .strip_prefix(root_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let kids = children.get(&idx).cloned().unwrap_or_default();
            let expanded = !collapsed.contains(&node.entry.path);
            rows.push(RowItem {
                node: idx,
                depth: node.depth,
                has_children: !kids.is_empty(),
                expanded,
                path: node.entry.path.clone(),
                name: node.entry.name.clone(),
                is_dir: node.entry.is_dir,
                size: node.total,
                children: node.child_count,
                relative: rel,
            });
            if expanded {
                for kid in kids {
                    walk(kid, scan, children, collapsed, rows, root_path);
                }
            }
        }
        // Root row is included so the tree has context.
        walk(0, &scan, &children, &collapsed, &mut rows, root_path);
        self.rows.replace(rows.clone());

        let items: Vec<glib::BoxedAnyObject> = rows
            .iter()
            .map(|r| glib::BoxedAnyObject::new(r.clone()))
            .collect();
        self.syncing.set(true);
        self.store.splice(0, self.store.n_items(), &items);
        self.syncing.set(false);

        self.empty.set_visible(rows.len() <= 1);
        if rows.len() <= 1 {
            self.empty.set_label("This folder has no contents below it");
        }

        let mut summary = format!(
            "{} files · {} folders · {}",
            scan.total_files,
            scan.total_folders,
            format_size(app, scan.total_bytes)
        );
        if scan.errors > 0 {
            summary.push_str(&format!(" · {} unreadable", scan.errors));
        }
        if scan.truncated {
            summary.push_str(" · truncated (entry limit)");
        }
        message_append(&mut summary, &format!(" · {} ms", scan.elapsed_ms));
        self.summary.set_text(&summary);

        // Restore selection highlight.
        let selection_set = {
            let tabs = app.tabs.borrow();
            tabs.iter()
                .find(|t| t.id == tab_id)
                .map(|t| t.state.borrow().selection.clone())
                .unwrap_or_default()
        };
        self.syncing.set(true);
        self.selection.unselect_all();
        for (i, row) in self.rows.borrow().iter().enumerate() {
            if selection_set.contains(&row.path) {
                self.selection.select_item(i as u32, false);
            }
        }
        self.syncing.set(false);
        let _ = self.scroller;
    }

    pub fn list_widget(&self) -> &gtk::ListView {
        &self.list
    }

    /// Lightweight update used while a scan is still running.
    pub fn update_progress_label(&self, text: &str) {
        self.progress_label.set_text(text);
    }
}

fn message_append(base: &mut String, extra: &str) {
    base.push_str(extra);
}

fn row_factory(app_ref: &Rc<App>, tab_id: u64) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let setup_weak = Rc::downgrade(app_ref);
    let bind_weak = Rc::downgrade(app_ref);
    factory.connect_setup(move |_, item| {
        let Some(list_item) = item.downcast_ref::<gtk::ListItem>() else { return };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_margin_top(2);
        row.set_margin_bottom(2);
        let arrow = gtk::Button::from_icon_name("pan-end-symbolic");
        arrow.add_css_class("flat");
        arrow.set_size_request(20, 20);
        arrow.set_valign(gtk::Align::Center);
        row.append(&arrow);
        let image = gtk::Image::new();
        image.set_pixel_size(16);
        row.append(&image);
        let name_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        name_box.set_hexpand(true);
        let name = gtk::Label::new(None);
        name.add_css_class("cell-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name_box.append(&name);
        let rel = gtk::Label::new(None);
        rel.add_css_class("cell-muted");
        rel.set_xalign(0.0);
        rel.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        rel.set_visible(false);
        name_box.append(&rel);
        row.append(&name_box);
        let detail = gtk::Label::new(None);
        detail.add_css_class("cell-muted");
        detail.set_width_chars(14);
        detail.set_xalign(1.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&detail);
        let size = gtk::Label::new(None);
        size.add_css_class("cell-meta");
        size.set_width_chars(9);
        size.set_xalign(1.0);
        row.append(&size);
        list_item.set_child(Some(&row));
        if let Some(app) = setup_weak.upgrade() {
            super::install_row_drop(&row, &app, tab_id);
        }

        // Clicking a folder row expands it one level in place.
        {
            let weak = setup_weak.clone();
            let row_click = row.clone();
            let click = gtk::GestureClick::new();
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            click.connect_released(move |g, _, _, _| {
                if g.current_button() != 1 {
                    return;
                }
                let Some(app) = weak.upgrade() else { return };
                let Some(path) = row_click.tooltip_text().map(PathBuf::from) else { return };
                if path.as_os_str().is_empty() || !path.is_dir() {
                    return;
                }
                let collapsed = app
                    .tab_by_id(tab_id)
                    .map(|t| t.state.borrow().collapsed.contains(&path))
                    .unwrap_or(false);
                if collapsed {
                    app.toggle_collapsed(tab_id, &path);
                }
            });
            row.add_controller(click);
        }

        // Expander click -> toggle in tab state.
        let weak = setup_weak.clone();
        let arrow_clone = arrow.clone();
        arrow.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = arrow_clone.tooltip_text().map(PathBuf::from) else { return };
            if !path.as_os_str().is_empty() {
                app.toggle_collapsed(tab_id, &path);
            }
        });
    });
    factory.connect_bind(move |_, item| {
        let (Some(list_item), Some(app)) = (item.downcast_ref::<gtk::ListItem>(), bind_weak.upgrade()) else {
            return;
        };
        let Some(obj) = list_item.item().and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok()) else {
            return;
        };
        let rowdata = obj.borrow::<RowItem>().clone();
        drop(obj);
        let Some(row) = list_item.child().and_then(|c| c.downcast::<gtk::Box>().ok()) else {
            return;
        };
        let arrow = row.first_child().and_then(|c| c.downcast::<gtk::Button>().ok());
        let image = row
            .first_child()
            .and_then(|c| c.next_sibling())
            .and_then(|c| c.downcast::<gtk::Image>().ok());
        if let Some(image) = image {
            let entry = Entry::from_path(rowdata.path.clone(), rowdata.name.clone());
            let replacement = entry_icon(&app, &entry, 16);
            row.remove(&image);
            row.insert_child_after(&replacement, arrow.as_ref());
        }
        if let Some(arrow) = arrow {
            arrow.set_visible(rowdata.has_children);
            arrow.set_icon_name(if rowdata.expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            });
            arrow.set_tooltip_text(Some(&rowdata.path.display().to_string()));
        }
        let name_box = row
            .first_child()
            .and_then(|c| c.next_sibling())
            .and_then(|c| c.next_sibling())
            .and_then(|c| c.downcast::<gtk::Box>().ok());
        if let Some(name_box) = name_box {
            if let Some(name) = name_box.first_child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                name.set_label(&rowdata.name);
            }
            if let Some(rel) = name_box
                .first_child()
                .and_then(|c| c.next_sibling())
                .and_then(|c| c.downcast::<gtk::Label>().ok())
            {
                rel.set_label(&rowdata.relative);
                rel.set_visible(!rowdata.relative.is_empty() && rowdata.depth > 1);
            }
            row.set_margin_start(rowdata.depth.saturating_sub(1) as i32 * 14);
        }
        let detail = row.last_child()
            .and_then(|c| c.prev_sibling())
            .and_then(|c| c.downcast::<gtk::Label>().ok());
        if let Some(detail) = detail {
            let detail_text = if rowdata.is_dir {
                format!("{} items", rowdata.children)
            } else {
                String::new()
            };
            detail.set_label(&detail_text);
        }
        if let Some(size) = row.last_child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
            let size_text = super::format_size(&app, rowdata.size);
            size.set_label(&size_text);
        }
        if let Some(child) = list_item.child() {
            child.set_tooltip_text(Some(&rowdata.path.display().to_string()));
        }
        let _ = &rowdata.node;
    });
    factory
}

impl RecursiveView {
    #[allow(dead_code)]
    fn dummy(&self, _: &ScanNode) {}
}
