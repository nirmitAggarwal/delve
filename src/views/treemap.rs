//! Treemap view: a proportional storage explorer with zoom, filters and
//! honest handling of unknown or unreadable sizes.

use super::format_size;
use crate::app::App;
use crate::fs::{ScanConfig, ScanResult};
use crate::model::TreemapFilter;
use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
    fn inset(&self, by: f64) -> Rect {
        Rect {
            x: self.x + by,
            y: self.y + by,
            w: (self.w - by * 2.0).max(0.0),
            h: (self.h - by * 2.0).max(0.0),
        }
    }
    fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }
}

#[derive(Clone)]
struct TRect {
    node: usize,
    rect: Rect,
    label: String,
    is_dir: bool,
    size: u64,
    children: usize,
    /// Palette colour for this folder (stable per path) or file type.
    color: (f64, f64, f64),
    /// Share of the visible root total, in percent.
    pct: f64,
}

pub struct TreemapView {
    pub root: gtk::Box,
    pub drawing: gtk::DrawingArea,
    rects: Rc<RefCell<Vec<TRect>>>,
    selected: Rc<RefCell<Option<PathBuf>>>,
    summary: gtk::Label,
    info: gtk::Label,
    back_btn: gtk::Button,
    metric_drop: gtk::DropDown,
    filter_drop: gtk::DropDown,
    legend: gtk::Box,
    empty: gtk::Label,
    app: std::rc::Weak<App>,
    tab_id: u64,            cursor: Rc<Cell<(f64, f64)>>,
    selected_node: Rc<Cell<Option<usize>>>,
    /// Name and total for the fixed header strip above the map.
    root_header: Rc<RefCell<Option<(String, u64)>>>,
    /// The scan whose nodes back the rectangles currently on screen.
    scan_used: Rc<RefCell<Option<Rc<ScanResult>>>>,
    /// Lazily loaded data for a folder opened from the treemap itself.
    local_scan: Rc<RefCell<Option<Rc<ScanResult>>>>,
    local_pending: Rc<RefCell<Option<PathBuf>>>,
}

const MAX_RECTS: usize = 2600;
const HEADER: f64 = 15.0;
const ROOT_HEADER: f64 = 18.0;
/// How many nested folder levels the treemap draws by default.
const TREEMAP_DEPTH: u32 = 3;

impl TreemapView {
    pub fn new(app: &Rc<App>, tab_id: u64) -> TreemapView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.add_css_class("delve-toolbar");
        bar.set_margin_start(10);
        bar.set_margin_end(10);
        bar.set_margin_top(4);
        bar.set_margin_bottom(4);

