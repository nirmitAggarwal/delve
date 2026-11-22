//! Left navigation sidebar: Places, Tags, Library, Network and System.

use crate::app::App;
use crate::model::Location;
use crate::paths::UserDir;
use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Sidebar {
    pub root: gtk::Widget,
    outer: gtk::Box,
    logo: gtk::Image,
    title: gtk::Label,
    toggle: gtk::Button,
    content: gtk::Box,
    tags_box: gtk::Box,
    tag_search: gtk::SearchEntry,
    tag_filter_text: Rc<RefCell<String>>,
    tag_filter_visible: Cell<bool>,
    match_all_btn: gtk::ToggleButton,
    sort_row: gtk::Box,
    order_btn: gtk::Button,
    sort_label: gtk::Label,
    settings_row: gtk::Button,
}

impl Sidebar {
    pub fn new(app: &Rc<App>) -> Sidebar {
        let mini = app.settings.borrow().sidebar_mini;
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.add_css_class("delve-sidebar");
        outer.set_size_request(if mini { 60 } else { 180 }, -1);
        if mini {
            outer.add_css_class("sidebar-mini");
        }

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header.set_margin_start(if mini { 6 } else { 10 });
        header.set_margin_end(if mini { 6 } else { 8 });
        header.set_margin_top(8);
        header.set_margin_bottom(4);
        // The collapse/expand toggle travels with the sidebar, matching the
        // compact rail interaction.
        let toggle = gtk::Button::from_icon_name(if mini {
            "pan-end-symbolic"
        } else {
            "pan-start-symbolic"
        });
        toggle.add_css_class("toolbar-button");
        toggle.add_css_class("sidebar-toggle");
        toggle.set_tooltip_text(Some(if mini {
            "Expand sidebar"
        } else {
            "Collapse to icons"
        }));
        {
            let strong = Rc::downgrade(app);
            toggle.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    let mini = app.settings.borrow().sidebar_mini;
                    app.set_sidebar_mini(!mini);
                }
            });
        }
        header.append(&toggle);
        let logo = gtk::Image::from_icon_name("folder-symbolic");
        logo.set_pixel_size(18);
        logo.add_css_class("accent");
        logo.set_visible(!mini);
        header.append(&logo);
        let title = gtk::Label::new(Some("Delve"));
        title.add_css_class("big-title");
        title.set_xalign(0.0);
        title.set_visible(!mini);
        header.append(&title);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        outer.append(&header);

        // Compact sort control: current field plus its direction, directly in
        // the sidebar so sorting never needs the command bar.
        let sort_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        sort_row.set_margin_start(10);
        sort_row.set_margin_end(10);
        sort_row.set_margin_top(2);
        sort_row.set_margin_bottom(6);
        let sort_btn = gtk::MenuButton::new();
        sort_btn.add_css_class("sort-pill");
        sort_btn.set_tooltip_text(Some("Sort this folder"));
        sort_btn.set_hexpand(true);
        sort_btn.set_halign(gtk::Align::Fill);
        let sort_body = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let sort_label = gtk::Label::new(Some("Name"));
        sort_label.add_css_class("sort-pill-label");
        sort_label.set_xalign(0.0);
        sort_body.append(&sort_label);
        let sort_arrow = gtk::Image::from_icon_name("pan-down-symbolic");
        sort_arrow.set_pixel_size(12);
        sort_arrow.add_css_class("dim");
        sort_body.append(&sort_arrow);
        sort_btn.set_child(Some(&sort_body));
        let sort_popover = gtk::Popover::new();
        sort_popover.set_has_arrow(false);
        sort_popover.add_css_class("sort-menu");
        sort_btn.set_popover(Some(&sort_popover));
        {
            let strong = Rc::downgrade(app);
            sort_popover.connect_show(move |p| {
                if let Some(app) = strong.upgrade() {
                    app.fill_sort_popover(p);
                }
            });
        }
        // A separate direction toggle sits to the right of the field pill.
        let order_btn = gtk::Button::from_icon_name("pan-up-symbolic");
        order_btn.add_css_class("sort-pill");
        order_btn.add_css_class("sort-order");
        order_btn.set_tooltip_text(Some("Ascending — click for descending"));
        order_btn.set_valign(gtk::Align::Center);
        {
            let strong = Rc::downgrade(app);
            order_btn.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    if let Some(tab) = app.active_tab() {
                        let sort = tab.state.borrow().sort;
                        app.set_sort(tab.id, sort.field, !sort.ascending, sort.folders_first);
                    }
                }
            });
        }
        sort_row.append(&sort_btn);
        sort_row.append(&order_btn);
        // The row is appended by `sync()` (inside the scrollable content);
        // appending it here as well would parent it twice.

        let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
        content.set_margin_bottom(10);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            // The rail must be allowed to become narrower than its (hidden)
            // label content.
            .propagate_natural_width(false)
            .build();
        scroller.add_css_class("delve-sidebar-scroller");
        outer.append(&scroller);

        // Footer settings stay reachable in the icon rail too.
        let footer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        footer.set_margin_bottom(8);
        footer.set_margin_top(2);
        let settings_row = nav_row(app, "emblem-system-symbolic", "Settings", None);
        {
            let strong = Rc::downgrade(app);
            settings_row.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.open_settings(None);
                }
            });
        }
        footer.append(&settings_row);
        outer.append(&footer);

        let tags_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let tag_search = gtk::SearchEntry::new();
        tag_search.set_placeholder_text(Some("Filter tags"));
        tag_search.set_visible(false);
        tag_search.set_margin_start(10);
        tag_search.set_margin_end(10);
        tag_search.set_margin_bottom(4);
        let tag_filter_text: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        {
            let strong = Rc::downgrade(app);
            let tags_box = tags_box.clone();
            let filter = tag_filter_text.clone();
            tag_search.connect_search_changed(move |s| {
                let Some(app) = strong.upgrade() else { return };
                *filter.borrow_mut() = s.text().to_string();
                rebuild_tags(&app, &tags_box, &s.text().to_string());
            });
        }

        let match_all_btn = gtk::ToggleButton::with_label("Match all");
        match_all_btn.add_css_class("pill-button");
        match_all_btn.set_tooltip_text(Some("Require every selected tag instead of any"));
        match_all_btn.set_visible(false);
        {
            let strong = Rc::downgrade(app);
            match_all_btn.connect_toggled(move |b| {
                if let Some(app) = strong.upgrade() {
                    app.set_tag_match_all(b.is_active());
                }
            });
        }

        let sidebar = Sidebar {
            root: outer.clone().upcast(),
            outer,
            logo,
            title,
            toggle,
            content,
            tags_box,
            tag_search,
            tag_filter_text: tag_filter_text.clone(),
            tag_filter_visible: Cell::new(false),
            match_all_btn,
            sort_row,
            order_btn,
            sort_label,
            settings_row: settings_row.clone(),
        };
        sidebar.sync(app);
        sidebar
    }

    pub fn set_filter_visible(&self, visible: bool) {
        self.tag_filter_visible.set(visible);
        self.tag_search.set_visible(visible);
    }

    /// Switch between the icon-only rail and the full labelled sidebar.
    pub fn set_mini(&self, app: &Rc<App>, mini: bool) {
        if mini {
            self.outer.add_css_class("sidebar-mini");
        } else {
            self.outer.remove_css_class("sidebar-mini");
        }
        self.outer.set_size_request(if mini { 60 } else { 180 }, -1);
        self.title.set_visible(!mini);
        self.logo.set_visible(!mini);
        self.sort_row.set_visible(!mini);
        self.tag_search.set_visible(!mini && self.tag_filter_visible.get());
        self.toggle.set_icon_name(if mini {
            "pan-end-symbolic"
        } else {
            "pan-start-symbolic"
        });
        self.toggle.set_tooltip_text(Some(if mini {
            "Expand sidebar"
        } else {
            "Collapse to icons"
        }));
        apply_nav_mini(&self.settings_row, mini);
        self.sync(app);
    }

    pub fn sync(&self, app: &Rc<App>) {
        let active = app.active_location();
        let sidebar_snapshot = {
            let s = app.settings.borrow();
            s.sidebar_collapsed.clone()
        };
        // Clear.
        let mut child = self.content.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            self.content.remove(&c);
            child = next;
        }

        let mini = app.settings.borrow().sidebar_mini;
        if let Some(tab) = app.active_tab() {
            let (field, ascending) = {
                let state = tab.state.borrow();
                (state.sort.field, state.sort.ascending)
            };
            self.sort_label.set_text(field.label());
            self.order_btn.set_icon_name(if ascending {
                "pan-up-symbolic"
            } else {
                "pan-down-symbolic"
            });
            self.order_btn.set_tooltip_text(Some(if ascending {
                "Ascending — click for descending"
            } else {
                "Descending — click for ascending"
            }));
        }
        self.sort_row.set_visible(!mini);
        self.content.append(&self.sort_row);
        self.tag_search.set_visible(!mini && self.tag_filter_visible.get());
        self.content.append(&self.tag_search);
        self.content.append(&self.match_all_btn);

        // --- Places ---------------------------------------------------------
        let places = section(app, &self.content, "PLACES", "places", &sidebar_snapshot);
        let mut add_place = |icon: &str, label: &str, dir: std::path::PathBuf| {
            if !dir.is_dir() {
                return;
            }
            let loc = Location::Path(dir.clone());
            let row = nav_row(app, icon, label, None);
            tint_nav_icon(app, &row, &dir);
            set_active(&row, &active == &loc);
            install_place_drop(app, &row, match &loc {
                Location::Path(p) => Some(p.clone()),
                _ => None,
            });
            row.connect_clicked({
                let strong = Rc::downgrade(app);
                let loc = loc.clone();
                move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.navigate_active(loc.clone());
                    }
                }
            });
            places.append(&row);
        };
        {
            let loc = Location::Home;
            let row = nav_row(app, "user-home", "Home", None);
            tint_nav_icon(app, &row, &crate::paths::home());
            set_active(&row, &active == &loc);
            install_place_drop(app, &row, Some(crate::paths::home()));
            row.connect_clicked({
                let strong = Rc::downgrade(app);
                move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.navigate_active(Location::Home);
                    }
                }
            });
            places.append(&row);
        }
        add_place("user-desktop", "Desktop", crate::paths::user_dir(UserDir::Desktop));
        add_place("folder-documents", "Documents", crate::paths::user_dir(UserDir::Documents));
        add_place("folder-download", "Downloads", crate::paths::user_dir(UserDir::Downloads));
        add_place("folder-pictures", "Pictures", crate::paths::user_dir(UserDir::Pictures));
        add_place("folder-videos", "Videos", crate::paths::user_dir(UserDir::Videos));
        add_place("folder-music", "Music", crate::paths::user_dir(UserDir::Music));

        // --- Tags -----------------------------------------------------------
        let tags_section = section(app, &self.content, "TAGS", "tags", &sidebar_snapshot);
        let tag_header = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        if mini {
            tag_header.set_halign(gtk::Align::Center);
        }
        let add_tag = gtk::Button::from_icon_name("list-add-symbolic");
        add_tag.add_css_class("flat");
        add_tag.set_tooltip_text(Some("Create a tag"));
        {
            let strong = Rc::downgrade(app);
            add_tag.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.show_create_tag_dialog();
                }
            });
        }
        tag_header.append(&add_tag);
        tags_section.append(&tag_header);
        tags_section.append(&self.tags_box);
        rebuild_tags(app, &self.tags_box, &self.tag_filter_text.borrow());
        let tag_active = matches!(active, Location::Tag { .. });
        self.match_all_btn.set_visible(tag_active && !mini);
        if let Location::Tag { match_all, .. } = &active {
            self.match_all_btn.set_active(*match_all);
        }

        // --- Library --------------------------------------------------------
        let library = section(app, &self.content, "LIBRARY", "library", &sidebar_snapshot);
        for (icon, label, loc) in [
            ("user-trash-symbolic", "Trash", Location::Trash),
            ("document-open-recent-symbolic", "Recent Files", Location::RecentFiles),
            ("document-open-recent-symbolic", "Recent Locations", Location::RecentLocations),
        ] {
            let row = nav_row(app, icon, label, None);
            set_active(&row, &active == &loc);
            row.connect_clicked({
                let strong = Rc::downgrade(app);
                let loc = loc.clone();
                move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.navigate_active(loc.clone());
                    }
                }
            });
            library.append(&row);
        }
        if let Location::Trash = active {
            let empty = gtk::Button::with_label("Empty Trash");
            empty.add_css_class("sidebar-row");
            empty.set_visible(!mini);
            empty.set_tooltip_text(Some("Permanently delete everything in the Trash"));
            {
                let strong = Rc::downgrade(app);
                empty.connect_clicked(move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.confirm_empty_trash();
                    }
                });
            }
            library.append(&empty);
        }

        // --- Network --------------------------------------------------------
        let network = section(app, &self.content, "NETWORK", "network", &sidebar_snapshot);
        let net_mounts = crate::paths::network_mounts();
        if net_mounts.is_empty() {
            let hint = gtk::Label::new(Some("No remote mounts detected"));
            hint.add_css_class("dim");
            hint.add_css_class("tiny");
            hint.set_xalign(0.0);
            hint.set_margin_start(14);
            hint.set_margin_bottom(2);
            hint.set_visible(!mini);
            network.append(&hint);
        }
        for (label, path) in net_mounts {
            let row = nav_row(app, "network-server-symbolic", &label, None);
            let loc = Location::Path(path);
            set_active(&row, &active == &loc);
            row.connect_clicked({
                let strong = Rc::downgrade(app);
                let loc = loc.clone();
                move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.navigate_active(loc.clone());
                    }
                }
            });
            network.append(&row);
        }
        let transfers = nav_row(app, "send-to-symbolic", "Send to device", None);
        match crate::transfers::availability() {
            crate::transfers::Availability::Unavailable { reason } => {
                transfers.set_sensitive(false);
                transfers.set_tooltip_text(Some(&reason));
            }
            crate::transfers::Availability::Available { detail } => {
                transfers.set_tooltip_text(Some(&detail));
                let strong = Rc::downgrade(app);
                transfers.connect_clicked(move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.show_transfer_dialog();
                    }
                });
            }
        }
        network.append(&transfers);

        // --- System ---------------------------------------------------------
        let system = section(app, &self.content, "SYSTEM", "system", &sidebar_snapshot);
        let root_loc = Location::Path(crate::paths::root());
        let row = nav_row(app, "drive-harddisk-symbolic", "File System", None);
        set_active(&row, &active == &root_loc);
        install_place_drop(app, &row, Some(crate::paths::root()));
        row.connect_clicked({
            let strong = Rc::downgrade(app);
            let loc = root_loc.clone();
            move |_| {
                if let Some(app) = strong.upgrade() {
                    app.navigate_active(loc.clone());
                }
            }
        });
        system.append(&row);
        for (label, path) in crate::paths::mounts() {
            if path == crate::paths::root() {
                continue;
            }
            let row = nav_row(app, "drive-removable-media-symbolic", &label, Some(&path));
            let loc = Location::Path(path.clone());
            set_active(&row, &active == &loc);
            install_place_drop(app, &row, Some(path));
            row.connect_clicked({
                let strong = Rc::downgrade(app);
                let loc = loc.clone();
                move |_| {
                    if let Some(app) = strong.upgrade() {
                        app.navigate_active(loc.clone());
                    }
                }
            });
            system.append(&row);
        }
    }
}

