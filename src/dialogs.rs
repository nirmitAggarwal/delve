//! Dialogs: settings, properties, organise, archives, transfers, tags and
//! confirmations. Every control here is wired to real behaviour.

use crate::app::App;
use crate::fs::{self, Entry, SortField};
use crate::settings::{
    ConflictPolicy, CwdRule, Density, LaunchMode, RunCommand, RunMode, RunScope, RunStep, Settings,
    ViewKind,
};
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

fn dialog_window(app: &Rc<App>, title: &str, width: i32, height: i32) -> gtk::Window {
    let window = gtk::Window::builder()
        .title(title)
        .transient_for(&app.window)
        .modal(true)
        .default_width(width)
        .default_height(height)
        .build();
    window.add_css_class("delve-dialog");
    // Escape always cancels a dialog; without this a modal dialog could only
    // be dismissed with its buttons.
    let key = gtk::EventControllerKey::new();
    let window_ref = window.clone();
    key.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk4::gdk::Key::Escape {
            window_ref.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key);
    window
}

fn header(title: &str, subtitle: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("big-title");
    t.set_xalign(0.0);
    b.append(&t);
    if !subtitle.is_empty() {
        let s = gtk::Label::new(Some(subtitle));
        s.add_css_class("hint");
        s.set_xalign(0.0);
        s.set_wrap(true);
        b.append(&s);
    }
    b
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("section-title");
    label.set_xalign(0.0);
    label.set_margin_top(10);
    label
}

fn row(label: &str, description: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    b.set_margin_top(3);
    b.set_margin_bottom(3);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
    texts.set_hexpand(true);
    let l = gtk::Label::new(Some(label));
    l.set_xalign(0.0);
    l.add_css_class("meta-value");
    texts.append(&l);
    if !description.is_empty() {
        let d = gtk::Label::new(Some(description));
        d.set_xalign(0.0);
        d.set_wrap(true);
        d.add_css_class("tiny");
        d.add_css_class("dim");
        texts.append(&d);
    }
    b.append(&texts);
    control.set_valign(gtk::Align::Center);
    b.append(control);
    b
}

pub fn confirm<F>(
    app: &Rc<App>,
    title: &str,
    detail: &str,
    confirm_label: &str,
    destructive: bool,
    on_confirm: F,
) where
    F: Fn(&Rc<App>) + 'static,
{
    let window = dialog_window(app, title, 430, 200);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(16);
    root.set_margin_bottom(14);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.append(&header(title, detail));
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_vexpand(true);
    buttons.set_valign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let ok = gtk::Button::with_label(confirm_label);
    if destructive {
        ok.add_css_class("destructive-action");
    } else {
        ok.add_css_class("suggested");
    }
    buttons.append(&ok);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let window = window.clone();
        let cb = Rc::new(on_confirm);
        ok.connect_clicked(move |_| {
            window.close();
            if let Some(app) = weak.upgrade() {
                cb(&app);
            }
        });
    }
    window.present();
    // Keyboard-first: the confirming action receives focus so Enter runs the
    // command profile immediately after the user has reviewed it.
    ok.grab_focus();
}

pub fn name_dialog<F>(
    app: &Rc<App>,
    title: &str,
    placeholder: &str,
    initial: &str,
    confirm_label: &str,
    on_accept: F,
) where
    F: Fn(&Rc<App>, String) -> Result<(), String> + 'static,
{
    let window = dialog_window(app, title, 420, 190);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(16);
    root.set_margin_bottom(14);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.append(&header(title, ""));
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);
    root.append(&entry);
    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    root.append(&error);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let ok = gtk::Button::with_label(confirm_label);
    ok.add_css_class("suggested");
    buttons.append(&ok);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    let submit = Rc::new({
        let weak = Rc::downgrade(app);
        let window = window.clone();
        let entry = entry.clone();
        let error = error.clone();
        move || {
            let name = entry.text().to_string();
            match fs::validate_name(&name) {
                Ok(()) => {}
                Err(err) => {
                    error.set_text(&err);
                    error.set_visible(true);
                    return;
                }
            }
            if let Some(app) = weak.upgrade() {
                match on_accept(&app, name) {
                    Ok(()) => window.close(),
                    Err(err) => {
                        error.set_text(&err);
                        error.set_visible(true);
                    }
                }
            }
        }
    });
    {
        let submit = submit.clone();
        ok.connect_clicked(move |_| submit());
    }
    {
        let submit = submit.clone();
        entry.connect_activate(move |_| submit());
    }
    entry.grab_focus();
    entry.select_region(0, -1);
    window.present();
}

pub fn inline_rename<F>(app: &Rc<App>, anchor: &impl IsA<gtk::Widget>, current: &str, on_accept: F)
where
    F: Fn(&Rc<App>, String) -> Result<(), String> + 'static,
{
    let popover = gtk::Popover::new();
    popover.set_parent(anchor);
    let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
    boxed.set_margin_top(8);
    boxed.set_margin_bottom(8);
    boxed.set_margin_start(8);
    boxed.set_margin_end(8);
    let label = gtk::Label::new(Some("Rename"));
    label.add_css_class("section-title");
    label.set_xalign(0.0);
    boxed.append(&label);
    let entry = gtk::Entry::new();
    entry.set_text(current);
    entry.set_width_chars(28);
    boxed.append(&entry);
    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    boxed.append(&error);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let ok = gtk::Button::with_label("Rename");
    ok.add_css_class("suggested");
    buttons.append(&ok);
    boxed.append(&buttons);
    popover.set_child(Some(&boxed));
    let weak = Rc::downgrade(app);
    let popover_ref = popover.clone();
    let entry_ref = entry.clone();
    let error_ref = error.clone();
    let submit = move || {
        let new_name = entry_ref.text().to_string();
        if let Err(err) = fs::validate_name(&new_name) {
            error_ref.set_text(&err);
            error_ref.set_visible(true);
            return;
        }
        if let Some(app) = weak.upgrade() {
            match on_accept(&app, new_name) {
                Ok(()) => popover_ref.popdown(),
                Err(err) => {
                    error_ref.set_text(&err);
                    error_ref.set_visible(true);
                }
            }
        }
    };
    {
        let submit = Rc::new(submit);
        let s1 = submit.clone();
        ok.connect_clicked(move |_| s1());
        let s2 = submit.clone();
        entry.connect_activate(move |_| s2());
        let popover_ref = popover.clone();
        cancel.connect_clicked(move |_| popover_ref.popdown());
    }
    popover.popup();
    entry.grab_focus();
    // Select the stem, leaving the extension unselected for quick replaces.
    let chars: Vec<char> = current.chars().collect();
    let stem_len = current.rfind('.').map(|i| current[..i].chars().count()).unwrap_or(chars.len());
    entry.select_region(0, stem_len as i32);
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

pub fn properties_dialog(app: &Rc<App>, entries: Vec<Entry>) {
    if entries.is_empty() {
        return;
    }
    let single = entries.len() == 1;
    let title = if single {
        format!("Properties — {}", entries[0].name)
    } else {
        format!("Properties — {} items", entries.len())
    };
    let window = dialog_window(app, &title, 640, 700);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    let scroller = gtk::ScrolledWindow::builder().vexpand(true).build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    scroller.set_child(Some(&content));
    root.append(&scroller);

    // --- Hero ---------------------------------------------------------
    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero.add_css_class("hero-card");
    let hero_icon = crate::views::entry_icon(app, &entries[0], 44);
    hero.append(&hero_icon);
    let hero_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    hero_text.set_hexpand(true);
    let hero_title = gtk::Label::new(Some(if single {
        &entries[0].name
    } else {
        "Multiple items"
    }));
    hero_title.add_css_class("big-title");
    hero_title.set_xalign(0.0);
    hero_title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    hero_text.append(&hero_title);
    let hero_subtitle = if single {
        entries[0].type_label()
    } else {
        format!("{} files and folders", entries.len())
    };
    let hero_sub = gtk::Label::new(Some(&hero_subtitle));
    hero_sub.add_css_class("secondary");
    hero_sub.set_xalign(0.0);
    hero_text.append(&hero_sub);
    let hero_path = gtk::Label::new(Some(&crate::paths::display_path(&entries[0].path)));
    hero_path.add_css_class("tiny");
    hero_path.add_css_class("dim");
    hero_path.set_xalign(0.0);
    hero_path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    hero_path.set_selectable(true);
    hero_text.append(&hero_path);
    hero.append(&hero_text);
    content.append(&hero);

    // --- At-a-glance stat cards --------------------------------------
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let folders = entries.iter().filter(|e| e.is_dir).count();
    let stats = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(8)
        .column_homogeneous(true)
        .build();
    let cards: Vec<gtk::Box> = if single {
        let entry = &entries[0];
        if entry.is_dir {
            vec![
                stat_card("SIZE ON DISK", &fs::human_size(entry.allocated), "Allocated"),
                stat_card("MODIFIED", &fs::format_time(entry.modified), &fs::format_time_long(entry.modified)),
                stat_card("PERMISSIONS", &entry.permissions_string(), &entry.mode_octal()),
                stat_card("OWNER", &entry.owner_name(), &entry.group_name()),
            ]
        } else {
            vec![
                stat_card("SIZE", &fs::human_size(entry.size), &format!("{} bytes", entry.size)),
                stat_card("ON DISK", &fs::human_size(entry.allocated), &format!("{} bytes", entry.allocated)),
                stat_card("MODIFIED", &fs::format_time(entry.modified), &fs::format_time_long(entry.modified)),
                stat_card("PERMISSIONS", &entry.permissions_string(), &entry.mode_octal()),
            ]
        }
    } else {
        vec![
            stat_card("ITEMS", &entries.len().to_string(), "Selected"),
            stat_card("FOLDERS", &folders.to_string(), "Directories"),
            stat_card("FILES", &(entries.len() - folders).to_string(), "Documents"),
            stat_card("TOTAL SIZE", &fs::human_size(total), &format!("{total} bytes")),
        ]
    };
    for (i, card) in cards.iter().enumerate() {
        stats.attach(card, (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    content.append(&stats);

    // --- Pie chart ----------------------------------------------------
    let dark = app.theme.current.borrow().spec.dark;
    if single {
        if let Some((used, free)) = disk_usage(&entries[0].path) {
            let total_disk = used.saturating_add(free);
            let slices = vec![
                ("Used".to_string(), used, (0.95, 0.45, 0.16)),
                ("Free".to_string(), free, (0.16, 0.62, 0.50)),
            ];
            content.append(&pie_card(
                "DISK USAGE",
                &fs::human_size(total_disk),
                "filesystem capacity",
                &slices,
                dark,
            ));
        }
    } else {
        let mut by_kind: std::collections::BTreeMap<&'static str, u64> = Default::default();
        for entry in &entries {
            let kind = if entry.is_dir {
                "Folders"
            } else {
                category_for(entry)
            };
            *by_kind.entry(kind).or_insert(0) += entry.size.max(entry.allocated);
        }
        let mut slices: Vec<(String, u64, (f64, f64, f64))> = by_kind
            .into_iter()
            .filter(|(_, v)| *v > 0)
            .map(|(k, v)| (k.to_string(), v, category_color(k)))
            .collect();
        slices.sort_by(|a, b| b.1.cmp(&a.1));
        if total > 0 {
            content.append(&pie_card(
                "SIZE BY TYPE",
                &fs::human_size(total),
                "selected items",
                &slices,
                dark,
            ));
        }
    }

    // --- Details ------------------------------------------------------
    let details = gtk::Box::new(gtk::Orientation::Vertical, 6);
    details.add_css_class("card");
    let details_label = gtk::Label::new(Some("DETAILS"));
    details_label.add_css_class("section-title");
    details_label.set_xalign(0.0);
    details.append(&details_label);
    let grid = gtk::Grid::builder().row_spacing(5).column_spacing(12).build();
    let mut detail_row = 0;
    let mut add = |key: &str, value: &str| {
        let k = gtk::Label::new(Some(key));
        k.add_css_class("meta-key");
        k.set_xalign(0.0);
        k.set_valign(gtk::Align::Start);
        grid.attach(&k, 0, detail_row, 1, 1);
        let v = gtk::Label::new(Some(value));
        v.add_css_class("meta-value");
        v.set_xalign(0.0);
        v.set_wrap(true);
        v.set_selectable(true);
        v.set_hexpand(true);
        grid.attach(&v, 1, detail_row, 1, 1);
        detail_row += 1;
    };
    if single {
        let entry = &entries[0];
        add("Name", &entry.name);
        add("Type", &entry.type_label());
        add("MIME type", &entry.mime());
        add("Location", &crate::paths::display_path(&entry.path));
        add("Modified", &fs::format_time_long(entry.modified));
        add("Created", &fs::format_time_long(entry.created));
        add("Accessed", &fs::format_time_long(entry.accessed));
        add(
            "Permissions",
            &format!("{} ({})", entry.permissions_string(), entry.mode_octal()),
        );
        add("Owner", &format!("{}:{}", entry.owner_name(), entry.group_name()));
        if entry.nlink > 1 {
            add("Links", &entry.nlink.to_string());
        }
        if entry.is_symlink {
            if let Ok(target) = std::fs::read_link(&entry.path) {
                add("Link target", &target.display().to_string());
            }
            add("Link status", if entry.broken_link { "Broken" } else { "Valid" });
        }
        if entry.is_dir {
            let (count, size) = {
                let mut count = 0usize;
                let mut size = 0u64;
                if let Ok(rd) = std::fs::read_dir(&entry.path) {
                    for item in rd.flatten() {
                        count += 1;
                        if let Ok(m) = item.metadata() {
                            if !m.is_dir() {
                                size += m.len();
                            }
                        }
                    }
                }
                (count, size)
            };
            add("Items", &format!("{count} (direct)"));
            add("Direct size", &fs::human_size(size));
        }
    } else {
        add("Items", &entries.len().to_string());
        add("Folders", &folders.to_string());
        add("Files", &(entries.len() - folders).to_string());
        add(
            "Total size",
            &format!("{} ({} bytes)", fs::human_size(total), total),
        );
        add("Location", &crate::paths::display_path(&entries[0].path));
    }
    details.append(&grid);
    content.append(&details);

    if entries.iter().any(|e| e.is_dir) {
        let folder_btn = gtk::Button::with_label("Calculate recursive folder size");
        folder_btn.add_css_class("flat");
        let result_label = gtk::Label::new(None);
        result_label.add_css_class("hint");
        result_label.set_xalign(0.0);
        result_label.set_wrap(true);
        {
            let weak = Rc::downgrade(app);
            let dirs: Vec<PathBuf> = entries.iter().filter(|e| e.is_dir).map(|e| e.path.clone()).collect();
            let result_label_btn = result_label.clone();
            let b = folder_btn.clone();
            let b_ref = folder_btn.clone();
            b.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                result_label_btn.set_text("Calculating…");
                b_ref.set_sensitive(false);
                let label_for_result = result_label_btn.clone();
                let (tx, rx) = std::sync::mpsc::channel::<String>();
                glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
                    match rx.try_recv() {
                        Ok(text) => {
                            label_for_result.set_text(&text);
                            glib::ControlFlow::Break
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                        Err(_) => glib::ControlFlow::Break,
                    }
                });
                let dirs = dirs.clone();
                std::thread::spawn(move || {
                    let cancel = std::sync::atomic::AtomicBool::new(false);
                    let mut total = 0u64;
                    let mut files = 0u64;
                    let mut folders = 0u64;
                    for dir in &dirs {
                        let size = fs::dir_size(dir, &cancel, 500_000);
                        total += size.bytes;
                        files += size.files;
                        folders += size.folders;
                    }
                    let _ = tx.send(format!(
                        "{} · {} files · {} folders (recursive)",
                        fs::human_size(total),
                        files,
                        folders
                    ));
                });
                let _ = &app;
            });
        }
        content.append(&folder_btn);
        content.append(&result_label);
    }

    // --- Footer -------------------------------------------------------
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let open = gtk::Button::with_label("Open containing folder");
    buttons.append(&open);
    let close = gtk::Button::with_label("Close");
    close.add_css_class("suggested");
    buttons.append(&close);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        let weak = Rc::downgrade(app);
        open.connect_clicked(move |_| {
            window.close();
            if let Some(app) = weak.upgrade() {
                app.open_containing_folder();
            }
        });
    }
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }
    window.present();
}

fn stat_card(key: &str, value: &str, sub: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 1);
    card.add_css_class("stat-card");
    card.set_hexpand(true);
    let k = gtk::Label::new(Some(key));
    k.add_css_class("stat-key");
    k.set_xalign(0.0);
    card.append(&k);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("stat-value");
    v.set_xalign(0.0);
    v.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    v.set_tooltip_text(Some(value));
    card.append(&v);
    if !sub.is_empty() {
        let s = gtk::Label::new(Some(sub));
        s.add_css_class("stat-sub");
        s.set_xalign(0.0);
        s.set_ellipsize(gtk::pango::EllipsizeMode::End);
        card.append(&s);
    }
    card
}

