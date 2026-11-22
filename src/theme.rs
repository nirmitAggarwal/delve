//! Delve's own design system.
//!
//! All component styling is generated from semantic tokens shipped with the
//! application. Nothing here inherits GNOME/KDE shell styling, and the theme can
//! be edited as files and reloaded live without restarting.

use crate::settings::{Density, Settings};
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ThemeSpec {
    pub name: String,
    pub dark: bool,
    pub colors: BTreeMap<String, String>,
    pub metrics: BTreeMap<String, String>,
    pub typography: BTreeMap<String, String>,
    pub css: String,
}

impl Default for ThemeSpec {
    fn default() -> Self {
        ThemeSpec {
            name: "Untitled".into(),
            dark: true,
            colors: BTreeMap::new(),
            metrics: BTreeMap::new(),
            typography: BTreeMap::new(),
            css: String::new(),
        }
    }
}

pub struct Theme {
    pub id: String,
    pub spec: ThemeSpec,
    pub path: Option<PathBuf>,
}

const REQUIRED_COLORS: &[&str] = &[
    "window_bg", "sidebar_bg", "canvas_bg", "toolbar_bg", "tab_bg", "tab_active_bg",
    "inspector_bg", "floating_bg", "overlay_bg", "text_primary", "text_secondary",
    "text_muted", "text_disabled", "text_inverse", "accent", "accent_hover",
    "accent_muted", "focus_ring", "success", "warning", "danger", "info",
    "border_subtle", "border_strong", "border_selected", "separator", "sel_bg",
    "sel_border", "sel_multi", "hover_bg", "folder", "shadow",
];

