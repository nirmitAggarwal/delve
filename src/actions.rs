//! User actions: clipboard, file operations, menus, drag and drop.

use crate::app::{App, Clipboard, DragState, UndoKind, UndoRecord};
use crate::fs::{self, Entry, SortField};
use crate::model::Location;
use crate::ops::{self, Outcome, Progress, ProgressFn};
use crate::settings::{ConflictPolicy, ViewKind};
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

enum OpMessage {
    Progress(Progress),
    Done(Outcome),
}

impl App {
    // -----------------------------------------------------------------
    // Selection actions
    // -----------------------------------------------------------------

    pub fn selected_paths(self: &Rc<Self>) -> Vec<PathBuf> {
        self.active_tab()
            .map(|t| t.state.borrow().selected_entries().iter().map(|e| e.path.clone()).collect())
            .unwrap_or_default()
    }

    pub fn selected_entries(self: &Rc<Self>) -> Vec<Entry> {
        self.active_tab()
            .map(|t| t.state.borrow().selected_entries())
            .unwrap_or_default()
    }

    pub fn copy_selection_to_clipboard(self: &Rc<Self>, cut: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.notify("Nothing is selected.", false);
            return;
        }
        let count = paths.len();
        let tab_id = self.active_tab().map(|t| t.id).unwrap_or(0);
        *self.clipboard.borrow_mut() = Some(Clipboard { paths, cut, source_tab: tab_id });
        self.update_toolbar();
        self.notify(
            &format!("{} {} item(s)", if cut { "Cut" } else { "Copied" }, count),
            false,
        );
    }

    pub fn paste_into_current(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let Some(dest) = tab.state.borrow().location.path().cloned() else {
            self.notify("Paste needs a folder location.", true);
            return;
        };
        let clipboard = self.clipboard.borrow().clone();
        let Some(clipboard) = clipboard else {
            self.notify("The clipboard is empty.", false);
            return;
        };
        let dest_dir = dest.clone();
        self.begin_transfer(clipboard.paths.clone(), dest_dir, clipboard.cut, tab.id);
        if clipboard.cut {
            *self.clipboard.borrow_mut() = None;
            self.update_toolbar();
        }
    }

    pub fn copy_text(self: &Rc<Self>, text: &str) {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(text);
        }
    }

    pub fn copy_paths(self: &Rc<Self>, as_uri: bool, name_only: bool) {
        let entries = self.selected_entries();
        if entries.is_empty() {
            return;
        }
        let text = entries
            .iter()
            .map(|e| {
                if as_uri {
                    fs::uri_for(&e.path)
                } else if name_only {
                    e.name.clone()
                } else {
                    e.path.display().to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.copy_text(&text);
        self.notify("Copied to clipboard", false);
    }

    pub fn open_entry(self: &Rc<Self>, tab_id: u64, entry: &Entry) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        if matches!(tab.state.borrow().location, Location::Trash) {
            self.notify(
                "This item is in the Trash. Restore it to open it.",
                false,
            );
            return;
        }
        if entry.is_dir {
            self.navigate(tab_id, Location::Path(entry.path.clone()), true);
            return;
        }
        let mime = entry.mime();
        // A saved choice wins on every later open. The first open of each
        // file type asks once and remembers the answer.
        let saved = self.settings.borrow().default_apps.get(&mime).cloned();
        if let Some(id) = saved.filter(|id| !id.is_empty()) {
            if let Some(info) = gio::AppInfo::all()
                .into_iter()
                .find(|a| a.id().as_deref() == Some(id.as_str()))
            {
                self.launch_file_with(entry, &info);
                return;
            }
        }
        if gio::AppInfo::all_for_type(&mime).is_empty() {
            // Nothing is registered for this type; let the desktop decide.
            {
                let mut settings = self.settings.borrow_mut();
                settings.add_recent_file(&entry.path);
                settings.save();
            }
            let uri = fs::uri_for(&entry.path);
            if let Err(err) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
                self.notify(&format!("Could not open “{}”: {err}", entry.name), true);
            }
            return;
        }
        crate::dialogs::open_with_dialog(self, entry);
    }

    /// Launch a file with a specific application and add it to recents.
    pub fn launch_file_with(self: &Rc<Self>, entry: &Entry, info: &gio::AppInfo) {
        {
            let mut settings = self.settings.borrow_mut();
            settings.add_recent_file(&entry.path);
            settings.save();
        }
        let uri = fs::uri_for(&entry.path);
        match info.launch_uris(&[uri.as_str()], None::<&gio::AppLaunchContext>) {
            Ok(()) => {}
            Err(err) => {
                self.notify(
                    &format!(
                        "Could not open “{}” with {}: {err}",
                        entry.name,
                        info.display_name()
                    ),
                    true,
                );
            }
        }
    }

    pub fn open_containing_folder(self: &Rc<Self>) {
        let entries = self.selected_entries();
        let Some(first) = entries.first() else { return };
        let parent = first
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(crate::paths::home);
        if let Some(tab) = self.active_tab() {
            self.navigate(tab.id, Location::Path(parent), true);
        }
    }

    // -----------------------------------------------------------------
    // File operations
    // -----------------------------------------------------------------

    pub fn delete_selection(self: &Rc<Self>, permanent: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let Some(tab) = self.active_tab() else { return };
        if matches!(tab.state.borrow().location, Location::Trash) {
            self.confirm_delete(paths, true);
            return;
        }
        if permanent {
            self.confirm_delete(paths, true);
        } else if paths.len() >= 50 {
            crate::dialogs::confirm(
                self,
                "Move items to Trash?",
                &format!("{} items will be moved to the Trash. You can restore them later.", paths.len()),
                "Move to Trash",
                false,
                move |app| app.perform_trash(&paths),
            );
        } else {
            self.perform_trash(&paths);
        }
    }

    pub fn perform_trash(self: &Rc<Self>, paths: &[PathBuf]) {
        let outcome = ops::trash_paths(paths);
        if !outcome.ok() {
            let message = outcome
                .failures
                .first()
                .map(|f| f.message.clone())
                .unwrap_or_else(|| "Could not move items to Trash".into());
            self.notify(&message, true);
        } else {
            self.push_undo(UndoRecord {
                label: "delete".into(),
                kind: UndoKind::Trash { paths: paths.to_vec() },
            });
            self.notify(&format!("Moved {} item(s) to Trash", paths.len()), false);
        }
        self.reload_tabs_for(paths);
        self.refresh_after_metadata_change();
    }

    pub fn confirm_delete(self: &Rc<Self>, paths: Vec<PathBuf>, permanent: bool) {
        let names = paths
            .iter()
            .take(4)
            .map(|p| fs::file_name_of(p))
            .collect::<Vec<_>>()
            .join(", ");
        let detail = if paths.len() > 4 {
            format!("{names} and {} more will be permanently deleted. This cannot be undone.", paths.len() - 4)
        } else {
            format!("{names} will be permanently deleted. This cannot be undone.")
        };
        if permanent {
            crate::dialogs::confirm(
                self,
                "Delete permanently?",
                &detail,
                "Delete permanently",
                true,
                move |app| app.perform_permanent_delete(&paths),
            );
        }
    }

    /// Generate thumbnails on a worker thread, then refresh visible icon cells.
    pub fn queue_thumbnails(self: &Rc<Self>, jobs: Vec<(PathBuf, Entry)>, size: u32) {
        let mut todo = Vec::new();
        {
            let cache = self.thumbs.borrow();
            let mut pending = self.thumb_pending.borrow_mut();
            for (path, entry) in jobs {
                if cache.contains_key(&path) || !pending.insert(path) {
                    continue;
                }
                todo.push(entry);
            }
        }
        if todo.is_empty() {
            return;
        }
        // Publish each finished thumbnail immediately, in the order the
        // folder listed them, so the first screenful fills in instead of
        // waiting for the whole folder.
        std::thread::spawn(move || {
            for entry in todo {
                let thumb = crate::thumbnails::thumbnail_for(&entry, size);
                let path = entry.path;
                crate::dispatch::post_simple(move |app| {
                    app.thumb_pending.borrow_mut().remove(&path);
                    app.thumbs.borrow_mut().insert(path, thumb);
                    app.schedule_thumb_refresh();
                });
            }
        });
    }

    /// Coalesce thumbnail repaints into one idle callback.
    pub fn schedule_thumb_refresh(self: &Rc<Self>) {
        if self.thumb_refresh_scheduled.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(app) = weak.upgrade() {
                app.refresh_thumbnails();
            }
        });
    }

    /// Re-apply cached thumbnails to icon views (batched inside the view).
    pub fn refresh_thumbnails(self: &Rc<Self>) {
        self.thumb_refresh_scheduled.set(false);
        let tabs = self.tabs.borrow();
        for tab in tabs.iter() {
            tab.views.icon.apply_thumbnails(self);
        }
    }

    fn perform_permanent_delete(self: &Rc<Self>, paths: &[PathBuf]) {
        let paths = paths.to_vec();
        let for_op = paths.clone();
        self.run_operation("Deleting items", move |cancel, progress| {
            ops::delete_permanent(&for_op, cancel, Some(progress))
        });
        self.reload_tabs_for(&paths);
        self.refresh_after_metadata_change();
    }

    pub fn begin_transfer(
        self: &Rc<Self>,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        move_items: bool,
        tab_id: u64,
    ) {
        if sources.is_empty() {
            return;
        }
        if !dest_dir.is_dir() {
            self.notify(
                &format!("{} is not an available folder.", dest_dir.display()),
                true,
            );
            return;
        }
        let conflicts: Vec<PathBuf> = sources
            .iter()
            .filter_map(|s| {
                let name = s.file_name()?;
                let target = dest_dir.join(name);
                if target.exists() {
                    Some(target)
                } else {
                    None
                }
            })
            .collect();
        let policy = self.settings.borrow().conflict;
        if !conflicts.is_empty() && policy == ConflictPolicy::Ask {
            let app = self.clone();
            crate::dialogs::conflict_dialog(self, &conflicts, move |choice| {
                app.execute_transfer(sources.clone(), dest_dir.clone(), move_items, choice, tab_id);
            });
        } else {
            self.execute_transfer(sources, dest_dir, move_items, policy, tab_id);
        }
    }

    fn execute_transfer(
        self: &Rc<Self>,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        move_items: bool,
        policy: ConflictPolicy,
        _tab_id: u64,
    ) {
        let policy = match policy {
            ConflictPolicy::Ask => ConflictPolicy::KeepBoth,
            other => other,
        };
        let op_policy = match policy {
            ConflictPolicy::Replace => ConflictPolicy::Replace,
            ConflictPolicy::Skip => ConflictPolicy::Skip,
            _ => ConflictPolicy::KeepBoth,
        };
        let app = self.clone();
        let title = if move_items { "Moving items" } else { "Copying items" };
        let dest_for_op = dest_dir.clone();
        let sources_for_op = sources.clone();
        let app2 = self.clone();
        let undo_sources = sources.clone();
        let undo_dest = dest_dir.clone();
        self.run_operation_with_done(
            title,
            move |cancel, progress| {
                ops::transfer(
                    &sources_for_op,
                    &dest_for_op,
                    move_items,
                    op_policy,
                    cancel,
                    Some(progress),
                )
            },
            Some(Box::new(move |app, outcome| {
                if outcome.cancelled || outcome.pairs.is_empty() {
                    return;
                }
                let record = if move_items {
                    UndoRecord {
                        label: "move".into(),
                        kind: UndoKind::Move { pairs: outcome.pairs.clone() },
                    }
                } else {
                    UndoRecord {
                        label: "copy".into(),
                        kind: UndoKind::Copy {
                            sources: undo_sources.clone(),
                            created: outcome.pairs.iter().map(|(_, dest)| dest.clone()).collect(),
                            dest_dir: undo_dest.clone(),
                        },
                    }
                };
                app.push_undo(record);
            })),
        );
        // Refresh after a short delay so the operation can make progress; the
        // operation window reports completion independently.
        let app_for_notify = app2.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(450), move || {
            app_for_notify.reload_tabs_for(&sources);
            app_for_notify.reload_tabs_for(&[dest_dir.clone()]);
            app_for_notify.refresh_sidebar();
            let _ = &app;
        });
    }

    pub fn reload_tabs_for(self: &Rc<Self>, paths: &[PathBuf]) {
        let dirs: Vec<PathBuf> = paths
            .iter()
            .filter_map(|p| {
                if p.is_dir() {
                    Some(p.clone())
                } else {
                    p.parent().map(|x| x.to_path_buf())
                }
            })
            .collect();
        let reload_ids: Vec<u64> = self
            .tabs
            .borrow()
            .iter()
            .filter_map(|tab| {
                let state = tab.state.borrow();
                match state.location.path() {
                    Some(loc) if dirs.iter().any(|d| d == loc) => Some(tab.id),
                    _ => None,
                }
            })
            .collect();
        for id in reload_ids {
            self.load(id);
        }
        self.update_status();
    }

    pub fn duplicate_selection(self: &Rc<Self>) {
        let entries = self.selected_entries();
        if entries.is_empty() {
            return;
        }
        let mut by_parent: std::collections::HashMap<PathBuf, Vec<PathBuf>> = Default::default();
        for entry in entries {
            let parent = entry.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            by_parent.entry(parent).or_default().push(entry.path);
        }
        for (parent, paths) in by_parent {
            self.begin_transfer_forced(paths, parent, false, ConflictPolicy::KeepBoth);
        }
    }

    fn begin_transfer_forced(
        self: &Rc<Self>,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        move_items: bool,
        policy: ConflictPolicy,
    ) {
        let tab_id = self.active_tab().map(|t| t.id).unwrap_or(0);
        self.execute_transfer(sources, dest_dir, move_items, policy, tab_id);
    }

    pub fn create_links(self: &Rc<Self>) {
        let entries = self.selected_entries();
        let mut errors = Vec::new();
        let mut created = 0;
        for entry in entries {
            let parent = entry.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let stem = entry
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| entry.name.clone());
            let target = fs::unique_path(&parent, &format!("{} (link)", stem));
            match std::os::unix::fs::symlink(&entry.path, &target) {
                Ok(()) => created += 1,
                Err(err) => errors.push(format!("{}: {err}", entry.name)),
            }
        }
        if errors.is_empty() {
            self.notify(&format!("Created {created} symbolic link(s)"), false);
        } else {
            self.notify(&errors.join(" · "), true);
        }
        self.reload_tabs_for(&self.selected_paths());
    }

    pub fn create_folder_or_file(self: &Rc<Self>, folder: bool) {
        let Some(tab) = self.active_tab() else { return };
        let Some(dir) = tab.state.borrow().location.path().cloned() else {
            self.notify("Open a folder first.", true);
            return;
        };
        let tab_id = tab.id;
        crate::dialogs::name_dialog(
            self,
            if folder { "New folder" } else { "New empty file" },
            if folder { "Folder name" } else { "File name" },
            "",
            if folder { "Create folder" } else { "Create file" },
            move |app, name| {
                let target = dir.join(&name);
                if target.exists() {
                    return Err(format!("“{name}” already exists."));
                }
                let result = if folder {
                    std::fs::create_dir(&target)
                } else {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)
                        .map(|_| ())
                };
                match result {
                    Ok(()) => {
                        let _ = tab_id;
                        app.load(app.active_tab().map(|t| t.id).unwrap_or(tab_id));
                        app.set_selection(
                            app.active_tab().map(|t| t.id).unwrap_or(tab_id),
                            vec![target.clone()],
                            Some(target.clone()),
                        );
                        app.notify(&format!("Created “{name}”"), false);
                        Ok(())
                    }
                    Err(err) => Err(format!("Could not create “{name}”: {err}")),
                }
            },
        );
    }

    pub fn rename_selection(self: &Rc<Self>) {
        let entries = self.selected_entries();
        let Some(entry) = entries.first() else { return };
        if entries.len() > 1 {
            self.notify("Select a single item to rename.", false);
            return;
        }
        let path = entry.path.clone();
        let current = entry.name.clone();
        let Some(tab) = self.active_tab() else { return };
        let tab_id = tab.id;
        let anchor: gtk::Widget = match tab.state.borrow().view {
            ViewKind::Icon => tab.views.icon.flow.clone().upcast(),
            ViewKind::Detail => tab.views.detail.selected_anchor_widget(),
            ViewKind::Recursive => tab.views.recursive.list_widget().clone().upcast(),
            _ => self.window.clone().upcast(),
        };
        crate::dialogs::inline_rename(self, &anchor, &current, move |app, new_name| {
            let mut target = path
                .parent()
                .map(|p| p.join(&new_name))
                .unwrap_or_else(|| PathBuf::from(&new_name));
            if target == path {
                return Ok(());
            }
            if target.exists() {
                // Case-only renames are allowed on case-sensitive filesystems.
                let same = std::fs::canonicalize(&target)
                    .ok()
                    .zip(std::fs::canonicalize(&path).ok())
                    .map(|(a, b)| a == b)
                    .unwrap_or(false);
                if !same {
                    return Err(format!("“{new_name}” already exists."));
                }
                target = path.clone();
            }
            match std::fs::rename(&path, &target) {
                Ok(()) => {
                    app.meta.borrow_mut().remap_path(&path, &target);
                    app.load(tab_id);
                    app.set_selection(tab_id, vec![target.clone()], Some(target));
                    app.refresh_sidebar();
                    Ok(())
                }
                Err(err) => Err(format!("Could not rename: {err}")),
            }
        });
    }

    // -----------------------------------------------------------------
    // Undo and redo
    // -----------------------------------------------------------------

    pub fn push_undo(self: &Rc<Self>, record: UndoRecord) {
        {
            let mut stack = self.undo_stack.borrow_mut();
            stack.push(record);
            if stack.len() > 50 {
                stack.remove(0);
            }
        }
        self.redo_stack.borrow_mut().clear();
        self.update_toolbar();
    }

    pub fn undo(self: &Rc<Self>) {
        self.step_history(true);
    }

    pub fn redo(self: &Rc<Self>) {
        self.step_history(false);
    }

    fn step_history(self: &Rc<Self>, undo: bool) {
        let record = if undo {
            self.undo_stack.borrow_mut().pop()
        } else {
            self.redo_stack.borrow_mut().pop()
        };
        let Some(record) = record else { return };
        let label = record.label.clone();
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut error: Option<String> = None;
        match &record.kind {
            UndoKind::Move { pairs } => {
                for (original, current) in pairs {
                    let (src, dest) = if undo {
                        (current, original)
                    } else {
                        (original, current)
                    };
                    if !src.exists() {
                        continue;
                    }
                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                        dirs.push(parent.to_path_buf());
                    }
                    if let Some(parent) = src.parent() {
                        dirs.push(parent.to_path_buf());
                    }
                    if let Err(e) = std::fs::rename(src, dest) {
                        error = Some(format!(
                            "Could not move “{}”: {e}",
                            crate::fs::file_name_of(src)
                        ));
                    }
                }
            }
            UndoKind::Copy { sources, created, dest_dir } => {
                if undo {
                    for path in created {
                        if let Some(parent) = path.parent() {
                            dirs.push(parent.to_path_buf());
                        }
                        let removed = if path.is_dir() {
                            std::fs::remove_dir_all(path)
                        } else {
                            std::fs::remove_file(path)
                        };
                        if let Err(e) = removed {
                            if path.exists() {
                                error = Some(format!("Could not remove a copied item: {e}"));
                            }
                        }
                    }
                } else {
                    let outcome = ops::transfer(
                        sources,
                        dest_dir,
                        false,
                        ConflictPolicy::KeepBoth,
                        Arc::new(AtomicBool::new(false)),
                        None,
                    );
                    dirs.push(dest_dir.clone());
                    if let Some(failure) = outcome.failures.first() {
                        error = Some(failure.message.clone());
                    }
                }
            }
            UndoKind::Trash { paths } => {
                for path in paths {
                    if let Some(parent) = path.parent() {
                        dirs.push(parent.to_path_buf());
                    }
                }
                let outcome = if undo {
                    ops::restore_paths_from_trash(paths)
                } else {
                    ops::trash_paths(paths)
                };
                if !outcome.ok() {
                    error = Some(
                        outcome
                            .failures
                            .first()
                            .map(|f| f.message.clone())
                            .unwrap_or_else(|| {
                                if undo {
                                    "Could not restore from Trash".into()
                                } else {
                                    "Could not move to Trash".into()
                                }
                            }),
                    );
                }
            }
        }
        if undo {
            self.redo_stack.borrow_mut().push(record);
        } else {
            self.undo_stack.borrow_mut().push(record);
        }
        self.update_toolbar();
        if !dirs.is_empty() {
            dirs.sort();
            dirs.dedup();
            self.reload_tabs_for(&dirs);
        }
        self.refresh_sidebar();
        self.update_status();
        if let Some(err) = error {
            self.notify(&err, true);
        } else {
            self.notify(
                &format!("{} {label}", if undo { "Undid" } else { "Redid" }),
                false,
            );
        }
    }

    // -----------------------------------------------------------------
    // Operations window
    // -----------------------------------------------------------------

    pub fn run_operation<F>(self: &Rc<Self>, title: &str, work: F)
    where
        F: FnOnce(Arc<AtomicBool>, ProgressFn) -> Outcome + Send + 'static,
    {
        self.run_operation_with(title, work)
    }

    pub fn run_operation_with<F>(self: &Rc<Self>, title: &str, work: F)
    where
        F: FnOnce(Arc<AtomicBool>, ProgressFn) -> Outcome + Send + 'static,
    {
        self.run_operation_with_done(title, work, None)
    }

    /// Like `run_operation_with`, with a callback that receives the finished
    /// outcome on the main thread (used to record undo history).
    pub fn run_operation_with_done<F>(
        self: &Rc<Self>,
        title: &str,
        work: F,
        on_done: Option<Box<dyn Fn(&Rc<App>, &Outcome)>>,
    ) where
        F: FnOnce(Arc<AtomicBool>, ProgressFn) -> Outcome + Send + 'static,
    {
        let window = gtk::Window::builder()
            .title(title)
            .transient_for(&self.window)
            .modal(false)
            .default_width(470)
            .build();
        window.add_css_class("delve-dialog");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_top(14);
        root.set_margin_bottom(12);
        root.set_margin_start(14);
        root.set_margin_end(14);
        let label = gtk::Label::new(Some(title));
        label.set_xalign(0.0);
        label.add_css_class("title-strong");
        root.append(&label);
        let detail = gtk::Label::new(Some("Starting…"));
        detail.set_xalign(0.0);
        detail.add_css_class("hint");
        detail.set_wrap(true);
        root.append(&detail);
        let bar = gtk::ProgressBar::new();
        bar.set_show_text(false);
        root.append(&bar);
        let failures = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let failures_scroller = gtk::ScrolledWindow::builder()
            .child(&failures)
            .min_content_height(60)
            .max_content_height(200)
            .visible(false)
            .build();
        root.append(&failures_scroller);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_button = gtk::Button::with_label("Cancel");
        buttons.append(&cancel_button);
        let close_button = gtk::Button::with_label("Close");
        close_button.add_css_class("suggested");
        close_button.set_visible(false);
        buttons.append(&close_button);
        root.append(&buttons);
        window.set_child(Some(&root));
        window.present();
        {
            let window = window.clone();
            close_button.connect_clicked(move |_| window.close());
        }
        {
            let cancel = cancel.clone();
            let detail = detail.clone();
            let button = cancel_button.clone();
            let cancel_button_ref = cancel_button.clone();
            button.connect_clicked(move |_| {
                cancel.store(true, Ordering::Relaxed);
                cancel_button_ref.set_sensitive(false);
                detail.set_text("Cancelling…");
            });
        }

        let window_for_done = window.clone();
        let (tx, rx) = std::sync::mpsc::channel::<OpMessage>();
        let tx_progress = tx.clone();
        let cancel_thread = cancel.clone();
        let progress: ProgressFn = Arc::new(move |p| {
            let _ = tx_progress.send(OpMessage::Progress(p));
        });
        let weak = Rc::downgrade(self);
        std::thread::spawn(move || {
            let outcome = work(cancel_thread, progress);
            let _ = tx.send(OpMessage::Done(outcome));
        });
        self.op_count.set(self.op_count.get() + 1);
        {
            let ui = self.ui();
            ui.status_progress.set_visible(true);
            ui.status_progress.set_fraction(0.0);
        }
        let weak2 = Rc::downgrade(self);
        let bar_clone = bar.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let mut finished: Option<Outcome> = None;
            for msg in rx.try_iter() {
                match msg {
                    OpMessage::Progress(p) => {
                        bar_clone.set_fraction(p.fraction.clamp(0.0, 1.0));
                        detail.set_text(&format!(
                            "{} · {}{}",
                            p.message,
                            if p.total > 0 {
                                format!("{} / {}", p.done.min(p.total), p.total)
                            } else {
                                "working…".to_string()
                            },
                            if p.message == "Done" { "" } else { "" }
                        ));
                        if let Some(app) = weak2.upgrade() {
                            app.ui().status_progress.set_fraction(p.fraction);
                        }
                    }
                    OpMessage::Done(outcome) => finished = Some(outcome),
                }
            }
            if let Some(outcome) = finished {
                if let Some(app) = weak2.upgrade() {
                    let ops_now = app.op_count.get().saturating_sub(1);
                    app.op_count.set(ops_now);
                    if ops_now == 0 {
                        app.ui().status_progress.set_visible(false);
                        app.ui().status_progress.set_fraction(0.0);
                    }
                    app.update_status();
                }
                if let Some(callback) = on_done.as_ref() {
                    if let Some(app) = weak2.upgrade() {
                        callback(&app, &outcome);
                    }
                }
                // Successful operations have nothing left to read; closing the
                // window returns focus so Undo/Redo shortcuts reach the file view.
                if outcome.failures.is_empty() && !outcome.cancelled {
                    let w = window_for_done.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(1400), move || {
                        w.close();
                    });
                }
                cancel_button.set_visible(false);
                close_button.set_visible(true);
                bar.set_fraction(1.0);
                if outcome.cancelled {
                    detail.set_text("Cancelled. Completed work was kept.");
                } else if outcome.failures.is_empty() {
                    detail.set_text(&outcome.message);
                } else {
                    detail.set_text(&format!(
                        "{} · {} item(s) failed",
                        outcome.message,
                        outcome.failures.len()
                    ));
                    failures_scroller.set_visible(true);
                    for failure in outcome.failures.iter().take(200) {
                        let row = gtk::Label::new(Some(&format!(
                            "{} — {}",
                            failure.path.display(),
                            failure.message
                        )));
                        row.set_xalign(0.0);
                        row.set_wrap(true);
                        row.add_css_class("tiny");
                        row.add_css_class("danger");
                        failures.append(&row);
                    }
                    let copy = gtk::Button::with_label("Copy error details");
                    copy.add_css_class("flat");
                    let text = outcome
                        .failures
                        .iter()
                        .map(|f| format!("{}: {}", f.path.display(), f.message))
                        .collect::<Vec<_>>()
                        .join("\n");
                    copy.connect_clicked(move |_| {
                        if let Some(display) = gdk::Display::default() {
                            display.clipboard().set_text(&text);
                        }
                    });
                    failures.append(&copy);
                }
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        let _ = weak;
    }

    // -----------------------------------------------------------------
    // Drag and drop
    // -----------------------------------------------------------------

    pub fn begin_drag(self: &Rc<Self>, tab_id: u64, paths: Vec<PathBuf>) -> gdk::ContentProvider {
        *self.drag.borrow_mut() = Some(DragState { paths: paths.clone(), source_tab: tab_id });
        let files: Vec<gio::File> = paths.iter().map(gio::File::for_path).collect();
        let list = gdk::FileList::from_array(&files);
        gdk::ContentProvider::for_value(&list.to_value())
    }

    pub fn end_drag(self: &Rc<Self>) {
        self.drag.borrow_mut().take();
    }

    pub fn drop_operation(self: &Rc<Self>, internal: bool) -> (bool, ConflictPolicy) {
        let modifiers = self.last_modifiers.get();
        if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
            (false, ConflictPolicy::Ask)
        } else if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
            (true, ConflictPolicy::Ask)
        } else if internal {
            // Dragging inside Delve moves by default; holds on other tabs and
            // sidebar places included. Ctrl copies, Shift forces a move.
            (true, ConflictPolicy::Ask)
        } else {
            (false, ConflictPolicy::Ask)
        }
    }

    pub fn handle_external_drop(
        self: &Rc<Self>,
        tab_id: u64,
        dest: &std::path::Path,
        paths: Vec<PathBuf>,
    ) {
        let internal = self.drag.borrow().is_some();
        let (move_items, _) = self.drop_operation(internal);
        self.begin_transfer(paths, dest.to_path_buf(), move_items, tab_id);
    }

    pub fn drop_paths_to(self: &Rc<Self>, dest: PathBuf, paths: Vec<PathBuf>) {
        let internal = self.drag.borrow().is_some();
        let (move_items, _) = self.drop_operation(internal);
        let tab_id = self.active_tab().map(|t| t.id).unwrap_or(0);
        self.begin_transfer(paths, dest, move_items, tab_id);
    }

    pub fn handle_drop_on_tab(self: &Rc<Self>, tab_id: u64, paths: Vec<PathBuf>) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        // Dropping onto a tab moves the items into the folder that tab shows.
        let dest = tab.state.borrow().location.path().cloned();
        if let Some(dest) = dest {
            let internal = self.drag.borrow().is_some();
            let (move_items, _) = self.drop_operation(internal);
            self.begin_transfer(paths, dest, move_items, tab_id);
        } else {
            self.notify("That tab is not showing a folder.", false);
        }
    }

    // -----------------------------------------------------------------
    // Tags and folder colours
    // -----------------------------------------------------------------

    pub fn assign_tag_to_paths(self: &Rc<Self>, tag_id: &str, paths: &[PathBuf]) {
        self.meta.borrow_mut().add_tag_bulk(paths, tag_id);
        self.refresh_after_metadata_change();
    }

    pub fn add_tag_to_path(self: &Rc<Self>, tag_id: &str, path: &std::path::Path) {
        self.meta.borrow_mut().add_tag(path, tag_id);
        self.refresh_after_metadata_change();
    }

    pub fn remove_tag_from_path(self: &Rc<Self>, tag_id: &str, path: &std::path::Path) {
        self.meta.borrow_mut().remove_tag(path, tag_id);
        self.refresh_after_metadata_change();
    }

    pub fn toggle_tag_for_selection(self: &Rc<Self>, tag_id: &str) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let all_have = {
            let meta = self.meta.borrow();
            paths.iter().all(|p| meta.tags_for(p).iter().any(|t| t == tag_id))
        };
        if all_have {
            self.meta.borrow_mut().remove_tags_bulk(&paths, tag_id);
        } else {
            self.meta.borrow_mut().add_tag_bulk(&paths, tag_id);
        }
        self.refresh_after_metadata_change();
    }

    pub fn set_folder_color(self: &Rc<Self>, path: &std::path::Path, color: Option<String>) {
        self.meta
            .borrow_mut()
            .set_folder_color(path, color.clone());
        let colors = self.meta.borrow().items.values().filter_map(|i| i.folder_color.clone()).collect::<Vec<_>>();
        let settings = self.settings.borrow().clone();
        self.theme.apply_folder_colors(&colors, &settings);
        self.refresh_after_metadata_change();
    }

    pub fn show_folder_color_menu(self: &Rc<Self>, anchor: &impl IsA<gtk::Widget>, paths: Vec<PathBuf>) {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.set_parent(anchor);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let title = gtk::Label::new(Some("Folder colour"));
        title.add_css_class("section-title");
        title.set_margin_start(8);
        title.set_xalign(0.0);
        list.append(&title);
        for (name, color) in crate::theme::FOLDER_PALETTE {
            let row = gtk::Button::new();
            row.add_css_class("flat");
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("color-dot");
            dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(color)));
            dot.set_size_request(12, 12);
            b.append(&dot);
            let label = gtk::Label::new(Some(name));
            label.set_xalign(0.0);
            b.append(&label);
            row.set_child(Some(&b));
            let weak = Rc::downgrade(self);
            let paths = paths.clone();
            let popover_ref = popover.clone();
            let color_value = color.to_string();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    for path in &paths {
                        app.set_folder_color(path, Some(color_value.clone()));
                    }
                    popover_ref.popdown();
                }
            });
            list.append(&row);
        }
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        {
            let row = gtk::Button::with_label("Custom colour…");
            row.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let paths = paths.clone();
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                popover_ref.popdown();
                let Some(app) = weak.upgrade() else { return };
                let dialog = gtk::ColorDialog::new();
                dialog.set_with_alpha(false);
                dialog.set_title("Choose a folder colour");
                let weak = Rc::downgrade(&app);
                let paths = paths.clone();
                dialog.choose_rgba(
                    Some(&app.window),
                    None,
                    None::<&gio::Cancellable>,
                    move |result| {
                        if let Ok(color) = result {
                            if let Some(app) = weak.upgrade() {
                                let hex = crate::util::color_to_hex(
                                    (color.red() * 255.0) as u8,
                                    (color.green() * 255.0) as u8,
                                    (color.blue() * 255.0) as u8,
                                );
                                for path in &paths {
                                    app.set_folder_color(path, Some(hex.clone()));
                                }
                            }
                        }
                    },
                );
            });
            list.append(&row);
        }
        {
            let row = gtk::Button::with_label("Clear colour");
            row.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let paths = paths.clone();
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    for path in &paths {
                        app.set_folder_color(path, None);
                    }
                    popover_ref.popdown();
                }
            });
            list.append(&row);
        }
        popover.set_child(Some(&list));
        popover.popup();
    }

    // -----------------------------------------------------------------
    // Trash
    // -----------------------------------------------------------------

    pub fn restore_trash_path(self: &Rc<Self>, trash_id: &std::path::Path) {
        let items = match trash::os_limited::list() {
            Ok(items) => items,
            Err(err) => {
                self.notify(&format!("Could not read Trash: {err}"), true);
                return;
            }
        };
        let target: Vec<_> = items
            .into_iter()
            .filter(|i| PathBuf::from(&i.id) == trash_id)
            .collect();
        if target.is_empty() {
            self.notify("That Trash item no longer exists.", true);
            return;
        }
        match trash::os_limited::restore_all(target) {
            Ok(()) => {
                self.notify("Restored from Trash", false);
                if let Some(tab) = self.active_tab() {
                    self.load(tab.id);
                }
            }
            Err(err) => self.notify(&format!("Could not restore: {err}"), true),
        }
    }

    pub fn confirm_empty_trash(self: &Rc<Self>) {
        crate::dialogs::confirm(
            self,
            "Empty the Trash?",
            "Everything in the Trash will be permanently deleted. This cannot be undone.",
            "Empty Trash",
            true,
            |app| {
                let outcome = ops::empty_trash();
                if outcome.ok() {
                    app.notify("Trash emptied", false);
                } else {
                    let msg = outcome
                        .failures
                        .first()
                        .map(|f| f.message.clone())
                        .unwrap_or_else(|| "Could not empty Trash".into());
                    app.notify(&msg, true);
                }
                if let Some(tab) = app.active_tab() {
                    app.load(tab.id);
                }
            },
        );
    }

    // -----------------------------------------------------------------
    // Actions and shortcuts
    // -----------------------------------------------------------------

    pub fn install_actions(self: &Rc<Self>) {
        let actions: Vec<(&str, Box<dyn Fn(&Rc<App>)>)> = vec![
            ("undo", Box::new(|a| a.undo())),
            ("redo", Box::new(|a| a.redo())),
            ("copy", Box::new(|a| a.copy_selection_to_clipboard(false))),
            ("cut", Box::new(|a| a.copy_selection_to_clipboard(true))),
            ("paste", Box::new(|a| a.paste_into_current())),
            ("trash", Box::new(|a| a.delete_selection(false))),
            ("delete", Box::new(|a| a.delete_selection(true))),
            ("rename", Box::new(|a| a.rename_selection())),
            ("select-all", Box::new(|a| a.select_all())),
            ("new-tab", Box::new(|a| {
                let loc = a.active_location();
                a.new_tab(loc);
            })),
            ("close-tab", Box::new(|a| {
                let idx = a.active.get();
                a.close_tab_index(idx);
            })),
            ("reopen-tab", Box::new(|a| {
                a.reopen_closed_tab();
            })),
            ("location", Box::new(|a| a.toggle_location_entry(true))),
            ("search", Box::new(|a| {
                a.ui().search_btn.set_active(true);
                let search = a.ui().search.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || {
                    search.grab_focus();
                });
            })),
            ("back", Box::new(|a| a.go_back())),
            ("forward", Box::new(|a| a.go_forward())),
            ("up", Box::new(|a| a.go_up())),
            ("hidden", Box::new(|a| a.toggle_hidden())),
            ("new-folder", Box::new(|a| a.create_folder_or_file(true))),
            ("refresh", Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    a.load(tab.id);
                }
            })),
            ("view-icon", Box::new(|a| a.select_view(ViewKind::Icon))),
            ("view-detail", Box::new(|a| a.select_view(ViewKind::Detail))),
            ("view-recursive", Box::new(|a| a.select_view(ViewKind::Recursive))),
            ("view-treemap", Box::new(|a| a.select_view(ViewKind::Treemap))),
            ("view-graph", Box::new(|a| a.select_view(ViewKind::Graph))),
            ("next-tab", Box::new(|a| a.next_tab(false))),
            ("prev-tab", Box::new(|a| a.next_tab(true))),
            ("settings", Box::new(|a| a.open_settings(None))),
            ("properties", Box::new(|a| {
                let entries = a.selected_entries();
                if !entries.is_empty() {
                    crate::dialogs::properties_dialog(a, entries);
                }
            })),
            ("terminal", Box::new(|a| a.open_terminal())),
            ("organise", Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    crate::dialogs::organise_dialog(a, tab.id);
                }
            })),
            ("compress", Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    crate::dialogs::compress_dialog(a, tab.id);
                }
            })),
            ("sidebar", Box::new(|a| {
                // The sidebar cannot be fully hidden; F9 flips the compact
                // icon rail instead.
                let mini = a.settings.borrow().sidebar_mini;
                a.set_sidebar_mini(!mini);
            })),
            ("inspector", Box::new(|a| {
                let visible = !a.settings.borrow().inspector_visible;
                a.set_inspector_visible(visible);
            })),
        ];
        for (id, callback) in actions {
            let action = gio::SimpleAction::new(id, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    callback(&app);
                }
            });
            self.window.add_action(&action);
        }
        self.apply_shortcuts();
        // Status bar cancel button
        {
            let weak = Rc::downgrade(self);
            self.ui().cancel_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    if let Some(tab) = app.active_tab() {
                        app.cancel_scan(tab.id);
                    }
                }
            });
        }
    }

    pub fn apply_shortcuts(self: &Rc<Self>) {
        let settings = self.settings.borrow().clone();
        let ids: Vec<&str> = crate::shortcuts::SHORTCUTS.iter().map(|(id, _, _)| *id).collect();
        for id in ids {
            let accel = crate::shortcuts::effective(&settings, id);
            let accel_string = accel.to_string();
            let mut accels: Vec<&str> = if accel_string.trim().is_empty() {
                Vec::new()
            } else {
                vec![accel_string.as_str()]
            };
            if id == "redo" && !accels.contains(&"<Control>y") {
                accels.push("<Control>y");
            }
            self.gtk_app
                .set_accels_for_action(&format!("win.{id}"), &accels);
        }
    }

    // -----------------------------------------------------------------
    // Terminal and commands
    // -----------------------------------------------------------------

    pub fn open_terminal(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let dir = tab
            .state
            .borrow()
            .selected_entries()
            .first()
            .filter(|e| e.is_dir)
            .map(|e| e.path.clone())
            .or_else(|| tab.state.borrow().location.path().cloned());
        let Some(dir) = dir else {
            self.notify("Open a folder first.", true);
            return;
        };
        let settings = self.settings.borrow().clone();
        match crate::terminal::launch_terminal(&settings, &dir) {
            Ok(()) => {}
            Err(err) => self.notify(&err, true),
        }
    }

    pub fn open_settings(self: &Rc<Self>, section: Option<&str>) {
        crate::dialogs::settings_window(self, section);
    }

    pub fn show_create_tag_dialog(self: &Rc<Self>) {
        crate::dialogs::create_tag_dialog(self);
    }

    pub fn show_tag_manager(self: &Rc<Self>) {
        crate::dialogs::tag_manager(self);
    }

    pub fn show_transfer_dialog(self: &Rc<Self>) {
        crate::dialogs::transfer_dialog(self);
    }

    // -----------------------------------------------------------------
    // Popovers
    // -----------------------------------------------------------------

    pub fn fill_sort_popover(self: &Rc<Self>, popover: &gtk::Popover) {
        let Some(tab) = self.active_tab() else { return };
        let tab_id = tab.id;
        let (sort, columns) = {
            let state = tab.state.borrow();
            (state.sort, self.settings.borrow().detail_columns.clone())
        };
        let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
        list.add_css_class("sort-menu-content");
        list.set_width_request(206);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(Some("SORT BY"));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        head.append(&title);
        let direction = gtk::Label::new(Some(if sort.ascending { "Ascending" } else { "Descending" }));
        direction.add_css_class("tiny");
        direction.add_css_class("dim");
        head.append(&direction);
        list.append(&head);
        for field in SortField::all() {
            let row = gtk::Button::new();
            row.add_css_class("flat");
            row.add_css_class("sort-row");
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let check = gtk::Label::new(Some(if field == sort.field { "✓" } else { " " }));
            check.set_width_chars(2);
            b.append(&check);
            let label = gtk::Label::new(Some(field.label()));
            label.set_xalign(0.0);
            b.append(&label);
            if field == sort.field {
                let arrow = gtk::Label::new(Some(if sort.ascending { "▲" } else { "▼" }));
                arrow.set_hexpand(true);
                arrow.set_xalign(1.0);
                arrow.add_css_class("dim");
                b.append(&arrow);
            }
            row.set_child(Some(&b));
            let weak = Rc::downgrade(self);
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.toggle_sort(tab_id, field);
                    popover_ref.popdown();
                }
            });
            list.append(&row);
        }
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let folders = gtk::CheckButton::with_label("Folders first");
        folders.add_css_class("sort-check");
        folders.set_active(sort.folders_first);
        {
            let weak = Rc::downgrade(self);
            folders.connect_toggled(move |c| {
                if let Some(app) = weak.upgrade() {
                    app.set_sort(tab_id, sort.field, sort.ascending, c.is_active());
                }
            });
        }
        list.append(&folders);
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let col_title = gtk::Label::new(Some("DETAIL COLUMNS"));
        col_title.add_css_class("section-title");
        col_title.set_xalign(0.0);
        list.append(&col_title);
        for (key, label) in [
            ("type", "Type"),
            ("size", "Size"),
            ("modified", "Modified"),
            ("created", "Created"),
            ("permissions", "Permissions"),
            ("owner", "Owner"),
            ("tags", "Tags"),
        ] {
            let check = gtk::CheckButton::with_label(label);
            check.add_css_class("sort-check");
            check.set_active(columns.iter().any(|c| c == key));
            let weak = Rc::downgrade(self);
            let key_owned = key.to_string();
            check.connect_toggled(move |c| {
                if let Some(app) = weak.upgrade() {
                    let mut settings = app.settings.borrow_mut();
                    settings.detail_columns.retain(|x| x != &key_owned);
                    if c.is_active() {
                        settings.detail_columns.push(key_owned.clone());
                    }
                    settings.save();
                    drop(settings);
                    if let Some(tab) = app.active_tab() {
                        app.sync_views(tab.id);
                    }
                }
            });
            list.append(&check);
        }
        popover.set_child(Some(&list));
    }

    pub fn fill_more_popover(self: &Rc<Self>, popover: &gtk::Popover) {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let mut item = |label: &str, accel: Option<&str>, action: Box<dyn Fn(&Rc<App>)>| {
            let row = gtk::Button::new();
            row.add_css_class("flat");
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            if let Some(icon_name) = context_icon(label) {
                let image = gtk::Image::from_icon_name(icon_name);
                image.set_pixel_size(16);
                image.add_css_class("context-icon");
                b.append(&image);
            }
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            b.append(&text);
            if let Some(accel) = accel {
                let hint = gtk::Label::new(Some(accel));
                hint.add_css_class("tiny");
                hint.add_css_class("dim");
                b.append(&hint);
            }
            row.set_child(Some(&b));
            let weak = Rc::downgrade(self);
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    popover_ref.popdown();
                    action(&app);
                }
            });
            list.append(&row);
        };
        item("New folder", Some("Ctrl+Shift+N"), Box::new(|a| a.create_folder_or_file(true)));
        item("New empty file", None, Box::new(|a| a.create_folder_or_file(false)));
        item("Rename…", Some("F2"), Box::new(|a| a.rename_selection()));
        item("Duplicate", Some("Ctrl+D"), Box::new(|a| a.duplicate_selection()));
        item("Create symbolic link", None, Box::new(|a| a.create_links()));
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        item("Organise folder…", Some("Ctrl+Shift+O"), Box::new(|a| {
            if let Some(tab) = a.active_tab() {
                crate::dialogs::organise_dialog(a, tab.id);
            }
        }));
        item("Compress selection…", Some("Ctrl+Shift+C"), Box::new(|a| {
            if let Some(tab) = a.active_tab() {
                crate::dialogs::compress_dialog(a, tab.id);
            }
        }));
        item("Properties", Some("Alt+Enter"), Box::new(|a| {
            let entries = a.selected_entries();
            if !entries.is_empty() {
                crate::dialogs::properties_dialog(a, entries);
            }
        }));
        item("Open in terminal", Some("Ctrl+Shift+E"), Box::new(|a| a.open_terminal()));
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        item("New tab", Some("Ctrl+T"), Box::new(|a| {
            let loc = a.active_location();
            a.new_tab(loc);
        }));
        item("Keyboard shortcuts", None, Box::new(|a| a.open_settings(Some("keyboard"))));
        item("Settings", Some("Ctrl+,"), Box::new(|a| a.open_settings(None)));
        item("Reload theme", None, Box::new(|a| a.reload_theme()));
        item("Manage tags", None, Box::new(|a| a.show_tag_manager()));
        item("Send to device…", None, Box::new(|a| a.show_transfer_dialog()));
        popover.set_child(Some(&list));
    }

    pub fn reload_theme(self: &Rc<Self>) {
        let settings = self.settings.borrow().clone();
        match self.theme.reload(&settings) {
            Ok(()) => self.notify("Theme reloaded", false),
            Err(err) => self.notify(&format!("Theme not applied: {err}"), true),
        }
    }

    pub fn fill_search_popover(self: &Rc<Self>, popover: &gtk::Popover) {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 5);
        let title = gtk::Label::new(Some("SEARCH SCOPE"));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        list.append(&title);
        let hint = gtk::Label::new(Some(
            "Typing filters the current folder. Press Enter or use the buttons here to search.",
        ));
        hint.set_wrap(true);
        hint.set_max_width_chars(38);
        hint.add_css_class("hint");
        hint.set_xalign(0.0);
        list.append(&hint);
        {
            let row = gtk::Button::with_label("Search this folder recursively");
            row.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    popover_ref.popdown();
                    app.start_recursive_search_from_filter();
                }
            });
            list.append(&row);
        }
        {
            let row = gtk::Button::with_label("Search only direct children");
            row.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    popover_ref.popdown();
                    let query = app.ui().search.text().to_string();
                    let filters = app.ui().search_filters.borrow().clone();
                    app.start_search(query, false, filters);
                }
            });
            list.append(&row);
        }
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let filter_title = gtk::Label::new(Some("FILTERS"));
        filter_title.add_css_class("section-title");
        filter_title.set_xalign(0.0);
        list.append(&filter_title);
        let current = self.ui().search_filters.borrow().clone();
        let ext = gtk::Entry::new();
        ext.set_placeholder_text(Some("Types, e.g. png,jpg,rs"));
        ext.set_text(&current.extensions);
        list.append(&ext);
        let min_size = gtk::Entry::new();
        min_size.set_placeholder_text(Some("Min size, e.g. 10M"));
        min_size.set_text(&current.min_size.map(size_text).unwrap_or_default());
        list.append(&min_size);
        let max_size = gtk::Entry::new();
        max_size.set_placeholder_text(Some("Max size, e.g. 2G"));
        max_size.set_text(&current.max_size.map(size_text).unwrap_or_default());
        list.append(&max_size);
        let days = gtk::SpinButton::with_range(0.0, 3650.0, 1.0);
        days.set_value(current.modified_days.unwrap_or(0) as f64);
        days.set_tooltip_text(Some("Modified within this many days (0 = any)"));
        list.append(&days);
        {
            let row = gtk::Button::with_label("Apply filters and search");
            row.add_css_class("suggested");
            let weak = Rc::downgrade(self);
            let popover_ref = popover.clone();
            let ext = ext.clone();
            let min_size = min_size.clone();
            let max_size = max_size.clone();
            let days = days.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    let filters = crate::model::SearchFilters {
                        extensions: ext.text().to_string(),
                        min_size: parse_size(&min_size.text()),
                        max_size: parse_size(&max_size.text()),
                        modified_days: {
                            let d = days.value() as u32;
                            if d == 0 { None } else { Some(d) }
                        },
                    };
                    *app.ui().search_filters.borrow_mut() = filters.clone();
                    popover_ref.popdown();
                    app.start_recursive_search_from_filter();
                    let _ = &filters;
                }
            });
            list.append(&row);
        }
        popover.set_child(Some(&list));
    }

    // -----------------------------------------------------------------
    // Context menu
    // -----------------------------------------------------------------

    pub fn show_context_menu(
        self: &Rc<Self>,
        tab_id: u64,
        anchor: &impl IsA<gtk::Widget>,
        path: Option<PathBuf>,
        x: f64,
        y: f64,
    ) {
        let Some(tab) = self.tab_by_id(tab_id) else { return };
        let in_trash = matches!(tab.state.borrow().location, Location::Trash);
        if let Some(path) = &path {
            if !tab.state.borrow().selection.contains(path) {
                self.set_selection(tab_id, vec![path.clone()], Some(path.clone()));
            }
        }
        let selected = tab.state.borrow().selected_entries();
        let paths: Vec<PathBuf> = selected.iter().map(|e| e.path.clone()).collect();
        let current_dir = tab.state.borrow().location.path().cloned();
        let clipboard_has = self.clipboard.borrow().is_some();

        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.add_css_class("context-menu");
        popover.set_parent(anchor);
        if x > 0.0 || y > 0.0 {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        }
        let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
        list.set_width_request(258);
        list.add_css_class("context-list");

        let add = |label: &str,
                   accel: Option<&str>,
                   enabled: bool,
                   action: Box<dyn Fn(&Rc<App>)>| {
            let row = gtk::Button::new();
            row.add_css_class("flat");
            row.set_sensitive(enabled);
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            if let Some(icon_name) = context_icon(label) {
                let image = gtk::Image::from_icon_name(icon_name);
                image.set_pixel_size(16);
                image.add_css_class("context-icon");
                b.append(&image);
            }
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            b.append(&text);
            if let Some(accel) = accel {
                let hint = gtk::Label::new(Some(accel));
                hint.add_css_class("tiny");
                hint.add_css_class("dim");
                b.append(&hint);
            }
            row.set_child(Some(&b));
            let weak = Rc::downgrade(self);
            let popover_ref = popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    popover_ref.popdown();
                    action(&app);
                }
            });
            list.append(&row);
        };
        let separator = || list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // Windows 11 style quick-action strip: the four actions people reach
        // for most often are always at the top of the menu.
        if !selected.is_empty() {
            let quick = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            quick.add_css_class("context-quick");
            quick.set_homogeneous(true);
            let quick_item = |icon: &str, label: &str, action: Box<dyn Fn(&Rc<App>)>| {
                let btn = gtk::Button::new();
                btn.add_css_class("context-quick-button");
                let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
                body.set_halign(gtk::Align::Center);
                let image = gtk::Image::from_icon_name(icon);
                image.set_pixel_size(20);
                body.append(&image);
                let text = gtk::Label::new(Some(label));
                text.add_css_class("context-quick-label");
                body.append(&text);
                btn.set_child(Some(&body));
                let weak = Rc::downgrade(self);
                let popover_ref = popover.clone();
                btn.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        popover_ref.popdown();
                        action(&app);
                    }
                });
                quick.append(&btn);
            };
            quick_item("edit-cut-symbolic", "Cut", Box::new(|a| a.copy_selection_to_clipboard(true)));
            quick_item("edit-copy-symbolic", "Copy", Box::new(|a| a.copy_selection_to_clipboard(false)));
            quick_item("document-edit-symbolic", "Rename", Box::new(|a| a.rename_selection()));
            quick_item("view-refresh-symbolic", "Duplicate", Box::new(|a| a.duplicate_selection()));
            quick_item("user-trash-symbolic", "Delete", Box::new(|a| a.delete_selection(false)));
            list.append(&quick);
            separator();
        }

        if in_trash {
            let names: Vec<PathBuf> = paths.clone();
            add("Restore", None, !paths.is_empty(), Box::new(move |a| {
                for id in &names {
                    a.restore_trash_path(id);
                }
            }));
            let names: Vec<PathBuf> = paths.clone();
            add("Delete permanently…", None, !paths.is_empty(), Box::new(move |a| {
                a.confirm_delete(names.clone(), true);
            }));
            separator();
            add("Empty Trash…", None, true, Box::new(|a| a.confirm_empty_trash()));
            add("Refresh", Some("F5"), true, Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    a.load(tab.id);
                }
            }));
            popover.set_child(Some(&list));
            popover.popup();
            return;
        }

        if !selected.is_empty() {
            let entries = selected.clone();
            add(
                if selected.len() == 1 { "Open" } else { "Open all" },
                None,
                true,
                Box::new(move |a| {
                    for entry in &entries {
                        a.open_entry(a.active_tab().map(|t| t.id).unwrap_or(0), entry);
                    }
                }),
            );
            if selected.len() == 1 && !selected[0].is_dir {
                let entry = selected[0].clone();
                add("Open with…", None, true, Box::new(move |a| {
                    crate::dialogs::open_with_dialog(a, &entry);
                }));
            }
            let first = selected[0].clone();
            add("Open containing folder", None, true, Box::new(move |a| {
                if let Some(parent) = first.path.parent() {
                    a.navigate_active(Location::Path(parent.to_path_buf()));
                }
            }));
            if selected.iter().all(|e| e.is_dir) {
                let dirs: Vec<Entry> = selected.clone();
                add("Open in new tab", None, true, Box::new(move |a| {
                    for entry in &dirs {
                        a.new_tab(Location::Path(entry.path.clone()));
                    }
                }));
            }
            separator();
            add("Duplicate", Some("Ctrl+D"), true, Box::new(|a| a.duplicate_selection()));
            add("Create symbolic link", None, true, Box::new(|a| a.create_links()));
            separator();
            let paths_del = paths.clone();
            add("Delete permanently…", Some("Shift+Delete"), true, Box::new(move |a| {
                a.confirm_delete(paths_del.clone(), true);
            }));
            separator();
            let paths_copy = paths.clone();
            add("Copy path", None, true, Box::new(move |a| {
                a.copy_text(&paths_copy.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"));
                a.notify("Path copied", false);
            }));
            let paths_copy = paths.clone();
            add("Copy filename", None, true, Box::new(move |a| {
                a.copy_text(&paths_copy.iter().map(|p| fs::file_name_of(p)).collect::<Vec<_>>().join("\n"));
                a.notify("Filename copied", false);
            }));
            let paths_copy = paths.clone();
            add("Copy as URI", None, true, Box::new(move |a| {
                a.copy_text(&paths_copy.iter().map(|p| fs::uri_for(p)).collect::<Vec<_>>().join("\n"));
                a.notify("URI copied", false);
            }));

            // Tags live in their own window so the menu stays short.
            separator();
            let tag_paths = paths.clone();
            add("Assign tags…", None, true, Box::new(move |a| {
                crate::dialogs::assign_tags_dialog(a, tag_paths.clone());
            }));

            // Folder colour
            let folder_paths: Vec<PathBuf> = selected
                .iter()
                .filter(|e| e.is_dir)
                .map(|e| e.path.clone())
                .collect();
            if !folder_paths.is_empty() {
                let row = gtk::Button::with_label("Folder colour…");
                row.add_css_class("flat");
                let weak = Rc::downgrade(self);
                let popover_ref = popover.clone();
                row.connect_clicked(move |b| {
                    popover_ref.popdown();
                    if let Some(app) = weak.upgrade() {
                        app.show_folder_color_menu(b, folder_paths.clone());
                    }
                });
                list.append(&row);
            }

            separator();
            let paths_term = paths.clone();
            add("Open in terminal", Some("Ctrl+Shift+E"), true, Box::new(move |a| {
                let dir = paths_term
                    .iter()
                    .find(|p| p.is_dir())
                    .cloned()
                    .unwrap_or_else(|| a.active_location().path().cloned().unwrap_or_else(crate::paths::home));
                let settings = a.settings.borrow().clone();
                match crate::terminal::launch_terminal(&settings, &dir) {
                    Ok(()) => {}
                    Err(err) => a.notify(&err, true),
                }
            }));
            add("Compress…", None, true, Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    crate::dialogs::compress_dialog(a, tab.id);
                }
            }));
            let archives: Vec<PathBuf> = paths
                .iter()
                .filter(|p| crate::ops::ArchiveFormat::from_path(p).is_some())
                .cloned()
                .collect();
            if !archives.is_empty() {
                let archives_here = archives.clone();
                add("Extract here", None, true, Box::new(move |a| {
                    for archive in &archives_here {
                        if let Some(parent) = archive.parent() {
                            crate::dialogs::extract_archive(a, archive, parent);
                        }
                    }
                }));
                let archives_to = archives.clone();
                add("Extract to…", None, true, Box::new(move |a| {
                    if let Some(archive) = archives_to.first() {
                        crate::dialogs::extract_dialog(a, archive);
                    }
                }));
            }
            add("Properties", Some("Alt+Enter"), true, Box::new(|a| {
                let entries = a.selected_entries();
                if !entries.is_empty() {
                    crate::dialogs::properties_dialog(a, entries);
                }
            }));
            match crate::transfers::availability() {
                crate::transfers::Availability::Available { .. } => {
                    add("Send to device…", None, true, Box::new(|a| a.show_transfer_dialog()));
                }
                crate::transfers::Availability::Unavailable { .. } => {}
            }
            // Folder run commands
            if let Some(dir) = current_dir.clone() {
                let commands: Vec<crate::settings::RunCommand> = self
                    .settings
                    .borrow()
                    .commands
                    .iter()
                    .filter(|c| c.enabled)
                    .filter(|c| match &c.folder {
                        Some(f) => f == &dir,
                        None => true,
                    })
                    .cloned()
                    .collect();
                if !commands.is_empty() {
                    separator();
                    let cmd_title = gtk::Label::new(Some("RUN COMMAND"));
                    cmd_title.add_css_class("section-title");
                    cmd_title.set_margin_start(8);
                    cmd_title.set_xalign(0.0);
                    list.append(&cmd_title);
                    for cmd in commands {
                        let entry = cmd.clone();
                        let selected_paths = paths.clone();
                        add(&cmd.name, None, true, Box::new(move |a| {
                            let entry = entry.clone();
                            let selected_paths = selected_paths.clone();
                            crate::dialogs::confirm(
                                a,
                                &format!("Run “{}”?", entry.name),
                                &format!(
                                    "This will execute:\n{} {}",
                                    entry.program,
                                    entry.args.join(" ")
                                ),
                                "Run",
                                false,
                                move |app| {
                                    let folder = app
                                        .active_location()
                                        .path()
                                        .cloned()
                                        .unwrap_or_else(crate::paths::home);
                                    match crate::terminal::launch_run_command(
                                        &entry,
                                        &folder,
                                        &selected_paths,
                                    ) {
                                        Ok(()) => app.notify("Command started", false),
                                        Err(err) => app.notify(&err, true),
                                    }
                                },
                            );
                        }));
                    }
                }
            }
        } else {
            add("New folder", None, true, Box::new(|a| a.create_folder_or_file(true)));
            add("New empty file", None, true, Box::new(|a| a.create_folder_or_file(false)));
            add("Paste", Some("Ctrl+V"), clipboard_has, Box::new(|a| a.paste_into_current()));
            separator();
            add("Select all", Some("Ctrl+A"), true, Box::new(|a| a.select_all()));
            add("Refresh", Some("F5"), true, Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    a.load(tab.id);
                }
            }));
            add("Toggle hidden files", Some("Ctrl+H"), true, Box::new(|a| a.toggle_hidden()));
            separator();
            add("Organise folder…", None, true, Box::new(|a| {
                if let Some(tab) = a.active_tab() {
                    crate::dialogs::organise_dialog(a, tab.id);
                }
            }));
            add("Open in terminal", None, true, Box::new(|a| a.open_terminal()));
            add("Properties", None, true, Box::new(|a| {
                if let Some(dir) = a.active_location().path() {
                    crate::dialogs::properties_dialog(a, vec![Entry::from_path(dir.clone(), fs::file_name_of(dir))]);
                }
            }));
        }
        popover.set_child(Some(&list));
        popover.popup();
    }

    pub fn show_context_menu_at_pointer(self: &Rc<Self>, tab_id: u64, path: &std::path::Path) {
        let anchor: gtk::Widget = self.window.clone().upcast();
        self.show_context_menu(tab_id, &anchor, Some(path.to_path_buf()), 0.0, 0.0);
    }
}