fn category_color(kind: &str) -> (f64, f64, f64) {
    match kind {
        "Images" => (0.13, 0.66, 0.60),
        "Video" => (0.52, 0.38, 0.86),
        "Audio" => (0.95, 0.65, 0.20),
        "Documents" => (0.23, 0.50, 0.86),
        "Code" => (0.24, 0.70, 0.44),
        "Archives" => (0.93, 0.52, 0.18),
        "Folders" => (0.46, 0.55, 0.66),
        _ => (0.55, 0.58, 0.64),
    }
}

fn color_swatch(rgb: (f64, f64, f64)) -> gtk::DrawingArea {
    let dot = gtk::DrawingArea::new();
    dot.set_content_width(10);
    dot.set_content_height(10);
    dot.set_valign(gtk::Align::Center);
    dot.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 1.0);
        cr.arc(w as f64 / 2.0, h as f64 / 2.0, (w.min(h) as f64 / 2.0) - 1.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    });
    dot
}

fn pie_card(
    title: &str,
    center: &str,
    caption: &str,
    slices: &[(String, u64, (f64, f64, f64))],
    dark: bool,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("pie-card");
    let label = gtk::Label::new(Some(title));
    label.add_css_class("section-title");
    label.set_xalign(0.0);
    card.append(&label);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let area = gtk::DrawingArea::new();
    area.set_content_width(150);
    area.set_content_height(150);
    area.set_halign(gtk::Align::Start);
    let draw_slices = slices.to_vec();
    let center_text = center.to_string();
    let caption_text = caption.to_string();
    area.set_draw_func(move |_, cr, w, h| {
        let total: f64 = draw_slices.iter().map(|s| s.1 as f64).sum::<f64>().max(1.0);
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let radius = (w.min(h) as f64 / 2.0) - 4.0;
        let mut angle = -std::f64::consts::FRAC_PI_2;
        for (_, value, (r, g, b)) in draw_slices.iter() {
            if *value == 0 {
                continue;
            }
            let sweep = (*value as f64 / total) * std::f64::consts::TAU;
            cr.move_to(cx, cy);
            cr.arc(cx, cy, radius, angle, angle + sweep);
            cr.close_path();
            cr.set_source_rgba(*r, *g, *b, 0.95);
            let _ = cr.fill();
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
            cr.set_line_width(1.0);
            cr.arc(cx, cy, radius, angle, angle + sweep);
            cr.line_to(cx, cy);
            let _ = cr.stroke();
            angle += sweep;
        }
        // Punch a donut hole so the total can sit in the middle.
        let _ = cr.save();
        cr.set_operator(gtk::cairo::Operator::Clear);
        cr.arc(cx, cy, radius * 0.62, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        let _ = cr.restore();
        let fg = if dark {
            (0.92, 0.94, 0.96, 0.96)
        } else {
            (0.10, 0.12, 0.16, 0.96)
        };
        cr.set_source_rgba(fg.0, fg.1, fg.2, fg.3);
        cr.select_font_face(
            "Sans",
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Bold,
        );
        cr.set_font_size(12.0);
        let ext = cr.text_extents(&center_text).map(|e| e.width()).unwrap_or(0.0);
        cr.move_to(cx - ext / 2.0, cy + 2.0);
        let _ = cr.show_text(&center_text);
        cr.set_font_size(8.0);
        let ext = cr.text_extents(&caption_text).map(|e| e.width()).unwrap_or(0.0);
        cr.move_to(cx - ext / 2.0, cy + 15.0);
        let _ = cr.show_text(&caption_text);
    });
    body.append(&area);
    let legend = gtk::Box::new(gtk::Orientation::Vertical, 4);
    legend.set_hexpand(true);
    legend.set_valign(gtk::Align::Center);
    let total: f64 = slices.iter().map(|s| s.1 as f64).sum::<f64>().max(1.0);
    for (name, value, rgb) in slices {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        row.append(&color_swatch(*rgb));
        let name_label = gtk::Label::new(Some(name));
        name_label.add_css_class("legend-label");
        name_label.set_xalign(0.0);
        name_label.set_hexpand(true);
        name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&name_label);
        let value_label = gtk::Label::new(Some(&format!(
            "{}  ·  {:.0}%",
            fs::human_size(*value),
            (*value as f64 / total) * 100.0
        )));
        value_label.add_css_class("tiny");
        value_label.add_css_class("dim");
        row.append(&value_label);
        legend.append(&row);
    }
    body.append(&legend);
    card.append(&body);
    card
}

/// Used and free bytes on the filesystem that holds `path`.
fn disk_usage(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let block = st.f_frsize as u64;
    let total = (st.f_blocks as u64).saturating_mul(block);
    let free = (st.f_bavail as u64).saturating_mul(block);
    Some((total.saturating_sub(free), free))
}

// ---------------------------------------------------------------------------
// Conflict resolution
// ---------------------------------------------------------------------------

pub fn conflict_dialog<F>(app: &Rc<App>, conflicts: &[PathBuf], on_choice: F)
where
    F: Fn(ConflictPolicy) + 'static,
{
    let window = dialog_window(app, "Items already exist", 470, 340);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Items already exist",
        "Choose how to handle destinations that already contain items. Nested conflicts use the same choice.",
    ));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .max_content_height(160)
        .build();
    for path in conflicts.iter().take(100) {
        let label = gtk::Label::new(Some(&crate::paths::display_path(path)));
        label.set_xalign(0.0);
        label.add_css_class("tiny");
        label.add_css_class("mono-path");
        list.append(&label);
    }
    root.append(&scroller);
    let buttons = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let choice = Rc::new(on_choice);
    for (label, policy, suggested) in [
        ("Keep both (rename new items)", ConflictPolicy::KeepBoth, true),
        ("Replace existing items", ConflictPolicy::Replace, false),
        ("Skip existing items", ConflictPolicy::Skip, false),
    ] {
        let b = gtk::Button::with_label(label);
        if suggested {
            b.add_css_class("suggested");
        }
        let window_ref = window.clone();
        let choice = choice.clone();
        b.connect_clicked(move |_| {
            choice(policy);
            window_ref.close();
        });
        buttons.append(&b);
    }
    let cancel = gtk::Button::with_label("Cancel");
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    buttons.append(&cancel);
    root.append(&buttons);
    window.set_child(Some(&root));
    window.present();
}

// ---------------------------------------------------------------------------
// Organise
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum OrganiseRule {
    Extension,
    Category,
    Date,
    Size,
    Name,
}

impl OrganiseRule {
    fn label(&self) -> &'static str {
        match self {
            OrganiseRule::Extension => "File extension",
            OrganiseRule::Category => "File type category",
            OrganiseRule::Date => "Modified month",
            OrganiseRule::Size => "Size range",
            OrganiseRule::Name => "First letter of name",
        }
    }
    fn folder_for(&self, entry: &Entry) -> String {
        match self {
            OrganiseRule::Extension => {
                let ext = entry.ext();
                if ext.is_empty() {
                    "No extension".to_string()
                } else {
                    ext.to_uppercase()
                }
            }
            OrganiseRule::Category => category_for(entry).to_string(),
            OrganiseRule::Date => {
                if let Some(modified) = entry.modified {
                    let dur = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    if let Some(dt) =
                        glib::DateTime::from_unix_local(dur.as_secs() as i64).ok()
                    {
                        if let Ok(text) = dt.format("%Y-%m") {
                            return text.to_string();
                        }
                    }
                }
                "Unknown date".to_string()
            }
            OrganiseRule::Size => {
                if entry.is_dir {
                    "Folders".to_string()
                } else if entry.size < 1_048_576 {
                    "Small (under 1 MB)".to_string()
                } else if entry.size < 104_857_600 {
                    "Medium (1–100 MB)".to_string()
                } else {
                    "Large (over 100 MB)".to_string()
                }
            }
            OrganiseRule::Name => {
                let first = entry.name.chars().next().unwrap_or('#');
                if first.is_alphabetic() {
                    first.to_uppercase().to_string()
                } else {
                    "#".to_string()
                }
            }
        }
    }
}

fn category_for(entry: &Entry) -> &'static str {
    if entry.is_dir {
        return "Folders";
    }
    let ext = entry.ext();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tif" | "tiff" | "heic" => {
            "Images"
        }
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "mpg" | "mpeg" => "Video",
        "mp3" | "flac" | "ogg" | "wav" | "m4a" | "opus" | "aac" => "Audio",
        "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" | "bz2" => "Archives",
        "pdf" | "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "xls" | "xlsx" | "ods" | "ppt"
        | "pptx" | "odp" => "Documents",
        "rs" | "py" | "js" | "ts" | "c" | "h" | "cpp" | "hpp" | "go" | "java" | "sh" | "json"
        | "toml" | "yaml" | "yml" | "css" | "html" => "Code",
        _ => "Other",
    }
}

pub fn organise_dialog(app: &Rc<App>, tab_id: u64) {
    let Some(tab) = app.tab_by_id(tab_id) else { return };
    let dir = tab.state.borrow().location.path().cloned();
    let Some(dir) = dir else {
        app.notify("Organise needs a folder location.", true);
        return;
    };
    let entries: Vec<Entry> = tab
        .state
        .borrow()
        .visible_entries()
        .into_iter()
        .filter(|e| !e.is_dir)
        .collect();
    if entries.is_empty() {
        app.notify("There are no files to organise in this folder.", false);
        return;
    }
    let window = dialog_window(app, "Organise folder", 600, 560);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Organise folder",
        "Preview the proposed changes before anything is moved. Delve never reorganises files without confirmation.",
    ));
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let rule_drop = gtk::DropDown::from_strings(&[
        OrganiseRule::Extension.label(),
        OrganiseRule::Category.label(),
        OrganiseRule::Date.label(),
        OrganiseRule::Size.label(),
        OrganiseRule::Name.label(),
    ]);
    controls.append(&rule_drop);
    let mode_drop = gtk::DropDown::from_strings(&["Move files into folders", "Preview only (no changes)"]);
    controls.append(&mode_drop);
    root.append(&controls);
    let summary = gtk::Label::new(None);
    summary.add_css_class("hint");
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    root.append(&summary);
    let preview = gtk::TextView::new();
    preview.set_editable(false);
    preview.set_monospace(true);
    preview.set_wrap_mode(gtk::WrapMode::None);
    preview.add_css_class("preview-text");
    let scroller = gtk::ScrolledWindow::builder()
        .child(&preview)
        .vexpand(true)
        .build();
    root.append(&scroller);

    let moves: Rc<RefCell<Vec<(PathBuf, PathBuf)>>> = Rc::new(RefCell::new(Vec::new()));
    let recompute = {
        let entries = entries.clone();
        let dir = dir.clone();
        let moves = moves.clone();
        let summary = summary.clone();
        let preview = preview.clone();
        let rule_drop = rule_drop.clone();
        Rc::new(move || {
            let rule = match rule_drop.selected() {
                1 => OrganiseRule::Category,
                2 => OrganiseRule::Date,
                3 => OrganiseRule::Size,
                4 => OrganiseRule::Name,
                _ => OrganiseRule::Extension,
            };
            let mut computed = Vec::new();
            let mut text = String::new();
            for entry in &entries {
                let folder = rule.folder_for(entry);
                let target = dir.join(&folder).join(&entry.name);
                text.push_str(&format!(
                    "{}\n    → {}/\n",
                    entry.name, folder
                ));
                computed.push((entry.path.clone(), target));
            }
            preview.buffer().set_text(&text);
            let folders: std::collections::HashSet<String> = entries
                .iter()
                .map(|e| rule.folder_for(e))
                .collect();
            summary.set_text(&format!(
                "{} files · {} folders will be created · rule: {}",
                entries.len(),
                folders.len(),
                rule.label()
            ));
            moves.replace(computed);
        })
    };
    recompute();
    {
        let recompute = recompute.clone();
        rule_drop.connect_selected_notify(move |_| recompute());
    }
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let apply = gtk::Button::with_label("Apply");
    apply.add_css_class("suggested");
    buttons.append(&apply);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let window_ref = window.clone();
        let moves = moves.clone();
        let mode_drop = mode_drop.clone();
        apply.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            if mode_drop.selected() == 1 {
                app.notify("Preview only — no files were moved.", false);
                window_ref.close();
                return;
            }
            let moves = moves.borrow().clone();
            let count = moves.len();
            window_ref.close();
            app.run_operation("Organising files", move |cancel, progress| {
                crate::ops::organise_moves(&moves, cancel, Some(progress))
            });
            app.notify(
                &format!("Organising {count} file(s) — progress is shown in the operation window."),
                false,
            );
            let weak = Rc::downgrade(&app);
            let reload_dir = dir.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                if let Some(app) = weak.upgrade() {
                    app.reload_tabs_for(&[reload_dir.clone()]);
                }
            });
        });
    }
    window.present();
}

// ---------------------------------------------------------------------------
// Archives
// ---------------------------------------------------------------------------

pub fn compress_dialog(app: &Rc<App>, tab_id: u64) {
    let Some(tab) = app.tab_by_id(tab_id) else { return };
    let selected = tab.state.borrow().selected_entries();
    let dir = tab.state.borrow().location.path().cloned();
    let Some(dir) = dir else {
        app.notify("Open a folder first.", true);
        return;
    };
    let paths: Vec<PathBuf> = if selected.is_empty() {
        vec![dir.clone()]
    } else {
        selected.iter().map(|e| e.path.clone()).collect()
    };
    let default_name = if selected.len() == 1 {
        format!(
            "{}.zip",
            selected[0]
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "archive".into())
        )
    } else {
        format!(
            "{}.zip",
            fs::file_name_of(&dir)
        )
    };
    let window = dialog_window(app, "Compress", 460, 280);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Create archive",
        &format!("{} item(s) will be added to the archive.", paths.len()),
    ));
    let format_drop = gtk::DropDown::from_strings(&["ZIP", "TAR", "TAR.GZ"]);
    root.append(&row("Format", "ZIP works everywhere; TAR.GZ is smaller for text.", &format_drop));
    let dest_entry = gtk::Entry::new();
    dest_entry.set_text(&dir.join(&default_name).display().to_string());
    root.append(&row("Destination", "Where the archive will be written.", &dest_entry));
    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    root.append(&error);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let create = gtk::Button::with_label("Create archive");
    create.add_css_class("suggested");
    buttons.append(&create);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let window_ref = window.clone();
        let error = error.clone();
        let format_drop = format_drop.clone();
        let dest_entry = dest_entry.clone();
        let paths = paths.clone();
        let dir = dir.clone();
        create.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let dest = PathBuf::from(dest_entry.text().to_string());
            if dest.as_os_str().is_empty() {
                error.set_text("Choose a destination path.");
                error.set_visible(true);
                return;
            }
            let format = match format_drop.selected() {
                1 => crate::ops::ArchiveFormat::Tar,
                2 => crate::ops::ArchiveFormat::TarGz,
                _ => crate::ops::ArchiveFormat::Zip,
            };
            let ext = format.extension();
            let dest = if crate::ops::ArchiveFormat::from_path(&dest).is_some() {
                dest
            } else {
                PathBuf::from(format!("{}{}", dest.display(), ext))
            };
            if dest.exists() {
                error.set_text("That archive already exists. Choose another name.");
                error.set_visible(true);
                return;
            }
            window_ref.close();
            let paths_op = paths.clone();
            let dest_op = dest.clone();
            app.run_operation("Creating archive", move |cancel, progress| {
                crate::ops::compress(&paths_op, &dest_op, format, cancel, Some(progress))
            });
            let weak = Rc::downgrade(&app);
            let dest_clone = dest.clone();
            let reload_dir = dir.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                if let Some(app) = weak.upgrade() {
                    app.notify(&format!("Archive written to {}", dest_clone.display()), false);
                    app.reload_tabs_for(&[reload_dir.clone()]);
                }
            });
        });
    }
    window.present();
}

