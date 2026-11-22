//! Right-hand inspector: preview, metadata, tags and folder colour.

use crate::app::App;
use crate::fs::Entry;
use crate::model::Location;
use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

pub struct Inspector {
    pub root: gtk::Widget,
    body: gtk::Box,
    frame: RefCell<Option<gtk::Box>>,
    current: RefCell<Option<PathBuf>>,
    app: Weak<App>,
}

impl Inspector {
    pub fn new(app: &Rc<App>) -> Inspector {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&body)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(false)
            .build();
        scroller.add_css_class("delve-inspector");
        scroller.add_css_class("inspector-root");
        scroller.set_size_request(200, -1);

        Inspector {
            root: scroller.clone().upcast(),
            body,
            frame: RefCell::new(None),
            current: RefCell::new(None),
            app: Rc::downgrade(app),
        }
    }

    pub fn update(&self, app: &Rc<App>, tab_id: u64) {
        let (selection, location, view) = {
            let tabs = app.tabs.borrow();
            let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else { return };
            let state = tab.state.borrow();
            let selected = state.selected_entries();
            (selected, state.location.clone(), state.view)
        };
        self.current.replace(selection.first().map(|e| e.path.clone()));
        self.frame.replace(None);
        let mut child = self.body.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            self.body.remove(&c);
            child = next;
        }

        // Detail-column controls belong to the preview pane, not the sort menu.
        if view == crate::settings::ViewKind::Detail {
            self.detail_columns_button(app);
        }

        if selection.is_empty() {
            self.empty_state(&location);
            return;
        }

        if selection.len() > 1 {
            self.multi_state(app, &selection);
            return;
        }

        let entry = &selection[0];
        self.single_state(app, tab_id, entry, &location);
    }

    fn detail_columns_button(&self, app: &Rc<App>) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_halign(gtk::Align::Start);
        let button = gtk::MenuButton::new();
        button.set_label("Detail columns");
        button.add_css_class("pill-button");
        button.set_tooltip_text(Some("Choose which columns the detail view shows"));
        let popover = gtk::Popover::new();
        button.set_popover(Some(&popover));
        let weak = Rc::downgrade(app);
        popover.connect_show(move |p| {
            if let Some(app) = weak.upgrade() {
                app.fill_columns_popover(p);
            }
        });
        row.append(&button);
        self.body.append(&row);
    }

    fn empty_state(&self, location: &Location) {
        let icon = gtk::Image::from_icon_name(location.icon_name());
        icon.set_pixel_size(48);
        icon.set_opacity(0.45);
        icon.set_margin_top(24);
        self.body.append(&icon);
        let title = gtk::Label::new(Some("Nothing selected"));
        title.add_css_class("inspector-title");
        title.set_margin_top(6);
        self.body.append(&title);
        let detail = gtk::Label::new(Some(match location {
            Location::Trash => "Select an item to restore or delete it permanently.",
            _ => "Select a file or folder to see its preview and details.",
        }));
        detail.add_css_class("hint");
        detail.set_wrap(true);
        detail.set_justify(gtk::Justification::Center);
        self.body.append(&detail);
        let hint = gtk::Label::new(None);
        hint.set_markup("<span size='small'>Ctrl+H toggles hidden files · F2 renames · Delete moves to Trash</span>");
        hint.add_css_class("dim");
        hint.set_wrap(true);
        hint.set_margin_top(16);
        self.body.append(&hint);
    }

    fn multi_state(&self, app: &Rc<App>, selection: &[Entry]) {
        let total: u64 = selection.iter().map(|e| e.size).sum();
        let folders = selection.iter().filter(|e| e.is_dir).count();
        let title = gtk::Label::new(Some(&format!("{} items selected", selection.len())));
        title.add_css_class("inspector-title");
        title.set_wrap(true);
        self.body.append(&title);
        let summary = gtk::Label::new(Some(&format!(
            "{} · {} folders · {} files",
            crate::fs::human_size(total),
            folders,
            selection.len() - folders
        )));
        summary.add_css_class("secondary");
        self.body.append(&summary);
        self.body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let section = gtk::Label::new(Some("TAGS"));
        section.add_css_class("inspector-section");
        section.set_xalign(0.0);
        self.body.append(&section);
        let tag_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        tag_row.set_halign(gtk::Align::Start);
        let add = gtk::MenuButton::new();
        add.set_label("Add tag");
        add.add_css_class("pill-button");
        let popover = gtk::Popover::new();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let meta = app.meta.borrow().tags.clone();
        if meta.is_empty() {
            list.append(&gtk::Label::new(Some("No tags yet")));
        }
        for tag in meta {
            let b = gtk::Button::with_label(&tag.name);
            b.add_css_class("flat");
            let strong = Rc::downgrade(app);
            let id = tag.id.clone();
            let paths: Vec<PathBuf> = selection.iter().map(|e| e.path.clone()).collect();
            b.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.assign_tag_to_paths(&id, &paths);
                }
            });
            list.append(&b);
        }
        popover.set_child(Some(&list));
        add.set_popover(Some(&popover));
        tag_row.append(&add);
        self.body.append(&tag_row);

        let folders_only: Vec<PathBuf> = selection.iter().filter(|e| e.is_dir).map(|e| e.path.clone()).collect();
        if !folders_only.is_empty() {
            let colour = gtk::Button::with_label("Folder colour…");
            colour.add_css_class("flat");
            let strong = Rc::downgrade(app);
            colour.connect_clicked(move |b| {
                if let Some(app) = strong.upgrade() {
                    app.show_folder_color_menu(b, folders_only.clone());
                }
            });
            self.body.append(&colour);
        }

        self.body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let list_title = gtk::Label::new(Some("SELECTION"));
        list_title.add_css_class("inspector-section");
        list_title.set_xalign(0.0);
        self.body.append(&list_title);
        for entry in selection.iter().take(12) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let img = crate::views::entry_icon(app, entry, 15);
            row.append(&img);
            let label = gtk::Label::new(Some(&entry.name));
            label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            label.set_xalign(0.0);
            label.add_css_class("meta-value");
            row.append(&label);
            self.body.append(&row);
        }
        if selection.len() > 12 {
            let more = gtk::Label::new(Some(&format!("…and {} more", selection.len() - 12)));
            more.add_css_class("dim");
            more.set_xalign(0.0);
            self.body.append(&more);
        }
    }

    fn single_state(&self, app: &Rc<App>, tab_id: u64, entry: &Entry, location: &Location) {
        let title = gtk::Label::new(Some(&entry.name));
        title.add_css_class("inspector-title");
        title.set_wrap(true);
        title.set_xalign(0.0);
        title.set_selectable(true);
        self.body.append(&title);
        let subtitle = gtk::Label::new(Some(&entry.type_label()));
        subtitle.add_css_class("secondary");
        subtitle.set_xalign(0.0);
        self.body.append(&subtitle);

        // Preview
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("preview-frame");
        // Fixed height and a one-pixel minimum width: images shrink to fit the
        // pane instead of forcing it wider.
        frame.set_size_request(1, 224);
        frame.set_hexpand(true);
        frame.set_overflow(gtk::Overflow::Hidden);
        let icon = crate::views::entry_icon(app, entry, 80);
        icon.set_vexpand(true);
        icon.set_valign(gtk::Align::Center);
        icon.set_halign(gtk::Align::Center);
        frame.append(&icon);
        self.body.append(&frame);
        self.frame.replace(Some(frame.clone()));
        self.load_preview(app, entry, &frame);

        if matches!(location, Location::Trash) {
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let restore = gtk::Button::with_label("Restore");
            restore.add_css_class("suggested");
            let strong = Rc::downgrade(app);
            let path = entry.path.clone();
            restore.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.restore_trash_path(&path);
                }
            });
            actions.append(&restore);
            let delete = gtk::Button::with_label("Delete permanently");
            delete.add_css_class("destructive-action");
            let strong = Rc::downgrade(app);
            let path = entry.path.clone();
            delete.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.confirm_delete(vec![path.clone()], true);
                }
            });
            actions.append(&delete);
            self.body.append(&actions);
        } else {
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let open = gtk::Button::with_label(if entry.is_dir { "Open" } else { "Open with default app" });
            open.add_css_class("suggested");
            open.set_hexpand(true);
            let strong = Rc::downgrade(app);
            let e = entry.clone();
            open.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.open_entry(tab_id, &e);
                }
            });
            actions.append(&open);
            let copy_path = gtk::Button::from_icon_name("edit-copy-symbolic");
            copy_path.add_css_class("flat");
            copy_path.set_tooltip_text(Some("Copy path"));
            let strong = Rc::downgrade(app);
            let path = entry.path.clone();
            copy_path.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.copy_text(&path.display().to_string());
                    app.notify("Path copied", false);
                }
            });
            actions.append(&copy_path);
            let props = gtk::Button::from_icon_name("document-properties-symbolic");
            props.add_css_class("flat");
            props.set_tooltip_text(Some("Properties"));
            let strong = Rc::downgrade(app);
            let e = entry.clone();
            props.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    crate::dialogs::properties_dialog(&app, vec![e.clone()]);
                }
            });
            actions.append(&props);
            self.body.append(&actions);
            let quick = gtk::Label::new(Some("Double-click to open · F2 to rename"));
            quick.add_css_class("tiny");
            quick.add_css_class("dim");
            quick.set_xalign(0.0);
            self.body.append(&quick);
        }

        // Tags
        let tags_label = gtk::Label::new(Some("TAGS"));
        tags_label.add_css_class("inspector-section");
        tags_label.set_xalign(0.0);
        self.body.append(&tags_label);
        let chips = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        chips.set_halign(gtk::Align::Start);
        let assigned = app.meta.borrow().tags_for(&entry.path);
        for tag_id in &assigned {
            let (name, color, icon) = {
                let meta = app.meta.borrow();
                match meta.tag(tag_id) {
                    Some(t) => (t.name.clone(), t.color.clone(), t.icon.clone()),
                    None => continue,
                }
            };
            let chip = gtk::Box::new(gtk::Orientation::Horizontal, 3);
            chip.add_css_class("chip");
            chip.add_css_class(&format!("tagchip-{}", crate::theme::color_class(&color)));
            let tag_icon = gtk::Image::from_icon_name(&icon);
            tag_icon.set_pixel_size(12);
            chip.append(&tag_icon);
            let label = gtk::Label::new(Some(&name));
            chip.append(&label);
            let remove = gtk::Button::from_icon_name("window-close-symbolic");
            remove.add_css_class("flat");
            remove.set_size_request(16, 16);
            let strong = Rc::downgrade(app);
            let id = tag_id.clone();
            let path = entry.path.clone();
            remove.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.remove_tag_from_path(&id, &path);
                }
            });
            chip.append(&remove);
            chips.append(&chip);
        }
        let add = gtk::MenuButton::new();
        add.set_icon_name("list-add-symbolic");
        add.add_css_class("flat");
        add.set_tooltip_text(Some("Add a tag"));
        let popover = gtk::Popover::new();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for tag in app.meta.borrow().tags.clone() {
            if assigned.contains(&tag.id) {
                continue;
            }
            let b = gtk::Button::with_label(&tag.name);
            b.add_css_class("flat");
            let strong = Rc::downgrade(app);
            let id = tag.id.clone();
            let path = entry.path.clone();
            b.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.add_tag_to_path(&id, &path);
                }
            });
            list.append(&b);
        }
        if list.first_child().is_none() {
            list.append(&gtk::Label::new(Some("All tags assigned")));
        }
        popover.set_child(Some(&list));
        add.set_popover(Some(&popover));
        chips.append(&add);
        self.body.append(&chips);

        if entry.is_dir {
            let colour_label = gtk::Label::new(Some("FOLDER COLOUR"));
            colour_label.add_css_class("inspector-section");
            colour_label.set_xalign(0.0);
            self.body.append(&colour_label);
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.set_halign(gtk::Align::Start);
            let current = app.meta.borrow().folder_color(&entry.path);
            let btn = gtk::Button::with_label(match &current {
                Some(c) => c,
                None => "Set colour…",
            });
            btn.add_css_class("pill-button");
            let strong = Rc::downgrade(app);
            let path = entry.path.clone();
            btn.connect_clicked(move |b| {
                if let Some(app) = strong.upgrade() {
                    app.show_folder_color_menu(b, vec![path.clone()]);
                }
            });
            row.append(&btn);
            if current.is_some() {
                let clear = gtk::Button::with_label("Clear");
                clear.add_css_class("flat");
                clear.add_css_class("tiny");
                let strong = Rc::downgrade(app);
                let path = entry.path.clone();
                clear.connect_clicked(move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.set_folder_color(&path, None);
                    }
                });
                row.append(&clear);
            }
            self.body.append(&row);
        }

        self.body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let details_label = gtk::Label::new(Some("GENERAL"));
        details_label.add_css_class("inspector-section");
        details_label.set_xalign(0.0);
        self.body.append(&details_label);
        let grid = gtk::Grid::builder()
            .row_spacing(5)
            .column_spacing(10)
            .build();
        let mut row = 0;
        let mut add_meta = |key: &str, value: &str| {
            let k = gtk::Label::new(Some(key));
            k.add_css_class("meta-key");
            k.set_xalign(0.0);
            k.set_valign(gtk::Align::Start);
            grid.attach(&k, 0, row, 1, 1);
            let v = gtk::Label::new(Some(value));
            v.add_css_class("meta-value");
            v.set_xalign(0.0);
            v.set_wrap(true);
            v.set_selectable(true);
            v.set_hexpand(true);
            grid.attach(&v, 1, row, 1, 1);
            row += 1;
        };
        if !entry.is_dir {
            add_meta("Size", &format!(
                "{} ({} bytes)",
                crate::fs::human_size(entry.size),
                entry.size
            ));
        }
        add_meta("Type", &entry.type_label());
        add_meta("MIME type", &entry.mime());
        add_meta("Location", &crate::paths::display_path(&entry.path));
        add_meta("Modified", &crate::fs::format_time_long(entry.modified));
        add_meta("Created", &crate::fs::format_time_long(entry.created));
        add_meta("Accessed", &crate::fs::format_time_long(entry.accessed));
        add_meta(
            "Permissions",
            &format!("{} ({})", entry.permissions_string(), entry.mode_octal()),
        );
        add_meta("Owner", &format!("{}:{}", entry.owner_name(), entry.group_name()));
        if entry.nlink > 1 {
            add_meta("Links", &entry.nlink.to_string());
        }
        if entry.is_symlink {
            if let Ok(target) = std::fs::read_link(&entry.path) {
                add_meta("Link target", &target.display().to_string());
            }
            if entry.broken_link {
                add_meta("Link status", "Broken or unavailable target");
            }
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
            add_meta("Items", &format!("{count} (direct)"));
            add_meta("Direct size", &crate::fs::human_size(size));
        }
        self.body.append(&grid);
        let advanced = gtk::Label::new(Some("SYSTEM"));
        advanced.add_css_class("inspector-section");
        advanced.set_xalign(0.0);
        self.body.append(&advanced);
        let system = gtk::Label::new(Some(&format!(
            "{}  ·  owner {}:{}  ·  inode {}",
            entry.permissions_string(),
            entry.owner_name(),
            entry.group_name(),
            entry.inode
        )));
        system.add_css_class("meta-key");
        system.set_wrap(true);
        system.set_xalign(0.0);
        system.set_selectable(true);
        self.body.append(&system);

        let strong = self.app.clone();
        let _ = strong;
    }

    fn load_preview(&self, app: &Rc<App>, entry: &Entry, _frame: &gtk::Box) {
        let settings = app.settings.borrow().clone();
        let entry = entry.clone();
        if entry.is_dir || entry.broken_link {
            return;
        }
        if entry.is_image() || entry.is_pdf() || entry.is_video() {
            let path = entry.path.clone();
            let name = entry.name.clone();
            let size = if entry.is_pdf() { 1100 } else { 900 };
            std::thread::spawn(move || {
                let thumb = crate::thumbnails::thumbnail_for(&entry, size);
                crate::dispatch::post_simple(move |app| {
                    if let Some(thumb) = thumb {
                        app.ui().inspector_widget.apply_image(&path, &name, &thumb);
                    }
                });
            });
            return;
        }
        if settings.text_preview && entry.is_text() {
            let path = entry.path.clone();
            std::thread::spawn(move || {
                let text = read_text(&entry.path, 48 * 1024);
                crate::dispatch::post_simple(move |app| {
                    if let Some(text) = text {
                        app.ui().inspector_widget.apply_text(&path, &text);
                    }
                });
            });
        }
    }

    pub fn apply_image(&self, path: &std::path::Path, name: &str, thumb: &std::path::Path) {
        if self.current.borrow().as_deref() != Some(path) {
            return;
        }
        let Some(frame) = self.frame.borrow().clone() else { return };
        if let Ok(texture) = gtk::gdk::Texture::from_filename(thumb) {
            let pic = gtk::Picture::for_paintable(&texture);
            pic.set_content_fit(gtk::ContentFit::Contain);
            pic.set_can_shrink(true);
            pic.set_hexpand(true);
            pic.set_vexpand(true);
            pic.set_size_request(1, 1);
            pic.set_valign(gtk::Align::Fill);
            pic.set_halign(gtk::Align::Fill);
            while let Some(child) = frame.first_child() {
                frame.remove(&child);
            }
            frame.append(&pic);
            frame.set_tooltip_text(Some(&format!("{name} — preview")));
        }
    }

    pub fn apply_text(&self, path: &std::path::Path, text: &str) {
        if self.current.borrow().as_deref() != Some(path) {
            return;
        }
        let Some(frame) = self.frame.borrow().clone() else { return };
        let view = gtk::TextView::new();
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_monospace(true);
        view.set_wrap_mode(gtk::WrapMode::None);
        view.add_css_class("preview-text");
        view.buffer().set_text(text);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&view)
            .vexpand(true)
            .min_content_height(150)
            .build();
        while let Some(child) = frame.first_child() {
            frame.remove(&child);
        }
        frame.append(&scroller);
    }
}

fn read_text(path: &std::path::Path, limit: u64) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; limit as usize];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    String::from_utf8(buf).ok()
}