fn section(
    app: &Rc<App>,
    parent: &gtk::Box,
    title: &str,
    key: &str,
    collapsed: &[String],
) -> gtk::Box {
    let mini = app.settings.borrow().sidebar_mini;
    let expander = gtk::Expander::new(None);
    expander.add_css_class("sidebar-section");
    expander.set_expanded(mini || !collapsed.iter().any(|c| c == key));
    if mini {
        // The rail has no room for headings; keep every section open.
        expander.set_expanded(true);
    }
    let key_owned = key.to_string();
    let strong = Rc::downgrade(app);
    expander.connect_expanded_notify(move |ex| {
        let Some(app) = strong.upgrade() else { return };
        let mut settings = app.settings.borrow_mut();
        settings.sidebar_collapsed.retain(|k| k != &key_owned);
        if !ex.is_expanded() {
            settings.sidebar_collapsed.push(key_owned.clone());
        }
        settings.save();
    });
    let label = gtk::Label::new(None);
    label.set_markup(&format!("<span size='small' weight='bold'>{}</span>", title));
    label.add_css_class("sidebar-heading");
    label.set_xalign(0.0);
    label.set_visible(!mini);
    expander.set_label_widget(Some(&label));
    let body = gtk::Box::new(gtk::Orientation::Vertical, 1);
    expander.set_child(Some(&body));
    parent.append(&expander);
    body
}

