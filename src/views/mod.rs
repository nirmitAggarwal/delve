//! The five browsing views. Every view consumes the same tab state, selection
//! model and context menus.

pub mod detail;
pub mod graph;
pub mod home;
pub mod icon;
pub mod recursive;
pub mod treemap;

use crate::app::App;
use crate::fs::Entry;
use crate::settings::ViewKind;
use gtk4 as gtk;
use gtk4::gdk::prelude::PaintableExt as _;
use gtk4::glib;
use gtk4::gsk::prelude::GskRendererExt as _;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

pub struct Views {
    pub stack: gtk::Stack,
    pub home: home::HomeView,
    pub icon: icon::IconView,
    pub detail: detail::DetailView,
    pub recursive: recursive::RecursiveView,
    pub treemap: treemap::TreemapView,
    pub graph: graph::GraphView,
}

impl Views {
    pub fn new(app: &Rc<App>, tab_id: u64) -> Views {
        let home = home::HomeView::new(app);
        let icon = icon::IconView::new(app, tab_id);
        let detail = detail::DetailView::new(app, tab_id);
        let recursive = recursive::RecursiveView::new(app, tab_id);
        let treemap = treemap::TreemapView::new(app, tab_id);
        let graph = graph::GraphView::new(app, tab_id);
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(130)
            .vexpand(true)
            .hexpand(true)
            .build();
        stack.add_named(&home.root, Some("home"));
        stack.add_named(&icon.overlay, Some("icon"));
        stack.add_named(&detail.scroller, Some("detail"));
        stack.add_named(&recursive.root, Some("recursive"));
        stack.add_named(&treemap.root, Some("treemap"));
        stack.add_named(&graph.root, Some("graph"));
        icon.install_drop(app, tab_id);
        Views { stack, home, icon, detail, recursive, treemap, graph }
    }

    /// The dashboard is a page of its own, not a folder listing.
    pub fn show_home(&self) {
        self.stack.set_visible_child_name("home");
    }

    /// Select and scroll to a path for quick-select navigation.
    pub fn reveal(&self, path: &Path) {
        if self.stack.visible_child_name().as_deref() == Some("icon") {
            self.icon.reveal(path);
        }
    }

    pub fn show(&self, kind: ViewKind) {
        let name = match kind {
            ViewKind::Icon => "icon",
            ViewKind::Detail => "detail",
            ViewKind::Recursive => "recursive",
            ViewKind::Treemap => "treemap",
            ViewKind::Graph => "graph",
        };
        self.stack.set_visible_child_name(name);
    }

    pub fn current(&self) -> ViewKind {
        match self.stack.visible_child_name().as_deref() {
            Some("detail") => ViewKind::Detail,
            Some("recursive") => ViewKind::Recursive,
            Some("treemap") => ViewKind::Treemap,
            Some("graph") => ViewKind::Graph,
            Some("home") => ViewKind::Icon,
            _ => ViewKind::Icon,
        }
    }
}

/// Build a themed, theme-aware icon for an entry. Folder colours are applied as
/// a tint class so they remain legible in every view and theme.
pub fn folder_icon_name(entry: &Entry) -> &'static str {
    let lower = entry.name.to_lowercase();
    if entry.path == crate::paths::home() || lower == "home" || lower == "~" {
        return "user-home";
    }
    if entry.path == crate::paths::user_dir(crate::paths::UserDir::Desktop) || lower.contains("desktop") {
        return "user-desktop";
    }
    if lower.contains("document") {
        return "folder-documents";
    }
    if lower.contains("download") {
        return "folder-download";
    }
    if lower.contains("picture") || lower.contains("image") {
        return "folder-pictures";
    }
    if lower.contains("music") || lower.contains("audio") {
        return "folder-music";
    }
    if lower.contains("video") || lower.contains("movie") {
        return "folder-videos";
    }
    if lower.contains("public") {
        return "folder-publicshare";
    }
    if lower.contains("template") {
        return "folder-templates";
    }
    "folder"
}

thread_local! {
    /// Recoloured folder artwork, keyed by (theme, icon, size, colour).
    static TINTED_FOLDERS: RefCell<HashMap<(String, String, i32, String), gtk::gdk::Texture>> =
        RefCell::new(HashMap::new());
}

/// The symbolic counterpart of a place-specific folder glyph.
fn symbolic_glyph(name: &str) -> String {
    if name.ends_with("-symbolic") {
        name.to_string()
    } else {
        format!("{name}-symbolic")
    }
}

