//! Quick Spotlight: keyboard-first navigation across locations, tags, files,
//! folders, command profiles and common actions.
//!
//! The launcher is intentionally a thin projection over existing systems: tab
//! entries, sidebar tags, settings.commands and the zoxide-style visit history.
//! It does not maintain a second index or a private tag/command store.

use crate::app::App;
use crate::model::Location;
use crate::settings::RunCommand;
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Debug)]
enum HitAction {
    Navigate(Location),
    OpenPath(PathBuf, bool),
    Tag(String),
    Command(RunCommand, PathBuf),
    Builtin(&'static str),
}

#[derive(Clone, Debug)]
struct Hit {
    title: String,
    subtitle: String,
    icon: String,
    category: &'static str,
    score: f64,
    action: HitAction,
}

pub fn open(app: &Rc<App>) {
    // One Spotlight at a time.
    for widget in gtk::Window::list_toplevels() {
        if let Ok(window) = widget.downcast::<gtk::Window>() {
            if window.widget_name() == "delve-spotlight" {
                window.present();
                return;
            }
        }
    }

    let window = gtk::Window::builder()
        .title("Quick Spotlight")
        .transient_for(&app.window)
        .modal(true)
        .decorated(false)
        .default_width(660)
        .default_height(500)
        .build();
    window.set_widget_name("delve-spotlight");
    window.add_css_class("spotlight-window");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("spotlight");
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some(
        "Search locations, tags, files, folders and run commands…",
    ));
    entry.add_css_class("spotlight-entry");
    entry.set_margin_top(12);
    entry.set_margin_start(14);
    entry.set_margin_end(14);
    entry.set_margin_bottom(8);
    root.append(&entry);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    root.append(&scroller);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    footer.add_css_class("spotlight-footer");
    let hint = gtk::Label::new(Some("↑↓ Navigate · Enter Open · Esc Close"));
    hint.add_css_class("tiny");
    hint.add_css_class("dim");
    footer.append(&hint);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    footer.append(&spacer);
    let provider = gtk::Label::new(Some("Super + Space"));
    provider.add_css_class("tiny");
    provider.add_css_class("dim");
    footer.append(&provider);
    root.append(&footer);
    window.set_child(Some(&root));