pub fn extract_dialog(app: &Rc<App>, archive: &std::path::Path) {
    let contents = crate::ops::inspect_archive(archive).unwrap_or_default();
    let window = dialog_window(app, "Extract archive", 520, 480);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Extract archive",
        &format!(
            "{} · {} entries",
            fs::file_name_of(archive),
            contents.len()
        ),
    ));
    let dest_entry = gtk::Entry::new();
    let default_dest = archive
        .parent()
        .map(|p| p.join(archive.file_stem().unwrap_or_default()))
        .unwrap_or_else(|| PathBuf::from("."));
    dest_entry.set_text(&default_dest.display().to_string());
    root.append(&row("Destination", "Unsafe archive entries are always skipped.", &dest_entry));
    let policy_drop = gtk::DropDown::from_strings(&[
        ConflictPolicy::KeepBoth.label(),
        ConflictPolicy::Replace.label(),
        ConflictPolicy::Skip.label(),
    ]);
    root.append(&row("If files exist", "", &policy_drop));
    let list = gtk::TextView::new();
    list.set_editable(false);
    list.set_monospace(true);
    list.set_wrap_mode(gtk::WrapMode::None);
    list.add_css_class("preview-text");
    let mut preview = String::new();
    for (name, size, is_dir) in contents.iter().take(500) {
        preview.push_str(&format!(
            "{}{}  {}\n",
            name,
            if *is_dir { "/" } else { "" },
            if *is_dir { String::new() } else { fs::human_size(*size) }
        ));
    }
    if contents.len() > 500 {
        preview.push_str(&format!("…and {} more\n", contents.len() - 500));
    }
    list.buffer().set_text(&preview);
    let scroller = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();
    root.append(&scroller);
    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    root.append(&error);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let extract = gtk::Button::with_label("Extract");
    extract.add_css_class("suggested");
    buttons.append(&extract);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let window_ref = window.clone();
        let dest_entry = dest_entry.clone();
        let policy_drop = policy_drop.clone();
        let archive = archive.to_path_buf();
        let error = error.clone();
        extract.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let dest = PathBuf::from(dest_entry.text().to_string());
            if dest.as_os_str().is_empty() {
                error.set_text("Choose a destination folder.");
                error.set_visible(true);
                return;
            }
            let policy = match policy_drop.selected() {
                1 => ConflictPolicy::Replace,
                2 => ConflictPolicy::Skip,
                _ => ConflictPolicy::KeepBoth,
            };
            window_ref.close();
            let app_clone = app.clone();
            let archive_clone = archive.clone();
            let dest_clone = dest.clone();
            app.run_operation("Extracting archive", move |cancel, progress| {
                let policy = match policy {
                    ConflictPolicy::Replace => ConflictPolicy::Replace,
                    ConflictPolicy::Skip => ConflictPolicy::Skip,
                    _ => ConflictPolicy::KeepBoth,
                };
                crate::ops::extract(&archive_clone, &dest_clone, policy, cancel, Some(progress))
            });
            let weak = Rc::downgrade(&app_clone);
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                if let Some(app) = weak.upgrade() {
                    app.reload_tabs_for(&[dest.clone()]);
                }
            });
        });
    }
    window.present();
}

pub fn extract_archive(app: &Rc<App>, archive: &std::path::Path, dest: &std::path::Path) {
    let archive = archive.to_path_buf();
    let dest = dest.to_path_buf();
    let dest_op = dest.clone();
    app.run_operation("Extracting archive", move |cancel, progress| {
        crate::ops::extract(
            &archive,
            &dest_op,
            ConflictPolicy::KeepBoth,
            cancel,
            Some(progress),
        )
    });
    let weak = Rc::downgrade(app);
    glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
        if let Some(app) = weak.upgrade() {
            app.reload_tabs_for(&[dest.clone()]);
        }
    });
}

// ---------------------------------------------------------------------------
// Transfers
// ---------------------------------------------------------------------------

pub fn transfer_dialog(app: &Rc<App>) {
    let availability = crate::transfers::availability();
    let window = dialog_window(app, "Send to device", 520, 460);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Send to a nearby device",
        "Uses LocalSend when it is installed. Files are only sent when you press Send.",
    ));
    match &availability {
        crate::transfers::Availability::Unavailable { reason } => {
            let label = gtk::Label::new(Some(reason));
            label.set_wrap(true);
            label.set_xalign(0.0);
            label.add_css_class("hint");
            root.append(&label);
            let close = gtk::Button::with_label("Close");
            close.set_halign(gtk::Align::End);
            let window_ref = window.clone();
            close.connect_clicked(move |_| window_ref.close());
            root.append(&close);
            window.set_child(Some(&root));
            window.present();
            return;
        }
        crate::transfers::Availability::Available { detail } => {
            let label = gtk::Label::new(Some(detail));
            label.add_css_class("tiny");
            label.add_css_class("success");
            label.set_xalign(0.0);
            root.append(&label);
        }
    }
    let selected_info = {
        let Some(tab) = app.active_tab() else {
            app.notify("Select files to send first.", true);
            return;
        };
        let entries = tab.state.borrow().selected_entries();
        entries
            .iter()
            .map(|e| e.path.clone())
            .collect::<Vec<PathBuf>>()
    };
    let paths = Rc::new(selected_info);
    if paths.is_empty() {
        app.notify("Select files to send first.", true);
        return;
    }
    let count_label = gtk::Label::new(Some(&format!(
        "{} selected item(s) will be sent",
        paths.len()
    )));
    count_label.set_xalign(0.0);
    count_label.add_css_class("hint");
    root.append(&count_label);
    let device_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let device_scroller = gtk::ScrolledWindow::builder()
        .child(&device_box)
        .vexpand(true)
        .min_content_height(150)
        .build();
    root.append(&device_scroller);
    let status = gtk::Label::new(Some("Press Discover to look for nearby devices."));
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.add_css_class("tiny");
    status.add_css_class("dim");
    root.append(&status);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let discover = gtk::Button::with_label("Discover");
    buttons.append(&discover);
    let send = gtk::Button::with_label("Send");
    send.add_css_class("suggested");
    send.set_sensitive(false);
    buttons.append(&send);
    let close = gtk::Button::with_label("Close");
    buttons.append(&close);
    root.append(&buttons);
    window.set_child(Some(&root));
    let devices: Rc<RefCell<Vec<crate::transfers::Device>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }
    {
        let device_box = device_box.clone();
        let devices = devices.clone();
        let selected = selected.clone();
        let status = status.clone();
        let send = send.clone();
        discover.connect_clicked(move |_| {
            status.set_text("Searching the local network…");
            let device_box = device_box.clone();
            let devices = devices.clone();
            let selected = selected.clone();
            let status = status.clone();
            let send = send.clone();
            let (tx, rx) = std::sync::mpsc::channel::<(Vec<crate::transfers::Device>, Vec<String>)>();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok((found, errors)) => {
                        while let Some(child) = device_box.first_child() {
                            device_box.remove(&child);
                        }
                        if found.is_empty() {
                            status.set_text(&format!(
                                "No devices found. Make sure LocalSend is running and both devices are on the same network.{}",
                                if errors.is_empty() {
                                    String::new()
                                } else {
                                    format!(" ({})", errors.join("; "))
                                }
                            ));
                            send.set_sensitive(false);
                        } else {
                            status.set_text(&format!("{} device(s) found", found.len()));
                            for (i, device) in found.iter().enumerate() {
                                let b = gtk::CheckButton::with_label(&crate::transfers::device_summary(device));
                                let selected = selected.clone();
                                let send = send.clone();
                                b.connect_toggled(move |c| {
                                    if c.is_active() {
                                        *selected.borrow_mut() = Some(i);
                                        send.set_sensitive(true);
                                    }
                                });
                                device_box.append(&b);
                            }
                            devices.replace(found);
                        }
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(_) => glib::ControlFlow::Break,
                }
            });
            std::thread::spawn(move || {
                let result = crate::transfers::discover(std::time::Duration::from_secs(3));
                let _ = tx.send(result);
            });
        });
    }
    {
        let weak = Rc::downgrade(app);
        let devices = devices.clone();
        let selected = selected.clone();
        let paths = paths.clone();
        let window_ref = window.clone();
        send.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let index = *selected.borrow();
            let Some(index) = index else { return };
            let device = match devices.borrow().get(index).cloned() {
                Some(d) => d,
                None => return,
            };
            window_ref.close();
            let paths: Vec<PathBuf> = (*paths).clone();
            app.run_operation(&format!("Sending to {}", device.info.alias), move |cancel, progress| {
                match                    crate::transfers::send_paths(
                    &device,
                    &paths,
                    cancel,
                    std::sync::Arc::new(move |fraction, message| {
                        progress(crate::ops::Progress {
                            fraction,
                            message,
                            done: 0,
                            total: 0,
                        });
                    }),
                ) {
                    Ok(sent) => crate::ops::Outcome {
                        succeeded: sent,
                        message: format!("Sent {sent} file(s)"),
                        ..Default::default()
                    },
                    Err(err) => crate::ops::Outcome {
                        failures: vec![crate::ops::Failure {
                            path: PathBuf::new(),
                            message: err,
                        }],
                        ..Default::default()
                    },
                }
            });
        });
    }
    window.present();
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

/// Assign or remove tags for any number of items in one focused window. This
/// replaces the long inline tag list that used to live in the context menu.
pub fn assign_tags_dialog(app: &Rc<App>, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let count = paths.len();
    let window = dialog_window(app, "Assign tags", 440, 520);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Assign tags",
        &format!(
            "Apply tags to {} selected item{}. Changes are saved immediately.",
            count,
            if count == 1 { "" } else { "s" }
        ),
    ));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    root.append(&scroller);

    let refresh: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let build = {
        let weak = Rc::downgrade(app);
        let list = list.clone();
        let refresh = refresh.clone();
        let paths = paths.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let tags = app.meta.borrow().tags.clone();
            if tags.is_empty() {
                let hint = gtk::Label::new(Some("No tags yet — create one below."));
                hint.add_css_class("hint");
                hint.set_xalign(0.0);
                list.append(&hint);
                return;
            }
            for tag in tags {
                let all_have = {
                    let meta = app.meta.borrow();
                    paths
                        .iter()
                        .all(|p| meta.tags_for(p).iter().any(|id| id == &tag.id))
                };
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row.add_css_class("card");
                row.set_margin_top(2);
                row.set_margin_bottom(2);
                let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                dot.add_css_class("color-dot");
                dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(&tag.color)));
                dot.set_size_request(12, 12);
                dot.set_valign(gtk::Align::Center);
                row.append(&dot);
                let check = gtk::CheckButton::with_label(&tag.name);
                check.set_active(all_have);
                check.set_hexpand(true);
                {
                    let weak = Rc::downgrade(&app);
                    let id = tag.id.clone();
                    let paths = paths.clone();
                    let name = tag.name.clone();
                    check.connect_toggled(move |c| {
                        let Some(app) = weak.upgrade() else { return };
                        if c.is_active() {
                            app.meta.borrow_mut().add_tag_bulk(&paths, &id);
                        } else {
                            app.meta.borrow_mut().remove_tags_bulk(&paths, &id);
                        }
                        app.refresh_after_metadata_change();
                        app.notify(
                            &format!(
                                "{} “{}” for {} item(s)",
                                if c.is_active() { "Tagged" } else { "Removed" },
                                name,
                                paths.len()
                            ),
                            false,
                        );
                    });
                }
                row.append(&check);
                list.append(&row);
            }
        }
    };
    *refresh.borrow_mut() = Some(Rc::new(build.clone()));
    build();

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let new_tag = gtk::Button::with_label("New tag…");
    buttons.append(&new_tag);
    let manage = gtk::Button::with_label("Manage tags…");
    buttons.append(&manage);
    let done = gtk::Button::with_label("Done");
    done.add_css_class("suggested");
    buttons.append(&done);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        done.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let build = build.clone();
        new_tag.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                create_tag_dialog(&app);
                let build = build.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || build());
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        let build = build.clone();
        manage.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                tag_manager(&app);
                let build = build.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || build());
            }
        });
    }
    window.present();
}

/// Assign a folder colour. This is a real dialog rather than a popover anchored
/// inside the context menu: the old nested popover was parented to a menu item
/// that had already been popped down, which is what produced the flicker and
/// swallowed clicks.
pub fn folder_color_dialog(app: &Rc<App>, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let window = dialog_window(
        app,
        if paths.len() == 1 { "Folder colour" } else { "Folder colours" },
        440,
        330,
    );
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Folder colour",
        "Colours are stored by Delve and never modify the folder itself.",
    ));
    let current = paths
        .first()
        .and_then(|p| app.meta.borrow().folder_color(p))
        .unwrap_or_default();

    let grid = gtk::Grid::builder()
        .column_spacing(6)
        .row_spacing(6)
        .build();
    for (index, (name, hex)) in crate::theme::FOLDER_PALETTE.iter().enumerate() {
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.set_tooltip_text(Some(*name));
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("color-dot");
        dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(hex)));
        dot.set_size_request(26, 26);
        button.set_child(Some(&dot));
        if current.eq_ignore_ascii_case(hex) {
            button.add_css_class("suggested");
        }
        let weak = Rc::downgrade(app);
        let paths = paths.clone();
        let window_ref = window.clone();
        let hex = (*hex).to_string();
        button.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                for path in &paths {
                    app.set_folder_color(path, Some(hex.clone()));
                }
                window_ref.close();
            }
        });
        grid.attach(&button, (index % 6) as i32, (index / 6) as i32, 1, 1);
    }
    root.append(&grid);

    let custom = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("#RRGGBB"));
    entry.set_text(if current.is_empty() { "#4c8dff" } else { &current });
    entry.set_hexpand(true);
    custom.append(&entry);
    let apply = gtk::Button::with_label("Apply custom colour");
    apply.add_css_class("suggested");
    {
        let weak = Rc::downgrade(app);
        let paths = paths.clone();
        let window_ref = window.clone();
        let entry = entry.clone();
        apply.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let text = entry.text().trim().to_string();
            let valid = text.len() == 7
                && text.starts_with('#')
                && text[1..].chars().all(|c| c.is_ascii_hexdigit());
            if !valid {
                entry.add_css_class("error");
                return;
            }
            entry.remove_css_class("error");
            for path in &paths {
                app.set_folder_color(path, Some(text.clone()));
            }
            window_ref.close();
        });
    }
    custom.append(&apply);
    root.append(&custom);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let reset = gtk::Button::with_label("Reset to default");
    {
        let weak = Rc::downgrade(app);
        let paths = paths.clone();
        let window_ref = window.clone();
        reset.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                for path in &paths {
                    app.set_folder_color(path, None);
                }
                window_ref.close();
            }
        });
    }
    buttons.append(&reset);
    let cancel = gtk::Button::with_label("Cancel");
    {
        let window_ref = window.clone();
        cancel.connect_clicked(move |_| window_ref.close());
    }
    buttons.append(&cancel);
    root.append(&buttons);
    window.set_child(Some(&root));
    window.present();
}

pub fn create_tag_dialog(app: &Rc<App>) {
    let window = dialog_window(app, "New tag", 400, 300);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header("New tag", "Tags can be assigned to files and folders anywhere."));
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Tag name"));
    root.append(&name);
    let color = Rc::new(RefCell::new(crate::theme::FOLDER_PALETTE[0].1.to_string()));
    let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for (_, hex) in crate::theme::FOLDER_PALETTE {
        let button = gtk::Button::new();
        button.add_css_class("flat");
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("color-dot");
        dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(hex)));
        dot.set_size_request(16, 16);
        button.set_child(Some(&dot));
        let hex_owned = hex.to_string();
        let color = color.clone();
        button.connect_clicked(move |_| {
            *color.borrow_mut() = hex_owned.clone();
        });
        swatches.append(&button);
    }
    root.append(&swatches);
    let icon_names: Vec<&str> = crate::metadata::TAG_ICONS.iter().map(|(_, label)| *label).collect();
    let icon_drop = gtk::DropDown::from_strings(&icon_names);
    let icon_preview = gtk::Image::from_icon_name(crate::metadata::TAG_ICONS[0].0);
    icon_preview.set_pixel_size(18);
    {
        let preview = icon_preview.clone();
        icon_drop.connect_selected_notify(move |drop| {
            if let Some((icon, _)) = crate::metadata::TAG_ICONS.get(drop.selected() as usize) {
                preview.set_icon_name(Some(icon));
            }
        });
    }
    let icon_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    icon_box.append(&icon_drop);
    icon_box.append(&icon_preview);
    root.append(&row("Icon", "Shown beside the tag in the sidebar.", &icon_box));
    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    root.append(&error);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let create = gtk::Button::with_label("Create tag");
    create.add_css_class("suggested");
    buttons.append(&create);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let window_ref = window.clone();
        let name = name.clone();
        let color = color.clone();
        let error = error.clone();
        let icon_drop = icon_drop.clone();
        create.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let icon = crate::metadata::TAG_ICONS
                .get(icon_drop.selected() as usize)
                .map(|(icon, _)| *icon)
                .unwrap_or("tag-symbolic");
            let result = app
                .meta
                .borrow_mut()
                .create_tag_with_icon(&name.text(), &color.borrow(), icon);
            match result {
                Ok(_) => {
                    window_ref.close();
                    app.refresh_after_metadata_change();
                    app.notify("Tag created", false);
                }
                Err(err) => {
                    error.set_text(&err);
                    error.set_visible(true);
                }
            }
        });
    }
    name.grab_focus();
    window.present();
}

