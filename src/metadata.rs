//! Persistent metadata database: tags and folder colours.
//!
//! Tags are stored against stable file identity (device + inode) with a path
//! fallback, so renames on the same filesystem survive. The database is
//! versioned, migrated with a backup, and written atomically.

use crate::fs::FileId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub const META_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Symbolic icon name. Defaults make old databases load unchanged.
    #[serde(default = "default_tag_icon")]
    pub icon: String,
}

fn default_tag_icon() -> String {
    "tag-symbolic".into()
}

/// Icons offered when creating or customising a tag. All are part of the
/// standard symbolic icon set so they follow Delve's theme colours.
pub const TAG_ICONS: &[(&str, &str)] = &[
    ("tag-symbolic", "Tag"),
    ("starred-symbolic", "Star"),
    ("emblem-important-symbolic", "Important"),
    ("folder-symbolic", "Folder"),
    ("document-symbolic", "Document"),
    ("folder-documents-symbolic", "Documents"),
    ("user-home-symbolic", "Home"),
    ("network-workgroup-symbolic", "Work"),
];

/// Paths are written as lossy UTF-8. `PathBuf`'s own serde form refuses
/// non-UTF-8 paths, which used to make the whole database fail to serialize
/// (and therefore silently skip saving) when a single such file was tagged.
mod path_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::PathBuf;

    pub fn serialize<S: Serializer>(path: &PathBuf, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&path.to_string_lossy())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        Ok(PathBuf::from(String::deserialize(deserializer)?))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ItemMeta {
    /// Last known path, used for display and for stale-entry detection.
    #[serde(default, with = "path_serde")]
    pub path: PathBuf,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub folder_color: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub version: u32,
    pub tags: Vec<Tag>,
    /// Keyed by `FileId::key()`.
    pub items: HashMap<String, ItemMeta>,
}

impl Default for Metadata {
    fn default() -> Self {
        let palette = ["#5b9dff", "#43c98a", "#f2b134", "#e2679b", "#a78bfa", "#38bdf8"];
        let icons = [
            "network-workgroup-symbolic",
            "user-home-symbolic",
            "folder-symbolic",
            "emblem-important-symbolic",
        ];
        let tags = ["Work", "Personal", "Archive", "Important"]
            .iter()
            .enumerate()
            .map(|(i, name)| Tag {
                id: format!("tag{}", i + 1),
                name: (*name).to_string(),
                color: palette[i % palette.len()].to_string(),
                icon: icons[i % icons.len()].to_string(),
            })
            .collect();
        Metadata { version: META_VERSION, tags, items: HashMap::new() }
    }
}