fn context_icon(label: &str) -> Option<&'static str> {
    let base = label.trim_start_matches('✓').trim_start_matches(' ');
    if base.starts_with("Open in") { return Some("utilities-terminal-symbolic"); }
    match base {
        "Open" | "Open all" => Some("document-open-symbolic"),
        "Open with…" => Some("application-x-executable-symbolic"),
        "Open containing folder" => Some("folder-open-symbolic"),
        "Open in new tab" => Some("tab-new-symbolic"),
        "Duplicate" => Some("edit-copy-symbolic"),
        "Create symbolic link" => Some("insert-link-symbolic"),
        "Delete permanently…" => Some("edit-delete-symbolic"),
        "Copy path" => Some("edit-copy-symbolic"),
        "Copy filename" => Some("edit-copy-symbolic"),
        "Copy as URI" => Some("edit-copy-symbolic"),
        "Assign tags…" | "Manage tags…" => Some("tag-symbolic"),
        "Compress…" => Some("package-x-generic-symbolic"),
        "Extract here" | "Extract to…" => Some("package-x-generic-symbolic"),
        "Properties" => Some("document-properties-symbolic"),
        "Send to device…" => Some("send-to-symbolic"),
        "New folder" => Some("folder-new-symbolic"),
        "New empty file" => Some("document-new-symbolic"),
        "Paste" => Some("edit-paste-symbolic"),
        "Select all" => Some("edit-select-all-symbolic"),
        "Refresh" => Some("view-refresh-symbolic"),
        "Toggle hidden files" => Some("view-reveal-symbolic"),
        "Organise folder…" => Some("view-sort-descending-symbolic"),
        "Restore" => Some("edit-undo-symbolic"),
        "Empty Trash…" => Some("user-trash-symbolic"),
        "Manage tags…" => Some("tag-symbolic"),
        _ => None,
    }
}

fn size_text(bytes: u64) -> String {
    crate::fs::human_size(bytes)
}

fn parse_size(text: &gtk::glib::GString) -> Option<u64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    let (num, mult) = if let Some(rest) = t.strip_suffix("gib").or_else(|| t.strip_suffix("gb")) {
        (rest, 1024u64 * 1024 * 1024)
    } else if let Some(rest) = t.strip_suffix("mib").or_else(|| t.strip_suffix("mb")) {
        (rest, 1024u64 * 1024)
    } else if let Some(rest) = t.strip_suffix("kib").or_else(|| t.strip_suffix("kb")) {
        (rest, 1024u64)
    } else if let Some(rest) = t.strip_suffix('g') {
        (rest, 1024u64 * 1024 * 1024)
    } else if let Some(rest) = t.strip_suffix('m') {
        (rest, 1024u64 * 1024)
    } else if let Some(rest) = t.strip_suffix('k') {
        (rest, 1024u64)
    } else {
        (t.as_str(), 1u64)
    };
    num.trim().parse::<f64>().ok().map(|v| (v * mult as f64) as u64)
}

/// Keep the unused import warning-free while documenting intent.
#[allow(dead_code)]
fn _unused(_: RefCell<()>) {}
