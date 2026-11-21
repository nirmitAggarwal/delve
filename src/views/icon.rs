//! Icon view: a responsive, keyboard-navigable grid with thumbnails and
//! rubber-band selection.

use super::{entry_icon, format_size};
use crate::app::App;
use crate::fs::Entry;
use crate::model::TabState;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub struct IconView {
    pub scroller: gtk::ScrolledWindow,
    pub flow: gtk::FlowBox,
    pub overlay: gtk::Overlay,
    pub empty: gtk::Box,
    empty_label: gtk::Label,
    entries: Rc<RefCell<Vec<Entry>>>,
    /// Fixed-size holders that swap their icon for a thumbnail picture.
    images: Rc<RefCell<HashMap<PathBuf, glib::WeakRef<gtk::Widget>>>>,
    syncing: Rc<Cell<bool>>,
}

impl IconView {
    pub fn new(app: &Rc<App>, tab_id: u64) -> IconView {
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::Multiple)
            .activate_on_single_click(false)
            .homogeneous(true)
            .column_spacing(2)
            .row_spacing(2)
            .margin_top(12)
            .margin_bottom(18)
            .margin_start(12)
            .margin_end(12)
            .valign(gtk::Align::Start)
            .halign(gtk::Align::Fill)
            .build();
        flow.add_css_class("delve-flowbox");

        let scroller = gtk::ScrolledWindow::builder()
            .child(&flow)
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scroller.add_css_class("delve-canvas");

        let rubber_area = gtk::DrawingArea::new();
        rubber_area.set_hexpand(true);
        rubber_area.set_vexpand(true);
        rubber_area.set_can_target(false);

        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty.set_valign(gtk::Align::Center);
        empty.set_halign(gtk::Align::Center);
        empty.add_css_class("delve-empty");
        empty.set_visible(false);
        let empty_icon = gtk::Image::from_icon_name("folder-open-symbolic");
        empty_icon.set_pixel_size(40);
        empty_icon.set_opacity(0.55);
        empty.append(&empty_icon);
        let empty_label = gtk::Label::new(Some("This folder is empty"));
        empty_label.add_css_class("delve-empty");
        empty.append(&empty_label);
        let empty_label_for_state = empty_label.clone();

        let overlay = gtk::Overlay::builder().child(&scroller).build();
        overlay.add_overlay(&rubber_area);
        overlay.add_overlay(&empty);