/// In the narrow rail the place icons keep their meaning but use dedicated
/// symbolic artwork instead of the shared folder glyph.
fn mini_icon(icon: &str) -> &str {
    match icon {
        "user-home" => "user-home-symbolic",
        "user-desktop" => "user-desktop-symbolic",
        "folder-documents" => "x-office-document-symbolic",
        "folder-download" => "folder-download-symbolic",
        "folder-pictures" => "image-x-generic-symbolic",
        "folder-videos" => "video-x-generic-symbolic",
        "folder-music" => "audio-x-generic-symbolic",
        other => other,
    }
}

fn nav_row(app: &Rc<App>, icon: &str, label: &str, _path: Option<&std::path::Path>) -> gtk::Button {
    let mini = app.settings.borrow().sidebar_mini;
    let row = gtk::Button::new();
    row.add_css_class("sidebar-row");
    row.set_has_frame(false);
    row.set_hexpand(true);
    let b = gtk::Box::new(gtk::Orientation::Horizontal, if mini { 0 } else { 8 });
    b.set_hexpand(!mini);
    b.set_halign(if mini { gtk::Align::Center } else { gtk::Align::Fill });
    let img = gtk::Image::from_icon_name(if mini { mini_icon(icon) } else { icon });
    img.set_pixel_size(if mini { 18 } else { 15 });
    b.append(&img);
    let text = gtk::Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.set_visible(!mini);
    b.append(&text);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    spacer.set_visible(!mini);
    b.append(&spacer);
    row.set_child(Some(&b));
    row.set_tooltip_text(Some(label));
    apply_nav_mini(&row, mini);
    row
}