        let back_btn = gtk::Button::from_icon_name("go-up-symbolic");
        back_btn.add_css_class("flat");
        back_btn.set_tooltip_text(Some("Zoom out one level"));
        back_btn.set_sensitive(false);
        bar.append(&back_btn);
        {
            let strong = Rc::downgrade(app);
            back_btn.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.treemap_zoom_out(tab_id);
                }
            });
        }

        let metric_drop = gtk::DropDown::from_strings(&["Logical size", "Allocated size"]);
        metric_drop.set_tooltip_text(Some("Which size metric to visualise"));
        bar.append(&metric_drop);
        let filter_drop = gtk::DropDown::from_strings(&[
            TreemapFilter::All.label(),
            TreemapFilter::Files.label(),
            TreemapFilter::Folders.label(),
        ]);
        filter_drop.set_tooltip_text(Some("Filter which items are drawn"));
        bar.append(&filter_drop);
        {
            let strong = Rc::downgrade(app);
            metric_drop.connect_selected_notify(move |d| {
                if let Some(app) = strong.upgrade() {
                    app.set_treemap_options(tab_id, d.selected() == 1, None, false);
                }
            });
        }
        {
            let strong = Rc::downgrade(app);
            filter_drop.connect_selected_notify(move |d| {
                if let Some(app) = strong.upgrade() {
                    let filter = match d.selected() {
                        1 => TreemapFilter::Files,
                        2 => TreemapFilter::Folders,
                        _ => TreemapFilter::All,
                    };
                    app.set_treemap_options(tab_id, false, Some(filter), true);
                }
            });
        }
        // Each block's colour is derived from its share of the visible total.
        let legend = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let hint = gtk::Label::new(Some("Colour = size · blue → teal → amber"));
        hint.add_css_class("tiny");
        hint.add_css_class("dim");
        legend.append(&hint);
        bar.append(&legend);

        let summary = gtk::Label::new(None);
        summary.add_css_class("status-text");
        summary.set_hexpand(true);
        summary.set_xalign(1.0);
        summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        bar.append(&summary);
        root.append(&bar);

        let drawing = gtk::DrawingArea::new();
        drawing.set_hexpand(true);
        drawing.set_vexpand(true);
        drawing.add_css_class("delve-canvas");

        let info = gtk::Label::new(Some("Click a block to inspect · double-click a folder to zoom"));
        info.add_css_class("status-text");
        info.set_xalign(0.0);
        info.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        info.set_margin_start(12);
        info.set_margin_end(12);
        info.set_margin_top(3);
        info.set_margin_bottom(5);
        root.append(&info);

        let empty = gtk::Label::new(Some("No sizes to display"));
        empty.add_css_class("delve-empty");
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        empty.set_visible(false);
        let overlay = gtk::Overlay::builder().child(&drawing).build();
        overlay.add_overlay(&empty);
        overlay.set_vexpand(true);
        root.append(&overlay);

        let rects: Rc<RefCell<Vec<TRect>>> = Rc::new(RefCell::new(Vec::new()));
        let selected: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        let cursor: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));
        let root_header: Rc<RefCell<Option<(String, u64)>>> = Rc::new(RefCell::new(None));
        let scan_used: Rc<RefCell<Option<Rc<ScanResult>>>> = Rc::new(RefCell::new(None));
        let local_scan: Rc<RefCell<Option<Rc<ScanResult>>>> = Rc::new(RefCell::new(None));
        let local_pending: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

        // Draw callback
        let selected_node: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        {
            let rects = rects.clone();
            let selected_node = selected_node.clone();
            let root_header_draw = root_header.clone();
            drawing.set_draw_func(move |_, cr, width, _height| {
                // Root strip, matching the reference's labelled table header.
                if let Some((name, total)) = root_header_draw.borrow().as_ref() {
                    let (r, g, b) = ROOT_COLOR;
                    cr.set_source_rgba(r, g, b, 0.95);
                    cr.rectangle(0.0, 0.0, width as f64, ROOT_HEADER);
                    let _ = cr.fill();
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                    cr.select_font_face(
                        "Sans",
                        gtk::cairo::FontSlant::Normal,
                        gtk::cairo::FontWeight::Bold,
                    );
                    cr.set_font_size(10.0);
                    cr.move_to(7.0, 12.5);
                    let _ = cr.show_text(&format!(
                        "{}  ·  {}",
                        crate::util::ellipsize(name, 42),
                        crate::fs::human_size(*total)
                    ));
                }
                for tr in rects.borrow().iter() {
                    if tr.rect.area() <= 1.0 {
                        continue;
                    }
                    // The colour already encodes relative size; nesting is
                    // conveyed by the border instead of a hue shift.
                    let (r, g, b) = tr.color;
                    cr.set_source_rgba(r, g, b, 0.95);
                    cr.rectangle(tr.rect.x, tr.rect.y, tr.rect.w, tr.rect.h);
                    let _ = cr.fill();
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.22);
                    cr.set_line_width(1.0);
                    cr.rectangle(tr.rect.x, tr.rect.y, tr.rect.w, tr.rect.h);
                    let _ = cr.stroke();

                    if tr.rect.w > 46.0 && tr.rect.h > 18.0 {
                        // Only the lightest amber needs dark text; blue and
                        // teal blocks keep crisp white labels.
                        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                        let (trl, trg, trb) = if lum > 0.72 {
                            (0.08, 0.07, 0.04)
                        } else {
                            (1.0, 1.0, 1.0)
                        };
                        cr.set_source_rgba(trl, trg, trb, 0.95);
                        cr.select_font_face(
                            "Sans",
                            gtk::cairo::FontSlant::Normal,
                            gtk::cairo::FontWeight::Bold,
                        );
                        cr.set_font_size(10.0);
                        let max_chars = (tr.rect.w / 6.2) as usize;
                        let label = crate::util::ellipsize(&tr.label, max_chars.max(4));
                        let ty = tr.rect.y + 11.0;
                        cr.move_to(tr.rect.x + 4.0, ty);
                        let _ = cr.show_text(&label);
                        if tr.rect.h > 34.0 && tr.rect.w > 70.0 {
                            cr.set_source_rgba(trl, trg, trb, 0.78);
                            cr.set_font_size(9.0);
                            cr.move_to(tr.rect.x + 4.0, ty + 12.0);
                            let _ = cr.show_text(&format!(
                                "{} · {:.0}%",
                                crate::fs::human_size(tr.size),
                                tr.pct
                            ));
                        }
                    }
                }
                if let Some(node) = selected_node.get() {
                    for tr in rects.borrow().iter() {
                        if tr.node == node {
                            cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                            cr.set_line_width(2.0);
                            cr.rectangle(tr.rect.x + 1.0, tr.rect.y + 1.0, tr.rect.w - 2.0, tr.rect.h - 2.0);
                            let _ = cr.stroke();
                        }
                    }
                }
            });
        }

        // Hit testing
        let hit = {
            let rects = rects.clone();
            move |x: f64, y: f64| -> Option<TRect> {
                let list = rects.borrow();
                list.iter()
                    .rev()
                    .find(|tr| tr.rect.contains(x, y))
                    .cloned()
            }
        };
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            let hit = hit.clone();
            let strong = Rc::downgrade(app);
            let selected = selected.clone();
            let selected_node2 = selected_node.clone();
            let scan_used_click = scan_used.clone();
            gesture.connect_released(move |g, n_press, x, y| {
                let Some(app) = strong.upgrade() else { return };
                let Some(tr) = hit(x, y) else {
                    app.set_selection(tab_id, Vec::new(), None);
                    selected_node2.set(None);
                    return;
                };
                if let Some(path) = scan_path_for_rect(&scan_used_click, &tr) {
                    selected.replace(Some(path.clone()));
                    selected_node2.set(Some(tr.node));
                    app.set_selection(tab_id, vec![path.clone()], Some(path.clone()));
                    if n_press >= 2 && tr.is_dir {
                        app.set_treemap_zoom(tab_id, Some(path));
                    }
                }
                let _ = g;
            });
            drawing.add_controller(gesture);
        }
        {
            let motion = gtk::EventControllerMotion::new();
            let hit = hit.clone();
            let info_ref = info.clone();
            let cursor = cursor.clone();
            let strong = Rc::downgrade(app);
            let scan_used_hover = scan_used.clone();
            motion.connect_motion(move |_, x, y| {
                cursor.set((x, y));
                if let Some(tr) = hit(x, y) {
                    if let Some(app) = strong.upgrade() {
                        let path = scan_path_for_rect(&scan_used_hover, &tr)
                            .map(|p| crate::paths::display_path(&p))
                            .unwrap_or_default();
                        info_ref.set_text(&format!(
                            "{} · {} · {}",
                            tr.label,
                            format_size(&app, tr.size),
                            path
                        ));
                    }
                }
            });
            drawing.add_controller(motion);
        }

        TreemapView {
            root,
            drawing,
            rects,
            selected,
            summary,
            info,
            back_btn,
            metric_drop,
            filter_drop,
            legend,
            empty,
            app: Rc::downgrade(app),
            tab_id,
            cursor,
            selected_node,
            root_header,
            scan_used,
            local_scan,
            local_pending,
        }
    }

    /// Load a folder opened from inside the treemap. A shallow pass paints the
    /// direct children immediately, then a deeper pass fills in the detail, so
    /// browsing never falls back to an empty "no data" state.
    fn load_local_scan(&self, tab_id: u64, path: PathBuf) {
        if self.local_pending.borrow().as_ref() == Some(&path) {
            return;
        }
        self.local_pending.replace(Some(path.clone()));
        self.empty.set_visible(true);
        self.empty.set_label("Loading this folder…");
        std::thread::spawn(move || {
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            // First pass: direct children only, so the map appears immediately.
            let shallow = crate::fs::scan_tree(
                &path,
                ScanConfig { max_depth: 2, max_entries: 40_000, follow_symlinks: false },
                cancel.clone(),
                None,
            );
            crate::dispatch::post_simple(move |app| {
                if let Some(tab) = app.tab_by_id(tab_id) {
                    tab.views.treemap.set_local_scan(Rc::new(shallow));
                }
                app.sync_views(tab_id);
            });
            // Second pass: depth, laid over the shallow result.
            let deep = crate::fs::scan_tree(
                &path,
                ScanConfig { max_depth: 8, max_entries: 120_000, follow_symlinks: false },
                cancel,
                None,
            );
            crate::dispatch::post_simple(move |app| {
                if let Some(tab) = app.tab_by_id(tab_id) {
                    tab.views.treemap.finish_local_scan(Rc::new(deep));
                }
                app.sync_views(tab_id);
            });
        });
    }

    /// Publish a progressive result without releasing the pending lock.
    pub fn set_local_scan(&self, scan: Rc<ScanResult>) {
        self.local_scan.replace(Some(scan));
    }

    /// Publish the final result for the folder currently being loaded.
    pub fn finish_local_scan(&self, scan: Rc<ScanResult>) {
        if self.local_pending.borrow().as_ref() == Some(&scan.root) {
            self.local_pending.replace(None);
        }
        self.local_scan.replace(Some(scan));
    }

    pub fn sync(&self, app: &Rc<App>, tab_id: u64) {
        let (scan, zoom, metric_alloc, filter, selection, running) = {
            let tabs = app.tabs.borrow();
            let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else { return };
            let s = tab.state.borrow();
            (
                s.scan.clone(),
                s.treemap_root.clone(),
                s.treemap_metric_allocated,
                s.treemap_filter,
                s.selection.clone(),
                s.scan_running,
            )
        };
        self.metric_drop.set_selected(if metric_alloc { 1 } else { 0 });
        self.filter_drop.set_selected(match filter {
            TreemapFilter::All => 0,
            TreemapFilter::Files => 1,
            TreemapFilter::Folders => 2,
        });
        self.back_btn.set_sensitive(zoom.is_some());
        self.selected.replace(selection.iter().next().cloned());
        let selected_path = selection.iter().next().cloned();
        let selected_node_handle = self.selected_node.clone();
        // When the zoom target is outside the tab scan, load that folder on
        // demand instead of showing "no data".
        let scan = match &zoom {
            Some(path) => {
                let in_tab = scan
                    .as_ref()
                    .map(|s| s.nodes.iter().any(|n| &n.entry.path == path))
                    .unwrap_or(false);
                if in_tab {
                    scan.clone()
                } else if let Some(local) = self.local_scan.borrow().clone() {
                    if &local.root == path {
                        Some(local)
                    } else {
                        self.load_local_scan(tab_id, path.clone());
                        None
                    }
                } else {
                    self.load_local_scan(tab_id, path.clone());
                    None
                }
            }
            None => scan,
        };
        let Some(scan) = scan else {
            self.rects.borrow_mut().clear();
            self.root_header.replace(None);
            self.drawing.queue_draw();
            self.empty.set_visible(true);
            let loading = running || self.local_pending.borrow().is_some();
            self.empty.set_label(if loading { "Loading this folder…" } else { "No size data yet" });
            self.summary.set_text("");
            return;
        };
        self.scan_used.replace(Some(scan.clone()));
        selected_node_handle.set(
            selected_path
                .as_ref()
                .and_then(|p| scan.nodes.iter().position(|n| &n.entry.path == p)),
        );
        let width = self.drawing.width().max(320) as f64;
        let height = self.drawing.height().max(240) as f64;
        let mut out: Vec<TRect> = Vec::new();
        let root_idx = zoom
            .as_ref()
            .and_then(|z| scan.nodes.iter().position(|n| &n.entry.path == z))
            .unwrap_or(0);
        let children = build_children(&scan);
        let total = scan.nodes[root_idx].total.max(1);
        self.root_header.replace(Some((
            scan.nodes[root_idx].entry.name.clone(),
            scan.nodes[root_idx].total,
        )));
        let full = Rect {
            x: 0.0,
            y: ROOT_HEADER,
            w: width,
            h: (height - ROOT_HEADER).max(120.0),
        };
        layout_node(
            root_idx,
            full,
            0,
            &scan,
            &children,
            metric_alloc,
            filter,
            &mut out,
            app,
        );
        // Percent labels and colours always relate to the folder currently
        // being shown, so navigation, filtering and rescans all recolour.
        for rect in &mut out {
            rect.pct = rect.size as f64 / total as f64 * 100.0;
            rect.color = size_color(rect.size, total);
        }
        self.rects.replace(out.clone());
        self.empty.set_visible(out.is_empty());
        self.empty.set_label(if out.is_empty() {
            "No items with measurable size"
        } else {
            ""
        });
        let node = &scan.nodes[root_idx];
        let shown: u64 = out.iter().map(|r| r.size).sum();
        let mut text = format!(
            "{} · {} of {} · {} blocks",
            node.entry.name,
            format_size(app, shown),
            format_size(app, node.total),
            out.len()
        );
        if scan.truncated {
            text.push_str(" · truncated");
        }
        if scan.errors > 0 {
            text.push_str(&format!(" · {} unreadable", scan.errors));
        }
        if running {
            text.push_str(" · scanning…");
        }
        self.summary.set_text(&text);
        self.info.set_text("Click a block to inspect · double-click a folder to zoom");
        let _ = self.legend;
        let _ = self.cursor;
        self.drawing.queue_draw();
    }
}

