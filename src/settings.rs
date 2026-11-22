//! Versioned settings, persisted atomically under the XDG config directory.

use crate::fs::SortField;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub const SETTINGS_VERSION: u32 = 2;

fn default_window_opacity() -> f64 {
    1.0
}

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

/// How the steps of a run-command profile are scheduled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunMode {
    Sequential,
    Parallel,
    Mixed,
}

impl Default for RunMode {
    fn default() -> Self {
        RunMode::Sequential
    }
}

impl RunMode {
    pub fn label(&self) -> &'static str {
        match self {
            RunMode::Sequential => "Sequential",
            RunMode::Parallel => "Parallel",
            RunMode::Mixed => "Mixed groups",
        }
    }
    pub fn hint(&self) -> &'static str {
        match self {
            RunMode::Sequential => "Each command waits for the previous one.",
            RunMode::Parallel => "All commands start together.",
            RunMode::Mixed => "Groups run in order; commands inside a group run together.",
        }
    }
    pub fn all() -> [RunMode; 3] {
        [RunMode::Sequential, RunMode::Parallel, RunMode::Mixed]
    }
}

/// Where a profile is executed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchMode {
    Background,
    Terminal,
}

impl Default for LaunchMode {
    fn default() -> Self {
        LaunchMode::Background
    }
}

impl LaunchMode {
    pub fn label(&self) -> &'static str {
        match self {
            LaunchMode::Background => "Background (Delve panel)",
            LaunchMode::Terminal => "Separate terminal window",
        }
    }
    pub fn all() -> [LaunchMode; 2] {
        [LaunchMode::Background, LaunchMode::Terminal]
    }
}

/// Which folders a run-command profile is offered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunScope {
    Folder,
    Subtree,
    Global,
}

impl Default for RunScope {
    fn default() -> Self {
        RunScope::Subtree
    }
}

impl RunScope {
    pub fn label(&self) -> &'static str {
        match self {
            RunScope::Folder => "This folder only",
            RunScope::Subtree => "This folder and subfolders",
            RunScope::Global => "Every folder",
        }
    }
    pub fn all() -> [RunScope; 3] {
        [RunScope::Folder, RunScope::Subtree, RunScope::Global]
    }
}

/// One executable step in a profile. `group` is only meaningful in mixed mode.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunStep {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub group: u32,
}

impl RunStep {
    pub fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

/// A user-defined folder run-command profile. Commands are never executed
/// implicitly; the legacy `program`/`args` pair mirrors the first step for
/// files written by Delve 1.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
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
    /// Folder base for this profile. `None` means every folder.
    #[serde(default)]
    pub folder: Option<PathBuf>,
    /// Controls whether the profile appears at the base folder, its subtree,
    /// or everywhere. Old profiles without this field retain subtree scope.
    #[serde(default)]
    pub scope: RunScope,
    /// Extra environment variables for every step in this profile.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional quick-run slot (1–9). The user presses Shift+S, then this
    /// number to start the set. Zero means no shortcut.
    #[serde(default)]
    pub shortcut: u8,
    #[serde(default)]
    pub steps: Vec<RunStep>,
    #[serde(default)]
    pub mode: RunMode,
    #[serde(default)]
    pub launch: LaunchMode,
    #[serde(default = "default_true")]
    pub stop_on_failure: bool,
}

impl Default for RunCommand {
    fn default() -> Self {
        RunCommand {
            id: String::new(),
            name: String::new(),
            program: String::new(),
            args: Vec::new(),
            cwd_rule: CwdRule::Folder,
            description: String::new(),
            enabled: true,
            folder: None,
            scope: RunScope::Subtree,
            env: BTreeMap::new(),
            shortcut: 0,
            steps: Vec::new(),
            mode: RunMode::Sequential,
            launch: LaunchMode::Background,
            stop_on_failure: true,
        }
    }
}

impl RunCommand {
    /// Whether this profile should be offered while browsing `folder`.
    pub fn applies_to(&self, folder: &Path) -> bool {
        let Some(base) = &self.folder else { return true };
        match self.scope {
            RunScope::Global => true,
            RunScope::Folder => folder == base,
            RunScope::Subtree => folder == base || folder.starts_with(base),
        }
    }

    /// Steps to execute, synthesising one from the legacy fields when needed.
    pub fn effective_steps(&self) -> Vec<RunStep> {
        if !self.steps.is_empty() {
            return self.steps.clone();
        }
        if self.program.trim().is_empty() {
            Vec::new()
        } else {
            vec![RunStep {
                program: self.program.clone(),
                args: self.args.clone(),
                group: 0,
            }]
        }
    }

    /// Human-readable quick-run hint, if this profile has a slot.
    pub fn shortcut_label(&self) -> Option<String> {
        (self.shortcut > 0).then(|| format!("Shift+S, {}", self.shortcut))
    }

    /// Keep the legacy first-command fields in sync after an edit.
    pub fn sync_legacy(&mut self) {
        if let Some(first) = self.steps.first() {
            self.program = first.program.clone();
            self.args = first.args.clone();
        }
    }
}

/// zoxide-style navigation history: frequency and recency are both ranked.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VisitStat {
    pub path: PathBuf,
    #[serde(default)]
    pub visits: u32,
    /// Unix seconds of the most recent visit.
    #[serde(default)]
    pub last_visit: u64,
}

