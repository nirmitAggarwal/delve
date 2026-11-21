//! Versioned settings, persisted atomically under the XDG config directory.

use crate::fs::SortField;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewKind {
    Icon,
    Detail,
    Recursive,
    Treemap,
    Graph,
}

impl ViewKind {
    pub fn label(&self) -> &'static str {
        match self {
            ViewKind::Icon => "Icon",
            ViewKind::Detail => "Detail",
            ViewKind::Recursive => "Recursive",
            ViewKind::Treemap => "Treemap",
            ViewKind::Graph => "Graph",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            ViewKind::Icon => "view-grid-symbolic",
            ViewKind::Detail => "view-list-symbolic",
            ViewKind::Recursive => "view-list-bullet-symbolic",
            ViewKind::Treemap => "view-app-grid-symbolic",
            ViewKind::Graph => "network-workgroup-symbolic",
        }
    }

    pub fn all() -> [ViewKind; 5] {
        [ViewKind::Icon, ViewKind::Detail, ViewKind::Recursive, ViewKind::Treemap, ViewKind::Graph]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    Compact,
    Comfortable,
    Spacious,
}

impl Density {
    pub fn label(&self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Comfortable => "Comfortable",
            Density::Spacious => "Spacious",
        }
    }
    pub fn css_class(&self) -> &'static str {
        match self {
            Density::Compact => "density-compact",
            Density::Comfortable => "density-comfortable",
            Density::Spacious => "density-spacious",
        }
    }
    pub fn all() -> [Density; 3] {
        [Density::Compact, Density::Comfortable, Density::Spacious]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictPolicy {
    Ask,
    Replace,
    Skip,
    KeepBoth,
}

impl ConflictPolicy {
    pub fn label(&self) -> &'static str {
        match self {
            ConflictPolicy::Ask => "Ask every time",
            ConflictPolicy::Replace => "Replace existing",
            ConflictPolicy::Skip => "Skip existing",
            ConflictPolicy::KeepBoth => "Keep both (rename)",
        }
    }
    pub fn all() -> [ConflictPolicy; 4] {
        [ConflictPolicy::Ask, ConflictPolicy::Replace, ConflictPolicy::Skip, ConflictPolicy::KeepBoth]
    }
}

/// A user-defined folder run command. Commands are never executed implicitly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunCommand {
    pub id: String,
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_cwd_rule")]
    pub cwd_rule: CwdRule,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// When set, the command only appears for this exact folder.
    #[serde(default)]
    pub folder: Option<PathBuf>,
}

fn default_cwd_rule() -> CwdRule {
    CwdRule::Folder
}
fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CwdRule {
    Folder,
    Home,
    Selected,
}