pub fn tag_manager(app: &Rc<App>) {
    let window = dialog_window(app, "Tags", 520, 560);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Tags",
        "Rename, recolour or delete tags. Deleting a tag removes it from every item.",
    ));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    root.append(&scroller);
    let refresh: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let build = {
        let weak = Rc::downgrade(app);
        let list = list.clone();
        let refresh = refresh.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let tags = app.meta.borrow().tags.clone();
            if tags.is_empty() {
                list.append(&gtk::Label::new(Some("No tags yet.")));
            }
            for tag in tags {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row.set_margin_top(3);
                row.set_margin_bottom(3);
                let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                dot.add_css_class("color-dot");
                dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(&tag.color)));
                dot.set_size_request(12, 12);
                dot.set_valign(gtk::Align::Center);
                row.append(&dot);
                let name = gtk::Entry::new();
                name.set_text(&tag.name);
                name.set_hexpand(true);
                row.append(&name);
                let count = app.meta.borrow().paths_with_tags(&[tag.id.clone()], false).len();
                let count_label = gtk::Label::new(Some(&format!("{count} item(s)")));
                count_label.add_css_class("tiny");
                count_label.add_css_class("dim");
                row.append(&count_label);
                let save = gtk::Button::with_label("Save");
                save.add_css_class("flat");
                {
                    let weak = Rc::downgrade(&app);
                    let refresh = refresh.clone();
                    let id = tag.id.clone();
                    let name = name.clone();
                    save.connect_clicked(move |_| {
                        if let Some(app) = weak.upgrade() {
                            let result = app.meta.borrow_mut().rename_tag(&id, &name.text());
                            match result {
                                Ok(()) => {
                                    app.refresh_sidebar();
                                    app.notify("Tag renamed", false);
                                    if let Some(f) = refresh.borrow().as_ref() {
                                        f();
                                    }
                                }
                                Err(err) => app.notify(&err, true),
                            }
                        }
                    });
                }
                row.append(&save);
                let icon_btn = gtk::MenuButton::new();
                icon_btn.set_label("Icon");
                icon_btn.add_css_class("flat");
                let icon_pop = gtk::Popover::new();
                icon_pop.set_has_arrow(false);
                let icon_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
                for (icon_name, icon_label) in crate::metadata::TAG_ICONS {
                    let choice = gtk::Button::new();
                    choice.add_css_class("flat");
                    let body = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let image = gtk::Image::from_icon_name(icon_name);
                    image.set_pixel_size(16);
                    body.append(&image);
                    let label = gtk::Label::new(Some(*icon_label));
                    label.set_xalign(0.0);
                    body.append(&label);
                    choice.set_child(Some(&body));
                    {
                        let weak = Rc::downgrade(&app);
                        let refresh = refresh.clone();
                        let id = tag.id.clone();
                        let icon_name = (*icon_name).to_string();
                        let icon_pop = icon_pop.clone();
                        choice.connect_clicked(move |_| {
                            if let Some(app) = weak.upgrade() {
                                app.meta.borrow_mut().set_tag_icon(&id, &icon_name);
                                app.refresh_sidebar();
                                app.refresh_after_metadata_change();
                                icon_pop.popdown();
                                if let Some(f) = refresh.borrow().as_ref() {
                                    f();
                                }
                            }
                        });
                    }
                    icon_list.append(&choice);
                }
                icon_pop.set_child(Some(&icon_list));
                icon_btn.set_popover(Some(&icon_pop));
                row.append(&icon_btn);
                let recolour = gtk::Button::with_label("Colour");
                recolour.add_css_class("flat");
                {
                    let weak = Rc::downgrade(&app);
                    let refresh = refresh.clone();
                    let id = tag.id.clone();
                    let current = tag.color.clone();
                    recolour.connect_clicked(move |_| {
                        let Some(app) = weak.upgrade() else { return };
                        let dialog = gtk::ColorDialog::new();
                        dialog.set_with_alpha(false);
                        let weak = Rc::downgrade(&app);
                        let refresh = refresh.clone();
                        let id = id.clone();
                        dialog.choose_rgba(
                            Some(&app.window),
                            Some(&gdk_rgba(&current)),
                            None::<&gio::Cancellable>,
                            move |result| {
                                if let Ok(color) = result {
                                    if let Some(app) = weak.upgrade() {
                                        let hex = crate::util::color_to_hex(
                                            (color.red() * 255.0) as u8,
                                            (color.green() * 255.0) as u8,
                                            (color.blue() * 255.0) as u8,
                                        );
                                        app.meta.borrow_mut().recolor_tag(&id, &hex);
                                        app.refresh_sidebar();
                                        app.refresh_after_metadata_change();
                                        if let Some(f) = refresh.borrow().as_ref() {
                                            f();
                                        }
                                    }
                                }
                            },
                        );
                    });
                }
                row.append(&recolour);
                let delete = gtk::Button::with_label("Delete");
                delete.add_css_class("flat");
                delete.add_css_class("danger");
                {
                    let weak = Rc::downgrade(&app);
                    let refresh = refresh.clone();
                    let id = tag.id.clone();
                    let name_text = tag.name.clone();
                    delete.connect_clicked(move |_| {
                        let Some(app) = weak.upgrade() else { return };
                        let app_c = app.clone();
                        let refresh = refresh.clone();
                        let id = id.clone();
                        confirm(
                            &app,
                            "Delete tag?",
                            &format!(
                                "“{name_text}” will be removed from every item. Files are not deleted."
                            ),
                            "Delete tag",
                            true,
                            move |_| {
                                app_c.meta.borrow_mut().delete_tag(&id);
                                app_c.refresh_sidebar();
                                app_c.refresh_after_metadata_change();
                                if let Some(f) = refresh.borrow().as_ref() {
                                    f();
                                }
                            },
                        );
                    });
                }
                row.append(&delete);
                list.append(&row);
            }
        }
    };
    *refresh.borrow_mut() = Some(Rc::new(build.clone()));
    build();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let new_tag = gtk::Button::with_label("New tag…");
    buttons.append(&new_tag);
    let cleanup = gtk::Button::with_label("Clean up missing items");
    buttons.append(&cleanup);
    let close = gtk::Button::with_label("Close");
    close.add_css_class("suggested");
    buttons.append(&close);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let build = build.clone();
        new_tag.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                create_tag_dialog(&app);
                // Rebuild after the dialog is dismissed by the user.
                let build = build.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || build());
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        let refresh = refresh.clone();
        cleanup.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let removed = app.meta.borrow_mut().cleanup();
                app.refresh_sidebar();
                app.refresh_after_metadata_change();
                app.notify(&format!("Removed {removed} stale metadata entr(ies)"), false);
                if let Some(f) = refresh.borrow().as_ref() {
                    f();
                }
            }
        });
    }
    window.present();
}

fn gdk_rgba(value: &str) -> gtk4::gdk::RGBA {
    let (r, g, b, a) = crate::util::parse_color(value).unwrap_or((76, 141, 255, 255));
    gtk4::gdk::RGBA::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

const SECTIONS: &[(&str, &str)] = &[
    ("appearance", "Appearance"),
    ("icons", "Icons & theme"),
    ("layout", "Layout"),
    ("files", "Files"),
    ("thumbnails", "Thumbnails"),
    ("tags", "Tags & colours"),
    ("terminal", "Terminal"),
    ("commands", "Commands"),
    ("keyboard", "Keyboard"),
    ("transfers", "Transfers"),
    ("advanced", "Advanced"),
];

pub fn settings_window(app: &Rc<App>, section: Option<&str>) {
    let window = dialog_window(app, "Delve settings", 820, 620);
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let nav = gtk::ListBox::new();
    nav.set_size_request(190, -1);
    nav.add_css_class("delve-sidebar");
    nav.set_selection_mode(gtk::SelectionMode::Single);
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(80);
    let pages: Vec<(String, gtk::Widget)> = vec![
        ("appearance".into(), appearance_page(app, &window)),
        ("icons".into(), icons_page(app, &window)),
        ("layout".into(), layout_page(app)),
        ("files".into(), files_page(app)),
        ("thumbnails".into(), thumbnails_page(app)),
        ("tags".into(), tags_page(app)),
        ("terminal".into(), terminal_page(app, &window)),
        ("commands".into(), commands_page(app)),
        ("keyboard".into(), keyboard_page(app)),
        ("transfers".into(), transfers_page(app)),
        ("advanced".into(), advanced_page(app, &window)),
    ];
    for (id, label) in SECTIONS {
        let row = gtk::ListBoxRow::new();
        let text = gtk::Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_margin_top(8);
        text.set_margin_bottom(8);
        text.set_margin_start(12);
        row.set_child(Some(&text));
        row.set_widget_name(id);
        nav.append(&row);
    }
    let mut active_index = 0;
    for (i, (id, page)) in pages.into_iter().enumerate() {
        stack.add_named(&page, Some(&id));
        if Some(id.as_str()) == section {
            active_index = i;
        }
    }
    nav.connect_row_selected({
        let stack = stack.clone();
        move |_, row| {
            if let Some(row) = row {
                stack.set_visible_child_name(&row.widget_name());
            }
        }
    });
    if let Some(row) = nav.row_at_index(active_index as i32) {
        nav.select_row(Some(&row));
    }
    let nav_scroller = gtk::ScrolledWindow::builder()
        .child(&nav)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    nav_scroller.set_size_request(190, -1);
    root.append(&nav_scroller);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_hexpand(true);
    stack.set_vexpand(true);
    content.append(&stack);
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_top(8);
    footer.set_margin_bottom(10);
    footer.set_margin_start(16);
    footer.set_margin_end(16);
    let hint = gtk::Label::new(Some(
        "Settings are stored under the XDG config directory and apply immediately.",
    ));
    hint.add_css_class("tiny");
    hint.add_css_class("dim");
    hint.set_hexpand(true);
    hint.set_xalign(0.0);
    footer.append(&hint);
    let close = gtk::Button::with_label("Close");
    close.add_css_class("suggested");
    footer.append(&close);
    content.append(&footer);
    root.append(&content);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }
    window.present();
}

fn page_container(title: &str, subtitle: &str) -> (gtk::ScrolledWindow, gtk::Box) {
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 8);
    inner.set_margin_top(18);
    inner.set_margin_bottom(18);
    inner.set_margin_start(20);
    inner.set_margin_end(20);
    inner.append(&header(title, subtitle));
    let scroller = gtk::ScrolledWindow::builder()
        .child(&inner)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    (scroller, inner)
}

fn switch_row(label: &str, description: &str, value: bool, on_toggle: impl Fn(bool) + 'static) -> gtk::Box {
    let switch = gtk::Switch::new();
    switch.set_active(value);
    switch.set_valign(gtk::Align::Center);
    switch.connect_state_set(move |_, state| {
        on_toggle(state);
        glib::Propagation::Proceed
    });
    row(label, description, &switch)
}

fn combo_row(
    label: &str,
    description: &str,
    options: &[&str],
    selected: usize,
    on_change: impl Fn(usize) + 'static,
) -> gtk::Box {
    let drop = gtk::DropDown::from_strings(options);
    drop.set_selected(selected as u32);
    drop.connect_selected_notify(move |d| on_change(d.selected() as usize));
    row(label, description, &drop)
}

fn save(app: &App) {
    app.settings.borrow().save();
}

fn appearance_page(app: &Rc<App>, window: &gtk::Window) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Appearance",
        "Delve owns its design tokens. Colours, radii and fonts come from theme files under the XDG config directory.",
    );
    // Theme chooser
    let mut ids: Vec<String> = crate::theme::BUILTIN_THEMES
        .iter()
        .map(|(id, _, _)| format!("builtin:{id}"))
        .collect();
    let custom = crate::theme::list_custom_themes();
    for name in &custom {
        ids.push(format!("custom:{name}"));
    }
    let mut labels: Vec<String> = crate::theme::BUILTIN_THEMES
        .iter()
        .map(|(_, name, _)| (*name).to_string())
        .collect();
    for name in &custom {
        labels.push(format!("{name} (custom)"));
    }
    let current_theme = app.settings.borrow().theme.clone();
    let selected = ids.iter().position(|id| id == &current_theme).unwrap_or(0);
    let theme_row = combo_row(
        "Theme",
        "Applies live without restarting or losing your tabs.",
        &labels.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        selected,
        {
            let weak = Rc::downgrade(app);
            let ids = ids.clone();
            move |index| {
                if let Some(app) = weak.upgrade() {
                    let id = ids.get(index).cloned().unwrap_or_else(|| "builtin:dark".into());
                    app.settings.borrow_mut().theme = id;
                    save(&app);
                    app.reload_theme();
                    app.install_theme_monitor_on_change();
                }
            }
        },
    );
    inner.append(&theme_row);
    inner.append(&switch_row(
        "Live reload",
        "Watch the active theme file and re-apply it when it changes.",
        app.settings.borrow().theme_live_reload,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().theme_live_reload = on;
                    save(&app);
                    app.install_theme_monitor_on_change();
                }
            }
        },
    ));
    inner.append(&section_label("DENSITY AND TYPE"));
    let density = app.settings.borrow().density;
    inner.append(&combo_row(
        "Density",
        "Controls row and icon padding across every view.",
        &Density::all().map(|d| d.label()),
        Density::all().iter().position(|d| *d == density).unwrap_or(1),
        {
            let weak = Rc::downgrade(app);
            move |index| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().density = Density::all()[index.min(2)];
                    save(&app);
                    app.apply_density();
                }
            }
        },
    ));
    let font_entry = gtk::Entry::new();
    font_entry.set_text(&app.settings.borrow().ui_font);
    font_entry.set_placeholder_text(Some("Theme default"));
    {
        let weak = Rc::downgrade(app);
        font_entry.connect_changed(move |e| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().ui_font = e.text().to_string();
                save(&app);
                let settings = app.settings.borrow().clone();
                app.theme.apply_overrides(&settings);
            }
        });
    }
    inner.append(&row("UI font", "Leave empty to use the theme font.", &font_entry));
    let mono_entry = gtk::Entry::new();
    mono_entry.set_text(&app.settings.borrow().mono_font);
    mono_entry.set_placeholder_text(Some("monospace"));
    {
        let weak = Rc::downgrade(app);
        mono_entry.connect_changed(move |e| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().mono_font = e.text().to_string();
                save(&app);
                let settings = app.settings.borrow().clone();
                app.theme.apply_overrides(&settings);
            }
        });
    }
    inner.append(&row("Monospace font", "Used for previews and paths.", &mono_entry));
    inner.append(&section_label("WINDOW"));
    let opacity = gtk::Scale::with_range(gtk::Orientation::Horizontal, 35.0, 100.0, 1.0);
    opacity.set_digits(0);
    opacity.set_value((app.settings.borrow().window_opacity * 100.0).round());
    opacity.set_size_request(220, -1);
    opacity.set_draw_value(true);
    opacity.set_value_pos(gtk::PositionType::Right);
    {
        let weak = Rc::downgrade(app);
        opacity.connect_value_changed(move |scale| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().window_opacity = (scale.value() / 100.0).clamp(0.35, 1.0);
                save(&app);
                app.apply_window_opacity();
            }
        });
    }
    inner.append(&row(
        "Background opacity",
        "Only window and panel backgrounds fade. Text, icons and hover/selection states stay fully opaque.",
        &opacity,
    ));
    inner.append(&switch_row(
        "Translucent panels",
        "Uses alpha on panels. Turn off for fully opaque surfaces.",
        app.settings.borrow().transparency,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().transparency = on;
                    save(&app);
                    let settings = app.settings.borrow().clone();
                    app.theme.apply_overrides(&settings);
                }
            }
        },
    ));
    inner.append(&section_label("ACCENT"));
    let accent_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let accent_entry = gtk::Entry::new();
    accent_entry.set_text(&app.settings.borrow().accent);
    accent_entry.set_placeholder_text(Some("Theme accent, e.g. #4c8dff"));
    accent_entry.set_hexpand(true);
    {
        let weak = Rc::downgrade(app);
        accent_entry.connect_changed(move |e| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().accent = e.text().to_string();
                save(&app);
                let settings = app.settings.borrow().clone();
                app.theme.apply_overrides(&settings);
            }
        });
    }
    accent_row.append(&accent_entry);
    let pick = gtk::Button::with_label("Pick…");
    {
        let weak = Rc::downgrade(app);
        let parent = window.clone();
        let accent_entry_for_pick = accent_entry.clone();
        pick.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let dialog = gtk::ColorDialog::new();
            dialog.set_with_alpha(false);
            let weak = Rc::downgrade(&app);
            let accent_entry = accent_entry_for_pick.clone();
            dialog.choose_rgba(Some(&parent), None, None::<&gio::Cancellable>, move |result| {
                if let Ok(color) = result {
                    if let Some(app) = weak.upgrade() {
                        let hex = crate::util::color_to_hex(
                            (color.red() * 255.0) as u8,
                            (color.green() * 255.0) as u8,
                            (color.blue() * 255.0) as u8,
                        );
                        accent_entry.set_text(&hex);
                        app.settings.borrow_mut().accent = hex;
                        save(&app);
                        let settings = app.settings.borrow().clone();
                        app.theme.apply_overrides(&settings);
                    }
                }
            });
        });
    }
    accent_row.append(&pick);
    let clear = gtk::Button::with_label("Clear");
    {
        let weak = Rc::downgrade(app);
        let entry_for_clear = accent_entry.clone();
        clear.connect_clicked(move |_| {
            entry_for_clear.set_text("");
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().accent.clear();
                save(&app);
                let settings = app.settings.borrow().clone();
                app.theme.apply_overrides(&settings);
            }
        });
    }
    accent_row.append(&clear);
    inner.append(&row("Accent colour", "Overrides the accent token for the active theme.", &accent_row));
    inner.append(&section_label("THEME FILES"));
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let edit_copy = gtk::Button::with_label("Create editable copy");
    {
        let weak = Rc::downgrade(app);
        edit_copy.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let builtin = app
                    .settings
                    .borrow()
                    .theme
                    .strip_prefix("builtin:")
                    .unwrap_or("dark")
                    .to_string();
                match crate::theme::create_editable_theme_copy(&builtin) {
                    Ok(id) => {
                        app.settings.borrow_mut().theme = id.clone();
                        save(&app);
                        app.reload_theme();
                        app.notify(
                            &format!("Editable theme written to {}", crate::paths::themes_dir().join(id.trim_start_matches("custom:")).display()),
                            false,
                        );
                    }
                    Err(err) => app.notify(&err, true),
                }
            }
        });
    }
    actions.append(&edit_copy);
    let open_folder = gtk::Button::with_label("Open themes folder");
    {
        let weak = Rc::downgrade(app);
        open_folder.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let _ = std::fs::create_dir_all(crate::paths::themes_dir());
                let file = gio::File::for_path(crate::paths::themes_dir());
                let _ = gio::AppInfo::launch_default_for_uri(&file.uri(), None::<&gio::AppLaunchContext>);
                let _ = &app;
            }
        });
    }
    actions.append(&open_folder);
    let reload = gtk::Button::with_label("Reload theme");
    {
        let weak = Rc::downgrade(app);
        reload.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.reload_theme();
            }
        });
    }
    actions.append(&reload);
    let reset = gtk::Button::with_label("Reset to built-in");
    {
        let weak = Rc::downgrade(app);
        reset.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().theme = "builtin:dark".into();
                save(&app);
                app.reload_theme();
                app.notify("Reset to the built-in dark theme", false);
            }
        });
    }
    actions.append(&reset);
    inner.append(&actions);
    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_wrap(true);
    if let Some(err) = app.theme.last_error.borrow().clone() {
        status.set_text(&format!("Theme warning: {err}"));
        status.add_css_class("danger");
    } else {
        status.set_text(&format!(
            "Active theme: {}\nThemes folder: {}",
            app.theme.current.borrow().id,
            crate::paths::themes_dir().display()
        ));
        status.add_css_class("tiny");
        status.add_css_class("dim");
    }
    inner.append(&status);
    scroller.upcast()
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn icon_preview_slot() -> gtk::Box {
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.set_size_request(30, 30);
    slot.set_halign(gtk::Align::Center);
    slot.set_valign(gtk::Align::Center);
    slot.add_css_class("icon-preview");
    slot
}