/// Render a folder icon in a custom colour without changing its artwork.
///
/// The theme icon is rasterised once, its shading is kept and its hue is
/// replaced by the folder colour, so a Documents folder still reads as the
/// Documents folder. Until a renderer is available (before the window is
/// realised) the same glyph's symbolic form is used as a fallback.
pub fn folder_color_image(app: &App, entry: &Entry, pixel_size: i32, color: &str) -> gtk::Image {
    let name = folder_icon_name(entry);
    if let Some(texture) = tinted_folder_texture(app, name, pixel_size, color) {
        let image = gtk::Image::from_paintable(Some(&texture));
        image.set_pixel_size(pixel_size);
        return image;
    }
    gtk::Image::from_icon_name(&symbolic_glyph(name))
}

fn tinted_folder_texture(
    app: &App,
    name: &str,
    pixel_size: i32,
    color: &str,
) -> Option<gtk::gdk::Texture> {
    let (r, g, b, _) = crate::util::parse_color(color)?;
    let key = (
        app.settings.borrow().icon_theme.clone(),
        name.to_string(),
        pixel_size,
        color.to_ascii_lowercase(),
    );
    if let Some(texture) = TINTED_FOLDERS.with(|cache| cache.borrow().get(&key).cloned()) {
        return Some(texture);
    }
    let display = gtk::gdk::Display::default()?;
    let renderer = app.window.renderer()?;
    let theme = gtk::IconTheme::for_display(&display);
    let paintable = theme.lookup_icon(
        name,
        &[],
        pixel_size,
        1,
        gtk::TextDirection::None,
        gtk::IconLookupFlags::empty(),
    );
    let edge = pixel_size as f64;
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, edge, edge);
    let node = snapshot.to_node()?;
    let rect = gtk::graphene::Rect::new(0.0, 0.0, edge as f32, edge as f32);
    let texture = renderer.render_texture(&node, Some(&rect));
    let encoded = texture.save_to_png_bytes();
    let mut image = image::load_from_memory(&encoded).ok()?.to_rgba8();
    for pixel in image.pixels_mut() {
        let p = &mut pixel.0;
        if p[3] == 0 {
            continue;
        }
        // Keep each pixel's lightness so folder shading survives the recolour.
        let lum = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        let shade = 0.30 + 0.85 * lum;
        p[0] = (r as f32 * shade).min(255.0) as u8;
        p[1] = (g as f32 * shade).min(255.0) as u8;
        p[2] = (b as f32 * shade).min(255.0) as u8;
    }
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    let bytes = glib::Bytes::from(&png);
    let texture = gtk::gdk::Texture::from_bytes(&bytes).ok()?;
    TINTED_FOLDERS.with(|cache| {
        cache.borrow_mut().insert(key, texture.clone());
    });
    Some(texture)
}

/// Return the configured custom icon source for an entry, if any. Folder rules
/// match the folder colour, while file rules match MIME type or extension.
pub fn custom_icon_source(app: &App, entry: &Entry) -> Option<String> {
    let settings = app.settings.borrow();
    if entry.is_dir {
        let color = app.meta.borrow().folder_color(&entry.path);
        settings
            .custom_folder_icon(color.as_deref())
            .map(str::to_string)
    } else {
        let mime = entry.mime();
        settings
            .custom_file_icon(&mime, &entry.name)
            .map(str::to_string)
    }
}

/// True when a custom icon replaces the normal icon (and therefore should also
/// suppress thumbnail generation).
pub fn has_custom_icon(app: &App, entry: &Entry) -> bool {
    custom_icon_source(app, entry)
        .is_some_and(|source| crate::theme::custom_icon_image(&source, 16).is_some())
}

pub fn entry_icon(app: &App, entry: &Entry, pixel_size: i32) -> gtk::Image {
    let custom = custom_icon_source(app, entry);
    let custom_image = custom
        .as_deref()
        .and_then(|source| crate::theme::custom_icon_image(source, pixel_size));
    let image = if let Some(image) = custom_image {
        image
    } else if entry.is_dir {
        if let Some(color) = app.meta.borrow().folder_color(&entry.path) {
            // Recolour the folder's own theme artwork rather than substituting
            // a generic outlined folder, which looked like a different icon.
            folder_color_image(app, entry, pixel_size, &color)
        } else {
            gtk::Image::from_icon_name(folder_icon_name(entry))
        }
    } else {
        let mime = entry.mime();
        let named = match mime.as_str() {
            "application/zip" | "application/x-zip-compressed" | "application/x-compressed-tar"
            | "application/gzip" | "application/x-tar" | "application/x-7z-compressed"
            | "application/x-rar" | "application/x-rar-compressed" => Some("package-x-generic"),
            "application/pdf" => Some("application-pdf"),
            "image/svg+xml" => Some("image-x-generic"),
            _ => None,
        };
        if let Some(name) = named {
            gtk::Image::from_icon_name(name)
        } else {
            let icon = gtk::gio::content_type_get_icon(&mime);
            gtk::Image::from_gicon(&icon)
        }
    };
    image.set_pixel_size(pixel_size);
    image.add_css_class("entry-icon");
    if entry.is_dir {
        if let Some(color) = app.meta.borrow().folder_color(&entry.path) {
            image.add_css_class(&format!("tintfg-{}", crate::theme::color_class(&color)));
            image.set_tooltip_text(Some(&format!("Folder colour {}", color)));
        }
        if entry.broken_link {
            image.set_opacity(0.5);
        }
    }
    image
}