fn build_children(scan: &ScanResult) -> HashMap<usize, Vec<usize>> {
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, node) in scan.nodes.iter().enumerate().skip(1) {
        if let Some(parent) = node.parent {
            children.entry(parent).or_default().push(i);
        }
    }
    children
}

fn node_size(scan: &ScanResult, idx: usize, allocated: bool) -> u64 {
    let e = &scan.nodes[idx].entry;
    if e.is_dir {
        scan.nodes[idx].total
    } else if allocated {
        e.allocated.max(e.size)
    } else {
        e.size
    }
}

fn layout_node(
    idx: usize,
    rect: Rect,
    depth: u32,
    scan: &ScanResult,
    children: &HashMap<usize, Vec<usize>>,
    allocated: bool,
    filter: TreemapFilter,
    out: &mut Vec<TRect>,
    app: &Rc<App>,
) {
    if out.len() >= MAX_RECTS || rect.w < 4.0 || rect.h < 4.0 {
        return;
    }
    let kids = children.get(&idx).cloned().unwrap_or_default();
    let mut items: Vec<(usize, u64)> = kids
        .into_iter()
        .filter_map(|k| {
            let node = &scan.nodes[k];
            if filter == TreemapFilter::Files && node.entry.is_dir {
                return None;
            }
            if filter == TreemapFilter::Folders && !node.entry.is_dir {
                return None;
            }
            let size = node_size(scan, k, allocated);
            if size == 0 {
                return None;
            }
            Some((k, size))
        })
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));
    if items.is_empty() {
        return;
    }
    let total: u64 = items.iter().map(|(_, s)| *s).sum();
    let area = rect.area();
    let scale = area / total as f64;
    let scaled: Vec<(usize, f64)> = items
        .iter()
        .map(|(k, s)| (*k, (*s as f64 * scale).max(1.0)))
        .collect();
    let placed = squarify(&scaled, rect);
    for (node, r) in placed {
        let node_ref = &scan.nodes[node];
        if r.area() < 2.0 {
            continue;
        }
        // Filled in by sync() once the visible total is known.
        let color = (0.0, 0.0, 0.0);
        out.push(TRect {
            node,
            rect: r,
            label: node_ref.entry.name.clone(),
            is_dir: node_ref.entry.is_dir,
            size: node_size(scan, node, allocated),
            children: node_ref.child_count,
            color,
            pct: 0.0,
        });
        if node_ref.entry.is_dir && r.w > 42.0 && r.h > 32.0 && depth + 1 < TREEMAP_DEPTH {
            let inner = Rect {
                x: r.x + 2.0,
                y: r.y + HEADER.min(r.h * 0.4),
                w: r.w - 4.0,
                h: r.h - HEADER.min(r.h * 0.4) - 2.0,
            };
            layout_node(node, inner, depth + 1, scan, children, allocated, filter, out, app);
        }
    }
    let _ = (HEADER, &app);
}

