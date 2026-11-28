//! The Home dashboard.
//!
//! `Location::Home` is a distinct overview page — it is not the `/home/<user>`
//! folder listing, which stays available (and unchanged) at its real path.
//! The dashboard gathers storage, pinned folders, quick access and recent
//! places, and reveals its cards with a short, subtle stagger so the page feels
//! alive without getting in the way.

use crate::app::App;
use crate::fs::Entry;
use crate::model::Location;
use crate::paths::{self, UserDir};
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct HomeView {
    pub root: gtk::ScrolledWindow,
    header_box: gtk::Box,
    bento: Rc<Bento>,
    /// The first sync should leave the dashboard at the top, even if GTK
    /// scrolled the view to focus a child while it was being mapped.
    reset_scroll: Cell<bool>,
}

/// A responsive bento grid. Cards keep their identity while the column count
/// changes, so resizing the window reflows the dashboard without rebuilding it.
struct Bento {
    grid: gtk::Grid,
    cards: RefCell<Vec<gtk::Widget>>,
    columns: Cell<usize>,
}

impl Bento {
    fn new() -> Rc<Bento> {
        let grid = gtk::Grid::builder()
            .column_spacing(14)
            .row_spacing(14)
            .column_homogeneous(true)
            .hexpand(true)
            .valign(gtk::Align::Start)
            .build();
        Rc::new(Bento {
            grid,
            cards: RefCell::new(Vec::new()),
            // Start narrow: cards laid out in three columns would set a large
            // minimum window width before the first reflow can happen.
            columns: Cell::new(1),
        })
    }

    /// Three columns on wide windows, two on medium, one when stacked.
    fn columns_for(width: f64) -> usize {
        if width < 720.0 {
            1
        } else if width < 1120.0 {
            2
        } else {
            3
        }
    }

    fn set_cards(&self, cards: Vec<gtk::Widget>) {
        for card in self.cards.borrow().iter() {
            self.grid.remove(card);
        }
        *self.cards.borrow_mut() = cards;
        self.place(self.columns.get());
    }

    fn arrange(&self, columns: usize) {
        for card in self.cards.borrow().iter() {
            self.grid.remove(card);
        }
        self.place(columns);
    }

    /// Attach the current cards using `columns`; the caller detaches first.
    fn place(&self, columns: usize) {
        let columns = columns.clamp(1, 3);
        self.columns.set(columns);
        let mut column = 0usize;
        let mut row = 0usize;
        for (index, card) in self.cards.borrow().iter().enumerate() {
            // Storage is the hero card and Recent places closes the bento; both
            // take two columns where there is room.
            let span = match (columns, index) {
                (3, 0) | (3, 3) => 2,
                (2, 0) | (2, 3) => 2,
                _ => 1,
            }
            .min(columns);
            if column + span > columns {
                column = 0;
                row += 1;
            }
            self.grid
                .attach(card, column as i32, row as i32, span as i32, 1);
            column += span;
            if column >= columns {
                column = 0;
                row += 1;
            }
        }
    }
}

impl HomeView {
    pub fn new(app: &Rc<App>) -> HomeView {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
        body.add_css_class("dash-body");
        body.set_margin_top(26);
        body.set_margin_bottom(30);
        body.set_margin_start(24);
        body.set_margin_end(24);

        let header_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&header_box);
        let bento = Bento::new();
        body.append(&bento.grid);