pub fn tag_summary(app: &App, path: &Path) -> String {    let meta = app.meta.borrow();
    let names = meta.tag_names_for(path);
    names.join(", ")
}

/// True when an entry can show a preview instead of its file icon.
pub fn wants_thumbnail(app: &App, entry: &Entry) -> bool {
    !has_custom_icon(app, entry) && (entry.is_image() || entry.is_video() || entry.is_pdf())
}

/// Accept file drops on a row. A folder row is the destination; dropping on a
/// file row targets the tab's current folder.
pub fn install_row_drop<W: IsA<gtk::Widget>>(widget: &W, app: &Rc<App>, tab_id: u64) {
    let row = widget.clone();
    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY | gtk::gdk::DragAction::MOVE,
    );
    let row_enter = row.clone();
    target.connect_enter(move |_, _, _| {
        row_enter.add_css_class("drop-hover");
        gtk::gdk::DragAction::MOVE
    });
    let row_leave = row.clone();
    target.connect_leave(move |_| row_leave.remove_css_class("drop-hover"));
    let strong = Rc::downgrade(app);
    let row_drop = row.clone();
    target.connect_drop(move |_, value, _, _| {
        let Ok(files) = value.get::<gtk::gdk::FileList>() else { return false };
        let paths: Vec<PathBuf> = files.files().iter().filter_map(|f| f.path()).collect();
        if paths.is_empty() {
            return false;
        }
        let Some(app) = strong.upgrade() else { return false };
        let hovered = row_drop.tooltip_text().map(PathBuf::from);
        let dest = hovered
            .filter(|path| path.is_dir())
            .or_else(|| {
                app.tab_by_id(tab_id)
                    .and_then(|tab| tab.state.borrow().location.path().cloned())
            });
        let Some(dest) = dest else { return false };
        app.drop_paths_to(dest, paths);
        true
    });
    widget.add_controller(target);
}

pub fn clear_children(container: &impl IsA<gtk::Widget>) {
    let mut child = container.first_child();
    while let Some(c) = child {
        let next = c.next_sibling();
        if let Some(parent) = c.parent() {
            if let Some(boxed) = parent.downcast_ref::<gtk::FlowBox>() {
                boxed.remove(&c);
            } else if let Some(listbox) = parent.downcast_ref::<gtk::ListBox>() {
                listbox.remove(&c);
            } else if let Some(b) = parent.downcast_ref::<gtk::Box>() {
                b.remove(&c);
            }
        }
        child = next;
    }
}

/// Convert a selection of entries to paths for the tab model.
pub fn paths(entries: &[Entry]) -> Vec<PathBuf> {
    entries.iter().map(|e| e.path.clone()).collect()
}

pub fn format_size(app: &App, bytes: u64) -> String {
    if app.settings.borrow().byte_sizes {
        format!("{} B", bytes)
    } else {
        crate::fs::human_size(bytes)
    }
}

pub fn weak(widget: &impl IsA<gtk::Widget>) -> glib::WeakRef<gtk::Widget> {
    let obj: gtk::Widget = widget.clone().upcast();
    obj.downgrade()
}

pub type ImageMap = Rc<std::cell::RefCell<HashMap<PathBuf, glib::WeakRef<gtk::Image>>>>;

pub fn downgrade_app(app: &Rc<App>) -> Weak<App> {
    Rc::downgrade(app)
}

/// Visit the set bits of a selection bitset. GTK4's Bitset exposes no iterator.
pub fn bitset_indices(bitset: &gtk::Bitset) -> Vec<u32> {
    if bitset.is_empty() {
        return Vec::new();
    }
    let max = bitset.maximum();
    (0..=max).filter(|i| bitset.contains(*i)).collect()
}