        let entries: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(Vec::new()));
        let images: Rc<RefCell<HashMap<PathBuf, glib::WeakRef<gtk::Widget>>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let syncing = Rc::new(Cell::new(false));

        // --- Selection sync -------------------------------------------------
        {
            let paths_entries = entries.clone();
            let strong = Rc::downgrade(app);
            let syncing = syncing.clone();
            let flow_ref = flow.clone();
            flow.connect_selected_children_changed(move |flow| {
                if syncing.get() {
                    return;
                }
                let mut selected = Vec::new();
                let list = paths_entries.borrow();
                for child in flow.selected_children() {
                    let idx = child.index();
                    if idx >= 0 {
                        if let Some(entry) = list.get(idx as usize) {
                            selected.push(entry.path.clone());
                        }
                    }
                }
                drop(list);
                let anchor = selected.last().cloned();
                if let Some(app) = strong.upgrade() {
                    syncing.set(true);
                    app.set_selection(tab_id, selected, anchor);
                    // Nested main-loop calls must not re-enter the handler.
                    let syncing = syncing.clone();
                    let flow = flow_ref.clone();
                    glib::idle_add_local_once(move || {
                        let _ = &flow;
                        syncing.set(false);
                    });
                }
            });
        }

        // --- Activation -----------------------------------------------------
        {
            let paths_entries = entries.clone();
            let strong = Rc::downgrade(app);
            flow.connect_child_activated(move |_, child| {
                let idx = child.index();
                let list = paths_entries.borrow();
                if let Some(entry) = list.get(idx.max(0) as usize) {
                    let entry = entry.clone();
                    drop(list);
                    if let Some(app) = strong.upgrade() {
                        app.open_entry(tab_id, &entry);
                    }
                }
            });
        }

        // --- Context menus --------------------------------------------------
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let flow_ref = flow.clone();
            let paths_entries = entries.clone();
            let strong = Rc::downgrade(app);
            gesture.connect_pressed(move |_, _, x, y| {
                let child = flow_ref.child_at_pos(x as i32, y as i32);
                let path = child.as_ref().and_then(|c| {
                    let idx = c.index();
                    paths_entries.borrow().get(idx.max(0) as usize).map(|e| e.path.clone())
                });
                if let Some(path) = &path {
                    if let Some(app) = strong.upgrade() {
                        if !app.tab_selection(tab_id).contains(path) {
                            app.set_selection(tab_id, vec![path.clone()], Some(path.clone()));
                        }
                    }
                }
                if let Some(app) = strong.upgrade() {
                    let anchor: gtk::Widget = flow_ref.clone().upcast();
                    app.show_context_menu(tab_id, &anchor, path, x, y);
                }
            });
            flow.add_controller(gesture);
        }

        // --- Click on empty space clears the selection ----------------------
        {
            let strong = Rc::downgrade(app);
            let flow_probe = flow.clone();
            let scroller_probe = scroller.clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_released(move |_, _, x, y| {
                let Some(app) = strong.upgrade() else { return };
                let origin = flow_probe
                    .compute_point(&scroller_probe, &gtk::graphene::Point::new(0.0, 0.0))
                    .unwrap_or_default();
                let fx = x - origin.x() as f64;
                let fy = y - origin.y() as f64;
                if flow_probe.child_at_pos(fx as i32, fy as i32).is_none() {
                    app.set_selection(tab_id, Vec::new(), None);
                }
            });
            scroller.add_controller(gesture);
        }

        // --- Drag out -------------------------------------------------------
        {
            let source = gtk::DragSource::new();
            source.set_actions(gdk_actions());
            let flow_ref = flow.clone();
            let paths_entries = entries.clone();
            let strong = Rc::downgrade(app);
            source.connect_prepare(move |_, x, y| {
                let app = strong.upgrade()?;
                let mut paths: Vec<PathBuf> =
                    app.tab_selection(tab_id).into_iter().collect();
                if let Some(child) = flow_ref.child_at_pos(x as i32, y as i32) {
                    let idx = child.index();
                    if let Some(entry) = paths_entries.borrow().get(idx.max(0) as usize) {
                        if !paths.contains(&entry.path) {
                            paths = vec![entry.path.clone()];
                            app.set_selection(tab_id, paths.clone(), Some(entry.path.clone()));
                        }
                    }
                }
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
            flow.add_controller(source);
        }

        // --- Rubber-band selection -----------------------------------------
        // The gesture lives on the scroller so it also covers empty space below
        // the last row; coordinates stay correct while the view is scrolled.
        let drag_start: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
        let rubber_rect: Rc<RefCell<Option<(f64, f64, f64, f64)>>> = Rc::new(RefCell::new(None));
        {
            let rect = rubber_rect.clone();
            rubber_area.set_draw_func(move |_, cr, _, _| {
                let Some((x, y, w, h)) = *rect.borrow() else { return };
                if w < 2.0 || h < 2.0 {
                    return;
                }
                cr.set_source_rgba(0.30, 0.55, 1.0, 0.14);
                cr.rectangle(x, y, w, h);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(0.36, 0.62, 1.0, 0.85);
                cr.set_line_width(1.0);
                let _ = cr.stroke();
            });
        }
        {
            let strong = Rc::downgrade(app);
            let gesture = gtk::GestureDrag::new();
            gesture.set_button(1);
            gesture.set_propagation_phase(gtk::PropagationPhase::Bubble);
            let flow_probe = flow.clone();
            let scroller_probe = scroller.clone();
            let start_begin = drag_start.clone();
            let rect_begin = rubber_rect.clone();
            let area_begin = rubber_area.clone();
            gesture.connect_drag_begin(move |g, x, y| {
                // Dragging a cell is drag and drop, not rubber-band selection.
                let origin = flow_probe
                    .compute_point(&scroller_probe, &gtk::graphene::Point::new(0.0, 0.0))
                    .unwrap_or_default();
                let fx = x - origin.x() as f64;
                let fy = y - origin.y() as f64;
                if flow_probe.child_at_pos(fx as i32, fy as i32).is_some() {
                    g.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                start_begin.set(Some((x, y)));
                rect_begin.replace(Some((x, y, 0.0, 0.0)));
                area_begin.queue_draw();
            });
            let start2 = drag_start.clone();
            let rect2 = rubber_rect.clone();
            let area2 = rubber_area.clone();
            gesture.connect_drag_update(move |_, dx, dy| {
                let Some((sx, sy)) = start2.get() else { return };
                let (x, y, w, h) = if dx < 0.0 && dy < 0.0 {
                    (sx + dx, sy + dy, -dx, -dy)
                } else if dx < 0.0 {
                    (sx + dx, sy, -dx, dy)
                } else if dy < 0.0 {
                    (sx, sy + dy, dx, -dy)
                } else {
                    (sx, sy, dx, dy)
                };
                rect2.replace(Some((x, y, w, h)));
                area2.queue_draw();
            });
            let start_end = drag_start.clone();
            let rect_end = rubber_rect.clone();
            let area_end = rubber_area.clone();
            let entries_end = entries.clone();
            let flow_end = flow.clone();
            let scroller_end = scroller.clone();
            gesture.connect_drag_end(move |g, _, _| {
                let Some((x, y, w, h)) = rect_end.take() else {
                    start_end.set(None);
                    return;
                };
                start_end.set(None);
                area_end.queue_draw();
                if w < 4.0 || h < 4.0 {
                    return;
                }
                let mut hits = Vec::new();
                let mut child = flow_end.first_child();
                while let Some(c) = child {
                    if let Some(bounds) = c.compute_bounds(&scroller_end) {
                        let ix = bounds.x() as f64;
                        let iy = bounds.y() as f64;
                        let iw = bounds.width() as f64;
                        let ih = bounds.height() as f64;
                        if ix < x + w && ix + iw > x && iy < y + h && iy + ih > y {
                            if let Some(idx) = c.downcast_ref::<gtk::FlowBoxChild>().map(|f| f.index()) {
                                if idx >= 0 {
                                    if let Some(entry) = entries_end.borrow().get(idx as usize) {
                                        hits.push(entry.path.clone());
                                    }
                                }
                            }
                        }
                    }
                    child = c.next_sibling();
                }
                let modifiers = g
                    .current_event()
                    .map(|e| e.modifier_state())
                    .unwrap_or(gdk::ModifierType::empty());
                if let Some(app) = strong.upgrade() {
                    let mut all: Vec<PathBuf> = if modifiers.contains(gdk::ModifierType::CONTROL_MASK)
                        || modifiers.contains(gdk::ModifierType::SHIFT_MASK)
                    {
                        app.tab_selection(tab_id).into_iter().collect()
                    } else {
                        Vec::new()
                    };
                    for hit in hits {
                        if !all.contains(&hit) {
                            all.push(hit);
                        }
                    }
                    app.set_selection(tab_id, all, None);
                }
            });
            scroller.add_controller(gesture);
        }

        IconView {
            scroller,
            flow,
            overlay,
            empty,
            empty_label: empty_label_for_state,
            entries,
            images,
            syncing,
        }
    }

    /// Rebuild the grid from the tab's current state.
    pub fn sync(&self, app: &Rc<App>, tab_id: u64) {
        let (entries, selection, icon_size) = {
            let tabs = app.tabs.borrow();
            let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else { return };
            let state = tab.state.borrow();
            let visible = state.visible_entries();
            (visible, state.selection.clone(), app.settings.borrow().icon_size)
        };
        self.syncing.set(true);
        self.entries.replace(entries.clone());
        self.images.borrow_mut().clear();

        let mut child = self.flow.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            self.flow.remove(&c);
            child = next;
        }

        let show_tags = app.settings.borrow().show_tag_dots;
        for entry in &entries {
            let cell = self.build_cell(app, tab_id, entry, icon_size, show_tags);
            self.flow.append(&cell);
        }

        let no_entries = entries.is_empty();
        self.empty.set_visible(no_entries);
        if no_entries {
            let title = {
                let tabs = app.tabs.borrow();
                let status = tabs.iter().find(|t| t.id == tab_id).map(|t| {
                    let s = t.state.borrow();
                    (s.loading, s.filter_text.trim().is_empty())
                });
                match status {
                    Some((true, _)) => "Loading…",
                    Some((_, true)) => "This folder is empty",
                    _ => "No items match the current filter",
                }
            };
            self.empty_label.set_label(title);
        }

        // Apply selection state.
        let mut child = self.flow.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            if let Some(c) = c.downcast_ref::<gtk::FlowBoxChild>() {
                let idx = c.index();
                let selected = idx >= 0
                    && entries
                        .get(idx as usize)
                        .map(|e| selection.contains(&e.path))
                        .unwrap_or(false);
                if selected {
                    self.flow.select_child(c);
                } else {
                    self.flow.unselect_child(c);
                }
            }
            child = next;
        }
        self.syncing.set(false);

        // Thumbnails: apply cached images immediately, then generate the rest.
        self.apply_thumbnails(app);
        super::queue_icons(app, &entries, icon_size);
    }

    fn build_cell(
        &self,
        app: &Rc<App>,
        tab_id: u64,
        entry: &Entry,
        icon_size: i32,
        show_tags: bool,
    ) -> gtk::FlowBoxChild {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 3);
        b.add_css_class("icon-cell");
        b.set_halign(gtk::Align::Center);
        b.set_valign(gtk::Align::Start);
        // A single fixed cell size keeps every item aligned whether its
        // artwork is a thumbnail or a plain icon.
        b.set_size_request(icon_size + 46, icon_size + 64);

        let image = entry_icon(app, entry, icon_size);
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.set_halign(gtk::Align::Center);
        holder.set_valign(gtk::Align::Center);
        holder.set_size_request(icon_size + 8, icon_size + 8);
        holder.set_widget_name("thumb-holder");
        holder.append(&image);
        b.append(&holder);
        if entry.is_image() || entry.is_video() || entry.is_pdf() {
            let holder_widget: gtk::Widget = holder.clone().upcast();
            self.images
                .borrow_mut()
                .insert(entry.path.clone(), holder_widget.downgrade());
        }

        let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        name_row.set_halign(gtk::Align::Center);
        if show_tags {
            let meta = app.meta.borrow();
            if let Some(color) = meta.tag_names_for(&entry.path).first().and_then(|_| {
                meta.item(&entry.path).and_then(|i| i.tags.first()).and_then(|id| meta.tag(id).map(|t| t.color.clone()))
            }) {
                drop(meta);
                let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                dot.add_css_class("color-dot");
                dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(&color)));
                dot.set_valign(gtk::Align::Center);
                dot.set_size_request(6, 6);
                name_row.append(&dot);
            }
        }
        name_row.set_size_request(-1, 34);
        let label = gtk::Label::new(Some(&entry.name));
        label.add_css_class("icon-label");
        label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        label.set_max_width_chars(18);
        label.set_lines(2);
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_justify(gtk::Justification::Center);
        name_row.append(&label);
        b.append(&name_row);

        let meta_label = gtk::Label::new(Some(&if entry.is_dir {
            entry.type_label()
        } else {
            format_size(app, entry.size)
        }));
        meta_label.add_css_class("icon-meta");
        meta_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        meta_label.set_max_width_chars(20);
        meta_label.set_size_request(-1, 16);
        b.append(&meta_label);

        b.set_tooltip_text(Some(&format!(
            "{}\n{}\n{}",
            entry.name,
            entry.path.display(),
            entry.type_label()
        )));

        let child = gtk::FlowBoxChild::new();
        child.set_child(Some(&b));
        child.set_tooltip_text(Some(&entry.path.display().to_string()));
        super::install_row_drop(&child, app, tab_id);
        if entry.hidden {
            b.add_css_class("dimmed");
        }
        child.set_focusable(true);

        // Selection shortcut: single click selects (already handled by FlowBox).
        let gesture = gtk::GestureClick::new();
        let strong = Rc::downgrade(app);
        let path = entry.path.clone();
        let child_ref = child.clone();
        gesture.connect_released(move |g, _, _, _| {
            if g.current_button() != 1 {
                return;
            }
            let state = g.current_event().map(|e| e.modifier_state()).unwrap_or(gdk::ModifierType::empty());
            let Some(app) = strong.upgrade() else { return };
            if state.contains(gdk::ModifierType::SHIFT_MASK) {
                app.extend_selection_to(tab_id, &path);
            } else if state.contains(gdk::ModifierType::CONTROL_MASK) {
                app.toggle_selection(tab_id, &path);
            } else {
                let current = app.tab_selection(tab_id);
                if current.len() > 1 && current.contains(&path) {
                    app.set_selection(tab_id, vec![path.clone()], Some(path.clone()));
                }
            }
            let _ = &child_ref;
        });
        child.add_controller(gesture);
        child
    }

    /// Handle drop of files onto blank grid space (destination = current folder).
    ///
    /// Thumbnails are applied in small batches. A folder with hundreds of
    /// images therefore paints immediately with its icons, while decoded
    /// previews fade in progressively instead of freezing the window.
    pub fn apply_thumbnails(&self, app: &Rc<App>) {
        let icon_size = app.settings.borrow().icon_size.max(24);
        let cache = app.thumbs.borrow();
        let images = self.images.borrow();
        let mut budget = 40usize;
        let mut pending = false;
        for (path, weak) in images.iter() {
            let Some(Some(thumb)) = cache.get(path) else { continue };
            let Some(holder) = weak.upgrade().and_then(|w| w.downcast::<gtk::Box>().ok()) else {
                continue;
            };
            // Already showing a thumbnail: never re-decode it.
            if holder
                .first_child()
                .map(|c| c.is::<gtk::Picture>())
                .unwrap_or(false)
            {
                continue;
            }
            if budget == 0 {
                pending = true;
                continue;
            }
            let Ok(texture) = gtk::gdk::Texture::from_filename(thumb) else { continue };
            budget -= 1;
            while let Some(child) = holder.first_child() {
                holder.remove(&child);
            }
            let pic = gtk::Picture::for_paintable(&texture);
            pic.set_content_fit(gtk::ContentFit::Contain);
            pic.set_can_shrink(true);
            pic.set_size_request(icon_size + 2, icon_size + 2);
            pic.set_halign(gtk::Align::Center);
            pic.set_valign(gtk::Align::Center);
            holder.append(&pic);
            fade_in(&pic);
        }
        drop(images);
        drop(cache);
        if pending {
            let app = app.clone();
            glib::idle_add_local_once(move || app.refresh_thumbnails());
        }
    }

    pub fn install_drop(&self, app: &Rc<App>, tab_id: u64) {
        let target = gtk::DropTarget::new(
            gtk::gdk::FileList::static_type(),
            gtk::gdk::DragAction::COPY | gtk::gdk::DragAction::MOVE,
        );
        let strong = Rc::downgrade(app);
        let flow = self.flow.clone();
        target.connect_enter(move |_, _, _| {
            flow.add_css_class("drop-hover");
            gtk::gdk::DragAction::MOVE
        });
        let flow2 = self.flow.clone();
        target.connect_leave(move |_| {
            flow2.remove_css_class("drop-hover");
        });
        let strong2 = strong.clone();
        target.connect_drop(move |_, value, _, _| {
            let Ok(files) = value.get::<gtk::gdk::FileList>() else { return false };
            let paths: Vec<PathBuf> = files.files().iter().filter_map(|f| f.path()).collect();
            let Some(app) = strong2.upgrade() else { return false };
            let Some(dir) = app
                .tab_by_id(tab_id)
                .and_then(|tab| tab.state.borrow().location.path().cloned())
            else {
                return false;
            };
            app.handle_external_drop(tab_id, &dir, paths);
            true
        });
        let _ = strong;
        self.flow.add_controller(target);
    }

    pub fn apply_selection(&self, selection: &std::collections::HashSet<PathBuf>) {
        self.syncing.set(true);
        let mut child = self.flow.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            if let Some(c) = c.downcast_ref::<gtk::FlowBoxChild>() {
                let idx = c.index();
                let selected = idx >= 0
                    && self
                        .entries
                        .borrow()
                        .get(idx as usize)
                        .map(|e| selection.contains(&e.path))
                        .unwrap_or(false);
                if selected {
                    self.flow.select_child(c);
                } else {
                    self.flow.unselect_child(c);
                }
            }
            child = next;
        }
        self.syncing.set(false);
    }
}

/// Fade a freshly applied thumbnail in so previews do not flash into place.
fn fade_in(widget: &impl IsA<gtk::Widget>) {
    let widget: gtk::Widget = widget.clone().upcast();
    widget.set_opacity(0.0);
    let start = std::time::Instant::now();
    widget.add_tick_callback(move |w, _| {
        let t = (start.elapsed().as_secs_f64() / 0.16).min(1.0);
        w.set_opacity(t);
        if t >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn gdk_actions() -> gtk::gdk::DragAction {
    gtk::gdk::DragAction::COPY | gtk::gdk::DragAction::MOVE | gtk::gdk::DragAction::LINK
}

/// Keep the `TabState` type in scope for documentation links.
#[allow(dead_code)]
fn _state_ty(_: &TabState) {}