fn dark_colors() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("window_bg", "#0f1115"),
        ("sidebar_bg", "#12151a"),
        ("canvas_bg", "#0b0d10"),
        ("toolbar_bg", "#12151a"),
        ("tab_bg", "#12151a"),
        ("tab_active_bg", "#1b2027"),
        ("inspector_bg", "#12151a"),
        ("floating_bg", "#1a1f26"),
        ("overlay_bg", "rgba(15,17,21,0.96)"),
        ("text_primary", "#edf2f0"),
        ("text_secondary", "#aab6b3"),
        ("text_muted", "#798581"),
        ("text_disabled", "#555f5c"),
        ("text_inverse", "#04110e"),
        ("accent", "#14b8a6"),
        ("accent_hover", "#2dd4bf"),
        ("accent_muted", "rgba(20,184,166,0.16)"),
        ("focus_ring", "rgba(45,212,191,0.75)"),
        ("success", "#43c98a"),
        ("warning", "#f2b134"),
        ("danger", "#f1646b"),
        ("info", "#5bc8c0"),
        ("border_subtle", "rgba(255,255,255,0.06)"),
        ("border_strong", "rgba(255,255,255,0.14)"),
        ("border_selected", "rgba(20,184,166,0.65)"),
        ("separator", "rgba(255,255,255,0.07)"),
        ("sel_bg", "rgba(20,184,166,0.20)"),
        ("sel_border", "rgba(20,184,166,0.55)"),
        ("sel_multi", "rgba(20,184,166,0.13)"),
        ("hover_bg", "rgba(255,255,255,0.05)"),
        ("folder", "#2dd4bf"),
        ("shadow", "rgba(0,0,0,0.45)"),
    ] {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

fn light_colors() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("window_bg", "#eef0f4"),
        ("sidebar_bg", "#e7eaf0"),
        ("canvas_bg", "#ffffff"),
        ("toolbar_bg", "#f4f6f9"),
        ("tab_bg", "#e9ecf2"),
        ("tab_active_bg", "#ffffff"),
        ("inspector_bg", "#f0f2f6"),
        ("floating_bg", "#ffffff"),
        ("overlay_bg", "rgba(238,240,244,0.98)"),
        ("text_primary", "#12151b"),
        ("text_secondary", "#3d4655"),
        ("text_muted", "#626c7c"),
        ("text_disabled", "#98a1ae"),
        ("text_inverse", "#ffffff"),
        ("accent", "#0b8277"),
        ("accent_hover", "#0a6f66"),
        ("accent_muted", "rgba(11,130,119,0.14)"),
        ("focus_ring", "rgba(11,130,119,0.7)"),
        ("success", "#178a56"),
        ("warning", "#8f6206"),
        ("danger", "#b32c39"),
        ("info", "#1a6f90"),
        ("border_subtle", "rgba(16,24,40,0.11)"),
        ("border_strong", "rgba(16,24,40,0.22)"),
        ("border_selected", "rgba(11,130,119,0.55)"),
        ("separator", "rgba(16,24,40,0.11)"),
        ("sel_bg", "rgba(11,130,119,0.16)"),
        ("sel_border", "rgba(11,130,119,0.5)"),
        ("sel_multi", "rgba(11,130,119,0.10)"),
        ("hover_bg", "rgba(16,24,40,0.06)"),
        ("folder", "#0b8277"),
        ("shadow", "rgba(16,24,40,0.14)"),
    ] {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

fn ember_colors() -> BTreeMap<String, String> {
    let mut m = dark_colors();
    for (k, v) in [
        ("accent", "#ff8352"),
        ("accent_hover", "#ff9d72"),
        ("accent_muted", "rgba(255,131,82,0.16)"),
        ("focus_ring", "rgba(255,157,114,0.75)"),
        ("border_selected", "rgba(255,131,82,0.6)"),
        ("sel_bg", "rgba(255,131,82,0.2)"),
        ("sel_border", "rgba(255,131,82,0.5)"),
        ("sel_multi", "rgba(255,131,82,0.12)"),
        ("folder", "#e8a06d"),
        ("canvas_bg", "#0f0e0d"),
        ("window_bg", "#131110"),
        ("sidebar_bg", "#181514"),
        ("toolbar_bg", "#181514"),
        ("tab_bg", "#181514"),
        ("tab_active_bg", "#211d1b"),
        ("inspector_bg", "#181514"),
        ("floating_bg", "#211d1b"),
        ("overlay_bg", "rgba(19,17,16,0.96)"),
    ] {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

fn default_metrics() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("radius_small", "8px"),
        ("radius_medium", "12px"),
        ("radius_large", "18px"),
        ("border_width", "1px"),
        ("row_height", "30px"),
        ("tab_height", "40px"),
        ("toolbar_height", "46px"),
        ("status_height", "26px"),
        ("panel_gap", "8px"),
    ] {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

fn default_typography() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("ui_font", ""),
        ("mono_font", "monospace"),
        ("base_size", "10.5pt"),
        ("small_size", "9pt"),
        ("title_size", "12pt"),
    ] {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

fn builtin_spec(id: &str) -> Option<ThemeSpec> {
    let (dark, colors, name) = match id {
        "dark" => (true, dark_colors(), "Delve Dark"),
        "light" => (false, light_colors(), "Delve Light"),
        "ember" => (true, ember_colors(), "Delve Ember"),
        _ => return None,
    };
    Some(ThemeSpec {
        name: name.into(),
        dark,
        colors,
        metrics: default_metrics(),
        typography: default_typography(),
        css: String::new(),
    })
}

pub const BUILTIN_THEMES: &[(&str, &str, bool)] =
    &[("dark", "Delve Dark", true), ("light", "Delve Light", false), ("ember", "Delve Ember", true)];

/// Resolve a theme id. Returns an error suitable for display when a custom theme
/// is missing or malformed.
pub fn load_theme(id: &str) -> Result<Theme, String> {
    if let Some(name) = id.strip_prefix("builtin:") {
        let spec = builtin_spec(name).ok_or_else(|| format!("Unknown built-in theme “{name}”"))?;
        return Ok(Theme { id: id.to_string(), spec, path: None });
    }
    if let Some(name) = id.strip_prefix("custom:") {
        let dir = crate::paths::themes_dir().join(name);
        let path = if dir.join("theme.json").is_file() {
            dir.join("theme.json")
        } else {
            dir.clone()
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("Could not read theme “{}”: {e}", path.display()))?;
        let mut spec: ThemeSpec = serde_json::from_str(&text)
            .map_err(|e| format!("Theme “{}” is not valid JSON: {e}", spec_display(&dir)))?;
        let fallback = if spec.dark { dark_colors() } else { light_colors() };
        for key in REQUIRED_COLORS {
            if !spec.colors.contains_key(*key) {
                if let Some(v) = fallback.get(*key) {
                    spec.colors.insert((*key).to_string(), v.clone());
                }
            }
        }
        for (k, v) in default_metrics() {
            spec.metrics.entry(k).or_insert(v);
        }
        for (k, v) in default_typography() {
            spec.typography.entry(k).or_insert(v);
        }
        // Optional companion stylesheet.
        let extra = dir.join("style.css");
        if let Ok(css) = std::fs::read_to_string(&extra) {
            spec.css.push_str(&css);
        }
        return Ok(Theme { id: id.to_string(), spec, path: Some(path) });
    }
    Err(format!("Unknown theme “{id}”"))
}

fn spec_display(dir: &std::path::Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.display().to_string())
}

/// The component stylesheet. Every value is a semantic token or metric.
fn base_css(spec: &ThemeSpec) -> String {
    let mut colors = String::new();
    for (k, v) in &spec.colors {
        colors.push_str(&format!("@define-color {} {};\n", k, v));
    }
    let typography = &spec.typography;
    let font_family = typography.get("ui_font").cloned().unwrap_or_default();
    let mono_family = typography.get("mono_font").cloned().unwrap_or_default();
    colors.push_str(&format!(
        "window {{ font-family: {}; font-size: {}; }}\n",
        if font_family.is_empty() { "inherit".to_string() } else { format!("\"{}\"", crate::util::css_string(&font_family)) },
        typography.get("base_size").cloned().unwrap_or_else(|| "10.5pt".into())
    ));
    if !mono_family.is_empty() {
        colors.push_str(&format!(
            ".mono {{ font-family: \"{}\"; }}\n",
            crate::util::css_string(&mono_family)
        ));
    }
    let mut out = format!(
        "{}\n{}\n{}",
        colors,
        r#"
/* ---------- window shell ---------- */
window.delve, .delve-root {
  background-color: @window_bg;
  color: @text_primary;
}
.delve-shell { background-color: @window_bg; }
.delve-sidebar { background-color: @sidebar_bg; border-right: 1px solid @border_subtle; }
.delve-inspector { background-color: @inspector_bg; border-left: 1px solid @border_subtle; }
.delve-canvas { background-color: @canvas_bg; }
.delve-toolbar { background-color: @toolbar_bg; border-bottom: 1px solid @border_subtle; }
.delve-navbar {
  background-color: @toolbar_bg;
  border: 1px solid @border_subtle;
  border-radius: 14px;
  padding: 5px 10px;
  min-height: 44px;
}
.delve-commandbar {
  background-color: @toolbar_bg;
  border: 1px solid @border_subtle;
  border-radius: 14px;
  padding: 3px 8px 4px 8px;
}
.command-button {
  background: transparent;
  border: none;
  box-shadow: none;
  border-radius: @radius_small;
  padding: 5px 9px;
  min-height: 30px;
  color: @text_primary;
}
.command-button:hover { background-color: @hover_bg; color: @text_primary; border: none; }
.command-button:disabled { color: @text_disabled; }
.command-button:active { background-color: @accent_muted; color: @text_primary; }
.command-button:disabled { color: @text_disabled; background: transparent; opacity: 0.55; }
.command-label { font-size: 9pt; }
.location-bar {
  background-color: @canvas_bg;
  border: 1px solid @border_subtle;
  border-radius: 999px;
  padding: 2px 5px 2px 7px;
  min-height: 34px;
}
.location-bar:focus-within { border-color: @accent; box-shadow: inset 0 0 0 1px @focus_ring; }
.location-bar .crumb { min-height: 28px; padding: 3px 8px; font-size: 9.5pt; }
.location-bar .crumb.current { background-color: @hover_bg; }
.delve-statusbar { background-color: @toolbar_bg; border-top: 1px solid @border_subtle; }
.delve-tabbar { background-color: @tab_bg; border-bottom: 1px solid @border_subtle; padding: 2px 4px 0 4px; }
.delve-tabbar-scrolled { background-color: @tab_bg; }
.delve-tab {
  background-color: transparent;
  border-radius: 8px 8px 0 0;
  border: 1px solid transparent;
  border-bottom: none;
  box-shadow: none;
  padding: 3px 4px 3px 10px;
  min-height: 32px;
  color: @text_secondary;
}
.delve-tab:hover { background-color: @hover_bg; color: @text_primary; }
.delve-tab.active {
  background-color: @tab_active_bg;
  color: @text_primary;
  border-color: @border_subtle;
  box-shadow: inset 0 2px 0 @accent, 0 -1px 0 @tab_active_bg;
}
.delve-tab.drop-target { background-color: @accent_muted; box-shadow: inset 0 0 0 2px @accent; }
.delve-tab-name { font-weight: 600; font-size: 9.5pt; }
.delve-tab-path { font-size: 8pt; color: @text_muted; }
.delve-tab-close { min-width: 20px; min-height: 20px; padding: 0; border-radius: 5px; background: transparent; box-shadow: none; }
.delve-tab-close:hover { background-color: @hover_bg; color: @text_primary; }
.delve-tab-add { min-width: 26px; min-height: 26px; padding: 0 6px; background: transparent; box-shadow: none; color: @text_secondary; border-radius: 6px; }
.delve-tab-add:hover { background-color: @hover_bg; color: @text_primary; }
.delve-tab-busy-badge { color: @warning; }

/* ---------- controls ---------- */
button {
  border-radius: @radius_small;
  border: 1px solid @border_subtle;
  background-image: none;
  background-color: @floating_bg;
  color: @text_primary;
  box-shadow: none;
  text-shadow: none;
  min-height: 26px;
  padding: 3px 10px;
}
button:hover { background-color: @hover_bg; border-color: @border_strong; }
button:active { background-color: @accent_muted; }
button:focus-visible, button:focus { box-shadow: inset 0 0 0 2px @focus_ring; outline: none; }
button:disabled { color: @text_disabled; border-color: @border_subtle; background-color: transparent; }
button.flat, button.toolbar-button, button.delve-flat {
  background-color: transparent; border: none; box-shadow: none; padding: 4px 7px; border-radius: @radius_small;
}
button.flat:hover, button.toolbar-button:hover, button.delve-flat:hover { background-color: @hover_bg; }
button.toolbar-button:checked, button.flat:checked, button.delve-flat:checked { background-color: @accent_muted; color: @text_primary; }
button.suggested, .suggested-action { background-color: @accent; color: @text_inverse; border-color: @accent; }
button.suggested:hover, .suggested-action:hover { background-color: @accent_hover; }
button.destructive-action { background-color: @danger; color: #ffffff; border-color: @danger; }
button.destructive-action:hover { background-color: @danger; }
separator { background-color: @separator; min-width: 1px; min-height: 1px; }
.section-title { font-size: 8.5pt; font-weight: 700; color: @text_muted; letter-spacing: 0.06em; }
.dim { color: @text_muted; }
.secondary { color: @text_secondary; }
.danger { color: @danger; }
.success { color: @success; }
.warning { color: @warning; }
.title-strong { font-weight: 700; }
.big-title { font-size: 15pt; font-weight: 700; }
.tiny { font-size: 8pt; }
.hint { font-size: 9pt; color: @text_secondary; }
.mono-path { font-family: monospace; font-size: 9pt; color: @text_muted; }
.card {
  background-color: @floating_bg;
  border: 1px solid @border_subtle;
  border-radius: @radius_medium;
  padding: 10px;
}
entry, spinbutton, dropdown, menubutton > button, .delve-entry {
  background-color: @canvas_bg;
  border: 1px solid @border_strong;
  border-radius: @radius_small;
  color: @text_primary;
  min-height: 26px;
  box-shadow: none;
}
entry:focus-within, entry:focus, .delve-entry:focus-within { border-color: @accent; box-shadow: inset 0 0 0 1px @focus_ring; }
entry placeholder { color: @text_muted; }
entry selection { background-color: @accent; color: @text_inverse; }
switch { background-color: @border_strong; border-radius: 999px; }
switch:checked { background-color: @accent; }
switch > slider { background-color: #ffffff; border-radius: 999px; }
checkbutton check, radiobutton radio { border: 1px solid @border_strong; background-color: @canvas_bg; }
checkbutton check:checked { background-color: @accent; color: @text_inverse; }
scrollbar { background-color: transparent; border: none; }
scrollbar slider { background-color: @border_strong; border-radius: 999px; min-width: 6px; min-height: 6px; margin: 2px; }
scrollbar slider:hover { background-color: @text_muted; }
scale trough { background-color: @border_subtle; border-radius: 999px; min-height: 4px; }
scale highlight { background-color: @accent; border-radius: 999px; }
progressbar trough { background-color: @border_subtle; border-radius: 999px; min-height: 5px; }
progressbar progress { background-color: @accent; border-radius: 999px; min-height: 5px; }

/* ---------- toolbar / breadcrumb ---------- */
.breadcrumb { background: transparent; }
.crumb { background: transparent; border: none; box-shadow: none; padding: 2px 7px; min-height: 24px; color: @text_secondary; border-radius: @radius_small; font-size: 9.5pt; }
.crumb:hover { background-color: @hover_bg; color: @text_primary; }
.crumb.current { color: @text_primary; font-weight: 600; }
.crumb-sep { color: @text_muted; font-size: 8pt; padding: 0; }
.toolbar-sep { background-color: @separator; min-width: 1px; min-height: 20px; margin: 0 4px; }
.search-pill { background-color: @canvas_bg; border: 1px solid @border_strong; border-radius: 999px; padding: 0 8px; }
.search-pill:focus-within { border-color: @accent; }
.search-pill entry { background: transparent; border: none; box-shadow: none; min-height: 24px; }

/* ---------- tabs / sidebar ---------- */
.delve-sidebar-scroller { background-color: @sidebar_bg; }
.sidebar-heading { font-size: 8.5pt; font-weight: 700; color: @text_muted; letter-spacing: 0.05em; padding: 10px 10px 4px 12px; }
.sidebar-row {
  padding: 5px 10px; margin: 0 6px; border-radius: @radius_small; min-height: 22px;
  color: @text_secondary; background: transparent; border: none; box-shadow: none;
}
.sidebar-row:hover { background-color: @hover_bg; color: @text_primary; }
.sidebar-row.active { background-color: @accent_muted; color: @text_primary; box-shadow: inset 0 0 0 1px @border_selected; }
.sidebar-row.drop-target { background-color: @accent_muted; box-shadow: inset 0 0 0 2px @accent; }
.sidebar-row label { font-size: 9.5pt; }
.sidebar-badge { font-size: 8pt; color: @text_muted; }
.pill-button { border-radius: 999px; padding: 1px 8px; min-height: 20px; font-size: 8.5pt; }

/* ---------- file items ---------- */
.delve-canvas { background-color: @canvas_bg; }
.icon-cell {
  border-radius: 10px;
  padding: 10px 8px;
  border: 1px solid transparent;
  background: transparent;
}
.icon-cell:hover { background-color: @hover_bg; }
.icon-preview {
  background-color: alpha(@floating_bg, 0.55);
  border: 1px solid @border_subtle;
  border-radius: 8px;
}
.icon-cell.selected { background-color: @sel_bg; border-color: @sel_border; }
.icon-cell.selected.multi { background-color: @sel_multi; }
/* GTK paints selected flow-box children with the system accent; keep the
   selection inside Delve's own cell so it matches the detail view. */
flowbox > flowboxchild { background: transparent; border-radius: 12px; }
flowbox > flowboxchild:selected { background: transparent; color: @text_primary; }
flowbox > flowboxchild:selected .icon-cell {
  background-color: @sel_bg;
  border-color: @sel_border;
}
flowbox > flowboxchild:hover .icon-cell { background-color: @hover_bg; }
flowbox > flowboxchild:focus-visible .icon-cell { box-shadow: inset 0 0 0 2px @focus_ring; }
.icon-cell.drop-target { background-color: @accent_muted; border-color: @accent; }
.icon-cell:focus-visible { box-shadow: inset 0 0 0 2px @focus_ring; }
.icon-label { font-size: 9pt; color: @text_primary; }
.icon-meta { font-size: 8pt; color: @text_muted; }
.entry-icon { -gtk-icon-shadow: none; }
.icon-cell:hover .entry-icon { -gtk-icon-transform: scale(1.03); }
.icon-cell.dimmed .icon-label, .icon-cell.dimmed .icon-meta { color: @text_muted; }
.thumb-placeholder { background-color: @hover_bg; border-radius: @radius_small; }
.rubber-band {
  background-color: @accent_muted;
  border: 1px solid @accent;
  border-radius: 2px;
}
.list-row, .detail-row { min-height: 26px; }
.detail-row:hover { background-color: @hover_bg; }
.detail-row.selected { background-color: @sel_bg; }
.detail-row:focus-visible { box-shadow: inset 0 0 0 1px @focus_ring; }
.cell-name { font-size: 9.5pt; }
.cell-meta { font-size: 9pt; color: @text_secondary; }
.cell-muted { font-size: 9pt; color: @text_muted; }
.column-header { color: @text_muted; font-size: 8.5pt; font-weight: 600; }
row:selected, .selected { color: @text_primary; }
listview, columnview, list, flowbox { background-color: transparent; }
listview > row, columnview > listview > row { border-radius: 0; }
listview > row:selected, columnview > listview > row:selected { background-color: @sel_bg; color: @text_primary; }
listview > row:hover { background-color: @hover_bg; }
.delve-empty { color: @text_muted; font-size: 10pt; }
.banner { background-color: @accent_muted; border: 1px solid @border_selected; border-radius: @radius_medium; padding: 6px 10px; }
.banner.error { background-color: rgba(241,100,107,0.12); border-color: rgba(241,100,107,0.4); }
.banner.warning { background-color: rgba(242,177,52,0.10); border-color: rgba(242,177,52,0.35); }
.banner .title { font-weight: 600; font-size: 9.5pt; }
.banner .detail { font-size: 9pt; color: @text_secondary; }

/* ---------- inspector ---------- */
.inspector-title { font-size: 11.5pt; font-weight: 700; }
.inspector-section { font-size: 8.5pt; font-weight: 700; color: @text_muted; letter-spacing: 0.05em; margin-top: 6px; }
.meta-key { font-size: 9pt; color: @text_muted; }
.meta-value { font-size: 9pt; color: @text_primary; }
.preview-frame { background-color: @canvas_bg; border: 1px solid @border_subtle; border-radius: @radius_medium; }
.preview-text { font-family: monospace; font-size: 8.5pt; color: @text_secondary; }
.chip {
  border-radius: 999px; padding: 1px 9px; font-size: 8.5pt;
  background-color: @accent_muted; border: 1px solid @border_selected; color: @text_primary;
}
.color-dot { border-radius: 999px; min-width: 10px; min-height: 10px; }

/* ---------- Run command panel ---------- */
.runner-panel {
  background-color: @panel_bg;
  border-top: 1px solid @border_subtle;
}
.runner-title { font-weight: 600; font-size: 9pt; }
.runner-session { border-top: 1px solid @border_subtle; }
.runner-session:first-child { border-top: none; }
.runner-command { font-size: 9pt; }
.runner-dot { font-size: 8pt; }
.runner-output-scroll { background-color: @canvas_bg; border-radius: 6px; margin: 2px 0 2px 0; }
.runner-output { font-family: monospace; font-size: 8.5pt; color: @text_secondary; padding: 6px; }
.runner-expander { font-size: 8.5pt; }

/* ---------- Quick Spotlight ---------- */
.spotlight-window { background: transparent; }
.spotlight {
  background-color: @floating_bg;
  border: 1px solid @border_strong;
  border-radius: 12px;
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45);
  margin: 6px;
}
.spotlight-entry { font-size: 12pt; padding: 8px 10px; }
.spotlight-row { border-radius: 7px; margin: 0 8px; padding: 1px 0; }
.spotlight-row:hover { background-color: @hover_bg; }
.spotlight-row.selected { background-color: @accent_muted; border: none; }
.spotlight-footer { padding: 6px 14px; border-top: 1px solid @border_subtle; }

/* Tag hover discovery: a compact card that floats above the sidebar. */
.tag-hover-card { padding: 3px 5px; }
.tag-popover-title { font-weight: 600; font-size: 9.5pt; }
.tag-icon { -gtk-icon-style: symbolic; }

/* ---------- status bar ---------- */
.status-text { font-size: 8.5pt; color: @text_secondary; }
.status-path { font-size: 8.5pt; color: @text_muted; }
.op-progress { min-height: 4px; min-width: 90px; }
.busy-spinner { color: @accent; }

/* ---------- popovers, menus, dialogs ---------- */
popover > contents, popover > arrow, .menu, menupopup, menu {
  background-color: @floating_bg;
  color: @text_primary;
  border: 1px solid @border_strong;
  border-radius: @radius_medium;
  box-shadow: 0 10px 30px @shadow;
  padding: 5px;
}
popover > contents > listview > row, menuitem { border-radius: @radius_small; padding: 4px 8px; }
.context-menu > contents { border-radius: 12px; padding: 0; }
.context-list { padding: 8px; }
.context-quick { padding: 1px; margin-bottom: 2px; }
.context-quick-button {
  background: transparent; border: none; box-shadow: none;
  border-radius: @radius_small; padding: 7px 3px; min-width: 44px;
}
.context-quick-button:hover { background-color: @hover_bg; }
.context-quick-label { font-size: 8.5pt; color: @text_primary; }
.context-icon { opacity: 0.9; }
menuitem:hover, popover row:hover { background-color: @hover_bg; }
modelbutton { padding: 4px 8px; border-radius: @radius_small; }
modelbutton:hover { background-color: @hover_bg; }
window.delve-dialog, dialog, .delve-dialog { background-color: @window_bg; color: @text_primary; }
window.delve-dialog headerbar, headerbar { background-color: @toolbar_bg; border-bottom: 1px solid @border_subtle; box-shadow: none; }
windowcontrols button { background: transparent; border: none; }
tooltip { background-color: @floating_bg; color: @text_primary; border: 1px solid @border_strong; border-radius: @radius_small; padding: 3px 7px; }
levelbar block { background-color: @accent; border-radius: 3px; }
levelbar trough { background-color: @border_subtle; border-radius: 3px; }

/* ---------- panels, splitters ---------- */
paned > separator { background-color: @separator; min-width: 1px; }
paned > separator:hover { background-color: @accent; }
.drawer { background-color: @sidebar_bg; border: 1px solid @border_strong; border-radius: @radius_large; box-shadow: 0 18px 50px @shadow; }
.drawer-scrim { background-color: rgba(0,0,0,0.35); }

/* ---------- density ---------- */
.density-compact .icon-cell { padding: 4px 3px; }
.density-compact .detail-row { min-height: 22px; }
.density-compact .sidebar-row { min-height: 20px; padding: 3px 8px; }
.density-compact .delve-tab { min-height: 30px; }
.density-spacious .icon-cell { padding: 12px 8px; }
.density-spacious .detail-row { min-height: 34px; }
.density-spacious .sidebar-row { min-height: 28px; padding: 7px 12px; }
.density-spacious .delve-tab { min-height: 42px; }
"#,
        spec.css
    );
    out.push_str(POLISH_CSS);
    if !spec.dark {
        out.push_str(LIGHT_CSS);
    }
    // GTK CSS has no custom properties for lengths, so metric tokens are
    // substituted textually after every sheet (including polish) is assembled.
    for (key, value) in &spec.metrics {
        out = out.replace(&format!("@{key}"), value);
    }
    out
}

/// Cross-cutting surface, spacing and motion rules. Kept separate from the
/// themeable sheet so every custom theme inherits the same polish. Metric
/// tokens are substituted over the combined sheet afterwards.
const POLISH_CSS: &str = r#"
/* ---------- glass surfaces ---------- */
window.delve { background-color: alpha(@window_bg, 0.6); }
.delve-shell { background-color: transparent; }
.delve-tabbar { background-color: alpha(@tab_bg, 0.55); }
.delve-navbar { background-color: alpha(@toolbar_bg, 0.55); }
.delve-commandbar { background-color: alpha(@toolbar_bg, 0.42); border-bottom: 1px solid @border_subtle; }
.delve-statusbar { background-color: alpha(@toolbar_bg, 0.42); }
.delve-sidebar { background-color: alpha(@sidebar_bg, 0.55); border-right: 1px solid @border_subtle; }
.delve-canvas, .delve-sidebar-scroller { background-color: alpha(@canvas_bg, 0.55); }
.inspector-root { background-color: alpha(@inspector_bg, 0.55); border-left: 1px solid @border_subtle; }

/* ---------- nested radii: outer = inner + surrounding padding ---------- */
.icon-cell { border-radius: 12px; padding: 8px 6px; }
.banner, .card, .preview-frame { border-radius: 12px; }
button, entry, dropdown, spinbutton, .delve-entry, .sidebar-row, .command-button { border-radius: 8px; }

/* ---------- lightweight, readable popovers ---------- */
popover.background { background: transparent; border: none; box-shadow: none; padding: 0; }
popover > contents {
  background-color: alpha(@floating_bg, 0.97);
  border: 1px solid @border_strong;
  border-radius: 14px;
  box-shadow: 0 12px 30px @shadow;
  padding: 6px;
  transition: opacity 120ms ease;
}
popover > arrow { background: none; border: none; box-shadow: none; }
popover.background > arrow { background: none; }
.context-menu > contents { border-radius: 14px; padding: 0; }
popover > contents modelbutton, popover > contents button.flat {
  border-radius: 8px; padding: 5px 8px; min-height: 26px;
}

/* ---------- view switcher & search ---------- */
.view-menu-button {
  background-color: transparent; border: 1px solid @border_subtle; border-radius: 8px;
  padding: 4px 8px; min-height: 28px;
}
.view-menu-button:hover { background-color: @hover_bg; border-color: @border_strong; }
.view-menu-label { font-size: 9pt; font-weight: 600; }
.view-menu-row { border-radius: 8px; }
.view-menu-row.current { background-color: @accent_muted; }

/* ---------- mini sidebar ---------- */
.sidebar-mini { padding: 0; }
.sidebar-mini .sidebar-row {
  padding: 6px 0; margin: 2px 10px; min-height: 34px;
  min-width: 0;
}
.sidebar-mini .sidebar-row image { -gtk-icon-size: 18px; }
.sidebar-mini .sidebar-row box { padding: 0; margin: 0; }
.sidebar-mini .sidebar-heading { opacity: 0; padding: 2px 0; min-height: 2px; font-size: 0; }
.sidebar-mini expander > title { min-height: 4px; padding: 0; }
.sidebar-mini expander > title > arrow { opacity: 0; min-width: 0; }
.sidebar-mini .tag-hint { opacity: 0; }

/* ---------- compact sort pill ---------- */
.sort-pill {
  background-color: alpha(@hover_bg, 0.55);
  border: 1px solid @border_subtle;
  border-radius: 999px;
  padding: 1px 9px;
  min-height: 24px;
}
.sort-pill:hover { background-color: @hover_bg; border-color: @border_strong; }
.sort-pill-label { font-size: 8.5pt; font-weight: 600; }
.sort-order { min-width: 26px; padding: 1px 6px; }

/* compact sort menu */
.sort-menu-content { padding: 6px; }
.sort-menu-content .sort-row { padding: 3px 8px; min-height: 24px; }
.sort-menu-content .sort-check { min-height: 22px; font-size: 9pt; padding: 2px 4px; }
.sort-menu-content .section-title { padding: 2px 4px 3px 4px; }

/* ---------- search grows, location shrinks ---------- */
.search-pill entry { min-width: 170px; }
.location-bar { transition: min-width 160ms ease; }

/* ---------- motion ---------- */
button, .sidebar-row, .command-button, .icon-cell, .detail-row, .delve-tab, .crumb {
  transition: background-color 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.icon-cell .entry-icon { transition: -gtk-icon-transform 140ms ease; }
.revealer-slide { transition: all 160ms cubic-bezier(0.2, 0, 0, 1); }

/* ---------- treemap legend ---------- */
.treemap-legend-bar { border-radius: 999px; }

/* ---------- properties ---------- */
.stat-card {
  background-color: @floating_bg;
  border: 1px solid @border_subtle;
  border-radius: @radius_medium;
  padding: 9px 11px;
}
.stat-card .stat-value { font-size: 12pt; font-weight: 700; }
.stat-card .stat-key { font-size: 8pt; color: @text_muted; letter-spacing: 0.04em; }
.stat-card .stat-sub { font-size: 8.5pt; color: @text_secondary; }
.hero-card {
  background-color: @accent_muted;
  border: 1px solid @border_selected;
  border-radius: @radius_medium;
  padding: 12px;
}
.pie-card {
  background-color: @floating_bg;
  border: 1px solid @border_subtle;
  border-radius: @radius_medium;
  padding: 10px;
}
.legend-label { font-size: 8.5pt; color: @text_secondary; }
"#;

/// Solid surfaces and contrast corrections for the light theme. The shared
/// polish sheet uses translucent surfaces that wash out on light backgrounds.
const LIGHT_CSS: &str = r#"
window.delve { background-color: @window_bg; }
.delve-shell { background-color: @window_bg; }
.delve-tabbar { background-color: @tab_bg; }
.delve-navbar, .delve-commandbar {
  background-color: @toolbar_bg;
  border-color: @border_subtle;
  box-shadow: 0 1px 2px alpha(@shadow, 0.35);
}
.delve-statusbar { background-color: @toolbar_bg; }
.delve-sidebar { background-color: @sidebar_bg; }
.delve-canvas, .delve-sidebar-scroller { background-color: @canvas_bg; }
.inspector-root { background-color: @inspector_bg; }
.delve-tab.active { background-color: @tab_active_bg; }
popover > contents {
  background-color: @floating_bg;
  border-color: @border_strong;
}
.command-button:disabled { color: @text_disabled; }
.sidebar-row { color: @text_secondary; }
.sidebar-row.active { color: @text_primary; }
.cell-meta, .meta-value { color: @text_primary; }
.delve-empty { color: @text_muted; }
.preview-frame { background-color: @canvas_bg; }
"#;

impl Theme {
    /// Full stylesheet for the component provider.
    pub fn css(&self) -> String {
        base_css(&self.spec)
    }

    /// Base directory for live-reload watching.
    pub fn watch_path(&self) -> Option<PathBuf> {
        self.path.clone()
    }
}

pub struct ThemeManager {
    pub provider: gtk::CssProvider,
    pub dynamic: gtk::CssProvider,
    pub overrides: gtk::CssProvider,
    pub current: RefCell<Theme>,
    pub last_error: RefCell<Option<String>>,
    pub folder_colors: RefCell<Vec<String>>,
}

impl ThemeManager {
    pub fn new(display: &gdk::Display, settings: &Settings) -> Rc<ThemeManager> {
        let provider = gtk::CssProvider::new();
        let dynamic = gtk::CssProvider::new();
        let overrides = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(display, &provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
        gtk::style_context_add_provider_for_display(
            display,
            &dynamic,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
        gtk::style_context_add_provider_for_display(
            display,
            &overrides,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 2,
        );
        let manager = Rc::new(ThemeManager {
            provider,
            dynamic,
            overrides,
            current: RefCell::new(Theme {
                id: "builtin:dark".into(),
                spec: builtin_spec("dark").unwrap(),
                path: None,
            }),
            last_error: RefCell::new(None),
            folder_colors: RefCell::new(Vec::new()),
        });
        manager.apply(settings);
        manager
    }

    /// Apply the theme selected in settings, keeping the previous theme if the
    /// new one cannot be parsed.
    pub fn apply(&self, settings: &Settings) {
        match load_theme(&settings.theme) {
            Ok(theme) => {
                let css = theme.css();
                match self.try_load(&self.provider, &css) {
                    Ok(()) => {
                        let dark = theme.spec.dark;
                        if let Some(display) = gdk::Display::default() {
                            gtk::Settings::for_display(&display)
                                .set_gtk_application_prefer_dark_theme(dark);
                        }
                        *self.current.borrow_mut() = theme;
                        *self.last_error.borrow_mut() = None;
                    }
                    Err(err) => {
                        *self.last_error.borrow_mut() = Some(err.clone());
                        crate::util::log(&format!("Theme rejected: {err}"));
                    }
                }
            }
            Err(err) => {
                *self.last_error.borrow_mut() = Some(err.clone());
                crate::util::log(&format!("Theme could not be loaded: {err}"));
            }
        }
        self.apply_overrides(settings);
        let colors = self.folder_colors.borrow().clone();
        self.apply_folder_colors(&colors, settings);
    }

    /// Load CSS into a temporary provider first so parse errors do not replace
    /// the last known-good theme.
    fn try_load(&self, provider: &gtk::CssProvider, css: &str) -> Result<(), String> {
        let probe = gtk::CssProvider::new();
        let error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let sink = error.clone();
        probe.connect_parsing_error(move |_, section, err| {
            let mut slot = sink.borrow_mut();
            if slot.is_none() {
                let loc = section.start_location();
                *slot = Some(format!(
                    "CSS error at line {} col {}: {err}",
                    loc.lines() + 1,
                    loc.line_chars() + 1
                ));
            }
        });
        probe.load_from_data(css);
        if let Some(err) = error.borrow().clone() {
            return Err(err);
        }
        provider.load_from_data(css);
        Ok(())
    }

    pub fn apply_overrides(&self, settings: &Settings) {
        let mut css = String::new();
        if !settings.accent.is_empty() {
            css.push_str(&format!(
                "@define-color accent {};\n@define-color accent_hover {};\n@define-color focus_ring {};\n",
                settings.accent,
                settings.accent,
                settings.accent
            ));
        }
        if !settings.ui_font.is_empty() {
            css.push_str(&format!(
                "window {{ font-family: \"{}\"; }}\n",
                crate::util::css_string(&settings.ui_font)
            ));
        }
        if !settings.mono_font.is_empty() {
            css.push_str(&format!(
                ".mono, .preview-text, .mono-path {{ font-family: \"{}\"; }}\n",
                crate::util::css_string(&settings.mono_font)
            ));
        }
        // Background opacity is applied only to surfaces. Text, icons, selection
        // fills and hover states keep the theme's full-strength colours.
        let mut alpha = settings.window_opacity.clamp(0.35, 1.0);
        if settings.transparency {
            alpha = alpha.min(0.90);
        }
        css.push_str(&format!(
            "window.delve, .delve-root {{ background-color: alpha(@window_bg, {alpha:.3}); }}\n\
             .delve-shell {{ background-color: transparent; }}\n\
             .delve-sidebar {{ background-color: alpha(@sidebar_bg, {alpha:.3}); }}\n\
             .delve-sidebar-scroller {{ background-color: transparent; }}\n\
             .delve-inspector, .inspector-root {{ background-color: alpha(@inspector_bg, {alpha:.3}); }}\n\
             .delve-canvas {{ background-color: alpha(@canvas_bg, {alpha:.3}); }}\n\
             .delve-toolbar, .delve-navbar, .delve-commandbar, .delve-statusbar {{ background-color: alpha(@toolbar_bg, {alpha:.3}); }}\n\
             .delve-tabbar {{ background-color: alpha(@tab_bg, {alpha:.3}); }}\n\
             .delve-tabbar-scrolled {{ background-color: transparent; }}\n\
             .delve-tab {{ background-color: alpha(@tab_bg, {alpha:.3}); }}\n\
             .delve-tab.active {{ background-color: alpha(@tab_active_bg, {alpha:.3}); }}\n"
        ));
        let _ = self.try_load(&self.overrides, &css);
    }

    /// Generate styling for tag colours and folder colours. Colours are
    /// sanitised before interpolation so theme files cannot inject CSS.
    pub fn apply_folder_colors(&self, colors: &[String], _settings: &Settings) {
        let mut css = String::new();
        let mut seen: Vec<String> = Vec::new();
        // Always define the built-in folder and tag palettes so swatches work
        // even in a fresh profile with no coloured folders yet.
        let mut all: Vec<String> = FOLDER_PALETTE.iter().map(|(_, c)| (*c).to_string()).collect();
        all.extend([
            "#5b9dff", "#43c98a", "#f2b134", "#e2679b", "#a78bfa", "#38bdf8",
        ].iter().map(|c| (*c).to_string()));
        all.extend(colors.iter().cloned());
        for color in &all {
            let Some(hex) = sanitize_color(color) else { continue };
            if seen.contains(&hex) {
                continue;
            }
            seen.push(hex.clone());
            let class = color_class(&hex);
            let text = if crate::util::is_light_color(&hex) { "#101216" } else { "#f2f5fa" };
            let (r, g, b, _) = crate::util::parse_color(&hex).unwrap_or((120, 120, 120, 255));
            css.push_str(&format!(
                ".{class} {{ color: {hex}; }}\n\
                 .tagchip-{class} {{ background-color: rgba({r},{g},{b},0.22); color: {text}; border: 1px solid rgba({r},{g},{b},0.55); }}\n\
                 .tintbg-{class} {{ background-color: rgba({r},{g},{b},0.18); }}\n\
                 .tintfg-{class} {{ color: {hex}; }}\n\
                 .entry-icon.tintfg-{class}, .tag-icon.tintfg-{class} {{ color: {hex}; }}\n\
                 .swatch-{class} {{ background-color: {hex}; border: 1px solid rgba(0,0,0,0.25); }}\n"
            ));
        }
        let _ = self.try_load(&self.dynamic, &css);
        *self.folder_colors.borrow_mut() = seen;
    }

    pub fn reload(&self, settings: &Settings) -> Result<(), String> {
        self.apply(settings);
        match self.last_error.borrow().clone() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Path to watch for live reload, if the active theme is file-backed.
    pub fn watch_path(settings: &Settings) -> Option<PathBuf> {
        settings
            .theme
            .strip_prefix("custom:")
            .map(|name| crate::paths::themes_dir().join(name).join("theme.json"))
    }
}

fn sanitize_color(value: &str) -> Option<String> {
    let (r, g, b, a) = crate::util::parse_color(value)?;
    if a == 255 {
        Some(crate::util::color_to_hex(r, g, b))
    } else {
        Some(format!("rgba({r},{g},{b},{:.2})", a as f32 / 255.0))
    }
}

/// Canonical colour representation used for icon-rule keys and CSS classes.
pub fn normalize_color(value: &str) -> Option<String> {
    sanitize_color(value)
}

/// Copy an uploaded icon into Delve's data directory and return a stable
/// `file:` source. Only image formats GTK can render are accepted.
pub fn import_icon_file(source: &std::path::Path) -> Result<String, String> {
    let metadata = std::fs::metadata(source)
        .map_err(|err| format!("Could not read that icon: {err}"))?;
    if !metadata.is_file() {
        return Err("Choose an image file.".into());
    }
    if metadata.len() > 8 * 1024 * 1024 {
        return Err("Icon files must be smaller than 8 MB.".into());
    }
    let extension = source
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
        return Err("Use a PNG, JPEG, WebP or GIF icon.".into());
    }
    let dir = crate::paths::icons_dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("Could not create the icons folder: {err}"))?;
    let destination = dir.join(format!("custom-{}.{extension}", crate::util::now_millis()));
    std::fs::copy(source, &destination)
        .map_err(|err| format!("Could not save that icon: {err}"))?;
    Ok(format!("file:{}", destination.display()))
}

/// Short human-readable label for an icon source.
pub fn icon_source_label(source: &str) -> String {
    if let Some(path) = source.strip_prefix("file:") {
        std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| source.to_string())
    } else {
        source.to_string()
    }
}

/// Build an icon widget from a stored source. Missing files or unknown themed
/// icons return `None`, letting callers use the normal fallback icon.
pub fn custom_icon_image(source: &str, pixel_size: i32) -> Option<gtk::Image> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    let file_path = source
        .strip_prefix("file:")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let path = std::path::PathBuf::from(source);
            path.is_file().then_some(path)
        });
    if let Some(path) = file_path {
        if !path.is_file() {
            return None;
        }
        let image = gtk::Image::from_file(&path);
        image.set_pixel_size(pixel_size);
        return Some(image);
    }
    if let Some(display) = gdk::Display::default() {
        if !gtk::IconTheme::for_display(&display).has_icon(source) {
            return None;
        }
    }
    let image = gtk::Image::from_icon_name(source);
    image.set_pixel_size(pixel_size);
    Some(image)
}

pub fn color_class(color: &str) -> String {
    // The stylesheet always hashes the normalised colour, so class lookups
    // must normalise too ("#3EA09A" and "#3ea09a" are the same colour).
    let normalized = sanitize_color(color).unwrap_or_else(|| color.trim().to_ascii_lowercase());
    format!("c{}", crate::util::hash_key(&[normalized.as_str()]))
}

/// Known palette offered in folder-colour menus.
pub const FOLDER_PALETTE: &[(&str, &str)] = &[
    ("Blue", "#4c8dff"),
    ("Teal", "#2ec4b6"),
    ("Green", "#43c98a"),
    ("Lime", "#a3d977"),
    ("Amber", "#f2b134"),
    ("Orange", "#f28c4b"),
    ("Red", "#f1646b"),
    ("Pink", "#e2679b"),
    ("Purple", "#a78bfa"),
    ("Slate", "#7f8c9b"),
];

/// The complete icon theme embedded in the Delve binary. Keeping this as the
/// default means the UI never depends on a desktop icon theme being installed.
pub const BUNDLED_ICON_THEME: &str = "Delve";

/// Register the embedded icon theme with the current display. This must run
/// after GTK has initialised and before the first themed widget is built.
pub fn init_bundled_icons() {
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_resource_path("/io/github/delve/FileManager/icons");
    }
}