impl CwdRule {
    pub fn label(&self) -> &'static str {
        match self {
            CwdRule::Folder => "Current folder",
            CwdRule::Home => "Home folder",
            CwdRule::Selected => "Selected folder",
        }
    }
    pub fn all() -> [CwdRule; 3] {
        [CwdRule::Folder, CwdRule::Home, CwdRule::Selected]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// "builtin:dark", "builtin:light", "builtin:ember" or "custom:<name>".
    pub theme: String,
    pub theme_live_reload: bool,
    pub density: Density,
    pub ui_font: String,
    pub mono_font: String,
    pub transparency: bool,
    pub accent: String,
    pub icon_theme: String,
    pub sidebar_visible: bool,
    pub inspector_visible: bool,
    pub sidebar_collapsed: Vec<String>,
    pub sidebar_width: i32,
    pub sidebar_mini: bool,
    pub inspector_width: i32,
    pub default_view: ViewKind,
    pub icon_size: i32,
    pub show_tag_dots: bool,
    pub detail_columns: Vec<String>,
    pub show_hidden: bool,
    pub folders_first: bool,
    pub default_sort: SortField,
    pub sort_ascending: bool,
    pub byte_sizes: bool,
    pub conflict: ConflictPolicy,
    pub thumbnails: bool,
    pub thumbnail_size: i32,
    pub text_preview: bool,
    pub terminal: String,
    /// MIME type -> desktop application id, chosen the first time a file of
    /// that type is opened.
    pub default_apps: HashMap<String, String>,
    pub transfers_enabled: bool,
    pub shortcuts: HashMap<String, String>,
    pub commands: Vec<RunCommand>,
    pub window_width: i32,
    pub window_height: i32,
    pub window_maximized: bool,
    pub recent_locations: Vec<PathBuf>,
    pub recent_files: Vec<PathBuf>,
    pub tips_seen: bool,
    pub panned_widths: Vec<i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SETTINGS_VERSION,
            theme: "builtin:dark".into(),
            theme_live_reload: true,
            density: Density::Comfortable,
            ui_font: String::new(),
            mono_font: String::new(),
            transparency: false,
            accent: String::new(),
            icon_theme: String::new(),
            sidebar_visible: true,
            inspector_visible: true,
            sidebar_collapsed: Vec::new(),
            sidebar_width: 218,
            sidebar_mini: false,
            inspector_width: 280,
            default_view: ViewKind::Icon,
            icon_size: 56,
            show_tag_dots: true,
            detail_columns: vec![
                "name".into(),
                "type".into(),
                "size".into(),
                "modified".into(),
                "permissions".into(),
                "owner".into(),
                "tags".into(),
            ],
            show_hidden: false,
            folders_first: true,
            default_sort: SortField::Name,
            sort_ascending: true,
            byte_sizes: false,
            conflict: ConflictPolicy::Ask,
            thumbnails: true,
            thumbnail_size: 160,
            text_preview: true,
            terminal: String::new(),
            default_apps: HashMap::new(),
            transfers_enabled: true,
            shortcuts: HashMap::new(),
            commands: Vec::new(),
            window_width: 1240,
            window_height: 800,
            window_maximized: false,
            recent_locations: Vec::new(),
            recent_files: Vec::new(),
            tips_seen: false,
            panned_widths: vec![218, 280],
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let file = crate::paths::settings_file();
        let mut settings = match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => s,
                Err(err) => {
                    crate::util::log(&format!("Settings file is invalid ({err}); using defaults"));
                    let backup = file.with_extension("json.invalid");
                    let _ = std::fs::copy(&file, backup);
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        settings.migrate();
        settings.validate();
        settings
    }

    fn migrate(&mut self) {
        if self.version > SETTINGS_VERSION {
            crate::util::log("Settings come from a newer Delve version; keeping known fields");
        }
        self.version = SETTINGS_VERSION;
    }

    pub fn validate(&mut self) {
        if self.sidebar_width < 150 {
            self.sidebar_width = 150;
        }
        if self.sidebar_width > 520 {
            self.sidebar_width = 520;
        }
        if self.inspector_width < 200 {
            self.inspector_width = 200;
        }
        if self.inspector_width > 620 {
            self.inspector_width = 620;
        }
        if self.thumbnail_size < 48 {
            self.thumbnail_size = 48;
        }
        if self.thumbnail_size > 512 {
            self.thumbnail_size = 512;
        }
        self.icon_size = self.icon_size.clamp(32, 160);
        if self.detail_columns.is_empty() {
            self.detail_columns = vec![
                "name".into(),
                "type".into(),
                "size".into(),
                "modified".into(),
                "permissions".into(),
                "owner".into(),
                "tags".into(),
            ];
        }
        if self.window_width < 480 {
            self.window_width = 480;
        }
        if self.window_height < 360 {
            self.window_height = 360;
        }
        if !(0.5..=2.0).contains(&1.0) {
            self.density = Density::Comfortable;
        }
        // Drop commands without a program; they cannot do anything.
        self.commands.retain(|c| !c.program.trim().is_empty());
    }

    pub fn save(&self) {
        let file = crate::paths::settings_file();
        let _ = std::fs::create_dir_all(crate::paths::config_dir());
        let tmp = file.with_extension("json.tmp");
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &file).is_err() {
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            Err(err) => crate::util::log(&format!("Could not serialise settings: {err}")),
        }
    }

    pub fn add_recent_location(&mut self, path: &Path) {
        self.recent_locations.retain(|p| p != path);
        self.recent_locations.insert(0, path.to_path_buf());
        self.recent_locations.truncate(30);
    }

    pub fn add_recent_file(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(60);
    }

    pub fn shortcut(&self, action: &str, default: &str) -> String {
        self.shortcuts.get(action).cloned().unwrap_or_else(|| default.to_string())
    }
}

/// Export/import payload shared by the Advanced settings section.
#[derive(Serialize, Deserialize)]
pub struct ExportBundle {
    pub app: String,
    pub version: u32,
    pub settings: Settings,
    pub metadata: crate::metadata::Metadata,
    #[serde(default)]
    pub warning: String,
}

pub fn export_bundle(settings: &Settings, metadata: &crate::metadata::Metadata) -> Result<PathBuf, String> {
    let bundle = ExportBundle {
        app: "delve".into(),
        version: SETTINGS_VERSION,
        settings: settings.clone(),
        metadata: metadata.clone(),
        warning: "Paths in this export are specific to the machine it was created on. \
                  Tags and folder colours use stable file identity where possible."
            .into(),
    };
    let path = crate::paths::export_file();
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

pub fn import_bundle(path: &Path) -> Result<ExportBundle, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bundle: ExportBundle =
        serde_json::from_str(&text).map_err(|e| format!("Not a valid Delve export: {e}"))?;
    if bundle.app != "delve" {
        return Err("This file was not exported by Delve.".into());
    }
    Ok(bundle)
}