        // Let the dashboard fill narrow windows and cap it on very wide ones;
        // the bento reflows on its own as this box changes width.
        let clamp = gtk::Box::new(gtk::Orientation::Vertical, 0);
        clamp.set_halign(gtk::Align::Center);
        clamp.set_hexpand(true);
        clamp.set_valign(gtk::Align::Start);
        clamp.append(&body);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&clamp)
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scroller.add_css_class("delve-canvas");
        scroller.add_css_class("dash-scroller");

        {
            // Right-click on the dashboard's empty space (or a card's
            // background) opens the workspace menu. Rows claim their own
            // right-click first, so their menus still win.
            let strong = Rc::downgrade(app);
            let view = scroller.clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            gesture.connect_pressed(move |_, _, x, y| {
                if let Some(app) = strong.upgrade() {
                    let anchor: gtk::Widget = view.clone().upcast();
                    app.show_dashboard_context_menu(&anchor, x, y);
                }
            });
            scroller.add_controller(gesture);
        }

        {
            // The scrolled window's adjustment reports its content width, not
            // the visible viewport, so the breakpoints follow the widget's real
            // allocated width. Poll it: tick callbacks stop once the reveal
            // animations settle, which used to strand the grid on the old
            // column count. The check is a single f64 comparison.
            let weak = Rc::downgrade(&bento);
            let scroller_weak = scroller.downgrade();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                let Some(bento) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let Some(scroller) = scroller_weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let width = scroller.width() as f64;
                if width > 0.0 {
                    let columns = Bento::columns_for(width);
                    if columns != bento.columns.get() {
                        // Defer out of the current layout pass before
                        // detaching and re-attaching cards.
                        bento.columns.set(columns);
                        let bento = bento.clone();
                        glib::idle_add_local_once(move || bento.arrange(columns));
                    }
                }
                glib::ControlFlow::Continue
            });
        }

        HomeView { root: scroller, header_box, bento, reset_scroll: Cell::new(true) }
    }

    pub fn sync(&self, app: &Rc<App>, tab_id: u64) {
        let mut child = self.header_box.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            self.header_box.remove(&c);
            child = next;
        }
        let header = self.header(app);
        subtle_reveal(header.upcast_ref(), 0);
        self.header_box.append(&header);

        let home_space = paths::disk_space(&paths::home());
        let volumes = crate::volumes::snapshot();
        let cards: Vec<gtk::Widget> = vec![
            self.storage_card(app, home_space, &volumes).upcast(),
            self.pinned_card(app).upcast(),
            self.quick_access_card(app, tab_id).upcast(),
            self.recent_card(app).upcast(),
        ];
        for (index, card) in cards.iter().enumerate() {
            subtle_reveal(card, (index as u32 + 1) * 45);
        }
        self.bento.set_cards(cards);

        if self.reset_scroll.replace(false) {
            // GTK scrolls to the first focusable child when the view is
            // mapped; the dashboard should open showing its header instead.
            let adjustment = self.root.vadjustment();
            glib::idle_add_local_once(move || adjustment.set_value(0.0));
        }
    }

    fn header(&self, app: &Rc<App>) -> gtk::Box {
        let header = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let icon = gtk::Image::from_icon_name("user-home-symbolic");
        icon.set_pixel_size(26);
        icon.add_css_class("accent");
        title_row.append(&icon);
        let title = gtk::Label::new(None);
        title.set_markup("<span size='xx-large' weight='bold'>Home</span>");
        title.set_xalign(0.0);
        title_row.append(&title);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        title_row.append(&spacer);
        let open = gtk::Button::with_label("Open folder");
        open.add_css_class("pill-button");
        open.set_tooltip_text(Some(&paths::home().display().to_string()));
        {
            let weak = Rc::downgrade(app);
            open.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.navigate_active(Location::Path(paths::home()));
                }
            });
        }
        title_row.append(&open);
        header.append(&title_row);
        let subtitle = gtk::Label::new(Some(&format!(
            "An overview of your space and places · {}",
            paths::home().display()
        )));
        subtitle.add_css_class("dim");
        subtitle.add_css_class("small");
        subtitle.set_xalign(0.0);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        header.append(&subtitle);
        header
    }

    fn card(&self, title: &str, icon_name: &str) -> (gtk::Box, gtk::Box) {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
        card.add_css_class("dash-card");
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::from_icon_name(icon_name);
        icon.set_pixel_size(15);
        icon.add_css_class("dim");
        heading.append(&icon);
        let label = gtk::Label::new(None);
        label.set_markup(&format!(
            "<span weight='bold' size='small'>{}</span>",
            glib::markup_escape_text(title)
        ));
        label.set_xalign(0.0);
        heading.append(&label);
        card.append(&heading);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.append(&content);
        (card, content)
    }

    fn storage_card(
        &self,
        app: &Rc<App>,
        home_space: Option<paths::DiskSpace>,
        volumes: &[crate::volumes::VolumeInfo],
    ) -> gtk::Box {
        let (card, content) = self.card("Storage", "drive-harddisk-symbolic");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        row.set_hexpand(true);

        let space = home_space.unwrap_or_default();
        let dark = app.theme.current.borrow().spec.dark;
        row.append(&donut(space.percent_used(), accent_color(app), dark));

        let detail = gtk::Box::new(gtk::Orientation::Vertical, 6);
        detail.set_hexpand(true);
        detail.set_valign(gtk::Align::Center);
        let name = gtk::Label::new(Some("Home volume"));
        name.add_css_class("section-title");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        detail.append(&name);
        let used = gtk::Label::new(None);
        used.set_markup(&format!(
            "<span weight='bold' size='large'>{}</span><span size='small' alpha='70%'> of {} used</span>",
            glib::markup_escape_text(&bytes(space.used())),
            glib::markup_escape_text(&bytes(space.total))
        ));
        used.set_xalign(0.0);
        used.set_ellipsize(gtk::pango::EllipsizeMode::End);
        detail.append(&used);
        let bar = gtk::ProgressBar::new();
        bar.add_css_class("dash-bar");
        bar.set_fraction((space.percent_used() / 100.0).clamp(0.0, 1.0));
        bar.set_valign(gtk::Align::Center);
        bar.set_hexpand(true);
        detail.append(&bar);
        let free = gtk::Label::new(Some(&format!(
            "{} free · {}",
            bytes(space.free),
            paths::home().display()
        )));
        free.add_css_class("dim");
        free.add_css_class("tiny");
        free.set_xalign(0.0);
        free.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        detail.append(&free);
        row.append(&detail);
        content.append(&row);

        let others: Vec<&crate::volumes::VolumeInfo> = volumes
            .iter()
            .filter(|v| v.mount_point.as_deref() != Some(paths::home().as_path()))
            .filter(|v| v.mount_point.is_some() || v.can_mount)
            .collect();
        if !others.is_empty() {
            content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        for volume in others.into_iter().take(5) {
            content.append(&volume_row(app, volume));
        }
        card
    }

    fn pinned_card(&self, app: &Rc<App>) -> gtk::Box {
        let (card, content) = self.card("Pinned folders", "emblem-favorite-symbolic");
        let pinned: Vec<PathBuf> = app
            .settings
            .borrow()
            .pinned
            .iter()
            .filter(|p| p.is_dir())
            .cloned()
            .collect();
        if pinned.is_empty() {
            let hint = gtk::Label::new(Some(
                "Pin a folder from its context menu or from the sidebar to keep it here.",
            ));
            hint.add_css_class("dim");
            hint.add_css_class("small");
            hint.set_xalign(0.0);
            // Wrapping keeps the card's minimum width small enough for narrow
            // windows; without it the text dictates the whole window width.
            hint.set_wrap(true);
            hint.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            hint.set_max_width_chars(46);
            content.append(&hint);
        }
        for path in pinned {
            let row = folder_row(app, &path, Some("pinned-badge"));
            content.append(&row);
        }
        card
    }

    fn quick_access_card(&self, app: &Rc<App>, _tab_id: u64) -> gtk::Box {
        let (card, content) = self.card("Quick access", "folder-symbolic");
        let grid = gtk::FlowBox::new();
        grid.set_selection_mode(gtk::SelectionMode::None);
        grid.set_homogeneous(true);
        grid.set_column_spacing(8);
        grid.set_row_spacing(8);
        grid.set_max_children_per_line(4);
        // One tile per line is allowed so the card never overflows a narrow
        // window; the flow box fits as many as there is room for.
        grid.set_min_children_per_line(1);
        let mut dirs: Vec<PathBuf> = Vec::new();
        for dir in [
            UserDir::Desktop,
            UserDir::Documents,
            UserDir::Downloads,
            UserDir::Pictures,
            UserDir::Music,
            UserDir::Videos,
        ] {
            let path = paths::user_dir(dir);
            if path.is_dir() && !dirs.contains(&path) {
                dirs.push(path);
            }
        }
        for path in dirs {
            let entry = Entry::from_path(path.clone(), file_label(&path));
            let tile = gtk::Button::new();
            tile.add_css_class("dash-tile");
            tile.set_tooltip_text(Some(&path.display().to_string()));
            let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
            body.set_halign(gtk::Align::Center);
            body.set_margin_top(12);
            body.set_margin_bottom(12);
            let icon = crate::views::entry_icon(app, &entry, 34);
            icon.set_halign(gtk::Align::Center);
            body.append(&icon);
            let label = gtk::Label::new(Some(&file_label(&path)));
            label.add_css_class("small");
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(16);
            body.append(&label);
            let count = std::fs::read_dir(&path).map(|rd| rd.flatten().count()).unwrap_or(0);
            let hint = gtk::Label::new(Some(&format!(
                "{count} item{}",
                if count == 1 { "" } else { "s" }
            )));
            hint.add_css_class("tiny");
            hint.add_css_class("dim");
            body.append(&hint);
            tile.set_child(Some(&body));
            let weak = Rc::downgrade(app);
            let target = path.clone();
            tile.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.navigate_active(Location::Path(target.clone()));
                }
            });
            grid.append(&tile);
        }
        content.append(&grid);
        card
    }

    fn recent_card(&self, app: &Rc<App>) -> gtk::Box {
        let (card, content) = self.card("Recent places", "document-open-recent-symbolic");
        let home = paths::home();
        let mut shown = 0;
        for path in app.settings.borrow().recent_locations.clone() {
            if shown >= 5 {
                break;
            }
            if !path.is_dir() || path == home {
                continue;
            }
            content.append(&folder_row(app, &path, None));
            shown += 1;
        }
        if shown == 0 {
            let hint = gtk::Label::new(Some("Folders you visit will show up here."));
            hint.add_css_class("dim");
            hint.add_css_class("small");
            hint.set_xalign(0.0);
            hint.set_wrap(true);
            hint.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            hint.set_max_width_chars(46);
            content.append(&hint);
        } else {
            let clear = gtk::Button::with_label("Clear recent places");
            clear.add_css_class("flat");
            clear.add_css_class("tiny");
            clear.set_halign(gtk::Align::Start);
            let weak = Rc::downgrade(app);
            clear.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.settings.borrow_mut().recent_locations.clear();
                    app.settings.borrow().save();
                    app.refresh_home_dashboard();
                }
            });
            content.append(&clear);
        }
        card
    }
}