/// Squarified treemap layout: produces rectangles with aspect ratios close to 1.
fn squarify(items: &[(usize, f64)], rect: Rect) -> Vec<(usize, Rect)> {
    let mut out = Vec::new();
    if items.is_empty() {
        return out;
    }
    let total: f64 = items.iter().map(|(_, s)| *s).sum();
    if total <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return out;
    }
    let scale = rect.area() / total;
    let mut remaining = Rect { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
    let mut row: Vec<(usize, f64)> = Vec::new();
    let mut idx = 0;
    while idx < items.len() {
        let (node, size) = items[idx];
        let area = size * scale;
        let mut candidate = row.clone();
        candidate.push((node, area));
        let short = remaining.w.min(remaining.h);
        if row.is_empty() || worst_ratio(&candidate, short) <= worst_ratio(&row, short) {
            row = candidate;
            idx += 1;
        } else {
            remaining = layout_row(&row, remaining, &mut out);
            row.clear();
        }
    }
    if !row.is_empty() {
        layout_row(&row, remaining, &mut out);
    }
    out
}

fn worst_ratio(row: &[(usize, f64)], short: f64) -> f64 {
    if row.is_empty() || short <= 0.0 {
        return f64::INFINITY;
    }
    let total: f64 = row.iter().map(|(_, a)| *a).sum();
    let max = row.iter().map(|(_, a)| *a).fold(0.0, f64::max);
    let min = row.iter().map(|(_, a)| *a).fold(f64::INFINITY, f64::min);
    if total <= 0.0 || min <= 0.0 {
        return f64::INFINITY;
    }
    let side = total / short;
    (short * short * max / (total * total)).max(total * total / (short * short * min))
        .max(side / short)
}

