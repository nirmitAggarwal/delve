//! Graph view: a layered, expanding map of the folder tree.
//!
//! The root sits on the left and every level grows to the right, so the shape
//! always reads as a hierarchy. Folders start two levels deep and expand when
//! clicked; positions animate between layouts instead of jumping around.

use super::format_size;
use crate::app::App;
use crate::fs::ScanResult;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Debug)]
struct GraphNode {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    depth: u32,
    label: String,
    is_dir: bool,
    size: u64,
    path: PathBuf,
    expandable: bool,
    expanded: bool,
}

const NODE_W: f64 = 158.0;
const NODE_H: f64 = 24.0;
const GAP_X: f64 = 54.0;
const GAP_Y: f64 = 9.0;
const LEVELS: u32 = 2;
const MAX_NODES: usize = 600;

pub struct GraphView {
    pub root: gtk::Box,
    pub drawing: gtk::DrawingArea,
    nodes: Rc<RefCell<Vec<GraphNode>>>,
    edges: Rc<RefCell<Vec<(usize, usize)>>>,
    transform: Rc<Cell<(f64, f64, f64)>>,
    selected: Rc<Cell<Option<usize>>>,
    hover: Rc<Cell<Option<usize>>>,
    anim_serial: Rc<Cell<u64>>,
    layout_key: Rc<RefCell<String>>,
    search: gtk::SearchEntry,
    summary: gtk::Label,
    empty: gtk::Label,
}

impl GraphView {
    pub fn new(app: &Rc<App>, tab_id: u64) -> GraphView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.add_css_class("delve-toolbar");
        bar.set_margin_start(10);
        bar.set_margin_end(10);
        bar.set_margin_top(4);
        bar.set_margin_bottom(4);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Find a node…"));
        search.set_width_chars(16);
        bar.append(&search);
        {
            let strong = Rc::downgrade(app);
            search.connect_search_changed(move |s| {
                if let Some(app) = strong.upgrade() {
                    app.set_graph_search(tab_id, &s.text());
                }
            });
        }

        let collapse = gtk::Button::with_label("Collapse");
        collapse.add_css_class("flat");
        collapse.set_tooltip_text(Some("Collapse every folder back to two levels"));
        bar.append(&collapse);
        {
            let strong = Rc::downgrade(app);
            collapse.connect_clicked(move |_| {
                if let Some(app) = strong.upgrade() {
                    app.collapse_graph(tab_id);
                }
            });
        }

        let fit = gtk::Button::with_label("Fit");
        fit.add_css_class("flat");
        fit.set_tooltip_text(Some("Fit the graph to the window"));
        bar.append(&fit);
        let reset = gtk::Button::from_icon_name("view-refresh-symbolic");
        reset.add_css_class("flat");
        reset.set_tooltip_text(Some("Reset pan and zoom"));
        bar.append(&reset);

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

        let nodes: Rc<RefCell<Vec<GraphNode>>> = Rc::new(RefCell::new(Vec::new()));
        let edges: Rc<RefCell<Vec<(usize, usize)>>> = Rc::new(RefCell::new(Vec::new()));
        let transform: Rc<Cell<(f64, f64, f64)>> = Rc::new(Cell::new((1.0, 0.0, 0.0)));
        let selected: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let hover: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let anim_serial: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let layout_key: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