fn folder_row(app: &Rc<App>, path: &Path, _extra: Option<&str>) -> gtk::Button {
    let entry = Entry::from_path(path.to_path_buf(), file_label(path));
    let row = gtk::Button::new();
    row.add_css_class("dash-row");
    row.set_tooltip_text(Some(&path.display().to_string()));
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let icon = crate::views::entry_icon(app, &entry, 20);
    body.append(&icon);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texts.set_hexpand(true);
    let name = gtk::Label::new(Some(&file_label(path)));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    texts.append(&name);
    let parent = path.parent().and_then(|p| p.to_str()).unwrap_or_default();
    let hint = gtk::Label::new(Some(&shorten_home(parent)));
    hint.add_css_class("tiny");
    hint.add_css_class("dim");
    hint.set_xalign(0.0);
    hint.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    texts.append(&hint);
    body.append(&texts);
    let chevron = gtk::Image::from_icon_name("go-next-symbolic");
    chevron.set_pixel_size(12);
    chevron.add_css_class("dim");
    body.append(&chevron);
    row.set_child(Some(&body));
    let weak = Rc::downgrade(app);
    let target = path.to_path_buf();
    row.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            app.navigate_active(Location::Path(target.clone()));
        }
    });
    // Right-click pins/unpins without leaving the dashboard.
    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let weak = Rc::downgrade(app);
    let target = path.to_path_buf();
    let anchor = row.clone();
    gesture.connect_pressed(move |_, _, x, y| {
        if let Some(app) = weak.upgrade() {
            app.show_path_context_menu(&anchor, target.clone(), x, y);
        }
    });
    row.add_controller(gesture);
    row
}