fn update_icon_preview(slot: &gtk::Box, source: Option<&str>, size: i32) {
    clear_box(slot);
    let image = source
        .and_then(|value| crate::theme::custom_icon_image(value, size))
        .unwrap_or_else(|| gtk::Image::from_icon_name("image-x-generic-symbolic"));
    image.set_pixel_size(size);
    if source.is_none() {
        image.set_opacity(0.45);
    }
    slot.append(&image);
}

/// Shared editor for icon sources. It accepts a themed icon name, an already
/// uploaded file source, or a new image upload copied into Delve's data folder.
fn icon_source_dialog(
    app: &Rc<App>,
    parent: &gtk::Window,
    title: &str,
    initial: &str,
    on_apply: Rc<dyn Fn(Option<String>)>,
) {
    let window = dialog_window(app, title, 500, 330);
    window.set_transient_for(Some(parent));
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(16);
    root.set_margin_bottom(14);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.append(&header(
        title,
        "Use an icon-theme name, or upload a PNG, JPEG, WebP or GIF.",
    ));

    let preview = icon_preview_slot();
    update_icon_preview(&preview, (!initial.is_empty()).then_some(initial), 52);
    let preview_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    preview_row.append(&preview);
    let initial_label = if initial.is_empty() {
        "No icon selected".to_string()
    } else {
        crate::theme::icon_source_label(initial)
    };
    let preview_text = gtk::Label::new(Some(&initial_label));
    preview_text.add_css_class("tiny");
    preview_text.add_css_class("dim");
    preview_text.set_xalign(0.0);
    preview_text.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    preview_text.set_hexpand(true);
    preview_row.append(&preview_text);
    root.append(&preview_row);

    let entry = gtk::Entry::new();
    entry.set_text(initial);
    entry.set_placeholder_text(Some("e.g. folder-star-symbolic or file:/path/icon.png"));
    entry.set_hexpand(true);
    root.append(&row("Icon source", "Icon names follow the active icon theme.", &entry));

    let upload = gtk::Button::with_label("Upload image…");
    upload.set_halign(gtk::Align::Start);
    let status = gtk::Label::new(None);
    status.add_css_class("tiny");
    status.add_css_class("dim");
    status.set_xalign(0.0);
    status.set_wrap(true);
    {
        let weak = Rc::downgrade(app);
        let window_c = window.clone();
        let entry_c = entry.clone();
        let preview_c = preview.clone();
        let preview_text_c = preview_text.clone();
        let status_c = status.clone();
        upload.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let chooser = gtk::FileDialog::new();
            chooser.set_title("Choose an icon image");
            let entry_c = entry_c.clone();
            let preview_c = preview_c.clone();
            let preview_text_c = preview_text_c.clone();
            let status_c = status_c.clone();
            chooser.open(
                Some(&window_c),
                None::<&gio::Cancellable>,
                move |result| {
                    let Ok(file) = result else { return };
                    let Some(path) = file.path() else { return };
                    match crate::theme::import_icon_file(&path) {
                        Ok(source) => {
                            entry_c.set_text(&source);
                            update_icon_preview(&preview_c, Some(&source), 52);
                            preview_text_c.set_text(&crate::theme::icon_source_label(&source));
                            status_c.remove_css_class("danger");
                            status_c.set_text(&format!(
                                "Copied into {}",
                                crate::paths::icons_dir().display()
                            ));
                        }
                        Err(err) => {
                            status_c.add_css_class("danger");
                            status_c.set_text(&err);
                        }
                    }
                    let _ = &app;
                },
            );
        });
    }
    root.append(&upload);
    root.append(&status);

    {
        let preview = preview.clone();
        let preview_text = preview_text.clone();
        entry.connect_changed(move |entry| {
            let source = entry.text().trim().to_string();
            update_icon_preview(&preview, (!source.is_empty()).then_some(source.as_str()), 52);
            let label = if source.is_empty() {
                "No icon selected".to_string()
            } else {
                crate::theme::icon_source_label(&source)
            };
            preview_text.set_text(&label);
        });
    }

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(4);
    let remove = gtk::Button::with_label("Remove override");
    let cancel = gtk::Button::with_label("Cancel");
    let apply = gtk::Button::with_label("Apply");
    apply.add_css_class("suggested");
    buttons.append(&remove);
    buttons.append(&cancel);
    buttons.append(&apply);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let status = status.clone();
        let on_apply = on_apply.clone();
        apply.connect_clicked(move |_| {
            let source = entry.text().trim().to_string();
            if source.is_empty() {
                return;
            }
            if crate::theme::custom_icon_image(&source, 24).is_none() {
                status.add_css_class("danger");
                status.set_text(
                    "That icon name was not found in the active icon theme, or the image path is missing.",
                );
                return;
            }
            on_apply(Some(source));
            window.close();
        });
    }
    {
        let window = window.clone();
        remove.connect_clicked(move |_| {
            on_apply(None);
            window.close();
        });
    }
    window.present();
}

fn file_icon_rule_rows(app: &Rc<App>, parent: &gtk::Window, container: &gtk::Box) {
    clear_box(container);
    let rules: Vec<(String, String)> = app
        .settings
        .borrow()
        .custom_file_icons
        .iter()
        .map(|(key, source)| (key.clone(), source.clone()))
        .collect();
    if rules.is_empty() {
        let empty = gtk::Label::new(Some(
            "No file-type overrides yet. Add one below, for example “pdf” or “image/*”.",
        ));
        empty.add_css_class("tiny");
        empty.add_css_class("dim");
        empty.set_xalign(0.0);
        empty.set_wrap(true);
        container.append(&empty);
        return;
    }
    for (key, source) in rules {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_top(3);
        row.set_margin_bottom(3);
        let preview = icon_preview_slot();
        update_icon_preview(&preview, Some(&source), 24);
        row.append(&preview);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(&key));
        name.add_css_class("meta-value");
        name.set_xalign(0.0);
        texts.append(&name);
        let detail = gtk::Label::new(Some(&crate::theme::icon_source_label(&source)));
        detail.add_css_class("tiny");
        detail.add_css_class("dim");
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        texts.append(&detail);
        row.append(&texts);
        let edit = gtk::Button::with_label("Change…");
        row.append(&edit);
        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some("Remove this icon override"));
        row.append(&remove);
        container.append(&row);

        {
            let weak = Rc::downgrade(app);
            let key = key.clone();
            let container = container.clone();
            let parent = parent.clone();
            edit.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                let initial = app
                    .settings
                    .borrow()
                    .custom_file_icons
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                let weak = Rc::downgrade(&app);
                let key = key.clone();
                let container = container.clone();
                let parent = parent.clone();
                let parent_for_dialog = parent.clone();
                icon_source_dialog(
                    &app,
                    &parent_for_dialog,
                    &format!("Icon for {key}"),
                    &initial,
                    Rc::new(move |source: Option<String>| {
                        let Some(app) = weak.upgrade() else { return };
                        match source {
                            Some(source) => {
                                app.settings
                                    .borrow_mut()
                                    .custom_file_icons
                                    .insert(key.clone(), source);
                            }
                            None => {
                                app.settings.borrow_mut().custom_file_icons.remove(&key);
                            }
                        }
                        app.settings.borrow().save();
                        app.refresh_after_icon_change();
                        file_icon_rule_rows(&app, &parent, &container);
                    }),
                );
            });
        }
        {
            let weak = Rc::downgrade(app);
            let key = key.clone();
            let container = container.clone();
            let parent = parent.clone();
            remove.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                app.settings.borrow_mut().custom_file_icons.remove(&key);
                app.settings.borrow().save();
                app.refresh_after_icon_change();
                file_icon_rule_rows(&app, &parent, &container);
            });
        }
    }
}

fn folder_icon_rule_rows(app: &Rc<App>, parent: &gtk::Window, container: &gtk::Box) {
    clear_box(container);
    let mut entries: Vec<(String, String, Option<String>)> = vec![
        ("default".to_string(), "All folders".to_string(), None),
    ];
    for (name, hex) in crate::theme::FOLDER_PALETTE {
        if let Some(key) = crate::theme::normalize_color(hex) {
            entries.push((key.clone(), (*name).to_string(), Some(key)));
        }
    }
    let meta_colors: Vec<String> = {
        let meta = app.meta.borrow();
        let mut colors: Vec<String> = meta
            .items
            .values()
            .filter_map(|item| item.folder_color.as_deref())
            .filter_map(crate::theme::normalize_color)
            .collect();
        colors.sort();
        colors.dedup();
        colors
    };
    for color in meta_colors {
        if !entries.iter().any(|(key, _, _)| key == &color) {
            entries.push((color.clone(), format!("Custom {color}"), Some(color)));
        }
    }
    let extra: Vec<String> = app
        .settings
        .borrow()
        .custom_folder_icons
        .keys()
        .filter(|key| *key != "default")
        .cloned()
        .collect();
    for color in extra {
        if !entries.iter().any(|(key, _, _)| key == &color) {
            entries.push((color.clone(), format!("Colour {color}"), Some(color)));
        }
    }

    let rules = app.settings.borrow().custom_folder_icons.clone();
    for (key, label, swatch) in entries {
        let source = rules.get(&key).cloned();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_top(3);
        row.set_margin_bottom(3);
        if let Some(color) = &swatch {
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("color-dot");
            dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(color)));
            dot.set_size_request(12, 12);
            dot.set_valign(gtk::Align::Center);
            row.append(&dot);
        }
        let preview = icon_preview_slot();
        update_icon_preview(&preview, source.as_deref(), 24);
        row.append(&preview);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(&label));
        name.add_css_class("meta-value");
        name.set_xalign(0.0);
        texts.append(&name);
        let detail = gtk::Label::new(Some(
            source
                .as_deref()
                .map(crate::theme::icon_source_label)
                .unwrap_or_else(|| "Theme default icon".to_string())
                .as_str(),
        ));
        detail.add_css_class("tiny");
        detail.add_css_class("dim");
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        texts.append(&detail);
        row.append(&texts);
        let edit = gtk::Button::with_label(if source.is_some() { "Change…" } else { "Set icon…" });
        row.append(&edit);
        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some("Clear this folder icon override"));
        remove.set_sensitive(source.is_some());
        row.append(&remove);
        container.append(&row);

        {
            let weak = Rc::downgrade(app);
            let key = key.clone();
            let label = label.clone();
            let initial = source.clone().unwrap_or_default();
            let container = container.clone();
            let parent = parent.clone();
            edit.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                let weak = Rc::downgrade(&app);
                let key = key.clone();
                let container = container.clone();
                let parent = parent.clone();
                let parent_for_dialog = parent.clone();
                icon_source_dialog(
                    &app,
                    &parent_for_dialog,
                    &format!("Folder icon — {label}"),
                    &initial,
                    Rc::new(move |source: Option<String>| {
                        let Some(app) = weak.upgrade() else { return };
                        match source {
                            Some(source) => {
                                app.settings
                                    .borrow_mut()
                                    .custom_folder_icons
                                    .insert(key.clone(), source);
                            }
                            None => {
                                app.settings.borrow_mut().custom_folder_icons.remove(&key);
                            }
                        }
                        app.settings.borrow().save();
                        app.refresh_after_icon_change();
                        folder_icon_rule_rows(&app, &parent, &container);
                    }),
                );
            });
        }
        {
            let weak = Rc::downgrade(app);
            let key = key.clone();
            let container = container.clone();
            let parent = parent.clone();
            remove.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                app.settings.borrow_mut().custom_folder_icons.remove(&key);
                app.settings.borrow().save();
                app.refresh_after_icon_change();
                folder_icon_rule_rows(&app, &parent, &container);
            });
        }
    }
}