/// Detect installed icon themes without assuming any particular one.
pub fn detect_icon_themes() -> Vec<String> {
    let mut names = vec![BUNDLED_ICON_THEME.to_string()];
    let mut roots = vec![
        crate::paths::home().join(".local/share/icons"),
        crate::paths::home().join(".icons"),
    ];
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&dirs) {
            roots.push(dir.join("icons"));
        }
    }
    roots.push(PathBuf::from("/usr/share/icons"));
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for item in rd.flatten() {
            let path = item.path();
            if path.is_dir() && path.join("index.theme").is_file() {
                if let Some(name) = path.file_name() {
                    names.push(name.to_string_lossy().to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

pub fn apply_icon_theme(name: &str) {
    // GTK's display icon theme is a singleton and must not be renamed
    // directly; setting the GtkSettings property is the supported way and it
    // re-resolves themed icons live.
    if let Some(display) = gdk::Display::default() {
        let settings = gtk::Settings::for_display(&display);
        if name.is_empty() {
            if let Some(default_name) = fallback_icon_theme() {
                settings.set_gtk_icon_theme_name(Some(&default_name));
            }
        } else {
            settings.set_gtk_icon_theme_name(Some(name));
        }
    }
}

/// The bundled theme is always available, so it is the safe default. Users can
/// still select any installed icon theme from Settings.
fn fallback_icon_theme() -> Option<String> {
    Some(BUNDLED_ICON_THEME.to_string())
}

/// Write a documented, editable copy of a built-in theme and return its id.
pub fn create_editable_theme_copy(builtin: &str) -> Result<String, String> {
    let spec = builtin_spec(builtin).ok_or_else(|| "Unknown built-in theme".to_string())?;
    let dir = crate::paths::themes_dir().join(format!("my-{builtin}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&spec).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("theme.json"), json).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("style.css"),
        "/* Extra component overrides for this theme.\n   Edit theme.json for colours, radii and fonts. */\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(format!("custom:my-{builtin}"))
}

pub fn list_custom_themes() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(crate::paths::themes_dir()) else { return out };
    for item in rd.flatten() {
        let path = item.path();
        if path.is_dir() && (path.join("theme.json").is_file() || path.join("style.css").is_file()) {
            if let Some(name) = path.file_name() {
                out.push(name.to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

pub fn density_class(density: Density) -> &'static str {
    density.css_class()
}