impl VisitStat {
    /// Higher is more relevant. Frequency dominates, recency breaks ties and
    /// decays month-old entries without erasing their history.
    pub fn score(&self, now: u64) -> f64 {
        let age_days = now.saturating_sub(self.last_visit) as f64 / 86_400.0;
        self.visits as f64 * 12.0 / (1.0 + age_days * 0.35)
    }
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
    /// Background-only opacity, 0.35–1.0. Text and interactive states keep
    /// their normal opacity; only window/panel surfaces become translucent.
    #[serde(default = "default_window_opacity")]
    pub window_opacity: f64,
    pub accent: String,
    pub icon_theme: String,
    /// File-type icon overrides. Keys are MIME types (`image/*` wildcards are
    /// accepted) or extensions (`pdf`). Values are icon names or `file:` paths.
    #[serde(default)]
    pub custom_file_icons: BTreeMap<String, String>,
    /// Folder icon overrides keyed by normalised folder colour, plus `default`
    /// for folders without a colour-specific rule.
    #[serde(default)]
    pub custom_folder_icons: BTreeMap<String, String>,
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
    #[serde(default)]
    pub visits: Vec<VisitStat>,
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
            window_opacity: 1.0,
            accent: String::new(),
            icon_theme: String::new(),
            custom_file_icons: BTreeMap::new(),
            custom_folder_icons: BTreeMap::new(),
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
            visits: Vec::new(),
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
        self.window_opacity = self.window_opacity.clamp(0.35, 1.0);
        // Keep icon-rule keys canonical so lookups do not depend on how the
        // user capitalised an extension or HEX colour.
        self.custom_file_icons = self
            .custom_file_icons
            .iter()
            .filter(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect();
        let folder_icons: BTreeMap<String, String> = self
            .custom_folder_icons
            .iter()
            .filter_map(|(k, v)| {
                if v.trim().is_empty() {
                    return None;
                }
                if k.eq_ignore_ascii_case("default") {
                    Some(("default".to_string(), v.trim().to_string()))
                } else {
                    crate::theme::normalize_color(k)
                        .map(|color| (color, v.trim().to_string()))
                }
            })
            .collect();
        self.custom_folder_icons = folder_icons;
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
        // Drop commands without any executable step; they cannot do anything.
        self.commands.retain(|c| !c.effective_steps().is_empty());
        let mut used_shortcuts = Vec::new();
        for command in &mut self.commands {
            if command.folder.is_none() {
                command.scope = RunScope::Global;
            }
            command.shortcut = command.shortcut.min(9);
            if command.shortcut > 0 {
                if used_shortcuts.contains(&command.shortcut) {
                    command.shortcut = 0;
                } else {
                    used_shortcuts.push(command.shortcut);
                }
            }
            command.sync_legacy();
        }
        self.visits.retain(|v| v.path.is_absolute() && v.visits > 0);
        self.visits.sort_by(|a, b| b.visits.cmp(&a.visits));
        self.visits.truncate(200);
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

    /// Resolve the best custom icon for a file by MIME type, MIME wildcard,
    /// then extension. Exact MIME rules win over broader rules.
    pub fn custom_file_icon(&self, mime: &str, name: &str) -> Option<&str> {
        let mime = mime.to_ascii_lowercase();
        if let Some(icon) = self.custom_file_icons.get(&mime) {
            return Some(icon.as_str());
        }
        for (key, icon) in &self.custom_file_icons {
            let key = key.trim().to_ascii_lowercase();
            if let Some(prefix) = key.strip_suffix("/*") {
                if mime.starts_with(prefix) {
                    return Some(icon.as_str());
                }
            }
        }
        let extension = name
            .rsplit_once('.')
            .filter(|(stem, ext)| !stem.is_empty() && !ext.is_empty())
            .map(|(_, ext)| ext.to_ascii_lowercase())?;
        for (key, icon) in &self.custom_file_icons {
            let normalized = key
                .trim()
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if !normalized.contains('/') && !normalized.contains('*') && normalized == extension {
                return Some(icon.as_str());
            }
        }
        None
    }

    /// Folder icons follow the folder's own colour, then the `default` rule.
    pub fn custom_folder_icon(&self, color: Option<&str>) -> Option<&str> {
        if let Some(color) = color.and_then(crate::theme::normalize_color) {
            if let Some(icon) = self.custom_folder_icons.get(&color) {
                return Some(icon.as_str());
            }
            for (key, icon) in &self.custom_folder_icons {
                if key != "default"
                    && crate::theme::normalize_color(key).as_deref() == Some(color.as_str())
                {
                    return Some(icon.as_str());
                }
            }
        }
        self.custom_folder_icons.get("default").map(|icon| icon.as_str())
    }

    pub fn add_recent_location(&mut self, path: &Path) {
        self.recent_locations.retain(|p| p != path);
        self.recent_locations.insert(0, path.to_path_buf());
        self.recent_locations.truncate(30);
    }

    /// Record a real navigation visit for frequency/recency ranking.
    pub fn record_visit(&mut self, path: &Path) {
        if !path.is_absolute() {
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(stat) = self.visits.iter_mut().find(|v| v.path == path) {
            stat.visits = stat.visits.saturating_add(1);
            stat.last_visit = now;
        } else {
            self.visits.push(VisitStat {
                path: path.to_path_buf(),
                visits: 1,
                last_visit: now,
            });
        }
        if self.visits.len() > 200 {
            self.visits.sort_by(|a, b| b.visits.cmp(&a.visits));
            self.visits.truncate(200);
        }
    }

    pub fn visit_score(&self, path: &Path, now: u64) -> f64 {
        self.visits
            .iter()
            .find(|v| v.path == path)
            .map(|v| v.score(now))
            .unwrap_or(0.0)
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