fn icons_page(app: &Rc<App>, window: &gtk::Window) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Icons & theme",
        "Icon theme selection, per-type overrides and folder-colour icons. Changes apply live to every open view.",
    );
    let themes = crate::theme::detect_icon_themes();
    let mut labels = vec!["System default".to_string()];
    labels.extend(themes.iter().cloned());
    let current = app.settings.borrow().icon_theme.clone();
    let selected = if current.is_empty() {
        0
    } else {
        themes.iter().position(|t| t == &current).map(|i| i + 1).unwrap_or(0)
    };
    let theme_row = combo_row(
        "Icon theme",
        "Detected from the standard icon directories.",
        &labels.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        selected,
        {
            let weak = Rc::downgrade(app);
            let themes = themes.clone();
            move |index| {
                if let Some(app) = weak.upgrade() {
                    let name = if index == 0 {
                        String::new()
                    } else {
                        themes.get(index - 1).cloned().unwrap_or_default()
                    };
                    app.settings.borrow_mut().icon_theme = name.clone();
                    save(&app);
                    crate::theme::apply_icon_theme(&name);
                }
            }
        },
    );
    inner.append(&theme_row);
    let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 32.0, 128.0, 4.0);
    size.set_value(app.settings.borrow().icon_size as f64);
    size.set_size_request(200, -1);
    {
        let weak = Rc::downgrade(app);
        size.connect_value_changed(move |s| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().icon_size = s.value() as i32;
                save(&app);
                let weak = weak.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
                    if let Some(app) = weak.upgrade() {
                        if let Some(tab) = app.active_tab() {
                            if tab.state.borrow().view == ViewKind::Icon {
                                app.sync_views(tab.id);
                            }
                        }
                    }
                });
            }
        });
    }
    inner.append(&row("Icon size", "Affects the icon view grid.", &size));
    inner.append(&switch_row(
        "Tag colour dots",
        "Show a small colour dot for tagged items in icon and list views.",
        app.settings.borrow().show_tag_dots,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().show_tag_dots = on;
                    save(&app);
                    if let Some(tab) = app.active_tab() {
                        app.sync_views(tab.id);
                    }
                }
            }
        },
    ));

    inner.append(&section_label("CUSTOM FILE TYPE ICONS"));
    let file_hint = gtk::Label::new(Some(
        "Enter an extension (pdf), a MIME type (application/pdf), or a MIME wildcard (image/*). \
         Custom icons replace thumbnails for overridden types.",
    ));
    file_hint.add_css_class("tiny");
    file_hint.add_css_class("dim");
    file_hint.set_xalign(0.0);
    file_hint.set_wrap(true);
    inner.append(&file_hint);
    let file_add_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let file_key_entry = gtk::Entry::new();
    file_key_entry.set_placeholder_text(Some("File type, e.g. pdf or image/*"));
    file_key_entry.set_hexpand(true);
    file_add_row.append(&file_key_entry);
    let file_add = gtk::Button::with_label("Add rule…");
    file_add_row.append(&file_add);
    inner.append(&file_add_row);
    let file_rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    file_icon_rule_rows(app, window, &file_rows);
    inner.append(&file_rows);
    {
        let weak = Rc::downgrade(app);
        let entry = file_key_entry.clone();
        let rows = file_rows.clone();
        let parent = window.clone();
        file_add.connect_clicked(move |_| {
            let key = entry.text().trim().to_ascii_lowercase();
            if key.is_empty() {
                if let Some(app) = weak.upgrade() {
                    app.notify("Enter a file extension or MIME type first.", true);
                }
                return;
            }
            let Some(app) = weak.upgrade() else { return };
            let initial = app
                .settings
                .borrow()
                .custom_file_icons
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let weak = Rc::downgrade(&app);
            let entry = entry.clone();
            let key = key.clone();
            let rows = rows.clone();
            let parent = parent.clone();
            let parent_for_dialog = parent.clone();
            icon_source_dialog(
                &app,
                &parent_for_dialog,
                &format!("Icon for {key}"),
                &initial,
                Rc::new(move |source: Option<String>| {
                    let Some(app) = weak.upgrade() else { return };
                    match source {
                        Some(source) => {
                            app.settings
                                .borrow_mut()
                                .custom_file_icons
                                .insert(key.clone(), source);
                        }
                        None => {
                            app.settings.borrow_mut().custom_file_icons.remove(&key);
                        }
                    }
                    app.settings.borrow().save();
                    app.refresh_after_icon_change();
                    entry.set_text("");
                    file_icon_rule_rows(&app, &parent, &rows);
                }),
            );
        });
    }

    inner.append(&section_label("CUSTOM FOLDER ICONS"));
    let folder_hint = gtk::Label::new(Some(
        "Folder icons follow the colour assigned to the folder. “All folders” is the
         fallback for folders without a colour-specific icon.",
    ));
    folder_hint.add_css_class("tiny");
    folder_hint.add_css_class("dim");
    folder_hint.set_xalign(0.0);
    folder_hint.set_wrap(true);
    inner.append(&folder_hint);
    let folder_rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    folder_icon_rule_rows(app, window, &folder_rows);
    inner.append(&folder_rows);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let open_icons = gtk::Button::with_label("Open icons folder");
    {
        let weak = Rc::downgrade(app);
        open_icons.connect_clicked(move |_| {
            if weak.upgrade().is_some() {
                let _ = std::fs::create_dir_all(crate::paths::icons_dir());
                let file = gio::File::for_path(crate::paths::icons_dir());
                let _ = gio::AppInfo::launch_default_for_uri(
                    &file.uri(),
                    None::<&gio::AppLaunchContext>,
                );
            }
        });
    }
    actions.append(&open_icons);
    inner.append(&actions);
    let location = gtk::Label::new(Some(&format!(
        "Uploaded icons are stored in {}",
        crate::paths::icons_dir().display()
    )));
    location.add_css_class("tiny");
    location.add_css_class("dim");
    location.set_xalign(0.0);
    location.set_wrap(true);
    inner.append(&location);
    scroller.upcast()
}

fn layout_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Layout",
        "Panels are remembered across restarts. Below 900px wide they collapse into drawers.",
    );
    inner.append(&switch_row(
        "Compact sidebar",
        "Show the sidebar as an icon-only rail instead of the full panel.",
        app.settings.borrow().sidebar_mini,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.set_sidebar_mini(on);
                }
            }
        },
    ));
    inner.append(&switch_row(
        "Show inspector",
        "Preview and metadata for the selection.",
        app.settings.borrow().inspector_visible,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.set_inspector_visible(on);
                }
            }
        },
    ));
    let default_view = app.settings.borrow().default_view;
    inner.append(&combo_row(
        "Default view",
        "Used for new tabs.",
        &ViewKind::all().map(|v| v.label()),
        ViewKind::all().iter().position(|v| *v == default_view).unwrap_or(0),
        {
            let weak = Rc::downgrade(app);
            move |index| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().default_view = ViewKind::all()[index.min(4)];
                    save(&app);
                }
            }
        },
    ));
    let sidebar_width = gtk::SpinButton::with_range(150.0, 520.0, 10.0);
    sidebar_width.set_value(app.settings.borrow().sidebar_width as f64);
    {
        let weak = Rc::downgrade(app);
        sidebar_width.connect_value_changed(move |s| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().sidebar_width = s.value() as i32;
                save(&app);
                app.ui().outer_paned.set_position(s.value() as i32);
            }
        });
    }
    inner.append(&row("Sidebar width", "Pixels.", &sidebar_width));
    let inspector_width = gtk::SpinButton::with_range(200.0, 620.0, 10.0);
    inspector_width.set_value(app.settings.borrow().inspector_width as f64);
    {
        let weak = Rc::downgrade(app);
        inspector_width.connect_value_changed(move |s| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().inspector_width = s.value() as i32;
                save(&app);
                let ui = app.ui();
                let width = ui.inner_paned.width();
                if width > 0 {
                    ui.inner_paned.set_position(width - s.value() as i32);
                }
            }
        });
    }
    inner.append(&row("Inspector width", "Pixels.", &inspector_width));
    inner.append(&switch_row(
        "Reopen closed tabs from the tab bar",
        "Right-click a tab for Duplicate, Close others and Reopen.",
        true,
        |_| {},
    ));
    scroller.upcast()
}

fn files_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container("Files", "Browsing, sorting and conflict behaviour.");
    inner.append(&switch_row(
        "Show hidden files",
        "Files whose name starts with a dot.",
        app.settings.borrow().show_hidden,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().show_hidden = on;
                    save(&app);
                    if let Some(tab) = app.active_tab() {
                        tab.state.borrow_mut().show_hidden = on;
                        app.sync_views(tab.id);
                        app.update_status();
                    }
                }
            }
        },
    ));
    inner.append(&switch_row(
        "Folders first",
        "Group folders before files in every view.",
        app.settings.borrow().folders_first,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().folders_first = on;
                    save(&app);
                    if let Some(tab) = app.active_tab() {
                        let sort = tab.state.borrow().sort;
                        app.set_sort(tab.id, sort.field, sort.ascending, on);
                    }
                }
            }
        },
    ));
    let sort = app.settings.borrow().default_sort;
    inner.append(&combo_row(
        "Default sort",
        "For new tabs.",
        &SortField::all().map(|f| f.label()),
        SortField::all().iter().position(|f| *f == sort).unwrap_or(0),
        {
            let weak = Rc::downgrade(app);
            move |index| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().default_sort = SortField::all()[index.min(3)];
                    save(&app);
                }
            }
        },
    ));
    inner.append(&switch_row(
        "Sort ascending",
        "Default direction for new tabs.",
        app.settings.borrow().sort_ascending,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().sort_ascending = on;
                    save(&app);
                }
            }
        },
    ));
    inner.append(&switch_row(
        "Show exact byte sizes",
        "Off shows human-readable IEC units.",
        app.settings.borrow().byte_sizes,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().byte_sizes = on;
                    save(&app);
                    if let Some(tab) = app.active_tab() {
                        app.sync_views(tab.id);
                    }
                }
            }
        },
    ));
    let policy = app.settings.borrow().conflict;
    inner.append(&combo_row(
        "File conflicts",
        "Applied when copy, move or extraction meets an existing item.",
        &ConflictPolicy::all().map(|c| c.label()),
        ConflictPolicy::all().iter().position(|c| *c == policy).unwrap_or(0),
        {
            let weak = Rc::downgrade(app);
            move |index| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().conflict = ConflictPolicy::all()[index.min(3)];
                    save(&app);
                }
            }
        },
    ));
    scroller.upcast()
}

fn thumbnails_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Thumbnails",
        "Thumbnails are generated off the UI thread and cached under the XDG cache directory.",
    );
    inner.append(&switch_row(
        "Enable thumbnails",
        "Images and, when a backend is installed, videos.",
        app.settings.borrow().thumbnails,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().thumbnails = on;
                    save(&app);
                    if let Some(tab) = app.active_tab() {
                        app.sync_views(tab.id);
                    }
                }
            }
        },
    ));
    let sizes = ["96 px", "128 px", "160 px", "256 px", "320 px"];
    let options = [96, 128, 160, 256, 320];
    let current = app.settings.borrow().thumbnail_size;
    inner.append(&combo_row(
        "Thumbnail size",
        "Larger thumbnails take longer to generate.",
        &sizes,
        options.iter().position(|s| *s == current).unwrap_or(2),
        {
            let weak = Rc::downgrade(app);
            move |index| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().thumbnail_size = options[index.min(4)];
                    save(&app);
                }
            }
        },
    ));
    inner.append(&switch_row(
        "Text previews",
        "Shows the first part of text files in the inspector. Content is never executed.",
        app.settings.borrow().text_preview,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().text_preview = on;
                    save(&app);
                    if let Some(tab) = app.active_tab() {
                        app.ui().inspector_widget.update(&app, tab.id);
                    }
                }
            }
        },
    ));
    let info = gtk::Label::new(Some(&format!(
        "Cache: {}\n{} in {} file(s)",
        crate::paths::thumbnails_dir().display(),
        fs::human_size(crate::thumbnails::cache_size()),
        std::fs::read_dir(crate::paths::thumbnails_dir())
            .map(|rd| rd.count())
            .unwrap_or(0)
    )));
    info.set_xalign(0.0);
    info.set_wrap(true);
    info.add_css_class("tiny");
    info.add_css_class("dim");
    inner.append(&info);
    let clear = gtk::Button::with_label("Clear thumbnail cache");
    {
        let weak = Rc::downgrade(app);
        clear.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let removed = crate::thumbnails::clear_cache();
                app.notify(&format!("Removed {removed} cached thumbnail(s)"), false);
            }
        });
    }
    inner.append(&clear);
    scroller.upcast()
}

fn tags_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Tags & colours",
        "Tags and folder colours are stored in Delve's metadata database using stable file identity.",
    );
    let manage = gtk::Button::with_label("Manage tags…");
    {
        let weak = Rc::downgrade(app);
        manage.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                tag_manager(&app);
            }
        });
    }
    inner.append(&row("Tags", "Create, rename, recolour and delete tags.", &manage));
    let palette = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for (_, hex) in crate::theme::FOLDER_PALETTE {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("color-dot");
        dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(hex)));
        dot.set_size_request(18, 18);
        dot.set_tooltip_text(Some(hex));
        palette.append(&dot);
    }
    inner.append(&row("Folder palette", "Available from any folder's context menu.", &palette));
    let clean = gtk::Button::with_label("Clean up missing items");
    {
        let weak = Rc::downgrade(app);
        clean.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let removed = app.meta.borrow_mut().cleanup();
                app.refresh_sidebar();
                app.refresh_after_metadata_change();
                app.notify(&format!("Removed {removed} stale entr(ies)"), false);
            }
        });
    }
    inner.append(&row("Metadata", "Drop records for files that no longer exist.", &clean));
    scroller.upcast()
}

fn terminal_page(app: &Rc<App>, _window: &gtk::Window) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Terminal",
        "Delve never assumes a terminal. Use {dir} in the template to place the working directory.",
    );
    let detected = crate::terminal::detect_terminals();
    let names: Vec<String> = detected.iter().map(|t| t.name.clone()).collect();
    let detected_label = gtk::Label::new(Some(&if names.is_empty() {
        "No terminal emulator detected on PATH.".to_string()
    } else {
        format!("Detected: {}", names.join(", "))
    }));
    detected_label.set_wrap(true);
    detected_label.set_xalign(0.0);
    detected_label.add_css_class("hint");
    inner.append(&detected_label);
    let template = gtk::Entry::new();
    template.set_text(&app.settings.borrow().terminal);
    template.set_placeholder_text(Some("Auto-detected; e.g. kitty --directory={dir}"));
    {
        let weak = Rc::downgrade(app);
        template.connect_changed(move |e| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().terminal = e.text().to_string();
                save(&app);
            }
        });
    }
    inner.append(&row(
        "Terminal command",
        "Arguments are passed directly; no shell is involved.",
        &template,
    ));
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let test = gtk::Button::with_label("Test launch");
    {
        let weak = Rc::downgrade(app);
        test.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let settings = app.settings.borrow().clone();
                let dir = crate::paths::home();
                match crate::terminal::build_command(&settings, &dir) {
                    Ok(mut cmd) => match cmd.spawn() {
                        Ok(_) => app.notify("Terminal launched", false),
                        Err(err) => app.notify(&format!("Could not launch: {err}"), true),
                    },
                    Err(err) => app.notify(&err, true),
                }
            }
        });
    }
    buttons.append(&test);
    let use_default = gtk::Button::with_label("Use auto-detected");
    {
        let weak = Rc::downgrade(app);
        use_default.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.settings.borrow_mut().terminal.clear();
                save(&app);
                template.set_text("");
                app.notify("Terminal set to auto-detect", false);
            }
        });
    }
    buttons.append(&use_default);
    inner.append(&buttons);
    inner.append(&section_label("FOLDER RUN COMMANDS"));
    let hint = gtk::Label::new(Some(
        "Commands are only run when you choose them from a context menu, and always ask for confirmation.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("hint");
    inner.append(&hint);
    let open = gtk::Button::with_label("Manage commands…");
    {
        let weak = Rc::downgrade(app);
        open.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                run_command_editor(&app, None);
            }
        });
    }
    inner.append(&open);
    scroller.upcast()
}

fn parse_env_text(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("Environment line {} must be KEY=value.", index + 1));
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) || key.contains('=') {
            return Err(format!("Invalid environment variable name on line {}.", index + 1));
        }
        env.insert(key.to_string(), value.trim().to_string());
    }
    Ok(env)
}