/// Restyle an existing navigation row for the icon rail without rebuilding it.
fn apply_nav_mini(row: &gtk::Button, mini: bool) {
    let Some(b) = row.child().and_then(|c| c.downcast::<gtk::Box>().ok()) else {
        return;
    };
    b.set_spacing(if mini { 0 } else { 8 });
    b.set_halign(if mini { gtk::Align::Center } else { gtk::Align::Fill });
    b.set_hexpand(!mini);
    let mut child = b.first_child();
    let mut index = 0;
    while let Some(c) = child {
        let next = c.next_sibling();
        if index == 1 {
            c.set_visible(!mini);
        } else if index >= 2 {
            if let Some(label) = c.downcast_ref::<gtk::Label>() {
                if label.has_css_class("sidebar-badge") {
                    c.set_visible(!mini);
                }
            }
        }
        child = next;
        index += 1;
    }
}

fn tint_nav_icon(app: &Rc<App>, row: &gtk::Button, path: &std::path::Path) {
    let Some(color) = app.meta.borrow().folder_color(path) else { return };
    let Some(body) = row.child().and_then(|c| c.downcast::<gtk::Box>().ok()) else { return };
    if let Some(image) = body.first_child().and_then(|c| c.downcast::<gtk::Image>().ok()) {
        image.add_css_class(&format!("tintfg-{}", crate::theme::color_class(&color)));
    }
}