        {
            let nodes = nodes.clone();
            let edges = edges.clone();
            let transform = transform.clone();
            let selected = selected.clone();
            let hover = hover.clone();
            drawing.set_draw_func(move |_, cr, width, height| {
                let (scale, tx, ty) = transform.get();
                let ox = width as f64 / 2.0 + tx;
                let oy = height as f64 / 2.0 + ty;
                let list = nodes.borrow();

                // Curved, dashed connectors from a parent's right edge to each
                // child's left edge.
                cr.set_source_rgba(0.52, 0.66, 0.92, 0.40);
                cr.set_line_width(1.2);
                cr.set_dash(&[5.0, 5.0], 0.0);
                for (a, b) in edges.borrow().iter() {
                    let (Some(parent), Some(child)) = (list.get(*a), list.get(*b)) else {
                        continue;
                    };
                    let x1 = ox + (parent.x + parent.w) * scale;
                    let y1 = oy + (parent.y + parent.h / 2.0) * scale;
                    let x2 = ox + child.x * scale;
                    let y2 = oy + (child.y + child.h / 2.0) * scale;
                    let bend = ((x2 - x1) * 0.55).max(12.0 * scale);
                    cr.move_to(x1, y1);
                    cr.curve_to(x1 + bend, y1, x2 - bend, y2, x2, y2);
                    let _ = cr.stroke();
                }
                cr.set_dash(&[], 0.0);

                for (i, node) in list.iter().enumerate() {
                    let x = ox + node.x * scale;
                    let y = oy + node.y * scale;
                    let w = node.w * scale;
                    let h = node.h * scale;
                    if w < 2.0 || h < 2.0 {
                        continue;
                    }
                    let (r, g, b) = node_color(node);
                    rounded_rect(cr, x, y, w, h, 7.0 * scale);
                    cr.set_source_rgba(r, g, b, 0.94);
                    let _ = cr.fill_preserve();
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.28);
                    cr.set_line_width(1.0);
                    let _ = cr.stroke();

                    let is_selected = selected.get() == Some(i);
                    let is_hovered = hover.get() == Some(i);
                    if is_selected || is_hovered {
                        rounded_rect(cr, x, y, w, h, 7.0 * scale);
                        cr.set_source_rgba(1.0, 1.0, 1.0, if is_selected { 0.95 } else { 0.55 });
                        cr.set_line_width(if is_selected { 2.0 } else { 1.4 });
                        let _ = cr.stroke();
                    }

                    if scale > 0.55 {
                        let _ = cr.save();
                        cr.rectangle(x, y, w, h);
                        let _ = cr.clip();
                        cr.set_source_rgba(0.97, 0.98, 1.0, 0.96);
                        cr.select_font_face(
                            "Sans",
                            gtk::cairo::FontSlant::Normal,
                            gtk::cairo::FontWeight::Bold,
                        );
                        cr.set_font_size(9.5 * scale);
                        let label_room = w - 56.0 * scale;
                        let max_chars = ((label_room / (5.6 * scale)) as usize).max(4);
                        let text = crate::util::ellipsize(&node.label, max_chars);
                        cr.move_to(x + 7.0 * scale, y + h / 2.0 + 3.4 * scale);
                        let _ = cr.show_text(&text);

                        let size_text = crate::fs::human_size(node.size);
                        cr.set_font_size(8.2 * scale);
                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.72);
                        let approx = size_text.chars().count() as f64 * 4.4 * scale;
                        cr.move_to(x + w - approx - 8.0 * scale, y + h / 2.0 + 3.0 * scale);
                        let _ = cr.show_text(&size_text);
                        let _ = cr.restore();

                        if node.expandable && w > 60.0 * scale {
                            cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
                            cr.set_font_size(10.0 * scale);
                            cr.move_to(x + w - 13.0 * scale, y + h / 2.0 + 3.5 * scale);
                            let _ = cr.show_text(if node.expanded { "−" } else { "+" });
                        }
                    } else {
                        // Too small for text: keep a faint expansion marker.
                        if node.expandable {
                            cr.set_source_rgba(1.0, 1.0, 1.0, 0.55);
                            cr.arc(x + w - 4.0, y + h / 2.0, 1.6, 0.0, std::f64::consts::TAU);
                            let _ = cr.fill();
                        }
                    }
                }
            });
        }

        let hit_test = {
            let nodes = nodes.clone();
            let transform = transform.clone();
            move |x: f64, y: f64, width: f64, height: f64| -> Option<usize> {
                let (scale, tx, ty) = transform.get();
                let wx = (x - (width / 2.0 + tx)) / scale;
                let wy = (y - (height / 2.0 + ty)) / scale;
                nodes
                    .borrow()
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, n)| wx >= n.x && wx <= n.x + n.w && wy >= n.y && wy <= n.y + n.h)
                    .map(|(i, _)| i)
            }
        };

        // Right-click: item menu on a node, workspace menu on empty canvas.
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let hit = hit_test.clone();
            let nodes = nodes.clone();
            let strong = Rc::downgrade(app);
            let view = drawing.clone();
            gesture.connect_pressed(move |_, _, x, y| {
                let Some(app) = strong.upgrade() else { return };
                let anchor: gtk::Widget = view.clone().upcast();
                let idx = hit(x, y, view.width() as f64, view.height() as f64);
                match idx.and_then(|i| nodes.borrow().get(i).cloned()) {
                    Some(node) => {
                        app.set_selection(tab_id, vec![node.path.clone()], Some(node.path.clone()));
                        app.show_context_menu(tab_id, &anchor, Some(node.path.clone()), x, y);
                    }
                    None => {
                        app.set_selection(tab_id, Vec::new(), None);
                        app.show_context_menu(tab_id, &anchor, None, x, y);
                    }
                }
                view.queue_draw();
            });
            drawing.add_controller(gesture);
        }

        // Click selects; a folder expands or collapses on a single click, and a
        // double click opens it.
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            let hit = hit_test.clone();
            let nodes = nodes.clone();
            let selected = selected.clone();
            let strong = Rc::downgrade(app);
            let view = drawing.clone();
            gesture.connect_released(move |g, n_press, x, y| {
                let Some(app) = strong.upgrade() else { return };
                let idx = hit(x, y, view.width() as f64, view.height() as f64);
                selected.set(idx);
                let Some(node) = idx.and_then(|i| nodes.borrow().get(i).cloned()) else {
                    app.set_selection(tab_id, Vec::new(), None);
                    view.queue_draw();
                    return;
                };
                app.set_selection(tab_id, vec![node.path.clone()], Some(node.path.clone()));
                if n_press >= 2 {
                    let entry = crate::fs::Entry::from_path(node.path.clone(), node.label.clone());
                    app.open_entry(tab_id, &entry);
                } else if node.is_dir && node.expandable {
                    app.toggle_graph_expanded(tab_id, &node.path);
                }
                let _ = g;
                view.queue_draw();
            });
            drawing.add_controller(gesture);
        }
        // Hover highlight
        {
            let motion = gtk::EventControllerMotion::new();
            let hit = hit_test.clone();
            let hover2 = hover.clone();
            let view = drawing.clone();
            motion.connect_motion(move |_, x, y| {
                let idx = hit(x, y, view.width() as f64, view.height() as f64);
                if hover2.get() != idx {
                    hover2.set(idx);
                    view.queue_draw();
                }
            });
            drawing.add_controller(motion);
        }
        // Pan. Gesture offsets are cumulative from the drag start, so rebase
        // from the transform captured on drag-begin instead of adding them on
        // every update (which accelerated the movement quadratically).
        {
            let drag = gtk::GestureDrag::new();
            drag.set_button(1);
            let transform = transform.clone();
            let view = drawing.clone();
            let start = Rc::new(Cell::new((1.0, 0.0, 0.0)));
            let start_begin = start.clone();
            let transform_begin = transform.clone();
            drag.connect_drag_begin(move |_, _, _| start_begin.set(transform_begin.get()));
            drag.connect_drag_update(move |_, dx, dy| {
                let (scale, tx, ty) = start.get();
                transform.set((scale, tx + dx, ty + dy));
                view.queue_draw();
            });
            drawing.add_controller(drag);
        }
        // Zoom via pinch
        {
            let zoom = gtk::GestureZoom::new();
            let transform = transform.clone();
            let view = drawing.clone();
            zoom.connect_scale_changed(move |_, factor| {
                let (scale, tx, ty) = transform.get();
                let new_scale = (scale * factor).clamp(0.15, 6.0);
                transform.set((new_scale, tx, ty));
                view.queue_draw();
            });
            drawing.add_controller(zoom);
        }
        // Zoom via scroll
        {
            let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
            let transform = transform.clone();
            let view = drawing.clone();
            scroll.connect_scroll(move |_, _, dy| {
                let (scale, tx, ty) = transform.get();
                let factor = if dy < 0.0 { 1.06 } else { 1.0 / 1.06 };
                transform.set(((scale * factor).clamp(0.15, 6.0), tx, ty));
                view.queue_draw();
                glib::Propagation::Stop
            });
            drawing.add_controller(scroll);
        }

        let info = gtk::Label::new(Some(
            "Click a folder to expand · double-click to open · drag to pan · scroll to zoom",
        ));
        info.add_css_class("status-text");
        info.set_xalign(0.0);
        info.set_margin_start(12);
        info.set_margin_end(12);
        info.set_margin_top(3);
        info.set_margin_bottom(5);
        root.append(&info);

        let empty = gtk::Label::new(Some("Nothing to draw yet"));
        empty.add_css_class("delve-empty");
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        empty.set_visible(false);
        let overlay = gtk::Overlay::builder().child(&drawing).build();
        overlay.add_overlay(&empty);
        overlay.set_vexpand(true);
        root.append(&overlay);

        {
            let transform = transform.clone();
            let view = drawing.clone();
            let nodes = nodes.clone();
            fit.connect_clicked(move |_| {
                fit_graph(&nodes, &transform, &view);
                view.queue_draw();
            });
        }
        {
            let transform = transform.clone();
            let view = drawing.clone();
            reset.connect_clicked(move |_| {
                transform.set((1.0, 0.0, 0.0));
                view.queue_draw();
            });
        }
        // Fit once the canvas has a size.
        {
            let nodes = nodes.clone();
            let transform = transform.clone();
            let view = drawing.clone();
            let fitted = Rc::new(Cell::new(false));
            view.add_tick_callback(move |view, _| {
                if view.width() <= 50 || view.height() <= 50 {
                    return glib::ControlFlow::Continue;
                }
                if !fitted.get() {
                    fitted.set(true);
                    fit_graph(&nodes, &transform, view);
                    view.queue_draw();
                }
                glib::ControlFlow::Break
            });
        }

        GraphView {
            root,
            drawing,
            nodes,
            edges,
            transform,
            selected,
            hover,
            anim_serial,
            layout_key,
            search,
            summary,
            empty,
        }
    }

    pub fn sync(&self, app: &Rc<App>, tab_id: u64) {
        let (scan, expanded, search_text, running, selection) = {
            let tabs = app.tabs.borrow();
            let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else { return };
            let s = tab.state.borrow();
            (
                s.scan.clone(),
                s.graph_expanded.clone(),
                s.graph_search.clone(),
                s.scan_running,
                s.selection.clone(),
            )
        };
        if self.search.text() != search_text {
            self.search.set_text(&search_text);
        }
        let Some(scan) = scan else {
            self.nodes.borrow_mut().clear();
            self.edges.borrow_mut().clear();
            self.layout_key.replace(String::new());
            self.drawing.queue_draw();
            self.empty.set_visible(true);
            self.empty.set_label(if running { "Scanning…" } else { "No graph data yet" });
            self.summary.set_text("");
            return;
        };
        let (nodes, edges) = build_layout(&scan, &expanded);
        let selected_idx = selection
            .iter()
            .next()
            .and_then(|p| nodes.iter().position(|n| &n.path == p));
        self.selected.set(selected_idx);
        self.summary.set_text(&format!(
            "{} nodes · {} · click a folder to expand",
            nodes.len(),
            format_size(app, scan.total_bytes)
        ));
        self.empty.set_visible(nodes.is_empty());
        self.empty.set_label("Nothing to draw yet");

        let key = nodes
            .iter()
            .map(|n| format!("{}:{}", n.path.to_string_lossy(), n.expanded))
            .collect::<Vec<_>>()
            .join("|");
        let changed = *self.layout_key.borrow() != key;
        let old_positions: HashMap<PathBuf, (f64, f64)> = self
            .nodes
            .borrow()
            .iter()
            .map(|n| (n.path.clone(), (n.x, n.y)))
            .collect();
        let animate = changed && !old_positions.is_empty() && !nodes.is_empty();
        self.layout_key.replace(key);

        self.edges.replace(edges);
        self.nodes.replace(nodes.clone());
        if changed {
            fit_graph(&self.nodes, &self.transform, &self.drawing);
        }

        if animate {
            let serial = self.anim_serial.get() + 1;
            self.anim_serial.set(serial);
            let from: Vec<(f64, f64)> = nodes
                .iter()
                .map(|n| *old_positions.get(&n.path).unwrap_or(&(n.x, n.y)))
                .collect();
            let nodes_rc = self.nodes.clone();
            let serial_rc = self.anim_serial.clone();
            let view = self.drawing.clone();
            let started = std::time::Instant::now();
            glib::timeout_add_local(std::time::Duration::from_millis(14), move || {
                if serial_rc.get() != serial {
                    return glib::ControlFlow::Break;
                }
                let t = (started.elapsed().as_secs_f64() / 0.20).min(1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                {
                    let mut current = nodes_rc.borrow_mut();
                    for (i, node) in current.iter_mut().enumerate() {
                        let (fx, fy) = from.get(i).copied().unwrap_or((node.x, node.y));
                        let target = &nodes[i];
                        node.x = fx + (target.x - fx) * eased;
                        node.y = fy + (target.y - fy) * eased;
                    }
                }
                view.queue_draw();
                if t >= 1.0 {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        }
        self.drawing.queue_draw();

        if !search_text.trim().is_empty() {
            if let Some(pos) = self
                .nodes
                .borrow()
                .iter()
                .position(|n| n.label.to_lowercase().contains(&search_text.to_lowercase()))
            {
                self.hover.set(Some(pos));
                self.drawing.queue_draw();
            }
        } else {
            self.hover.set(None);
        }
    }
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    cr.close_path();
}

fn fit_graph(
    nodes: &Rc<RefCell<Vec<GraphNode>>>,
    transform: &Rc<Cell<(f64, f64, f64)>>,
    view: &gtk::DrawingArea,
) {
    let list = nodes.borrow();
    if list.is_empty() {
        return;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for n in list.iter() {
        minx = minx.min(n.x);
        miny = miny.min(n.y);
        maxx = maxx.max(n.x + n.w);
        maxy = maxy.max(n.y + n.h);
    }
    let w = (maxx - minx).max(1.0);
    let h = (maxy - miny).max(1.0);
    let vw = view.width().max(60) as f64;
    let vh = view.height().max(60) as f64;
    let scale = ((vw * 0.9) / w).min((vh * 0.88) / h).clamp(0.14, 1.6);
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    transform.set((scale, -cx * scale, -cy * scale));
}

fn node_color(node: &GraphNode) -> (f64, f64, f64) {
    if node.depth == 0 {
        return (0.80, 0.32, 0.28);
    }
    if node.is_dir {
        return (0.24, 0.46, 0.80);
    }
    let ext = node
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif" | "tiff" | "tif" => {
            (0.16, 0.58, 0.55)
        }
        "mp4" | "mkv" | "webm" | "mov" | "avi" => (0.48, 0.38, 0.80),
        "mp3" | "flac" | "ogg" | "wav" | "m4a" => (0.82, 0.52, 0.26),
        "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" => (0.74, 0.60, 0.22),
        "pdf" | "doc" | "docx" | "odt" | "txt" | "md" | "rtf" => (0.24, 0.52, 0.78),
        "rs" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "go" | "java" | "sh" | "json" | "toml" => {
            (0.24, 0.62, 0.42)
        }
        _ => (0.40, 0.45, 0.55),
    }
}

fn build_layout(
    scan: &ScanResult,
    expanded: &HashSet<PathBuf>,
) -> (Vec<GraphNode>, Vec<(usize, usize)>) {
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, node) in scan.nodes.iter().enumerate().skip(1) {
        if let Some(parent) = node.parent {
            children.entry(parent).or_default().push(i);
        }
    }
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    if scan.nodes.is_empty() {
        return (nodes, edges);
    }
    let mut cursor = 0.0f64;
    layout_node(
        0,
        0,
        scan,
        &children,
        expanded,
        &mut nodes,
        &mut edges,
        &mut cursor,
    );
    (nodes, edges)
}

#[allow(clippy::too_many_arguments)]
fn layout_node(
    idx: usize,
    depth: u32,
    scan: &ScanResult,
    children: &HashMap<usize, Vec<usize>>,
    expanded: &HashSet<PathBuf>,
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<(usize, usize)>,
    cursor: &mut f64,
) -> Option<(f64, f64, usize)> {
    if nodes.len() >= MAX_NODES || idx >= scan.nodes.len() {
        return None;
    }
    let path = scan.nodes[idx].entry.path.clone();
    let mut kids = children.get(&idx).cloned().unwrap_or_default();
    kids.sort_by(|a, b| scan.nodes[*b].total.cmp(&scan.nodes[*a].total));
    // Two levels are always drawn; deeper levels appear only when the user
    // expands that folder.
    let show_kids = depth < LEVELS || expanded.contains(&path);
    let mut ranges: Vec<(f64, f64, usize)> = Vec::new();
    if show_kids {
        for kid in kids {
            if let Some(range) = layout_node(
                kid,
                depth + 1,
                scan,
                children,
                expanded,
                nodes,
                edges,
                cursor,
            ) {
                ranges.push(range);
            }
        }
    }
    let y = match (ranges.first(), ranges.last()) {
        (Some(first), Some(last)) => (first.0 + last.1) / 2.0 - NODE_H / 2.0,
        _ => {
            let y = *cursor;
            *cursor += NODE_H + GAP_Y;
            y
        }
    };
    let node = &scan.nodes[idx];
    let size = if node.entry.is_dir {
        node.total
    } else {
        node.entry.size.max(node.entry.allocated)
    };
    nodes.push(GraphNode {
        x: depth as f64 * (NODE_W + GAP_X),
        y,
        w: NODE_W,
        h: NODE_H,
        depth,
        label: node.entry.name.clone(),
        is_dir: node.entry.is_dir,
        size,
        path,
        expandable: node.entry.is_dir && node.child_count > 0,
        expanded: expanded.contains(&node.entry.path),
    });
    let my_index = nodes.len() - 1;
    for (_, _, child_index) in &ranges {
        edges.push((my_index, *child_index));
    }
    let top = ranges.first().map(|r| r.0).unwrap_or(y);
    let bottom = ranges.last().map(|r| r.1).unwrap_or(y + NODE_H);
    Some((top, bottom, my_index))
}
