//! Navigation, directory loading, recursive scans, search and selection.

use crate::app::{App, Tab};
use crate::fs::{self, Entry, ScanConfig, SortField, SortSpec};
use crate::paths::UserDir;
use crate::model::{Location, SearchFilters};
use crate::settings::ViewKind;
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

impl App {
    pub fn new_tab(self: &Rc<Self>, location: Location) -> Option<Rc<Tab>> {
        let id = self.next_tab_id.get();
        self.next_tab_id.set(id + 1);
        let tab = Rc::new(Tab::new(self, id, location.clone()));
        {
            let mut tabs = self.tabs.borrow_mut();
            tabs.push(tab.clone());
            self.active.set(tabs.len() - 1);
        }
        self.ui()
            .canvas_stack
            .add_named(&tab.canvas, Some(&id.to_string()));
        self.ui()
            .canvas_stack
            .set_visible_child_name(&id.to_string());
        self.navigate(id, location, false);
        self.refresh_ui();
        self.refresh_sidebar();
        Some(tab)
    }

    pub fn activate_tab(self: &Rc<Self>, index: usize) {
        if index >= self.tabs.borrow().len() {
            return;
        }
        self.active.set(index);
        let id = self.tabs.borrow()[index].id;
        self.ui().canvas_stack.set_visible_child_name(&id.to_string());
        self.sync_views(id);
        self.watch_tab(id);
        let view = self.tabs.borrow()[index].state.borrow().view;
        self.ui().view_btn_icon.set_icon_name(Some(view.icon_name()));
        self.ui().view_btn_label.set_text(view.label());
        self.refresh_ui();
        self.refresh_sidebar();
    }

    pub fn close_tab_by_id(self: &Rc<Self>, id: u64) {
        if let Some(index) = self.tab_index(id) {
            self.close_tab_index(index);
        }
    }

    pub fn close_tab_index(self: &Rc<Self>, index: usize) {
        let removed = {
            let mut tabs = self.tabs.borrow_mut();
            if index >= tabs.len() {
                return;
            }
            let tab = tabs.remove(index);
            let location = tab.state.borrow().location.clone();
            self.closed_tabs.borrow_mut().push(location);
            if self.closed_tabs.borrow().len() > 12 {
                self.closed_tabs.borrow_mut().remove(0);
            }
            self.ui()
                .canvas_stack
                .remove(&tab.canvas);
            tab.id
        };
        let _ = removed;
        if self.tabs.borrow().is_empty() {
            let loc = Location::Path(crate::paths::home());
            self.new_tab(loc);
            return;
        }
        let new_index = index.min(self.tabs.borrow().len().saturating_sub(1));
        self.activate_tab(new_index);
    }

    pub fn reopen_closed_tab(self: &Rc<Self>) {
        let location = self.closed_tabs.borrow_mut().pop();
        if let Some(location) = location {
            self.new_tab(location);
        } else {
            self.notify("No recently closed tabs.", false);
        }
    }

    pub fn reorder_tab(self: &Rc<Self>, source_id: u64, target_id: u64) {
        let (Some(from), Some(to)) = (self.tab_index(source_id), self.tab_index(target_id)) else {
            return;
        };
        if from == to {
            return;
        }
        {
            let mut tabs = self.tabs.borrow_mut();
            let tab = tabs.remove(from);
            tabs.insert(to, tab);
            if let Some(active) = tabs.iter().position(|t| t.id == self.active_tab().map(|a| a.id).unwrap_or(0)) {
                self.active.set(active);
            }
        }
        self.rebuild_tab_strip();
    }

    pub fn next_tab(self: &Rc<Self>, previous: bool) {
        let len = self.tabs.borrow().len();
        if len == 0 {
            return;
        }
        let current = self.active.get();
        let next = if previous {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        };
        self.activate_tab(next);
    }