fn set_active(row: &gtk::Button, active: bool) {
    if active {
        row.add_css_class("active");
    } else {
        row.remove_css_class("active");
    }
}

fn install_place_drop(app: &Rc<App>, row: &gtk::Button, dest: Option<std::path::PathBuf>) {
    let Some(dest) = dest else { return };
    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY | gtk::gdk::DragAction::MOVE,
    );
    let strong = Rc::downgrade(app);
    let row_ref = row.clone();
    let dest2 = dest.clone();
    target.connect_enter(move |_, _, _| {
        row_ref.add_css_class("drop-target");
        gtk::gdk::DragAction::MOVE
    });
    let row2 = row.clone();
    target.connect_leave(move |_| {
        row2.remove_css_class("drop-target");
    });
    target.connect_drop(move |_, value, _, _| {
        let Ok(files) = value.get::<gtk::gdk::FileList>() else { return false };
        let paths: Vec<std::path::PathBuf> = files.files().iter().filter_map(|f| f.path()).collect();
        if paths.is_empty() {
            return false;
        }
        if let Some(app) = strong.upgrade() {
            app.drop_paths_to(dest2.clone(), paths);
        }
        true
    });
    row.add_controller(target);
}

fn tag_hover_popover(row: &gtk::Button, name: &str, count: usize) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(row);
    popover.set_has_arrow(true);
    popover.set_position(gtk::PositionType::Right);
    popover.set_autohide(false);
    popover.set_can_target(false);
    popover.add_css_class("tag-hover-popover");

    let card = gtk::Box::new(gtk::Orientation::Vertical, 3);
    card.add_css_class("tag-hover-card");
    let title = gtk::Label::new(Some(name));
    title.add_css_class("tag-popover-title");
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.set_max_width_chars(28);
    card.append(&title);
    let detail = gtk::Label::new(Some(&format!(
        "{count} tagged item{}",
        if count == 1 { "" } else { "s" }
    )));
    detail.add_css_class("tiny");
    detail.add_css_class("dim");
    detail.set_xalign(0.0);
    card.append(&detail);

    // Two nested revealers combine a soft fade with a small horizontal slide.
    let slide = gtk::Revealer::new();
    slide.set_transition_type(gtk::RevealerTransitionType::SlideRight);
    slide.set_transition_duration(140);
    slide.set_reveal_child(true);
    slide.set_child(Some(&card));
    let fade = gtk::Revealer::new();
    fade.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    fade.set_transition_duration(120);
    fade.set_reveal_child(true);
    fade.set_child(Some(&slide));
    popover.set_child(Some(&fade));
    popover
}