fn layout_row(row: &[(usize, f64)], remaining: Rect, out: &mut Vec<(usize, Rect)>) -> Rect {
    let total: f64 = row.iter().map(|(_, a)| *a).sum();
    if total <= 0.0 {
        return remaining;
    }
    if remaining.w >= remaining.h {
        let col_w = (total / remaining.h).min(remaining.w);
        let mut y = remaining.y;
        for (node, area) in row {
            let h = if total > 0.0 { area / total * remaining.h } else { 0.0 };
            out.push((*node, Rect { x: remaining.x, y, w: col_w, h }));
            y += h;
        }
        Rect { x: remaining.x + col_w, y: remaining.y, w: (remaining.w - col_w).max(0.0), h: remaining.h }
    } else {
        let row_h = (total / remaining.w).min(remaining.h);
        let mut x = remaining.x;
        for (node, area) in row {
            let w = if total > 0.0 { area / total * remaining.w } else { 0.0 };
            out.push((*node, Rect { x, y: remaining.y, w, h: row_h }));
            x += w;
        }
        Rect { x: remaining.x, y: remaining.y + row_h, w: remaining.w, h: (remaining.h - row_h).max(0.0) }
    }
}

/// Root strip above the map: a deep red-brown so it frames the map without
/// competing with the size gradient.
const ROOT_COLOR: (f64, f64, f64) = (0.66, 0.26, 0.22);