fn volume_row(app: &Rc<App>, volume: &crate::volumes::VolumeInfo) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("dash-row");
    let icon = gtk::Image::from_icon_name(&volume.icon_name());
    icon.set_pixel_size(18);
    row.append(&icon);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_hexpand(true);
    let name = gtk::Label::new(Some(&volume.name));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    texts.append(&name);
    match volume.mount_point.clone() {
        Some(path) => {
            let space = paths::disk_space(&path);
            let detail = match space {
                Some(space) if space.total > 0 => format!(
                    "{} free of {}",
                    bytes(space.free),
                    bytes(space.total)
                ),
                _ => path.display().to_string(),
            };
            let hint = gtk::Label::new(Some(&detail));
            hint.add_css_class("tiny");
            hint.add_css_class("dim");
            hint.set_xalign(0.0);
            hint.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            texts.append(&hint);
            if let Some(space) = space {
                let bar = gtk::ProgressBar::new();
                bar.add_css_class("dash-bar");
                bar.set_fraction((space.percent_used() / 100.0).clamp(0.0, 1.0));
                bar.set_hexpand(true);
                texts.append(&bar);
            }
        }
        None => {
            let hint = gtk::Label::new(Some("Not mounted"));
            hint.add_css_class("tiny");
            hint.add_css_class("dim");
            hint.set_xalign(0.0);
            hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.append(&hint);
        }
    }
    row.append(&texts);

    if let Some(path) = volume.mount_point.clone() {
        let open = gtk::Button::with_label("Open");
        open.add_css_class("pill-button");
        open.add_css_class("tiny");
        let weak = Rc::downgrade(app);
        let open_target = path.clone();
        open.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.navigate_active(Location::Path(open_target.clone()));
            }
        });
        row.append(&open);
        if volume.can_eject {
            let eject = gtk::Button::from_icon_name("media-eject-symbolic");
            eject.add_css_class("flat");
            eject.set_tooltip_text(Some("Safely remove this volume"));
            let weak = Rc::downgrade(app);
            let target = path.clone();
            eject.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.eject_volume(&target);
                }
            });
            row.append(&eject);
        }
    } else if volume.can_mount {
        let mount = gtk::Button::with_label("Mount");
        mount.add_css_class("pill-button");
        mount.add_css_class("tiny");
        let weak = Rc::downgrade(app);
        let volume = volume.clone();
        mount.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.mount_volume(&volume);
            }
        });
        row.append(&mount);
    }
    row
}