/// Choose one or more command sets for the current folder. The selected sets
/// start together; each one still follows its own sequential/parallel/mixed
/// scheduling internally.
pub fn run_commands_picker(app: &Rc<App>, folder: PathBuf, selected: Vec<PathBuf>) {
    let window = dialog_window(app, "Run command sets", 620, 560);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 9);
    root.set_margin_top(16);
    root.set_margin_bottom(14);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.append(&header(
        "Run command sets",
        &format!(
            "{}
Choose one or more sets. Selected sets start together in the detached run window.",
            folder.display()
        ),
    ));

    let available: Vec<RunCommand> = app
        .settings
        .borrow()
        .commands
        .iter()
        .filter(|command| command.enabled && command.applies_to(&folder))
        .cloned()
        .collect();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let checks: Rc<RefCell<Vec<(RunCommand, gtk::CheckButton)>>> =
        Rc::new(RefCell::new(Vec::new()));
    if available.is_empty() {
        let empty = gtk::Label::new(Some(
            "No enabled command sets apply to this folder. Create one, or check its availability.",
        ));
        empty.set_xalign(0.0);
        empty.set_wrap(true);
        empty.add_css_class("hint");
        list.append(&empty);
    }
    for command in available {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        let check = gtk::CheckButton::new();
        check.set_valign(gtk::Align::Start);
        row.append(&check);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        let title = gtk::Label::new(Some(&command.name));
        title.add_css_class("meta-value");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title_row.append(&title);
        if let Some(shortcut) = command.shortcut_label() {
            let hint = gtk::Label::new(Some(&shortcut));
            hint.add_css_class("tiny");
            hint.add_css_class("dim");
            title_row.append(&hint);
        }
        texts.append(&title_row);
        let steps = command.effective_steps();
        let detail = gtk::Label::new(Some(&format!(
            "{} command{} · {} · {}{}",
            steps.len(),
            if steps.len() == 1 { "" } else { "s" },
            command.mode.label(),
            command.launch.label(),
            if command.description.trim().is_empty() {
                String::new()
            } else {
                format!(" · {}", command.description.trim())
            }
        )));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("tiny");
        detail.add_css_class("dim");
        texts.append(&detail);
        row.append(&texts);
        list.append(&row);
        checks.borrow_mut().push((command, check));
    }

    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .min_content_height(180)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    root.append(&scroller);

    let selection_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let select_all = gtk::Button::with_label("Select all");
    select_all.add_css_class("flat");
    let select_none = gtk::Button::with_label("Select none");
    select_none.add_css_class("flat");
    selection_buttons.append(&select_all);
    selection_buttons.append(&select_none);
    root.append(&selection_buttons);
    {
        let checks = checks.clone();
        select_all.connect_clicked(move |_| {
            for (_, check) in checks.borrow().iter() {
                check.set_active(true);
            }
        });
    }
    {
        let checks = checks.clone();
        select_none.connect_clicked(move |_| {
            for (_, check) in checks.borrow().iter() {
                check.set_active(false);
            }
        });
    }
    {
        let checks = checks.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gtk4::gdk::Key::a
                && state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
            {
                for (_, check) in checks.borrow().iter() {
                    check.set_active(true);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key);
    }

    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    root.append(&error);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let manage = gtk::Button::with_label("Manage command sets…");
    manage.add_css_class("flat");
    buttons.append(&manage);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    buttons.append(&spacer);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let run = gtk::Button::with_label("Run selected");
    run.add_css_class("suggested");
    buttons.append(&run);
    root.append(&buttons);
    window.set_child(Some(&root));

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let weak = Rc::downgrade(app);
        let window = window.clone();
        manage.connect_clicked(move |_| {
            window.close();
            if let Some(app) = weak.upgrade() {
                app.open_settings(Some("commands"));
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        let window = window.clone();
        let checks = checks.clone();
        let error = error.clone();
        let folder = folder.clone();
        let selected = selected.clone();
        run.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let profiles: Vec<RunCommand> = checks
                .borrow()
                .iter()
                .filter(|(_, check)| check.is_active())
                .map(|(command, _)| command.clone())
                .collect();
            if profiles.is_empty() {
                error.set_text("Select at least one command set to run.");
                error.set_visible(true);
                return;
            }
            window.close();
            crate::runner::start_many(
                &app,
                profiles,
                folder.clone(),
                selected.clone(),
            );
        });
    }
    run.grab_focus();
    window.present();
}

fn commands_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Command sets",
        "Each set can hold several commands and apply to a folder, its subtree, or every folder. Assign Shift+S slots to run sets instantly.",
    );
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let commands = app.settings.borrow().commands.clone();
    if commands.is_empty() {
        let empty = gtk::Label::new(Some("No commands yet."));
        empty.add_css_class("dim");
        empty.set_xalign(0.0);
        list.append(&empty);
    }
    for command in commands {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let title = gtk::Label::new(Some(&command.name));
        title.set_xalign(0.0);
        title.add_css_class("meta-value");
        texts.append(&title);
        let steps = command.effective_steps();
        let detail = gtk::Label::new(Some(&format!(
            "{} command{} · {} · {}{}{}{} · {}",
            steps.len(),
            if steps.len() == 1 { "" } else { "s" },
            command.mode.label(),
            command.scope.label(),
            if command.launch == LaunchMode::Background { "" } else { " · terminal" },
            if command.enabled { "" } else { " · disabled" },
            command
                .shortcut_label()
                .map(|shortcut| format!(" · {shortcut}"))
                .unwrap_or_default(),
            command
                .folder
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "any folder".into())
        )));
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        detail.add_css_class("tiny");
        detail.add_css_class("dim");
        texts.append(&detail);
        row_box.append(&texts);
        let run = gtk::Button::with_label("Run");
        run.add_css_class("flat");
        {
            let weak = Rc::downgrade(app);
            let command = command.clone();
            run.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    let folder = command
                        .folder
                        .clone()
                        .or_else(|| app.active_location().path().cloned())
                        .unwrap_or_else(crate::paths::home);
                    crate::runner::confirm_and_start(&app, command.clone(), folder, Vec::new());
                }
            });
        }
        row_box.append(&run);
        let edit = gtk::Button::with_label("Edit");
        edit.add_css_class("flat");
        {
            let weak = Rc::downgrade(app);
            let command = command.clone();
            edit.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    run_command_editor(&app, Some(command.clone()));
                }
            });
        }
        row_box.append(&edit);
        let toggle = gtk::Button::with_label(if command.enabled { "Disable" } else { "Enable" });
        toggle.add_css_class("flat");
        {
            let weak = Rc::downgrade(app);
            let id = command.id.clone();
            toggle.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    let mut settings = app.settings.borrow_mut();
                    if let Some(c) = settings.commands.iter_mut().find(|c| c.id == id) {
                        c.enabled = !c.enabled;
                    }
                    settings.save();
                    drop(settings);
                    app.notify("Command updated", false);
                }
            });
        }
        row_box.append(&toggle);
        let delete = gtk::Button::with_label("Delete");
        delete.add_css_class("flat");
        delete.add_css_class("danger");
        {
            let weak = Rc::downgrade(app);
            let id = command.id.clone();
            let name = command.name.clone();
            delete.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                let id = id.clone();
                let app_c = app.clone();
                let name = name.clone();
                confirm(
                    &app,
                    "Delete command?",
                    &format!("“{name}” will be removed from settings."),
                    "Delete",
                    true,
                    move |_| {
                        {
                            let mut settings = app_c.settings.borrow_mut();
                            settings.commands.retain(|c| c.id != id);
                            settings.save();
                        }
                        app_c.notify("Command set deleted", false);
                    },
                );
            });
        }
        row_box.append(&delete);
        list.append(&row_box);
    }
    inner.append(&list);
    let add = gtk::Button::with_label("Add command set…");
    add.add_css_class("suggested");
    {
        let weak = Rc::downgrade(app);
        add.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                run_command_editor(&app, None);
            }
        });
    }
    inner.append(&add);
    let shortcuts = gtk::Label::new(Some(
        "Quick launch: Shift+S, 1–9 runs the assigned set; Shift+S, 0 opens the multi-set picker.",
    ));
    shortcuts.add_css_class("tiny");
    shortcuts.add_css_class("dim");
    shortcuts.set_xalign(0.0);
    shortcuts.set_wrap(true);
    inner.append(&shortcuts);
    scroller.upcast()
}

fn keyboard_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Keyboard",
        "Every major action is reachable without a mouse. Changes apply immediately.",
    );
    let conflict = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let conflict_label = gtk::Label::new(None);
    conflict_label.add_css_class("danger");
    conflict_label.add_css_class("tiny");
    conflict_label.set_xalign(0.0);
    conflict_label.set_visible(false);
    conflict.append(&conflict_label);
    inner.append(&conflict);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for &(id, default, label) in crate::shortcuts::SHORTCUTS {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_top(2);
        row_box.set_margin_bottom(2);
        let name = gtk::Label::new(Some(label));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        row_box.append(&name);
        let accel_label = gtk::Label::new(Some(
            &crate::shortcuts::effective(&app.settings.borrow(), id).to_string(),
        ));
        accel_label.add_css_class("mono-path");
        accel_label.set_width_chars(22);
        accel_label.set_xalign(0.0);
        row_box.append(&accel_label);
        let change = gtk::Button::with_label("Change");
        change.add_css_class("flat");
        {
            let weak = Rc::downgrade(app);
            let id = id.to_string();
            let accel_label = accel_label.clone();
            let conflict_label = conflict_label.clone();
            change.connect_clicked(move |_| {
                let Some(app) = weak.upgrade() else { return };
                let app2 = app.clone();
                let id = id.clone();
                let accel_label = accel_label.clone();
                let conflict_label = conflict_label.clone();
                let id_for_lookup = id.clone();
                record_shortcut(&app, &id_for_lookup, move |accel| {
                    let app = &app2;
                    let settings = app.settings.borrow().clone();
                    if let Some(other) = crate::shortcuts::conflict_for(&settings.shortcuts, &id, &accel) {
                        conflict_label.set_text(&format!(
                            "Conflict: “{accel}” is already used by “{}”. The new shortcut was not saved.",
                            crate::shortcuts::label(&other)
                        ));
                        conflict_label.set_visible(true);
                        return;
                    }
                    let mut settings = app.settings.borrow_mut();
                    if accel.trim().is_empty() || accel == default {
                        settings.shortcuts.remove(&id);
                    } else {
                        settings.shortcuts.insert(id.clone(), accel.clone());
                    }
                    settings.save();
                    drop(settings);
                    accel_label.set_text(if accel.trim().is_empty() { "(none)" } else { &accel });
                    conflict_label.set_visible(false);
                    app.apply_shortcuts();
                });
            });
        }
        row_box.append(&change);
        let reset = gtk::Button::with_label("Reset");
        reset.add_css_class("flat");
        {
            let weak = Rc::downgrade(app);
            let id = id.to_string();
            let accel_label = accel_label.clone();
            reset.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().shortcuts.remove(&id);
                    save(&app);
                    accel_label.set_text(default);
                    app.apply_shortcuts();
                }
            });
        }
        row_box.append(&reset);
        list.append(&row_box);
    }
    let list_scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(180)
        .vexpand(true)
        .build();
    inner.append(&list_scroller);
    let reset_all = gtk::Button::with_label("Reset all to defaults");
    {
        let weak = Rc::downgrade(app);
        let conflict_label = conflict_label.clone();
        reset_all.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let app_c = app.clone();
            let conflict_label = conflict_label.clone();
            confirm(
                &app,
                "Reset all shortcuts?",
                "Every custom shortcut will be removed and the defaults restored.",
                "Reset all",
                false,
                move |_| {
                    app_c.settings.borrow_mut().shortcuts.clear();
                    save(&app_c);
                    app_c.apply_shortcuts();
                    conflict_label.set_text("Shortcuts reset. Reopen Settings to see the defaults.");
                    conflict_label.add_css_class("success");
                    conflict_label.set_visible(true);
                },
            );
        });
    }
    inner.append(&reset_all);
    scroller.upcast()
}

fn record_shortcut(app: &Rc<App>, action: &str, on_done: impl Fn(String) + 'static) {
    let window = dialog_window(app, "Record shortcut", 380, 160);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(16);
    root.set_margin_bottom(14);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.append(&header(
        &format!("Shortcut for “{}”", crate::shortcuts::label(action)),
        "Press a key combination. Escape cancels; Backspace clears the shortcut.",
    ));
    let display = gtk::Label::new(Some("Waiting for keys…"));
    display.add_css_class("mono-path");
    display.set_xalign(0.0);
    root.append(&display);
    window.set_child(Some(&root));
    let window_ref = window.clone();
    let key = gtk::EventControllerKey::new();
    let on_done = Rc::new(on_done);
    key.connect_key_pressed(move |_, keyval, _, state| {
        let name = keyval.name().map(|s| s.to_string()).unwrap_or_default();
        if keyval == gtk4::gdk::Key::Escape {
            window_ref.close();
            return glib::Propagation::Stop;
        }
        if keyval == gtk4::gdk::Key::BackSpace {
            on_done(String::new());
            window_ref.close();
            return glib::Propagation::Stop;
        }
        if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
            || state.contains(gtk4::gdk::ModifierType::ALT_MASK)
        {
            let mut accel = String::new();
            if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
                accel.push_str("<Control>");
            }
            if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                accel.push_str("<Shift>");
            }
            if state.contains(gtk4::gdk::ModifierType::ALT_MASK) {
                accel.push_str("<Alt>");
            }
            if state.contains(gtk4::gdk::ModifierType::SUPER_MASK) {
                accel.push_str("<Super>");
            }
            accel.push_str(&if name.chars().count() == 1 {
                name.to_lowercase()
            } else {
                name.clone()
            });
            let normalized = crate::shortcuts::normalize(&accel);
            on_done(normalized);
            window_ref.close();
            glib::Propagation::Stop
        } else if keyval == gtk4::gdk::Key::F1
            || keyval == gtk4::gdk::Key::F2
            || keyval == gtk4::gdk::Key::F5
            || keyval == gtk4::gdk::Key::Delete
        {
            on_done(crate::shortcuts::normalize(&name));
            window_ref.close();
            glib::Propagation::Stop
        } else {
            display.set_text(&format!("{} — add Ctrl or Alt", name));
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key);
    window.present();
}

fn transfers_page(app: &Rc<App>) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Transfers",
        "Optional device-to-device transfers use LocalSend when it is installed. Core features never depend on it.",
    );
    match crate::transfers::availability() {
        crate::transfers::Availability::Available { detail } => {
            let label = gtk::Label::new(Some(&detail));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.add_css_class("success");
            inner.append(&label);
        }
        crate::transfers::Availability::Unavailable { reason } => {
            let label = gtk::Label::new(Some(&reason));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.add_css_class("hint");
            inner.append(&label);
        }
    }
    inner.append(&switch_row(
        "Enable transfer actions",
        "Hides the Send to device actions when off.",
        app.settings.borrow().transfers_enabled,
        {
            let weak = Rc::downgrade(app);
            move |on| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().transfers_enabled = on;
                    save(&app);
                    app.refresh_sidebar();
                }
            }
        },
    ));
    let open = gtk::Button::with_label("Open transfer dialog");
    {
        let weak = Rc::downgrade(app);
        open.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                transfer_dialog(&app);
            }
        });
    }
    inner.append(&open);
    scroller.upcast()
}

fn advanced_page(app: &Rc<App>, _window: &gtk::Window) -> gtk::Widget {
    let (scroller, inner) = page_container(
        "Advanced",
        "Diagnostics never collect file contents. Configuration and metadata can be exported and imported.",
    );
    let paths_label = gtk::Label::new(Some(&format!(
        "Config: {}\nData: {}\nCache: {}\nLog: {}",
        crate::paths::config_dir().display(),
        crate::paths::data_dir().display(),
        crate::paths::cache_dir().display(),
        crate::paths::log_file().display()
    )));
    paths_label.set_xalign(0.0);
    paths_label.set_wrap(true);
    paths_label.add_css_class("mono-path");
    paths_label.set_selectable(true);
    inner.append(&paths_label);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let diagnostics = gtk::Button::with_label("Copy diagnostics");
    {
        let weak = Rc::downgrade(app);
        diagnostics.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let text = format!(
                    "Delve {}\nGTK: {}.{}.{}\nConfig: {}\nData: {}\nCache: {}\nTheme: {}\nTabs open: {}\nThumbnails: {}",
                    env!("CARGO_PKG_VERSION"),
                    gtk::major_version(),
                    gtk::minor_version(),
                    gtk::micro_version(),
                    crate::paths::config_dir().display(),
                    crate::paths::data_dir().display(),
                    crate::paths::cache_dir().display(),
                    app.settings.borrow().theme,
                    app.tabs.borrow().len(),
                    crate::thumbnails::cache_size()
                );
                app.copy_text(&text);
                app.notify("Diagnostics copied to clipboard", false);
            }
        });
    }
    buttons.append(&diagnostics);
    let log = gtk::Button::with_label("Reveal log file");
    {
        let weak = Rc::downgrade(app);
        log.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let file = gio::File::for_path(crate::paths::log_file());
                let _ = gio::AppInfo::launch_default_for_uri(&file.uri(), None::<&gio::AppLaunchContext>);
                let _ = &app;
            }
        });
    }
    buttons.append(&log);
    inner.append(&buttons);
    inner.append(&section_label("PORTABILITY"));
    let export = gtk::Button::with_label("Export configuration");
    {
        let weak = Rc::downgrade(app);
        export.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let settings = app.settings.borrow().clone();
                let meta = app.meta.borrow().clone();
                match crate::settings::export_bundle(&settings, &meta) {
                    Ok(path) => app.notify(&format!("Exported to {}", path.display()), false),
                    Err(err) => app.notify(&err, true),
                }
            }
        });
    }
    inner.append(&row("Export", "Writes settings and metadata as JSON.", &export));
    let import = gtk::Button::with_label("Import configuration…");
    {
        let weak = Rc::downgrade(app);
        let app = app.clone();
        import.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Import Delve configuration");
            let weak = Rc::downgrade(&app);
            dialog.open(
                Some(&app.window),
                None::<&gio::Cancellable>,
                move |result| {
                    let Ok(file) = result else { return };
                    let Some(app) = weak.upgrade() else { return };
                    let Some(path) = file.path() else { return };
                    match crate::settings::import_bundle(&path) {
                        Ok(bundle) => {
                            *app.settings.borrow_mut() = bundle.settings;
                            *app.meta.borrow_mut() = bundle.metadata;
                            app.settings.borrow().save();
                            app.meta.borrow().save();
                            app.apply_shortcuts();
                            app.reload_theme();
                            app.apply_density();
                            app.apply_window_opacity();
                            app.refresh_after_icon_change();
                            app.refresh_sidebar();
                            app.refresh_after_metadata_change();
                            app.notify("Configuration imported", false);
                        }
                        Err(err) => app.notify(&err, true),
                    }
                },
            );
        });
    }
    inner.append(&row("Import", "Machine-specific paths are kept as recorded.", &import));
    inner.append(&section_label("RESET"));
    let reset = gtk::Button::with_label("Reset all settings");
    reset.add_css_class("destructive-action");
    {
        let weak = Rc::downgrade(app);
        let app = app.clone();
        reset.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let app_c = app.clone();
            confirm(
                &app,
                "Reset all settings?",
                "Settings return to defaults. Tabs, tags and folder colours are not deleted.",
                "Reset settings",
                true,
                move |_| {
                    *app_c.settings.borrow_mut() = Settings::default();
                    app_c.settings.borrow().save();
                    app_c.apply_shortcuts();
                    app_c.reload_theme();
                    app_c.apply_density();
                    app_c.apply_window_opacity();
                    app_c.refresh_after_icon_change();
                    app_c.refresh_sidebar();
                    app_c.notify("Settings reset", false);
                },
            );
        });
    }
    inner.append(&reset);
    scroller.upcast()
}