fn rebuild_tags(app: &Rc<App>, container: &gtk::Box, filter: &str) {
    let mut child = container.first_child();
    while let Some(c) = child {
        let next = c.next_sibling();
        container.remove(&c);
        child = next;
    }
    let filter = filter.trim().to_lowercase();
    let active = app.active_location();
    let active_ids: Vec<String> = match &active {
        Location::Tag { ids, .. } => ids.clone(),
        _ => Vec::new(),
    };
    let tags = app.meta.borrow().tags.clone();
    let mini = app.settings.borrow().sidebar_mini;
    if tags.is_empty() {
        let hint = gtk::Label::new(Some("No tags yet"));
        hint.add_css_class("dim");
        hint.add_css_class("tiny");
        hint.set_xalign(0.0);
        hint.set_margin_start(14);
        hint.set_visible(!mini);
        container.append(&hint);
        return;
    }
    for tag in tags {
        if !filter.is_empty() && !tag.name.to_lowercase().contains(&filter) {
            continue;
        }
        let row = gtk::Button::new();
        row.add_css_class("sidebar-row");
        row.set_has_frame(false);
        let b = gtk::Box::new(gtk::Orientation::Horizontal, if mini { 0 } else { 8 });
        b.set_halign(if mini { gtk::Align::Center } else { gtk::Align::Fill });
        // Each tag carries its own symbolic icon, tinted with the tag colour so
        // the sidebar stays compact while remaining easy to scan.
        let icon = gtk::Image::from_icon_name(&tag.icon);
        icon.set_pixel_size(if mini { 16 } else { 14 });
        icon.add_css_class("tag-icon");
        icon.add_css_class(&format!("tintfg-{}", crate::theme::color_class(&tag.color)));
        icon.set_valign(gtk::Align::Center);
        b.append(&icon);
        let text = gtk::Label::new(Some(&tag.name));
        text.set_xalign(0.0);
        text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.set_visible(!mini);
        b.append(&text);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        spacer.set_visible(!mini);
        b.append(&spacer);
        let count = app.meta.borrow().paths_with_tags(&[tag.id.clone()], false).len();
        let badge = gtk::Label::new(Some(&count.to_string()));
        badge.add_css_class("sidebar-badge");
        badge.set_visible(!mini);
        b.append(&badge);
        row.set_child(Some(&b));
        row.set_tooltip_text(Some(&format!(
            "Show items tagged “{}” · {} item(s)",
            tag.name, count
        )));
        set_active(&row, active_ids.contains(&tag.id));
        {
            let strong = Rc::downgrade(app);
            let id = tag.id.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.activate_tag(&id);
                }
            });
        }
        // Drop files onto a tag assigns it.
        let target = gtk::DropTarget::new(
            gtk::gdk::FileList::static_type(),
            gtk::gdk::DragAction::COPY | gtk::gdk::DragAction::MOVE,
        );
        {
            let strong = Rc::downgrade(app);
            let id = tag.id.clone();
            let row_ref = row.clone();
            let name = tag.name.clone();
            target.connect_enter(move |_, _, _| {
                row_ref.add_css_class("drop-target");
                gtk::gdk::DragAction::COPY
            });
            let row2 = row.clone();
            target.connect_leave(move |_| {
                row2.remove_css_class("drop-target");
            });
            target.connect_drop(move |_, value, _, _| {
                let Ok(files) = value.get::<gtk::gdk::FileList>() else { return false };
                let paths: Vec<std::path::PathBuf> =
                    files.files().iter().filter_map(|f| f.path()).collect();
                if paths.is_empty() {
                    return false;
                }
                if let Some(app) = strong.upgrade() {
                    app.assign_tag_to_paths(&id, &paths);
                    app.notify(&format!("Tagged {} item(s) “{}”", paths.len(), name), false);
                }
                true
            });
        }
        row.add_controller(target.clone());

        // Hover discovery: a non-interactive popover shows the complete name
        // (which may be ellipsized in the sidebar) and the tagged item count.
        let popover = tag_hover_popover(&row, &tag.name, count);
        let motion = gtk::EventControllerMotion::new();
        {
            let popover = popover.clone();
            motion.connect_enter(move |_, _, _| {
                let fade = popover
                    .child()
                    .and_then(|c| c.downcast::<gtk::Revealer>().ok());
                let slide = fade
                    .as_ref()
                    .and_then(|outer| outer.child())
                    .and_then(|c| c.downcast::<gtk::Revealer>().ok());
                if let Some(fade) = &fade {
                    fade.set_reveal_child(false);
                }
                if let Some(slide) = &slide {
                    slide.set_reveal_child(false);
                }
                popover.popup();
                let fade = fade.clone();
                let slide = slide.clone();
                gtk4::glib::timeout_add_local_once(
                    std::time::Duration::from_millis(16),
                    move || {
                        if let Some(fade) = &fade {
                            fade.set_reveal_child(true);
                        }
                        if let Some(slide) = &slide {
                            slide.set_reveal_child(true);
                        }
                    },
                );
            });
        }
        {
            let popover = popover.clone();
            motion.connect_leave(move |_| {
                popover.popdown();
            });
        }
        row.add_controller(motion);
        {
            let popover = popover.clone();
            row.connect_destroy(move |_| popover.popdown());
        }
        container.append(&row);
    }
}
