//! Navigation model and per-tab state shared by every view.

use crate::fs::{Entry, ScanResult, SortSpec};
use crate::settings::ViewKind;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchFilters {
    /// Comma-separated extensions, e.g. "png,jpg".
    pub extensions: String,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_days: Option<u32>,
}

impl SearchFilters {
    pub fn active(&self) -> bool {
        !self.extensions.trim().is_empty()
            || self.min_size.is_some()
            || self.max_size.is_some()
            || self.modified_days.is_some()
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.extensions.trim().is_empty() {
            parts.push(format!("types: {}", self.extensions.trim()));
        }
        if let Some(min) = self.min_size {
            parts.push(format!("≥ {}", crate::fs::human_size(min)));
        }
        if let Some(max) = self.max_size {
            parts.push(format!("≤ {}", crate::fs::human_size(max)));
        }
        if let Some(days) = self.modified_days {
            parts.push(format!("modified in {days} days"));
        }
        if parts.is_empty() {
            "no filters".into()
        } else {
            parts.join(" · ")
        }
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        if !self.extensions.trim().is_empty() {
            let entry_ext = entry.ext();
            let ok = self
                .extensions
                .split(',')
                .map(|s| s.trim().trim_start_matches('.').to_lowercase())
                .filter(|s| !s.is_empty())
                .any(|ext| ext == entry_ext);
            if !ok {
                return false;
            }
        }
        if let Some(min) = self.min_size {
            if entry.size < min {
                return false;
            }
        }
        if let Some(max) = self.max_size {
            if entry.size > max {
                return false;
            }
        }
        if let Some(days) = self.modified_days {
            if let Some(modified) = entry.modified {
                if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
                    if age.as_secs() > days as u64 * 86_400 {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// Curated quick-access landing page: user folders plus recent folders.
    Home,
    Path(PathBuf),
    Trash,
    RecentFiles,
    RecentLocations,
    Search {
        root: PathBuf,
        query: String,
        recursive: bool,
        filters: SearchFilters,
    },
    Tag {
        ids: Vec<String>,
        match_all: bool,
    },
}

impl Location {
    pub fn title(&self) -> String {
        match self {
            Location::Home => "Home".into(),
            Location::Path(p) => crate::fs::file_name_of(p),
            Location::Trash => "Trash".into(),
            Location::RecentFiles => "Recent Files".into(),
            Location::RecentLocations => "Recent Locations".into(),
            Location::Search { query, .. } => {
                if query.trim().is_empty() {
                    "Search results".into()
                } else {
                    format!("Search: {}", crate::util::ellipsize(query, 40))
                }
            }
            Location::Tag { .. } => "Tagged items".into(),
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            Location::Home => "Overview of your space and places".into(),
            Location::Path(p) => crate::paths::display_path(p),
            Location::Search { root, recursive, .. } => {
                format!(
                    "in {} ({})",
                    crate::paths::display_path(root),
                    if *recursive { "recursive" } else { "this folder" }
                )
            }
            Location::Tag { ids, match_all } => format!(
                "{} tag{} · {} matching",
                ids.len(),
                if ids.len() == 1 { "" } else { "s" },
                if *match_all { "all" } else { "any" }
            ),
            _ => String::new(),
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Location::Home => "user-home-symbolic",
            Location::Path(_) => "folder-symbolic",
            Location::Trash => "user-trash-symbolic",
            Location::RecentFiles => "document-open-recent-symbolic",
            Location::RecentLocations => "document-open-recent-symbolic",
            Location::Search { .. } => "system-search-symbolic",
            Location::Tag { .. } => "tag-symbolic",
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Location::Path(p) => Some(p),
            Location::Search { root, .. } => Some(root),
            _ => None,
        }
    }

    pub fn is_path_like(&self) -> bool {
        matches!(self, Location::Path(_) | Location::Search { .. })
    }

    pub fn crumbs(&self) -> Vec<(String, PathBuf)> {
        let Some(p) = self.path() else { return Vec::new() };
        let mut parts = vec![("/".to_string(), PathBuf::from("/"))];
        let mut acc = PathBuf::from("/");
        for comp in p.components() {
            use std::path::Component;
            match comp {
                Component::RootDir => {}
                Component::Normal(name) => {
                    acc = acc.join(name);
                    parts.push((name.to_string_lossy().to_string(), acc.clone()));
                }
                Component::ParentDir => {
                    acc.push("..");
                }
                _ => {}
            }
        }
        parts
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreemapFilter {
    All,
    Files,
    Folders,
}

impl TreemapFilter {
    pub fn label(&self) -> &'static str {
        match self {
            TreemapFilter::All => "All items",
            TreemapFilter::Files => "Files only",
            TreemapFilter::Folders => "Folders only",
        }
    }
}

pub struct TabState {
    pub location: Location,
    pub history: Vec<Location>,
    pub history_pos: usize,
    pub entries: Vec<Entry>,
    pub selection: HashSet<PathBuf>,
    pub anchor: Option<PathBuf>,
    pub view: ViewKind,
    pub sort: SortSpec,
    pub show_hidden: bool,
    pub filter_text: String,
    pub loading: bool,
    pub error: Option<String>,
    pub banner: Option<(String, String, bool)>, // title, detail, is_error
    pub generation: u64,
    pub scan: Option<Rc<ScanResult>>,
    pub scan_running: bool,
    pub scan_progress: String,
    pub scan_cancel: Option<Arc<AtomicBool>>,
    pub collapsed: HashSet<PathBuf>,
    pub treemap_root: Option<PathBuf>,
    pub treemap_metric_allocated: bool,
    pub treemap_filter: TreemapFilter,
    pub recursive_depth: u32,
    /// Whether the recursive view has seeded its initial collapsed state.
    pub recursive_seeded: bool,
    pub graph_depth: u32,
    /// Folders whose children are drawn in the graph view.
    pub graph_expanded: HashSet<PathBuf>,
    pub graph_search: String,
    pub tag_filter: Vec<String>,
    pub tag_match_all: bool,
}

impl TabState {
    pub fn new(location: Location, settings: &crate::settings::Settings) -> TabState {
        let (tag_filter_init, tag_match_all_init) = match &location {
            Location::Tag { ids, match_all } => (ids.clone(), *match_all),
            _ => (Vec::new(), false),
        };
        TabState {
            history: vec![location.clone()],
            location,
            history_pos: 0,
            entries: Vec::new(),
            selection: HashSet::new(),
            anchor: None,
            view: settings.default_view,
            sort: SortSpec {
                field: settings.default_sort,
                ascending: settings.sort_ascending,
                folders_first: settings.folders_first,
            },
            show_hidden: settings.show_hidden,
            filter_text: String::new(),
            loading: false,
            error: None,
            banner: None,
            generation: 0,
            scan: None,
            scan_running: false,
            scan_progress: String::new(),
            scan_cancel: None,
            collapsed: HashSet::new(),
            treemap_root: None,
            treemap_metric_allocated: false,
            treemap_filter: TreemapFilter::All,
            recursive_depth: 32,
            recursive_seeded: false,
            graph_depth: 2,
            graph_expanded: HashSet::new(),
            graph_search: String::new(),
            tag_filter: tag_filter_init.clone(),
            tag_match_all: tag_match_all_init,
        }
    }

    pub fn visible_entries(&self) -> Vec<Entry> {
        let filter = self.filter_text.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|e| self.show_hidden || !e.hidden)
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .cloned()
            .collect()
    }

    pub fn selected_entries(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .filter(|e| self.selection.contains(&e.path))
            .cloned()
            .collect()
    }

    pub fn selected_size(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| self.selection.contains(&e.path))
            .map(|e| e.size)
            .sum()
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.anchor = None;
    }
}