/// A ring chart that animates from zero to the real percentage. The value is
/// drawn every frame, so the sweep stays smooth even on a throttled frame
/// clock.
fn donut(percent: f64, accent: Option<(f64, f64, f64)>, dark: bool) -> gtk::Overlay {
    let target = (percent / 100.0).clamp(0.0, 1.0);
    let value = Rc::new(Cell::new(0.0f64));
    let area = gtk::DrawingArea::new();
    area.set_content_width(96);
    area.set_content_height(96);
    area.set_halign(gtk::Align::Center);
    area.set_valign(gtk::Align::Center);
    area.add_css_class("dash-donut");
    let draw_value = value.clone();
    area.set_draw_func(move |_, cr, width, height| {
        let w = width as f64;
        let h = height as f64;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let radius = w.min(h) / 2.0 - 10.0;
        cr.set_line_width(11.0);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        // Track: a faint tint of the text colour, so it stays visible on both
        // light and dark themes.
        if dark {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.09);
        } else {
            cr.set_source_rgba(0.13, 0.16, 0.22, 0.12);
        }
        cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
        cr.stroke().ok();
        // Progress arc, starting at 12 o'clock.
        let fraction = draw_value.get().clamp(0.0, 1.0);
        if fraction > 0.001 {
            let start = -std::f64::consts::FRAC_PI_2;
            let end = start + std::f64::consts::TAU * fraction;
            if let Some((r, g, b)) = accent {
                cr.set_source_rgba(r, g, b, 1.0);
            } else {
                cr.set_source_rgba(0.42, 0.62, 1.0, 1.0);
            }
            cr.arc(cx, cy, radius, start, end);
            cr.stroke().ok();
        }
    });

    let start = Instant::now();
    let duration = 620.0;
    let area_draw = area.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let t = (start.elapsed().as_secs_f64() * 1000.0 / duration).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        value.set(target * eased);
        area_draw.queue_draw();
        if t >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&area));
    let centre = gtk::Box::new(gtk::Orientation::Vertical, 0);
    centre.set_halign(gtk::Align::Center);
    centre.set_valign(gtk::Align::Center);
    centre.set_can_target(false);
    let pct = gtk::Label::new(None);
    pct.set_markup(&format!(
        "<span size='large' weight='bold'>{:.0}%</span>",
        percent.clamp(0.0, 100.0)
    ));
    centre.append(&pct);
    let used = gtk::Label::new(Some("used"));
    used.add_css_class("tiny");
    used.add_css_class("dim");
    centre.append(&used);
    overlay.add_overlay(&centre);
    overlay.set_can_target(false);
    overlay
}

/// The theme's accent colour as 0–1 components, so the ring matches the rest
/// of the interface in every theme.
fn accent_color(app: &App) -> Option<(f64, f64, f64)> {
    let hex = app.theme.current.borrow().spec.colors.get("accent")?.clone();
    let (r, g, b, _) = crate::util::parse_color(&hex)?;
    Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
}

fn bytes(value: u64) -> String {
    crate::fs::human_size(value)
}

fn file_label(path: &Path) -> String {
    if path == paths::home() {
        return "Home".into();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// `~/Documents/Projects` keeps dashboards scannable.
fn shorten_home(path: &str) -> String {
    let home = paths::home();
    if let Some(stripped) = path.strip_prefix(&*home.to_string_lossy()) {
        if stripped.is_empty() {
            return "~".into();
        }
        return format!("~{}", stripped);
    }
    path.to_string()
}

/// Fade a widget in with a short delay. Subtle by design: 260 ms, no bounce.
fn subtle_reveal(widget: &gtk::Widget, delay_ms: u32) {
    widget.set_opacity(0.0);
    let w = widget.clone();
    glib::timeout_add_local_once(Duration::from_millis(delay_ms as u64), move || {
        let start = Instant::now();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let t = (start.elapsed().as_secs_f64() * 1000.0 / 240.0).min(1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            w.set_opacity(eased);
            if t >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });
}