struct StepRowUi {
    row: gtk::Box,
    group: gtk::SpinButton,
    program: gtk::Entry,
    args: gtk::Entry,
}

fn add_step_row(
    steps_box: &gtk::Box,
    rows: &Rc<RefCell<Vec<StepRowUi>>>,
    program_text: &str,
    args_text: &str,
    group: u32,
) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let group_label = gtk::Label::new(Some("G"));
    group_label.add_css_class("tiny");
    group_label.add_css_class("dim");
    row.append(&group_label);
    let group_spin = gtk::SpinButton::with_range(0.0, 99.0, 1.0);
    group_spin.set_width_chars(2);
    group_spin.set_value(group as f64);
    group_spin.set_tooltip_text(Some("Group number (used in mixed mode)"));
    row.append(&group_spin);
    let program = gtk::Entry::new();
    program.set_placeholder_text(Some("Program"));
    program.set_text(program_text);
    program.set_width_chars(14);
    row.append(&program);
    let args = gtk::Entry::new();
    args.set_placeholder_text(Some("Arguments; {folder} {selected} {name} {home}"));
    args.set_text(args_text);
    args.set_hexpand(true);
    row.append(&args);
    let remove = gtk::Button::from_icon_name("list-remove-symbolic");
    remove.add_css_class("flat");
    remove.set_tooltip_text(Some("Remove this command"));
    {
        let steps_box = steps_box.clone();
        let rows = rows.clone();
        let row_ref = row.clone();
        remove.connect_clicked(move |_| {
            steps_box.remove(&row_ref);
            rows.borrow_mut().retain(|r| r.row.parent().is_some());
        });
    }
    row.append(&remove);
    rows.borrow_mut().push(StepRowUi {
        row: row.clone(),
        group: group_spin,
        program,
        args,
    });
    steps_box.append(&row);
}

pub fn run_command_editor(app: &Rc<App>, existing: Option<RunCommand>) {
    let editing = existing.clone();
    let window = dialog_window(
        app,
        if existing.is_some() { "Edit command profile" } else { "New command profile" },
        660,
        800,
    );
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Run command profile",
        "Commands run only when chosen. They are spawned as argv, never through a shell.",
    ));
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Name shown in the menu and Spotlight"));
    let description = gtk::Entry::new();
    description.set_placeholder_text(Some("Optional description"));
    let folder = gtk::Entry::new();
    folder.set_placeholder_text(Some("Folder this profile belongs to"));
    let scope_drop = gtk::DropDown::from_strings(&RunScope::all().map(|s| s.label()));
    let env_view = gtk::TextView::new();
    env_view.set_monospace(true);
    env_view.set_wrap_mode(gtk::WrapMode::WordChar);
    env_view.set_top_margin(4);
    env_view.set_bottom_margin(4);
    env_view.set_left_margin(6);
    env_view.set_right_margin(6);
    let cwd_drop = gtk::DropDown::from_strings(&CwdRule::all().map(|c| c.label()));
    let enabled = gtk::Switch::new();
    enabled.set_active(true);
    let mode_drop = gtk::DropDown::from_strings(&RunMode::all().map(|m| m.label()));
    let launch_drop = gtk::DropDown::from_strings(&LaunchMode::all().map(|m| m.label()));
    let stop_on_failure = gtk::Switch::new();
    stop_on_failure.set_active(true);
    let shortcut_drop = gtk::DropDown::from_strings(&[
        "No shortcut",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
    ]);

    if let Some(cmd) = &existing {
        name.set_text(&cmd.name);
        description.set_text(&cmd.description);
        folder.set_text(
            &cmd.folder
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        );
        scope_drop.set_selected(
            RunScope::all()
                .iter()
                .position(|s| *s == cmd.scope)
                .unwrap_or(1) as u32,
        );
        let env_text = cmd
            .env
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n");
        env_view.buffer().set_text(&env_text);
        cwd_drop.set_selected(
            CwdRule::all()
                .iter()
                .position(|c| *c == cmd.cwd_rule)
                .unwrap_or(0) as u32,
        );
        enabled.set_active(cmd.enabled);
        mode_drop.set_selected(
            RunMode::all()
                .iter()
                .position(|m| *m == cmd.mode)
                .unwrap_or(0) as u32,
        );
        launch_drop.set_selected(
            LaunchMode::all()
                .iter()
                .position(|m| *m == cmd.launch)
                .unwrap_or(0) as u32,
        );
        stop_on_failure.set_active(cmd.stop_on_failure);
        shortcut_drop.set_selected(cmd.shortcut.min(9) as u32);
    } else {
        // Match the original context-menu behaviour: a new profile applies to
        // the active folder and its subfolders until the user narrows it.
        scope_drop.set_selected(1);
        if let Some(path) = app.active_location().path() {
            folder.set_text(&path.display().to_string());
        }
    }

    root.append(&row("Name", "Required.", &name));
    root.append(&row("Description", "", &description));
    root.append(&row(
        "Folder",
        "Base folder. Required unless availability is “Every folder”.",
        &folder,
    ));
    root.append(&row(
        "Availability",
        "Choose whether the profile is offered in this folder, its subtree, or everywhere.",
        &scope_drop,
    ));
    root.append(&row("Working directory", "", &cwd_drop));
    root.append(&row("Execution mode", "Sequential, all at once, or ordered groups.", &mode_drop));
    let mode_hint = gtk::Label::new(Some(RunMode::Sequential.hint()));
    mode_hint.add_css_class("tiny");
    mode_hint.add_css_class("dim");
    mode_hint.set_xalign(0.0);
    root.append(&mode_hint);
    root.append(&row("Launch in", "Background processes appear in the run panel.", &launch_drop));
    root.append(&row("Stop on failure", "Skip later steps/groups when one fails.", &stop_on_failure));
    root.append(&row("Enabled", "", &enabled));
    root.append(&row(
        "Quick shortcut",
        "Press Shift+S, then this number from the main window. Slots are unique.",
        &shortcut_drop,
    ));

    let env_title = gtk::Label::new(Some("ENVIRONMENT"));
    env_title.add_css_class("section-title");
    env_title.set_xalign(0.0);
    root.append(&env_title);
    let env_hint = gtk::Label::new(Some(
        "Optional KEY=value lines, one per line. Blank lines and lines starting with # are ignored.",
    ));
    env_hint.add_css_class("tiny");
    env_hint.add_css_class("dim");
    env_hint.set_xalign(0.0);
    env_hint.set_wrap(true);
    root.append(&env_hint);
    let env_scroll = gtk::ScrolledWindow::builder()
        .child(&env_view)
        .min_content_height(64)
        .max_content_height(110)
        .propagate_natural_height(true)
        .build();
    root.append(&env_scroll);

    let steps_title = gtk::Label::new(Some("COMMANDS"));
    steps_title.add_css_class("section-title");
    steps_title.set_xalign(0.0);
    root.append(&steps_title);
    let steps_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let steps_scroll = gtk::ScrolledWindow::builder()
        .child(&steps_box)
        .min_content_height(80)
        .max_content_height(210)
        .propagate_natural_height(true)
        .build();
    root.append(&steps_scroll);
    let rows: Rc<RefCell<Vec<StepRowUi>>> = Rc::new(RefCell::new(Vec::new()));
    if let Some(cmd) = &existing {
        for step in cmd.effective_steps() {
            add_step_row(
                &steps_box,
                &rows,
                &step.program,
                &step.args.join(" "),
                step.group,
            );
        }
    }
    if rows.borrow().is_empty() {
        add_step_row(&steps_box, &rows, "", "", 0);
    }
    {
        let steps_box = steps_box.clone();
        let rows = rows.clone();
        let add = gtk::Button::with_label("Add command");
        add.add_css_class("flat");
        add.connect_clicked(move |_| {
            add_step_row(&steps_box, &rows, "", "", 0);
        });
        root.append(&add);
    }
    {
        let hint = mode_hint.clone();
        mode_drop.connect_selected_notify(move |drop| {
            let mode = RunMode::all()[drop.selected().min(2) as usize];
            hint.set_text(mode.hint());
        });
    }
    {
        let folder = folder.clone();
        let apply_sensitivity = move |drop: &gtk::DropDown| {
            folder.set_sensitive(drop.selected() != 2);
        };
        apply_sensitivity(&scope_drop);
        scope_drop.connect_selected_notify(apply_sensitivity);
    }

    let error = gtk::Label::new(None);
    error.add_css_class("danger");
    error.add_css_class("tiny");
    error.set_xalign(0.0);
    error.set_visible(false);
    root.append(&error);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    let save_btn = gtk::Button::with_label("Save profile");
    save_btn.add_css_class("suggested");
    buttons.append(&save_btn);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    let name_focus = name.clone();
    {
        let weak = Rc::downgrade(app);
        let window_ref = window.clone();
        let editing = editing.clone();
        let rows = rows.clone();
        save_btn.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let steps: Vec<RunStep> = rows
                .borrow()
                .iter()
                .filter_map(|row| {
                    let program = row.program.text().trim().to_string();
                    if program.is_empty() {
                        return None;
                    }
                    Some(RunStep {
                        program,
                        args: crate::terminal::split_args(&row.args.text()),
                        group: row.group.value() as u32,
                    })
                })
                .collect();
            if name.text().trim().is_empty() || steps.is_empty() {
                error.set_text("A name and at least one program are required.");
                error.set_visible(true);
                return;
            }
            let scope = RunScope::all()[scope_drop.selected().min(2) as usize];
            let folder_path = {
                let text = folder.text().to_string();
                if scope == RunScope::Global || text.trim().is_empty() {
                    None
                } else {
                    Some(PathBuf::from(text.trim()))
                }
            };
            if scope != RunScope::Global && folder_path.is_none() {
                error.set_text("Choose a folder for folder-scoped profiles, or select “Every folder”.");
                error.set_visible(true);
                return;
            }
            let buffer = env_view.buffer();
            let env_text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
            let environment = match parse_env_text(&env_text) {
                Ok(environment) => environment,
                Err(message) => {
                    error.set_text(&message);
                    error.set_visible(true);
                    return;
                }
            };
            let shortcut = shortcut_drop.selected().min(9) as u8;
            let id = editing
                .as_ref()
                .map(|c| c.id.clone())
                .unwrap_or_else(|| format!("cmd-{}", crate::util::now_millis()));
            if shortcut > 0 {
                let settings = app.settings.borrow();
                if let Some(other) = settings
                    .commands
                    .iter()
                    .find(|c| c.id != id && c.shortcut == shortcut)
                {
                    let name = other.name.clone();
                    drop(settings);
                    error.set_text(&format!(
                        "Shift+S, {shortcut} is already assigned to “{name}”."
                    ));
                    error.set_visible(true);
                    return;
                }
            }
            let mut command = RunCommand {
                id,
                name: name.text().to_string(),
                program: String::new(),
                args: Vec::new(),
                cwd_rule: CwdRule::all()[cwd_drop.selected().min(2) as usize],
                description: description.text().to_string(),
                enabled: enabled.state(),
                folder: folder_path,
                scope,
                env: environment,
                shortcut,
                steps,
                mode: RunMode::all()[mode_drop.selected().min(2) as usize],
                launch: LaunchMode::all()[launch_drop.selected().min(1) as usize],
                stop_on_failure: stop_on_failure.state(),
            };
            command.sync_legacy();
            {
                let mut settings = app.settings.borrow_mut();
                if let Some(existing) = &editing {
                    if let Some(slot) =
                        settings.commands.iter_mut().find(|c| c.id == existing.id)
                    {
                        *slot = command.clone();
                    }
                } else {
                    settings.commands.push(command.clone());
                }
                settings.save();
            }
            window_ref.close();
            app.notify("Command profile saved", false);
        });
    }
    name_focus.grab_focus();
    window.present();
}

/// Reinstall the theme file monitor after changing theme live-reload settings.
impl App {
    pub fn install_theme_monitor_on_change(self: &Rc<Self>) {
        let monitor = self.theme_monitor.borrow_mut().take();
        if let Some(monitor) = monitor {
            monitor.cancel();
        }
        self.restart_theme_monitor();
    }
}

/// Ask which application should open a file type. The choice is stored in
/// settings and reused for every later file of the same MIME type.
pub fn open_with_dialog(app: &Rc<App>, entry: &Entry) {
    let mime = entry.mime();
    let default_app = gio::AppInfo::default_for_type(&mime, false);
    let mut apps: Vec<gio::AppInfo> = gio::AppInfo::all_for_type(&mime)
        .into_iter()
        .filter(|a| a.should_show())
        .collect();
    // Keep a hidden-but-registered default app selectable.
    if let Some(default) = &default_app {
        if !apps.iter().any(|a| a.id() == default.id()) {
            apps.insert(0, default.clone());
        }
    }
    apps.sort_by_key(|a| a.display_name().to_lowercase());
    apps.dedup_by(|a, b| a.id() == b.id());
    if apps.is_empty() {
        app.notify(
            &format!("No application is installed to open “{}”.", entry.name),
            true,
        );
        return;
    }
    let system_default = default_app.and_then(|a| a.id()).map(|id| id.to_string());

    let window = dialog_window(app, "Open with", 470, 500);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);
    root.append(&header(
        "Open with",
        &format!(
            "Choose the app Delve should use for {} files from now on.",
            entry.type_label().to_lowercase()
        ),
    ));
    let file_label = gtk::Label::new(Some(&entry.name));
    file_label.add_css_class("hint");
    file_label.set_xalign(0.0);
    file_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    file_label.set_tooltip_text(Some(&entry.path.display().to_string()));
    root.append(&file_label);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    root.append(&scroller);

    for info in apps {
        let row = gtk::Button::new();
        row.add_css_class("flat");
        row.add_css_class("view-menu-row");
        row.set_halign(gtk::Align::Fill);
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        if let Some(icon) = info.icon() {
            let image = gtk::Image::from_gicon(&icon);
            image.set_pixel_size(24);
            body.append(&image);
        }
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(info.display_name().as_str()));
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&name);
        let description = info
            .description()
            .map(|d| d.to_string())
            .unwrap_or_default();
        if !description.is_empty() && description != info.display_name().as_str() {
            let hint = gtk::Label::new(Some(&crate::util::ellipsize(&description, 70)));
            hint.add_css_class("tiny");
            hint.add_css_class("dim");
            hint.set_xalign(0.0);
            hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.append(&hint);
        }
        body.append(&texts);
        if info.id().as_deref() == system_default.as_deref() {
            let badge = gtk::Label::new(Some("System default"));
            badge.add_css_class("tiny");
            badge.add_css_class("dim");
            body.append(&badge);
        }
        row.set_child(Some(&body));
        {
            let app = app.clone();
            let window = window.clone();
            let entry = entry.clone();
            let info = info.clone();
            row.connect_clicked(move |_| {
                let mime = entry.mime();
                {
                    let mut settings = app.settings.borrow_mut();
                    settings.default_apps.insert(
                        mime,
                        info.id().map(|id| id.to_string()).unwrap_or_default(),
                    );
                    settings.save();
                }
                app.launch_file_with(&entry, &info);
                window.close();
            });
        }
        list.append(&row);
    }

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    buttons.append(&cancel);
    root.append(&buttons);
    window.set_child(Some(&root));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    window.present();
}