    pub fn show_tab_menu(self: &Rc<Self>, anchor: &impl IsA<gtk::Widget>, tab_id: u64) {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.set_parent(anchor);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let add = |label: &str, action: Rc<dyn Fn(&Rc<App>)>| {
            let button = gtk::Button::with_label(label);
            button.add_css_class("flat");
            button.set_halign(gtk::Align::Fill);
            let weak = Rc::downgrade(self);
            let popover = popover.clone();
            let action = action.clone();
            button.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    popover.popdown();
                    action(&app);
                }
            });
            list.append(&button);
        };
        add("New tab", Rc::new(|app| {
            app.new_tab(app.active_location());
        }));
        add("Duplicate tab", Rc::new(move |app| {
            if let Some(tab) = app.tab_by_id(tab_id) {
                let loc = tab.state.borrow().location.clone();
                app.new_tab(loc);
            }
        }));
        add("Copy location", Rc::new(move |app| {
            if let Some(tab) = app.tab_by_id(tab_id) {
                let loc = tab.state.borrow().location.clone();
                if let Some(path) = loc.path() {
                    app.copy_text(&path.display().to_string());
                    app.notify("Location copied", false);
                }
            }
        }));
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        add("Close", Rc::new(move |app| app.close_tab_by_id(tab_id)));
        add("Close others", Rc::new(move |app| {
            let ids: Vec<u64> = app
                .tabs
                .borrow()
                .iter()
                .filter(|t| t.id != tab_id)
                .map(|t| t.id)
                .collect();
            for id in ids {
                app.close_tab_by_id(id);
            }
        }));
        add("Close to right", Rc::new(move |app| {
            let Some(index) = app.tab_index(tab_id) else { return };
            let ids: Vec<u64> = app.tabs.borrow().iter().skip(index + 1).map(|t| t.id).collect();
            for id in ids {
                app.close_tab_by_id(id);
            }
        }));
        add("Reopen closed tab", Rc::new(|app| app.reopen_closed_tab()));
        popover.set_child(Some(&list));
        popover.popup();
    }

    pub fn navigate_active(self: &Rc<Self>, location: Location) {
        if let Some(tab) = self.active_tab() {
            self.navigate(tab.id, location, true);
        }
    }

    pub fn navigate(self: &Rc<Self>, tab_id: u64, location: Location, add_history: bool) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            if add_history {
                let pos = state.history_pos + 1;
                state.history.truncate(pos);
                state.history.push(location.clone());
                state.history_pos = state.history.len() - 1;
            } else if state.history.is_empty() {
                state.history.push(location.clone());
                state.history_pos = 0;
            }
            state.location = location.clone();
            state.selection.clear();
            state.anchor = None;
            state.entries.clear();
            state.error = None;
            state.filter_text.clear();
            state.scan = None;
            state.scan_running = false;
            if let Some(cancel) = state.scan_cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            state.collapsed.clear();
            state.recursive_seeded = false;
            state.treemap_root = None;
            state.graph_expanded.clear();
            state.banner = None;
            state.generation += 1;
            state.loading = true;
            if let Location::Tag { ids, match_all } = &location {
                state.tag_filter = ids.clone();
                state.tag_match_all = *match_all;
            }
        }
        if let Some(path) = location.path() {
            if location.is_path_like() && path.is_dir() {
                {
                    let mut settings = self.settings.borrow_mut();
                    settings.add_recent_location(path);
                    settings.record_visit(path);
                    settings.save();
                }
            }
        }
        if let Some(tab) = self.tab_by_id(tab_id) {
            tab.clear_banner();
        }
        {
            let ui = self.ui();
            if ui.search.text() != "" {
                ui.search.set_text("");
            }
        }
        self.refresh_ui();
        self.refresh_sidebar();
        self.load(tab_id);
        self.watch_tab(tab_id);
    }

    pub fn load(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let (location, generation, view, running) = {
            let state = tab.state.borrow();
            (
                state.location.clone(),
                state.generation,
                state.view,
                state.scan_running,
            )
        };
        // Scan-backed views must start scanning on navigation, not only when
        // the view is first selected: otherwise browsing folders inside the
        // treemap leaves an empty "No size data yet" canvas.
        if matches!(view, ViewKind::Recursive | ViewKind::Treemap | ViewKind::Graph)
            && !running
        {
            if let Some(path) = location.path() {
                if path.is_dir() {
                    let depth = self.recursive_depth(tab_id).max(6);
                    self.start_scan(tab_id, depth, true);
                }
            }
        }
        match location {
            Location::Home => self.load_home(tab_id, generation),
            Location::Path(path) => self.load_path(tab_id, path, generation),
            Location::Search { root, query, recursive, filters } => {
                self.load_search(tab_id, root, query, recursive, filters, generation)
            }
            Location::Trash => {
                std::thread::spawn(move || {
                    let result = fs::trash_entries();
                    crate::dispatch::post_simple(move |app| {
                        app.apply_trash(tab_id, generation, result);
                    });
                });
            }
            Location::RecentFiles => {
                let paths = self.settings.borrow().recent_files.clone();
                self.apply_simple_entries(tab_id, generation, paths, "No recently opened files");
            }
            Location::RecentLocations => {
                let paths = self.settings.borrow().recent_locations.clone();
                self.apply_simple_entries(tab_id, generation, paths, "No recent locations yet");
            }
            Location::Tag { ids, match_all } => {
                let paths = self.meta.borrow().paths_with_tags(&ids, match_all);
                let mut entries: Vec<Entry> = Vec::new();
                for path in paths {
                    if !path.exists() {
                        continue;
                    }
                    let name = fs::file_name_of(&path);
                    entries.push(Entry::from_path(path, name));
                }
                self.apply_entries(tab_id, generation, entries, None);
            }
        }
    }

    fn load_home(self: &Rc<Self>, tab_id: u64, generation: u64) {
        let recent = self.settings.borrow().recent_locations.clone();
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut push = |p: PathBuf| {
            if p.is_dir() && !dirs.contains(&p) {
                dirs.push(p);
            }
        };
        for dir in [
            UserDir::Desktop,
            UserDir::Documents,
            UserDir::Downloads,
            UserDir::Pictures,
            UserDir::Music,
            UserDir::Videos,
        ] {
            push(crate::paths::user_dir(dir));
        }
        let home = crate::paths::home();
        for path in recent {
            // Home is already the sidebar entry; skip it, the filesystem root
            // and any parent of home so quick access stays useful.
            if path == home || home.starts_with(&path) {
                continue;
            }
            push(path);
        }
        let entries: Vec<Entry> = dirs
            .into_iter()
            .map(|path| {
                let name = if path == crate::paths::home() {
                    "Home".to_string()
                } else {
                    fs::file_name_of(&path)
                };
                Entry::from_path(path, name)
            })
            .collect();
        self.apply_entries(tab_id, generation, entries, None);
    }

    fn load_path(self: &Rc<Self>, tab_id: u64, path: PathBuf, generation: u64) {
        std::thread::spawn(move || {
            let listing = fs::read_dir(&path);
            crate::dispatch::post_simple(move |app| {
                app.apply_listing(tab_id, generation, listing);
            });
        });
    }

    fn apply_listing(self: &Rc<Self>, tab_id: u64, generation: u64, listing: fs::Listing) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        if tab.state.borrow().generation != generation {
            return;
        }
        let entries = listing.entries;
        let error = listing.error.clone();
        // Live refreshes keep the selection on paths that still exist.
        {
            let valid: HashSet<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
            let mut state = tab.state.borrow_mut();
            state.selection.retain(|p| valid.contains(p));
            let anchor_missing = state
                .anchor
                .as_ref()
                .map(|a| !valid.contains(a))
                .unwrap_or(false);
            if anchor_missing {
                state.anchor = None;
            }
        }
        self.apply_entries(tab_id, generation, entries, None);
        if let Some(error) = error {
            if let Some(tab) = self.tab_by_id(tab_id) {
                tab.set_banner(&error, "The folder may have been moved, removed or made unreadable.", true);
                tab.state.borrow_mut().error = Some(error);
            }
            self.update_status();
        }
    }

    fn apply_simple_entries(
        self: &Rc<Self>,
        tab_id: u64,
        generation: u64,
        paths: Vec<PathBuf>,
        empty_hint: &str,
    ) {
        let mut entries = Vec::new();
        for path in paths {
            if path.exists() {
                entries.push(Entry::from_path(path.clone(), fs::file_name_of(&path)));
            }
        }
        self.apply_entries(tab_id, generation, entries, None);
        if let Some(tab) = self.tab_by_id(tab_id) {
            if tab.state.borrow().entries.is_empty() {
                tab.set_banner(empty_hint, "", false);
            }
        }
    }

    fn apply_trash(
        self: &Rc<Self>,
        tab_id: u64,
        generation: u64,
        result: Result<Vec<fs::TrashEntry>, String>,
    ) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        if tab.state.borrow().generation != generation {
            return;
        }
        match result {
            Ok(items) => {
                let mut entries = Vec::new();
                for item in items {
                    let mut entry = Entry::from_path(item.original_parent.join(&item.name), item.name.clone());
                    // The trash item is identified by its .trashinfo path; the
                    // entry keeps that identity so restore/delete are exact.
                    entry.path = item.path.clone();
                    entry.is_dir = item.is_dir;
                    entry.size = item.size.unwrap_or(0);
                    entry.modified = item.deleted;
                    entry.virtual_root = Some(item.original_parent.clone());
                    entry.name = item.name;
                    entries.push(entry);
                }
                self.apply_entries(tab_id, generation, entries, None);
            }
            Err(err) => {
                self.apply_entries(tab_id, generation, Vec::new(), None);
                if let Some(tab) = self.tab_by_id(tab_id) {
                    tab.set_banner(&err, "Trash is provided by the freedesktop.org trash specification.", true);
                }
            }
        }
    }

    fn apply_entries(
        self: &Rc<Self>,
        tab_id: u64,
        generation: u64,
        mut entries: Vec<Entry>,
        banner: Option<(String, String, bool)>,
    ) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let state = tab.state.borrow();
            if state.generation != generation {
                return;
            }
        }
        // Collect tag names once for deterministic tag sorting.
        let tag_map: HashMap<PathBuf, String> = {
            let meta = self.meta.borrow();
            let mut map = HashMap::new();
            for item in meta.items.values() {
                if item.tags.is_empty() {
                    continue;
                }
                let names: Vec<String> = item
                    .tags
                    .iter()
                    .filter_map(|id| meta.tag(id).map(|t| t.name.clone()))
                    .collect();
                map.insert(item.path.clone(), names.join(", "));
            }
            map
        };
        let (spec, previous_selection, show_banner_error) = {
            let state = tab.state.borrow();
            (state.sort, state.selection.clone(), state.error.is_some())
        };
        fs::sort_entries(&mut entries, spec, &|e| {
            tag_map.get(&e.path).cloned().unwrap_or_default()
        });
        {
            let mut state = tab.state.borrow_mut();
            state.entries = entries.clone();
            state.loading = false;
            state.selection.retain(|p| entries.iter().any(|e| &e.path == p));
            let _ = previous_selection;
            if let Some((title, detail, error)) = banner {
                drop(state);
                tab.set_banner(&title, &detail, error);
            }
        }
        self.sync_views(tab_id);
        self.refresh_ui();
        self.refresh_sidebar();
        let _ = show_banner_error;
    }

    pub fn go_back(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let target = {
            let mut state = tab.state.borrow_mut();
            if state.history_pos == 0 {
                return;
            }
            state.history_pos -= 1;
            state.history[state.history_pos].clone()
        };
        self.navigate(tab.id, target, false);
    }

    pub fn go_forward(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let target = {
            let mut state = tab.state.borrow_mut();
            if state.history_pos + 1 >= state.history.len() {
                return;
            }
            state.history_pos += 1;
            state.history[state.history_pos].clone()
        };
        self.navigate(tab.id, target, false);
    }

    pub fn go_up(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let path = tab.state.borrow().location.path().cloned();
        match path {
            Some(p) if p != std::path::Path::new("/") => {
                if let Some(parent) = p.parent() {
                    self.navigate(tab.id, Location::Path(parent.to_path_buf()), true);
                }
            }
            _ => self.navigate(tab.id, Location::Path(crate::paths::home()), true),
        }
    }

    pub fn toggle_location_entry(self: &Rc<Self>, show: bool) {
        let ui = self.ui();
        if show {
            let location = self.active_location();
            let text = match location {
                Location::Home => crate::paths::home().display().to_string(),
                other => other.path().map(|p| p.display().to_string()).unwrap_or_default(),
            };
            ui.location_entry.set_text(&text);
            ui.path_stack.set_visible_child_name("entry");
            ui.location_entry.grab_focus();
            ui.location_entry.select_region(0, -1);
        } else {
            ui.path_stack.set_visible_child_name("crumbs");
        }
    }

    pub fn handle_escape(self: &Rc<Self>) -> glib::Propagation {
        let ui = self.ui();
        if ui.path_stack.visible_child_name().as_deref() == Some("entry") {
            ui.path_stack.set_visible_child_name("crumbs");
            return glib::Propagation::Stop;
        }
        if ui.narrow.get() && (ui.left_drawer.reveals_child() || ui.right_drawer.reveals_child()) {
            let app = self.clone();
            app.close_drawers();
            return glib::Propagation::Stop;
        }
        let Some(tab) = self.active_tab() else { return glib::Propagation::Proceed };
        let scanning = tab.state.borrow().scan_running;
        if scanning {
            self.cancel_scan(tab.id);
            return glib::Propagation::Stop;
        }
        if !tab.state.borrow().selection.is_empty() {
            self.set_selection(tab.id, Vec::new(), None);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    // -----------------------------------------------------------------
    // Filtering and search
    // -----------------------------------------------------------------

    pub fn on_filter_changed(self: &Rc<Self>, text: &str) {
        let Some(tab) = self.active_tab() else { return };
        let tab_id = tab.id;
        tab.state.borrow_mut().filter_text = text.to_string();
        self.sync_views(tab_id);
        self.update_status();
    }

    pub fn start_recursive_search_from_filter(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let query = self.ui().search.text().to_string();
        let filters = self.ui().search_filters.borrow().clone();
        let Some(root) = tab.state.borrow().location.path().cloned() else {
            self.notify("Search needs a folder location.", true);
            return;
        };
        let location = Location::Search { root, query, recursive: true, filters };
        self.navigate(tab.id, location, true);
    }

    pub fn start_search(self: &Rc<Self>, query: String, recursive: bool, filters: SearchFilters) {
        let Some(tab) = self.active_tab() else { return };
        let Some(root) = tab.state.borrow().location.path().cloned() else {
            self.notify("Search needs a folder location.", true);
            return;
        };
        self.navigate(
            tab.id,
            Location::Search { root, query, recursive, filters },
            true,
        );
    }

    fn load_search(
        self: &Rc<Self>,
        tab_id: u64,
        root: PathBuf,
        query: String,
        recursive: bool,
        filters: SearchFilters,
        generation: u64,
    ) {
        let cancel = Arc::new(AtomicBool::new(false));
        if let Some(tab) = self.tab_by_id(tab_id) {
            tab.state.borrow_mut().scan_cancel = Some(cancel.clone());
            tab.set_banner("Searching…", &filters.describe(), false);
        }
        let cancel_thread = cancel.clone();
        std::thread::spawn(move || {
            let mut results: Vec<Entry> = Vec::new();
            let mut errors = 0u32;
            let mut scanned = 0usize;
            let query_lower = query.trim().to_lowercase();
            let mut stack = vec![root.clone()];
            let mut last_tick = std::time::Instant::now();
            while let Some(dir) = stack.pop() {
                if cancel_thread.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(rd) = std::fs::read_dir(&dir) else {
                    errors += 1;
                    continue;
                };
                for item in rd.flatten() {
                    if cancel_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    scanned += 1;
                    let name = item.file_name().to_string_lossy().to_string();
                    let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let path = item.path();
                    let entry = Entry::from_path(path.clone(), name.clone());
                    if !recursive && is_dir {
                        // Immediate children only.
                    }
                    let matches_name = query_lower.is_empty()
                        || name.to_lowercase().contains(&query_lower);
                    let hidden_ok = !entry.hidden;
                    let is_link = entry.is_symlink;
                    if matches_name && filters.matches(&entry) && hidden_ok {
                        results.push(entry);
                        if results.len() >= 20_000 {
                            break;
                        }
                    }
                    if recursive && is_dir && !is_link {
                        stack.push(path);
                    }
                    if last_tick.elapsed() > std::time::Duration::from_millis(120) {
                        last_tick = std::time::Instant::now();
                        let count = results.len();
                        let scanned_now = scanned;
                        crate::dispatch::post_simple(move |app| {
                            if let Some(tab) = app.tab_by_id(tab_id) {
                                let current = tab.state.borrow().generation;
                                if current == generation {
                                    tab.set_banner(
                                        &format!("Searching… {} match(es)", count),
                                        &format!("{} items scanned", scanned_now),
                                        false,
                                    );
                                }
                            }
                        });
                    }
                }
                if results.len() >= 20_000 {
                    break;
                }
            }
            let truncated = results.len() >= 20_000;
            let cancelled = cancel_thread.load(Ordering::Relaxed);
            crate::dispatch::post_simple(move |app| {
                app.apply_search(tab_id, generation, results, scanned, errors, truncated, cancelled);
            });
        });
    }

    fn apply_search(
        self: &Rc<Self>,
        tab_id: u64,
        generation: u64,
        entries: Vec<Entry>,
        scanned: usize,
        errors: u32,
        truncated: bool,
        cancelled: bool,
    ) {
        self.apply_entries(tab_id, generation, entries, None);
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        if tab.state.borrow().generation != generation {
            return;
        }
        let count = tab.state.borrow().entries.len();
        let mut detail = format!("{} items scanned", scanned);
        if errors > 0 {
            detail.push_str(&format!(" · {} unreadable", errors));
        }
        if truncated {
            detail.push_str(" · limited to 20,000 results");
        }
        if cancelled {
            detail.push_str(" · cancelled");
        }
        tab.set_banner(&format!("{count} result(s)"), &detail, false);
        tab.state.borrow_mut().scan_cancel = None;
        self.update_status();
    }

    // -----------------------------------------------------------------
    // Views, sorting, selection
    // -----------------------------------------------------------------

    pub fn select_view(self: &Rc<Self>, kind: ViewKind) {
        let Some(tab) = self.active_tab() else { return };
        let tab_id = tab.id;
        {
            let mut state = tab.state.borrow_mut();
            if state.view == kind {
                return;
            }
            state.view = kind;
        }
        tab.views.show(kind);
        let needs_scan = matches!(kind, ViewKind::Recursive | ViewKind::Treemap | ViewKind::Graph);
        if needs_scan {
            let (scan_root, has_scan, running) = {
                let state = tab.state.borrow();
                (
                    state.location.path().cloned(),
                    state.scan.as_ref().map(|s| s.root.clone()),
                    state.scan_running,
                )
            };
            if let Some(root) = scan_root {
                if !running && has_scan.as_ref() != Some(&root) {
                    let depth = tab.state.borrow().recursive_depth.max(6);
                    self.start_scan(tab_id, depth, true);
                }
            }
        }
        self.sync_views(tab_id);
        self.refresh_ui();
        {
            let mut settings = self.settings.borrow_mut();
            settings.default_view = kind;
            settings.save();
        }
    }

    pub fn sync_views(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let view = tab.state.borrow().view;
        tab.views.show(view);
        match view {
            ViewKind::Icon => tab.views.icon.sync(self, tab_id),
            ViewKind::Detail => tab.views.detail.sync(self, tab_id),
            ViewKind::Recursive => tab.views.recursive.sync(self, tab_id),
            ViewKind::Treemap => tab.views.treemap.sync(self, tab_id),
            ViewKind::Graph => tab.views.graph.sync(self, tab_id),
        }
    }

    pub fn toggle_sort(self: &Rc<Self>, tab_id: u64, field: SortField) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let (spec, folders_first) = {
            let mut state = tab.state.borrow_mut();
            if state.sort.field == field {
                state.sort.ascending = !state.sort.ascending;
            } else {
                state.sort.field = field;
                state.sort.ascending = true;
            }
            (state.sort, state.sort.folders_first)
        };
        self.resort(tab_id);
        let _ = (spec, folders_first);
    }

    pub fn set_sort(self: &Rc<Self>, tab_id: u64, field: SortField, ascending: bool, folders_first: bool) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            state.sort = SortSpec { field, ascending, folders_first };
        }
        self.resort(tab_id);
    }

    pub fn resort(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let spec = tab.state.borrow().sort;
        let tag_map: HashMap<PathBuf, String> = {
            let meta = self.meta.borrow();
            let mut map = HashMap::new();
            for item in meta.items.values() {
                if item.tags.is_empty() {
                    continue;
                }
                let names: Vec<String> = item
                    .tags
                    .iter()
                    .filter_map(|id| meta.tag(id).map(|t| t.name.clone()))
                    .collect();
                map.insert(item.path.clone(), names.join(", "));
            }
            map
        };
        {
            let mut state = tab.state.borrow_mut();
            let mut entries = state.entries.clone();
            fs::sort_entries(&mut entries, spec, &|e| {
                tag_map.get(&e.path).cloned().unwrap_or_default()
            });
            state.entries = entries;
        }
        self.sync_views(tab_id);
        self.refresh_ui();
        // Keep the sidebar pill and its direction button in lockstep with the
        // actual SortSpec after any sort change.
        self.refresh_sidebar();
    }

    pub fn toggle_hidden(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        {
            let mut state = tab.state.borrow_mut();
            state.show_hidden = !state.show_hidden;
        }
        let visible = tab.state.borrow().show_hidden;
        self.settings.borrow_mut().show_hidden = visible;
        self.settings.borrow().save();
        self.sync_views(tab.id);
        self.refresh_ui();
    }

    pub fn set_selection(self: &Rc<Self>, tab_id: u64, paths: Vec<PathBuf>, anchor: Option<PathBuf>) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            state.selection = paths.into_iter().collect();
            if anchor.is_some() {
                state.anchor = anchor;
            }
        }
        self.after_selection_change(tab_id);
    }

    pub fn toggle_selection(self: &Rc<Self>, tab_id: u64, path: &std::path::Path) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            if state.selection.contains(path) {
                state.selection.remove(path);
            } else {
                state.selection.insert(path.to_path_buf());
                state.anchor = Some(path.to_path_buf());
            }
        }
        self.after_selection_change(tab_id);
    }

    pub fn extend_selection_to(self: &Rc<Self>, tab_id: u64, path: &std::path::Path) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let visible: Vec<PathBuf> = tab.state.borrow().visible_entries().iter().map(|e| e.path.clone()).collect();
        let anchor = tab.state.borrow().anchor.clone().unwrap_or_else(|| path.to_path_buf());
        let (Some(a), Some(b)) = (
            visible.iter().position(|p| p == &anchor),
            visible.iter().position(|p| p == path),
        ) else {
            self.set_selection(tab_id, vec![path.to_path_buf()], Some(path.to_path_buf()));
            return;
        };
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let paths = visible[lo..=hi].to_vec();
        {
            let mut state = tab.state.borrow_mut();
            state.selection = paths.into_iter().collect();
            state.anchor = Some(anchor);
        }
        self.after_selection_change(tab_id);
    }

    pub fn select_all(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let paths: Vec<PathBuf> = tab.state.borrow().visible_entries().iter().map(|e| e.path.clone()).collect();
        self.set_selection(tab.id, paths, None);
    }

    fn after_selection_change(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let selection = tab.state.borrow().selection.clone();
        let view = tab.state.borrow().view;
        match view {
            ViewKind::Icon => tab.views.icon.apply_selection(&selection),
            ViewKind::Detail => tab.views.detail.apply_selection(&selection),
            _ => {}
        }
        if self
            .active_tab()
            .map(|t| t.id == tab_id)
            .unwrap_or(false)
        {
            self.ui().inspector_widget.update(self, tab_id);
            self.update_toolbar();
            self.update_status();
        }
    }

    // -----------------------------------------------------------------
    // Scans (Recursive, Treemap, Graph)
    // -----------------------------------------------------------------

    pub fn start_scan(self: &Rc<Self>, tab_id: u64, max_depth: u32, force: bool) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let root = tab.state.borrow().location.path().cloned();
        let Some(root) = root else {
            tab.set_banner(
                "Scanning is only available for folders",
                "Switch to a folder location to use this view.",
                false,
            );
            return;
        };
        if !root.is_dir() {
            return;
        }
        let previous_cancel = tab.state.borrow_mut().scan_cancel.take();
        if let Some(cancel) = previous_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut state = tab.state.borrow_mut();
            state.scan_cancel = Some(cancel.clone());
            state.scan_running = true;
            state.scan_progress = format!("Scanning {}", crate::paths::display_path(&root));
            if force {
                state.scan = None;
            }
        }
        self.sync_views(tab_id);
        self.update_status();
        let cancel_thread = cancel.clone();
        let generation = tab.state.borrow().generation;
        std::thread::spawn(move || {
            let cfg = ScanConfig {
                max_depth: max_depth.clamp(1, 64),
                max_entries: 180_000,
                follow_symlinks: false,
            };
            let last = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
            let progress: fs::ScanProgress = Arc::new(move |count, path| {
                let now = std::time::Instant::now();
                let should_update = {
                    let mut l = last.lock().unwrap();
                    if now.duration_since(*l) > std::time::Duration::from_millis(150) {
                        *l = now;
                        true
                    } else {
                        false
                    }
                };
                if should_update {
                    let path = path.to_path_buf();
                    crate::dispatch::post_simple(move |app| {
                        if let Some(tab) = app.tab_by_id(tab_id) {
                            let mut state = tab.state.borrow_mut();
                            if state.generation == generation && state.scan_running {
                                let text = format!(
                                    "Scanning · {} items · {}",
                                    count,
                                    crate::paths::display_path(&path)
                                );
                                state.scan_progress = text.clone();
                                drop(state);
                                tab.views.recursive.update_progress_label(&text);
                            }
                        }
                    });
                }
            });
            let result = fs::scan_tree(&root, cfg, cancel_thread, Some(progress));
            crate::dispatch::post_simple(move |app| {
                app.apply_scan(tab_id, generation, result);
            });
        });
    }

    fn apply_scan(self: &Rc<Self>, tab_id: u64, generation: u64, result: fs::ScanResult) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            if state.generation != generation {
                return;
            }
            if result.cancelled {
                state.scan_running = false;
                state.scan_progress.clear();
                state.scan_cancel = None;
            } else {
                state.scan = Some(Rc::new(result));
                state.scan_running = false;
                state.scan_progress.clear();
                state.scan_cancel = None;
            }
        }
        self.sync_views(tab_id);
        self.update_status();
        self.watch_tab(tab_id);
        if self.active_tab().map(|t| t.id == tab_id).unwrap_or(false) {
            self.refresh_ui();
        }
    }

    pub fn cancel_scan(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let cancel = tab.state.borrow_mut().scan_cancel.clone();
        if let Some(cancel) = cancel {
            cancel.store(true, Ordering::Relaxed);
            tab.set_banner("Scan cancelled", "Showing the last complete result.", false);
        }
    }

    pub fn recursive_depth(self: &Rc<Self>, tab_id: u64) -> u32 {
        self.tab_by_id(tab_id)
            .map(|t| t.state.borrow().recursive_depth)
            .unwrap_or(8)
    }

    pub fn set_recursive_depth(self: &Rc<Self>, tab_id: u64, depth: u32) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        tab.state.borrow_mut().recursive_depth = depth;
        self.start_scan(tab_id, depth, true);
    }

    /// Expand or collapse every folder in the recursive tree at once.
    pub fn set_all_expanded(self: &Rc<Self>, tab_id: u64, expand: bool) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let paths: Vec<PathBuf> = match tab.state.borrow().scan.clone() {
            Some(scan) => scan
                .nodes
                .iter()
                .skip(1)
                .filter(|n| n.entry.is_dir && n.child_count > 0)
                .map(|n| n.entry.path.clone())
                .collect(),
            None => Vec::new(),
        };
        {
            let mut state = tab.state.borrow_mut();
            state.recursive_seeded = true;
            if expand {
                state.collapsed.clear();
            } else {
                state.collapsed = paths.into_iter().collect();
            }
        }
        self.sync_views(tab_id);
    }

    pub fn toggle_collapsed(self: &Rc<Self>, tab_id: u64, path: &std::path::Path) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            if !state.collapsed.remove(path) {
                state.collapsed.insert(path.to_path_buf());
            }
        }
        self.sync_views(tab_id);
    }

    pub fn set_treemap_zoom(self: &Rc<Self>, tab_id: u64, path: Option<PathBuf>) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        tab.state.borrow_mut().treemap_root = path;
        tab.state.borrow_mut().selection.clear();
        self.sync_views(tab_id);
        self.ui().inspector_widget.update(self, tab_id);
    }

    pub fn treemap_zoom_out(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let next = {
            let state = tab.state.borrow();
            let root = state.scan.as_ref().map(|s| s.root.clone());
            state
                .treemap_root
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .filter(|p| root.map(|r| p.starts_with(&r)).unwrap_or(false))
        };
        tab.state.borrow_mut().treemap_root = next;
        self.sync_views(tab_id);
    }

    pub fn set_treemap_options(
        self: &Rc<Self>,
        tab_id: u64,
        metric_allocated: bool,
        filter: Option<crate::model::TreemapFilter>,
        _filter_set: bool,
    ) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            state.treemap_metric_allocated = metric_allocated;
            if let Some(filter) = filter {
                state.treemap_filter = filter;
            }
        }
        self.sync_views(tab_id);
    }

    /// Expand or collapse one folder in the graph view. The layout redraws
    /// with the new level of detail, animating nodes into place.
    pub fn toggle_graph_expanded(self: &Rc<Self>, tab_id: u64, path: &std::path::Path) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        {
            let mut state = tab.state.borrow_mut();
            if !state.graph_expanded.remove(path) {
                state.graph_expanded.insert(path.to_path_buf());
            }
        }
        self.sync_views(tab_id);
    }

    pub fn collapse_graph(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        tab.state.borrow_mut().graph_expanded.clear();
        self.sync_views(tab_id);
    }

    pub fn set_graph_depth(self: &Rc<Self>, tab_id: u64, depth: u32) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        tab.state.borrow_mut().graph_depth = depth.clamp(1, 8);
        self.sync_views(tab_id);
    }

    pub fn set_graph_search(self: &Rc<Self>, tab_id: u64, text: &str) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        tab.state.borrow_mut().graph_search = text.to_string();
        self.sync_views(tab_id);
    }

    // -----------------------------------------------------------------
    // Tags navigation
    // -----------------------------------------------------------------

    pub fn activate_tag(self: &Rc<Self>, tag_id: &str) {
        let Some(tab) = self.active_tab() else { return };
        let Some(tab_id) = Some(tab.id) else { return };
        let (mut ids, match_all) = {
            let state = tab.state.borrow();
            match &state.location {
                Location::Tag { ids, match_all } => (ids.clone(), *match_all),
                _ => (Vec::new(), false),
            }
        };
        if ids.iter().any(|id| id == tag_id) {
            ids.retain(|id| id != tag_id);
        } else {
            ids.push(tag_id.to_string());
        }
        if ids.is_empty() {
            self.navigate(tab_id, Location::Path(crate::paths::home()), true);
        } else {
            self.navigate(tab_id, Location::Tag { ids, match_all }, true);
        }
    }

    pub fn set_tag_match_all(self: &Rc<Self>, match_all: bool) {
        let Some(tab) = self.active_tab() else { return };
        let ids = tab.state.borrow().tag_filter.clone();
        if ids.is_empty() {
            return;
        }
        self.navigate(tab.id, Location::Tag { ids, match_all }, true);
    }

    pub fn refresh_after_metadata_change(self: &Rc<Self>) {
        // Tag and folder colours share one generated stylesheet, so rebuild it
        // whenever metadata changes (new tags, recolours, custom folder HEX).
        {
            let meta = self.meta.borrow();
            let mut colors: Vec<String> = meta
                .items
                .values()
                .filter_map(|item| item.folder_color.clone())
                .collect();
            colors.extend(meta.tags.iter().map(|tag| tag.color.clone()));
            drop(meta);
            let settings = self.settings.borrow().clone();
            colors.extend(
                settings
                    .custom_folder_icons
                    .keys()
                    .filter(|key| *key != "default")
                    .cloned(),
            );
            self.theme.apply_folder_colors(&colors, &settings);
        }
        if let Some(tab) = self.active_tab() {
            self.sync_views(tab.id);
            self.ui().inspector_widget.update(self, tab.id);
        }
        self.refresh_sidebar();
        self.update_status();
    }

    // -----------------------------------------------------------------
    // Live filesystem synchronization
    // -----------------------------------------------------------------

    /// Watch the folders currently on screen. The active tab's folder is
    /// always watched; recursive scans add their directories (bounded) so
    /// changes deep in the tree are noticed too.
    pub fn watch_tab(self: &Rc<Self>, tab_id: u64) {
        if self.active_tab().map(|t| t.id != tab_id).unwrap_or(true) {
            return;
        }
        self.watchers.borrow_mut().clear();
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let (location, scan_dirs) = {
            let state = tab.state.borrow();
            let dirs: Vec<PathBuf> = state
                .scan
                .as_ref()
                .map(|scan| {
                    scan.nodes
                        .iter()
                        .filter(|n| n.entry.is_dir)
                        .take(160)
                        .map(|n| n.entry.path.clone())
                        .collect()
                })
                .unwrap_or_default();
            (state.location.clone(), dirs)
        };
        let Some(current) = location.path().cloned() else { return };
        let mut dirs = vec![current];
        for dir in scan_dirs {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        for dir in dirs {
            let file = gio::File::for_path(&dir);
            let Ok(monitor) = file.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            else {
                continue;
            };
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_, _, _, event| {
                if matches!(
                    event,
                    gio::FileMonitorEvent::Changed
                        | gio::FileMonitorEvent::ChangesDoneHint
                        | gio::FileMonitorEvent::Created
                        | gio::FileMonitorEvent::Deleted
                        | gio::FileMonitorEvent::MovedIn
                        | gio::FileMonitorEvent::MovedOut
                        | gio::FileMonitorEvent::Renamed
                ) {
                    if let Some(app) = weak.upgrade() {
                        app.schedule_fs_refresh(tab_id);
                    }
                }
            });
            self.watchers.borrow_mut().push(monitor);
        }
    }

    fn schedule_fs_refresh(self: &Rc<Self>, tab_id: u64) {
        if self.fs_pending.get() {
            return;
        }
        self.fs_pending.set(true);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            let Some(app) = weak.upgrade() else { return };
            app.fs_pending.set(false);
            app.refresh_from_disk(tab_id);
        });
    }

    /// Reload a tab from disk without touching history or selection.
    pub fn refresh_from_disk(self: &Rc<Self>, tab_id: u64) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let (location, generation, view, running) = {
            let state = tab.state.borrow();
            (state.location.clone(), state.generation, state.view, state.scan_running)
        };
        let needs_scan = matches!(view, ViewKind::Recursive | ViewKind::Treemap | ViewKind::Graph);
        if needs_scan {
            if !running {
                let depth = self.recursive_depth(tab_id).max(8);
                self.start_scan(tab_id, depth, true);
            }
            return;
        }
        match location {
            Location::Path(path) => {
                // Forget cached thumbnails for the folder so edited images
                // and videos render fresh previews.
                self.thumbs
                    .borrow_mut()
                    .retain(|p, _| p.parent() != Some(path.as_path()));
                std::thread::spawn(move || {
                    let listing = fs::read_dir(&path);
                    crate::dispatch::post_simple(move |app| {
                        app.apply_listing(tab_id, generation, listing);
                    });
                });
            }
            Location::Home => self.load_home(tab_id, generation),
            _ => {}
        }
    }
}