    let hits: Rc<RefCell<Vec<Hit>>> = Rc::new(RefCell::new(Vec::new()));
    let buttons: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let rebuild: Rc<dyn Fn()> = {
        let weak = Rc::downgrade(app);
        let list = list.clone();
        let hits = hits.clone();
        let buttons = buttons.clone();
        let selected = selected.clone();
        let query = query.clone();
        let window_weak = window.downgrade();
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            buttons.borrow_mut().clear();
            let q = query.borrow().clone();
            let results = search(&app, &q);
            *hits.borrow_mut() = results.clone();
            if results.is_empty() {
                let empty = gtk::Label::new(Some(if q.trim().is_empty() {
                    "Start typing to search Delve"
                } else {
                    "No matching locations, tags, files or commands"
                }));
                empty.add_css_class("dim");
                empty.set_margin_top(24);
                empty.set_margin_bottom(24);
                list.append(&empty);
                selected.set(0);
                return;
            }
            selected.set(0);
            let mut current_category = "";
            for hit in results.iter() {
                if hit.category != current_category {
                    current_category = hit.category;
                    let header = gtk::Label::new(Some(current_category));
                    header.add_css_class("section-title");
                    header.set_xalign(0.0);
                    header.set_margin_start(18);
                    header.set_margin_top(8);
                    header.set_margin_bottom(3);
                    list.append(&header);
                }
                let row = gtk::Button::new();
                row.add_css_class("flat");
                row.add_css_class("spotlight-row");
                row.set_halign(gtk::Align::Fill);
                let body = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                body.set_margin_start(10);
                body.set_margin_end(10);
                body.set_margin_top(3);
                body.set_margin_bottom(3);
                let icon = gtk::Image::from_icon_name(&hit.icon);
                icon.set_pixel_size(18);
                icon.set_valign(gtk::Align::Center);
                body.append(&icon);
                let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
                texts.set_hexpand(true);
                let title = gtk::Label::new(None);
                title.set_markup(&highlight(&hit.title, &q));
                title.set_xalign(0.0);
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                texts.append(&title);
                if !hit.subtitle.is_empty() {
                    let subtitle = gtk::Label::new(Some(&hit.subtitle));
                    subtitle.add_css_class("tiny");
                    subtitle.add_css_class("dim");
                    subtitle.set_xalign(0.0);
                    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                    texts.append(&subtitle);
                }
                body.append(&texts);
                let badge = gtk::Label::new(Some(hit.category));
                badge.add_css_class("tiny");
                badge.add_css_class("dim");
                body.append(&badge);
                row.set_child(Some(&body));
                {
                    let strong = weak.clone();
                    let hit = hit.clone();
                    let window_weak = window_weak.clone();
                    row.connect_clicked(move |_| {
                        if let Some(app) = strong.upgrade() {
                            if let Some(window) = window_weak.upgrade() {
                                window.close();
                            }
                            activate(&app, &hit);
                        }
                    });
                }
                {
                    let selected = selected.clone();
                    let buttons = buttons.clone();
                    let row_ref = row.clone();
                    let motion = gtk::EventControllerMotion::new();
                    motion.connect_enter(move |_, _, _| {
                        if let Some(index) = buttons.borrow().iter().position(|b| b == &row_ref) {
                            select(&buttons, &selected, index);
                        }
                    });
                    row.add_controller(motion);
                }
                list.append(&row);
                buttons.borrow_mut().push(row);
            }
            select(&buttons, &selected, 0);
        })
    };
    rebuild();

    {
        let rebuild = rebuild.clone();
        let query = query.clone();
        entry.connect_search_changed(move |e| {
            *query.borrow_mut() = e.text().to_string();
            rebuild();
        });
    }

    let activate_current = {
        let weak = Rc::downgrade(app);
        let hits = hits.clone();
        let selected = selected.clone();
        let window_weak = window.downgrade();
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            let hit = hits.borrow().get(selected.get()).cloned();
            if let Some(hit) = hit {
                if let Some(window) = window_weak.upgrade() {
                    window.close();
                }
                activate(&app, &hit);
            }
        })
    };

    {
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        let selected = selected.clone();
        let buttons = buttons.clone();
        let hits = hits.clone();
        let window_ref = window.clone();
        let activate_current = activate_current.clone();
        key.connect_key_pressed(move |_, keyval, _, _| match keyval {
            gdk::Key::Escape => {
                window_ref.close();
                glib::Propagation::Stop
            }
            gdk::Key::Up | gdk::Key::KP_Up => {
                let count = buttons.borrow().len();
                if count > 0 {
                    let next = (selected.get() + count - 1) % count;
                    select(&buttons, &selected, next);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Down | gdk::Key::KP_Down => {
                let count = buttons.borrow().len();
                if count > 0 {
                    let next = (selected.get() + 1) % count;
                    select(&buttons, &selected, next);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                let _ = &hits;
                activate_current();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        window.add_controller(key);
    }

    window.present();
    let entry_focus = entry.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
        entry_focus.grab_focus();
    });
}

fn select(buttons: &Rc<RefCell<Vec<gtk::Button>>>, selected: &Rc<Cell<usize>>, index: usize) {
    let buttons = buttons.borrow();
    for (i, button) in buttons.iter().enumerate() {
        if i == index {
            button.add_css_class("selected");
        } else {
            button.remove_css_class("selected");
        }
    }
    selected.set(index);
}

fn search(app: &App, query: &str) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let settings = app.settings.borrow();
    let meta = app.meta.borrow();
    let active_path = app.active_location().path().cloned();
    let mut locations: Vec<Hit> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut candidates: Vec<PathBuf> = settings
        .visits
        .iter()
        .map(|v| v.path.clone())
        .chain(settings.recent_locations.iter().cloned())
        .collect();
    for path in candidates.drain(..) {
        if !seen.insert(path.clone()) || !path.is_dir() {
            continue;
        }
        let name = crate::fs::file_name_of(&path);
        let matches = q.is_empty()
            || name.to_lowercase().contains(&q)
            || path.to_string_lossy().to_lowercase().contains(&q);
        if !matches {
            continue;
        }
        let score = settings.visit_score(&path, now)
            + match_bonus(&name, &q)
            + if q.is_empty() { 2.0 } else { 0.0 };
        locations.push(Hit {
            title: name,
            subtitle: crate::paths::display_path(&path),
            icon: "folder-open-symbolic".into(),
            category: "Locations",
            score,
            action: HitAction::Navigate(Location::Path(path)),
        });
    }
    // Frequently visited locations sort first; recency is folded into the score.
    locations.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    locations.truncate(if q.is_empty() { 5 } else { 8 });

    let mut tags: Vec<Hit> = Vec::new();
    for tag in &meta.tags {
        if !q.is_empty() && !tag.name.to_lowercase().contains(&q) {
            continue;
        }
        let count = meta.paths_with_tags(&[tag.id.clone()], false).len();
        tags.push(Hit {
            title: tag.name.clone(),
            subtitle: format!("Tag · {count} item{}", if count == 1 { "" } else { "s" }),
            icon: if tag.icon.is_empty() { "tag-symbolic".into() } else { tag.icon.clone() },
            category: "Tags",
            score: match_bonus(&tag.name, &q) + 1.0,
            action: HitAction::Tag(tag.id.clone()),
        });
    }
    tags.truncate(6);

    let mut files: Vec<Hit> = Vec::new();
    if let Some(tab) = app.active_tab() {
        let entries = tab.state.borrow().visible_entries();
        for entry in entries {
            if files.len() >= 12 {
                break;
            }
            if !q.is_empty() && !entry.name.to_lowercase().contains(&q) {
                continue;
            }
            if q.is_empty() && files.len() >= 4 {
                break;
            }
            files.push(Hit {
                title: entry.name.clone(),
                subtitle: if entry.is_dir {
                    "Folder".to_string()
                } else {
                    format!(
                        "{} · {}",
                        entry.type_label(),
                        crate::util::ellipsize(&entry.path.display().to_string(), 70)
                    )
                },
                icon: if entry.is_dir { "folder-symbolic".into() } else { "document-symbolic".into() },
                category: "Files & folders",
                score: match_bonus(&entry.name, &q),
                action: HitAction::OpenPath(entry.path.clone(), entry.is_dir),
            });
        }
    }
    if q.is_empty() {
        for path in settings.recent_files.iter().take(4) {
            if files.len() >= 8 {
                break;
            }
            files.push(Hit {
                title: crate::fs::file_name_of(path),
                subtitle: path.display().to_string(),
                icon: "document-open-recent-symbolic".into(),
                category: "Files & folders",
                score: 1.0,
                action: HitAction::OpenPath(path.clone(), false),
            });
        }
    }
    files.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut commands: Vec<Hit> = Vec::new();
    for command in settings.commands.iter().filter(|c| {
        c.enabled
            && active_path
                .as_deref()
                .map(|folder| c.applies_to(folder))
                .unwrap_or(true)
    }) {
        let env_keys = command.env.keys().cloned().collect::<Vec<_>>().join(" ");
        let shortcut_hint = command
            .shortcut_label()
            .map(|shortcut| format!(" · {shortcut}"))
            .unwrap_or_default();
        let haystack = format!(
            "{} {} {} {} {} {} {}",
            command.name,
            command.description,
            command.effective_steps().first().map(|s| s.program.clone()).unwrap_or_default(),
            command.folder.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            command.scope.label(),
            env_keys,
            command.shortcut_label().unwrap_or_default()
        )
        .to_lowercase();
        if !q.is_empty() && !haystack.contains(&q) {
            continue;
        }
        let folder = command
            .folder
            .clone()
            .or_else(|| active_path.clone())
            .unwrap_or_else(crate::paths::home);
        commands.push(Hit {
            title: command.name.clone(),
            subtitle: format!(
                "{} · {} · {} · {}{}",
                command.effective_steps().len(),
                command.mode.label(),
                command.scope.label(),
                folder.display(),
                shortcut_hint
            ),
            icon: "system-run-symbolic".into(),
            category: "Run commands",
            score: match_bonus(&command.name, &q) + 2.0,
            action: HitAction::Command(command.clone(), folder),
        });
    }
    commands.truncate(6);

    let actions: &[(&str, &str, &str)] = &[
        ("Home", "Go to the Home landing page", "user-home-symbolic"),
        ("New tab", "Open a new tab", "tab-new-symbolic"),
        ("Recent files", "Show recently opened files", "document-open-recent-symbolic"),
        ("Recent locations", "Show recently visited folders", "document-open-recent-symbolic"),
        ("Trash", "Open the Trash", "user-trash-symbolic"),
        ("Toggle hidden files", "Show or hide dotfiles", "view-reveal-symbolic"),
        ("Open terminal here", "Launch the configured terminal", "utilities-terminal-symbolic"),
        ("Run command sets", "Choose one or more sets to run together", "system-run-symbolic"),
        ("Organise folder", "Batch rename and organise", "view-sort-descending-symbolic"),
        ("Settings", "Open Delve settings", "emblem-system-symbolic"),
    ];
    let mut action_hits: Vec<Hit> = Vec::new();
    for (title, subtitle, icon) in actions {
        if !q.is_empty() && !title.to_lowercase().contains(&q) {
            continue;
        }
        action_hits.push(Hit {
            title: (*title).into(),
            subtitle: (*subtitle).into(),
            icon: (*icon).into(),
            category: "Actions",
            score: match_bonus(title, &q),
            action: HitAction::Builtin(title),
        });
    }
    action_hits.truncate(if q.is_empty() { 4 } else { 6 });

    let mut out = Vec::new();
    out.extend(locations);
    out.extend(tags);
    out.extend(files);
    out.extend(commands);
    out.extend(action_hits);
    out
}

fn match_bonus(text: &str, query: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let text = text.to_lowercase();
    if text == query {
        30.0
    } else if text.starts_with(query) {
        20.0
    } else if text.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(query)) {
        12.0
    } else {
        4.0
    }
}

fn highlight(text: &str, query: &str) -> String {
    let escaped = glib::markup_escape_text(text);
    let q = query.trim();
    if q.is_empty() {
        return escaped.to_string();
    }
    let lower = text.to_lowercase();
    let needle = q.to_lowercase();
    if let Some(start) = lower.find(&needle) {
        let end = start + needle.len();
        // Byte offsets from the lowercase string are valid for ASCII queries;
        // for non-ASCII fall back to an unhighlighted label rather than slicing
        // in the middle of a codepoint.
        if text.is_char_boundary(start) && text.is_char_boundary(end) {
            return format!(
                "{}<b>{}</b>{}",
                glib::markup_escape_text(&text[..start]),
                glib::markup_escape_text(&text[start..end]),
                glib::markup_escape_text(&text[end..])
            );
        }
    }
    escaped.to_string()
}

fn activate(app: &Rc<App>, hit: &Hit) {
    match &hit.action {
        HitAction::Navigate(location) => app.navigate_active(location.clone()),
        HitAction::OpenPath(path, is_dir) => {
            if *is_dir {
                app.navigate_active(Location::Path(path.clone()));
            } else if let Some(tab) = app.active_tab() {
                app.open_entry(
                    tab.id,
                    &crate::fs::Entry::from_path(path.clone(), crate::fs::file_name_of(path)),
                );
            }
        }
        HitAction::Tag(id) => {
            app.navigate_active(Location::Tag { ids: vec![id.clone()], match_all: false })
        }
        HitAction::Command(profile, folder) => {
            let selected = app
                .active_tab()
                .map(|t| t.state.borrow().selection.iter().cloned().collect())
                .unwrap_or_default();
            crate::runner::confirm_and_start(app, profile.clone(), folder.clone(), selected);
        }
        HitAction::Builtin(name) => match *name {
            "Home" => app.navigate_active(Location::Home),
            "New tab" => {
                let location = app.active_location();
                app.new_tab(location);
            }
            "Recent files" => app.navigate_active(Location::RecentFiles),
            "Recent locations" => app.navigate_active(Location::RecentLocations),
            "Trash" => app.navigate_active(Location::Trash),
            "Toggle hidden files" => app.toggle_hidden(),
            "Open terminal here" => app.open_terminal(),
            "Run command sets" => {
                if let Some(folder) = app.active_location().path().cloned() {
                    crate::dialogs::run_commands_picker(app, folder, app.selected_paths());
                } else {
                    app.notify("Open a folder before running command sets.", true);
                }
            }
            "Organise folder" => {
                if let Some(tab) = app.active_tab() {
                    crate::dialogs::organise_dialog(app, tab.id);
                }
            }
            "Settings" => app.open_settings(None),
            _ => {}
        },
    }
}