/// Palette sampled from the Nirx reference, ordered from the smallest share of
/// the dataset to the largest: deep blue, muted blue, cyan-teal, teal, gold-green
/// and finally amber. Stops are interpolated, never snapped to categories.
const SIZE_STOPS: [(f64, (f64, f64, f64)); 8] = [
    (0.00, (0.13, 0.28, 0.46)), // #224876 deep blue
    (0.05, (0.17, 0.39, 0.61)), // #2C649B
    (0.12, (0.20, 0.43, 0.67)), // #346EAB muted blue
    (0.20, (0.24, 0.63, 0.60)), // #3EA09A cyan teal
    (0.35, (0.33, 0.64, 0.55)), // #55A28D teal
    (0.55, (0.60, 0.66, 0.39)), // #98A864 gold green
    (0.75, (0.86, 0.61, 0.23)), // #DC9D3B amber
    (1.00, (0.89, 0.64, 0.24)), // #E3A33D warm amber
];

/// Map an item's share of the visible total to a continuous colour. The square
/// root spreads tiny shares across the blue/teal range, so a single dominant
/// folder cannot flatten every other block into the same hue.
fn size_color(size: u64, total: u64) -> (f64, f64, f64) {
    if total == 0 {
        return SIZE_STOPS[0].1;
    }
    let share = (size as f64 / total as f64).clamp(0.0, 1.0);
    let t = share.sqrt();
    for pair in SIZE_STOPS.windows(2) {
        let (p0, c0) = pair[0];
        let (p1, c1) = pair[1];
        if t <= p1 {
            let span = p1 - p0;
            let f = if span <= f64::EPSILON { 0.0 } else { (t - p0) / span };
            return (
                c0.0 + (c1.0 - c0.0) * f,
                c0.1 + (c1.1 - c0.1) * f,
                c0.2 + (c1.2 - c0.2) * f,
            );
        }
    }
    SIZE_STOPS[SIZE_STOPS.len() - 1].1
}

fn scan_path_for_rect(
    scan: &Rc<RefCell<Option<Rc<ScanResult>>>>,
    tr: &TRect,
) -> Option<PathBuf> {
    let scan = scan.borrow().clone()?;
    scan.nodes.get(tr.node).map(|n| n.entry.path.clone())
}