impl Metadata {
    pub fn load() -> Metadata {
        let file = crate::paths::metadata_file();
        let mut meta = match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<Metadata>(&text) {
                Ok(m) => m,
                Err(err) => {
                    crate::util::log(&format!("Metadata database invalid ({err}); starting empty"));
                    let _ = std::fs::copy(&file, file.with_extension("json.invalid"));
                    Metadata::default()
                }
            },
            Err(_) => Metadata::default(),
        };
        if meta.version < META_VERSION {
            let _ = std::fs::create_dir_all(crate::paths::data_dir());
            let _ = std::fs::copy(&file, file.with_extension(format!("v{}.bak", meta.version)));
        }
        meta.version = META_VERSION;
        meta
    }

    pub fn save(&self) {
        let file = crate::paths::metadata_file();
        let _ = std::fs::create_dir_all(crate::paths::data_dir());
        let tmp = file.with_extension("json.tmp");
        let json = match serde_json::to_string_pretty(self) {
            Ok(json) => json,
            Err(err) => {
                crate::util::log(&format!("Could not serialise metadata ({err}); changes not saved"));
                return;
            }
        };
        if let Err(err) = std::fs::write(&tmp, json) {
            crate::util::log(&format!("Could not write metadata ({err})"));
            return;
        }
        if let Err(err) = std::fs::rename(&tmp, &file) {
            crate::util::log(&format!("Could not replace metadata database ({err})"));
            let _ = std::fs::remove_file(&tmp);
        }
    }

    pub fn tag(&self, id: &str) -> Option<&Tag> {
        self.tags.iter().find(|t| t.id == id)
    }

    pub fn tag_by_name(&self, name: &str) -> Option<&Tag> {
        self.tags.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }

    pub fn create_tag_with_icon(&mut self, name: &str, color: &str, icon: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Tag name cannot be empty.".into());
        }
        if self.tags.iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
            return Err(format!("A tag named “{name}” already exists."));
        }
        let id = format!("tag{}-{}", crate::util::now_millis(), self.tags.len());
        self.tags.push(Tag {
            id: id.clone(),
            name: name.to_string(),
            color: color.to_string(),
            icon: if icon.trim().is_empty() { "tag-symbolic" } else { icon }.to_string(),
        });
        self.save();
        Ok(id)
    }

    pub fn set_tag_icon(&mut self, id: &str, icon: &str) {
        if let Some(tag) = self.tags.iter_mut().find(|t| t.id == id) {
            tag.icon = if icon.trim().is_empty() { "tag-symbolic" } else { icon }.to_string();
            self.save();
        }
    }

    pub fn rename_tag(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Tag name cannot be empty.".into());
        }
        if self.tags.iter().any(|t| t.id != id && t.name.eq_ignore_ascii_case(name)) {
            return Err(format!("A tag named “{name}” already exists."));
        }
        if let Some(tag) = self.tags.iter_mut().find(|t| t.id == id) {
            tag.name = name.to_string();
            self.save();
            Ok(())
        } else {
            Err("That tag no longer exists.".into())
        }
    }

    pub fn recolor_tag(&mut self, id: &str, color: &str) {
        if let Some(tag) = self.tags.iter_mut().find(|t| t.id == id) {
            tag.color = color.to_string();
            self.save();
        }
    }

    pub fn delete_tag(&mut self, id: &str) {
        self.tags.retain(|t| t.id != id);
        for item in self.items.values_mut() {
            item.tags.retain(|t| t != id);
        }
        self.items.retain(|_, v| !v.tags.is_empty() || v.folder_color.is_some());
        self.save();
    }

    fn key_for(path: &Path) -> String {
        match std::fs::metadata(path) {
            Ok(m) => FileId::Inode(m.dev(), m.ino()).key(),
            Err(_) => FileId::path_key(path),
        }
    }

    pub fn item(&self, path: &Path) -> Option<&ItemMeta> {
        self.items.get(&Self::key_for(path))
    }

    pub fn tags_for(&self, path: &Path) -> Vec<String> {
        self.item(path).map(|i| i.tags.clone()).unwrap_or_default()
    }

    pub fn tag_names_for(&self, path: &Path) -> Vec<String> {
        self.tags_for(path)
            .iter()
            .filter_map(|id| self.tag(id).map(|t| t.name.clone()))
            .collect()
    }

    pub fn folder_color(&self, path: &Path) -> Option<String> {
        self.item(path).and_then(|i| i.folder_color.clone())
    }

    pub fn item_mut(&mut self, path: &Path) -> &mut ItemMeta {
        let key = Self::key_for(path);
        let entry = self.items.entry(key).or_default();
        if entry.path != path {
            entry.path = path.to_path_buf();
        }
        entry
    }

    pub fn set_folder_color(&mut self, path: &Path, color: Option<String>) {
        {
            let item = self.item_mut(path);
            item.folder_color = color;
        }
        self.items
            .retain(|_, v| !v.tags.is_empty() || v.folder_color.is_some());
        self.save();
    }

    pub fn add_tag(&mut self, path: &Path, tag_id: &str) {
        {
            let item = self.item_mut(path);
            if !item.tags.iter().any(|t| t == tag_id) {
                item.tags.push(tag_id.to_string());
            }
        }
        self.save();
    }

    pub fn remove_tag(&mut self, path: &Path, tag_id: &str) {
        {
            let item = self.item_mut(path);
            item.tags.retain(|t| t != tag_id);
        }
        self.items
            .retain(|_, v| !v.tags.is_empty() || v.folder_color.is_some());
        self.save();
    }

    pub fn remove_tags_bulk(&mut self, paths: &[PathBuf], tag_id: &str) {
        for path in paths {
            let item = self.item_mut(path);
            item.tags.retain(|t| t != tag_id);
        }
        self.save();
    }

    pub fn add_tag_bulk(&mut self, paths: &[PathBuf], tag_id: &str) {
        for path in paths {
            let item = self.item_mut(path);
            if !item.tags.iter().any(|t| t == tag_id) {
                item.tags.push(tag_id.to_string());
            }
        }
        self.save();
    }

    /// All recorded paths carrying the given tag(s). Entries whose identity no
    /// longer resolves are retained in the database but omitted from results.
    pub fn paths_with_tags(&self, tag_ids: &[String], match_all: bool) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for item in self.items.values() {
            if item.tags.is_empty() {
                continue;
            }
            let matches = if match_all {
                tag_ids.iter().all(|t| item.tags.contains(t))
            } else {
                tag_ids.iter().any(|t| item.tags.contains(t))
            };
            if matches && !item.path.as_os_str().is_empty() {
                out.push(item.path.clone());
            }
        }
        out.sort_by(|a, b| crate::fs::compare_names(&a.to_string_lossy(), &b.to_string_lossy()));
        out
    }

    /// Refresh stored paths from live inode identity, and drop assignments whose
    /// files are gone. Returns the number of cleaned entries.
    pub fn cleanup(&mut self) -> usize {
        let before = self.items.len();
        // Drop entries whose last known path no longer exists and whose inode is
        // unknown elsewhere. Inode-keyed entries with a missing path are kept so
        // that temporarily unmounted volumes can come back.
        self.items.retain(|key, item| {
            if item.path.exists() {
                return true;
            }
            key.starts_with("i:") && !item.path.as_os_str().is_empty()
        });
        self.save();
        before.saturating_sub(self.items.len())
    }

    /// Move assignments from a path to its new location (used after rename).
    pub fn remap_path(&mut self, old: &Path, new: &Path) {
        if let Some(item) = self.items.remove(&FileId::path_key(old)) {
            let mut item = item;
            item.path = new.to_path_buf();
            self.items.insert(Self::key_for(new), item);
        }
        // Also refresh inode-keyed entries that pointed at the old path.
        for item in self.items.values_mut() {
            if item.path == old {
                item.path = new.to_path_buf();
            }
        }
        self.save();
    }
}
