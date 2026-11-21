//! Filesystem abstraction: directory listing, metadata, identity, sorting and
//! bounded recursive scans.
//!
//! This module is deliberately independent of GTK so that UI code stays a thin
//! consumer of a single file model, as required by the specification.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;
use gtk4::gio::prelude::*;

/// A single directory entry, shared by all five views.
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub allocated: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
    pub inode: u64,
    pub device: u64,
    pub hidden: bool,
    /// True when a symlink could not be resolved (broken link).
    pub broken_link: bool,
    /// Set for items that do not live in the real filesystem (trash entries).
    pub virtual_root: Option<PathBuf>,
}

impl Entry {
    pub fn from_path(path: PathBuf, name: String) -> Entry {
        let link_meta = std::fs::symlink_metadata(&path);
        let is_symlink = link_meta.as_ref().map(|m| m.file_type().is_symlink()).unwrap_or(false);
        let mut broken_link = false;
        let meta = if is_symlink {
            match std::fs::metadata(&path) {
                Ok(m) => Some(m),
                Err(_) => {
                    broken_link = true;
                    link_meta.ok()
                }
            }
        } else {
            link_meta.ok()
        };
        let hidden = name.starts_with('.');
        match meta {
            Some(m) => Entry {
                path,
                name,
                is_dir: m.is_dir(),
                is_symlink,
                size: if m.is_dir() { 0 } else { m.len() },
                allocated: m.blocks() * 512,
                modified: m.modified().ok(),
                created: m.created().ok(),
                accessed: m.accessed().ok(),
                mode: m.mode(),
                uid: m.uid(),
                gid: m.gid(),
                nlink: m.nlink(),
                inode: m.ino(),
                device: m.dev(),
                hidden,
                broken_link,
                virtual_root: None,
            },
            None => Entry {
                path,
                name,
                is_dir: false,
                is_symlink,
                size: 0,
                allocated: 0,
                modified: None,
                created: None,
                accessed: None,
                mode: 0,
                uid: 0,
                gid: 0,
                nlink: 0,
                inode: 0,
                device: 0,
                hidden,
                broken_link,
                virtual_root: None,
            },
        }
    }

    pub fn identity(&self) -> FileId {
        if self.inode != 0 {
            FileId::Inode(self.device, self.inode)
        } else {
            FileId::Path(self.path.clone())
        }
    }

    pub fn mime(&self) -> String {
        if self.is_dir {
            return "inode/directory".into();
        }
        mime_for_path(&self.path)
    }

    pub fn type_label(&self) -> String {
        if self.is_dir {
            return "Folder".into();
        }
        if self.is_symlink && self.broken_link {
            return "Broken link".into();
        }
        if self.is_symlink {
            return "Symbolic link".into();
        }
        mime_guess::from_path(&self.path)
            .first()
            .map(|m| {
                let essence = m.essence_str().to_string();
                let sub = essence.split('/').nth(1).unwrap_or("file").to_string();
                match sub.as_str() {
                    "plain" => "Text file".into(),
                    "x-executable" | "x-sharedlib" | "x-pie-executable" => "Executable".into(),
                    other => {
                        let mut chars = other.chars();
                        match chars.next() {
                            Some(c) => format!("{} file", c.to_uppercase().collect::<String>() + chars.as_str()),
                            None => "File".into(),
                        }
                    }
                }
            })
            .unwrap_or_else(|| "File".into())
    }

    pub fn ext(&self) -> String {
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self.ext().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "avif" | "svg" | "ico"
        )
    }

    pub fn is_pdf(&self) -> bool {
        self.ext() == "pdf"
    }

    pub fn is_text(&self) -> bool {
        let m = mime_guess::from_path(&self.path).first_or_octet_stream();
        let essence = m.essence_str();
        essence.starts_with("text/")
            || matches!(
                essence,
                "application/json"
                    | "application/xml"
                    | "application/javascript"
                    | "application/x-shellscript"
                    | "application/toml"
                    | "application/x-yaml"
                    | "application/x-python"
                    | "application/rust"
            )
            || matches!(
                self.ext().as_str(),
                "rs" | "toml" | "lock" | "md" | "cfg" | "conf" | "ini" | "log" | "sh" | "py"
                    | "js" | "ts" | "css" | "html" | "c" | "h" | "cpp" | "hpp" | "go" | "java"
                    | "sql" | "yml" | "yaml" | "json" | "txt" | "desktop" | "service"
            )
    }

    pub fn is_video(&self) -> bool {
        matches!(
            self.ext().as_str(),
            "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "mpg" | "mpeg" | "ogv"
        )
    }

    pub fn is_audio(&self) -> bool {
        matches!(
            self.ext().as_str(),
            "mp3" | "flac" | "ogg" | "wav" | "m4a" | "opus" | "aac" | "wma"
        )
    }

    pub fn permissions_string(&self) -> String {
        if self.mode == 0 {
            return "—".into();
        }
        let mode = self.mode;
        let mut s = String::with_capacity(9);
        for (shift, ch) in [(6, 0), (6, 1), (6, 2), (3, 0), (3, 1), (3, 2), (0, 0), (0, 1), (0, 2)] {
            let bit = 1 << (shift + (2 - ch));
            let set = mode & bit != 0;
            s.push(match (ch, set) {
                (0, true) => 'r',
                (1, true) => 'w',
                (2, true) => 'x',
                (0, false) => '-',
                (1, false) => '-',
                _ => '-',
            });
        }
        s
    }

    pub fn mode_octal(&self) -> String {
        format!("{:04o}", self.mode & 0o7777)
    }

    pub fn owner_name(&self) -> String {
        uid_name(self.uid)
    }

    pub fn group_name(&self) -> String {
        gid_name(self.gid)
    }

    pub fn is_executable(&self) -> bool {
        self.mode & 0o111 != 0
    }
}

static PASSWD: OnceLock<HashMap<u32, String>> = OnceLock::new();
static GROUP: OnceLock<HashMap<u32, String>> = OnceLock::new();

fn uid_name(uid: u32) -> String {
    let map = PASSWD.get_or_init(|| {
        let mut m = HashMap::new();
        if let Ok(text) = std::fs::read_to_string("/etc/passwd") {
            for line in text.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() > 2 {
                    if let Ok(id) = fields[2].parse::<u32>() {
                        m.insert(id, fields[0].to_string());
                    }
                }
            }
        }
        m
    });
    map.get(&uid).cloned().unwrap_or_else(|| uid.to_string())
}

fn gid_name(gid: u32) -> String {
    let map = GROUP.get_or_init(|| {
        let mut m = HashMap::new();
        if let Ok(text) = std::fs::read_to_string("/etc/group") {
            for line in text.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() > 2 {
                    if let Ok(id) = fields[2].parse::<u32>() {
                        m.insert(id, fields[0].to_string());
                    }
                }
            }
        }
        m
    });
    map.get(&gid).cloned().unwrap_or_else(|| gid.to_string())
}

pub fn mime_for_path(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string()
}

/// Stable identity used for tags and folder colours. Device+inode is stable
/// across renames on the same filesystem; the path is the fallback.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileId {
    Inode(u64, u64),
    Path(PathBuf),
}

impl FileId {
    pub fn key(&self) -> String {
        match self {
            FileId::Inode(dev, ino) => format!("i:{}:{}", dev, ino),
            FileId::Path(p) => format!("p:{}", p.display()),
        }
    }

    pub fn path_key(path: &Path) -> String {
        format!("p:{}", path.display())
    }
}

/// A directory listing plus lightweight diagnostics, produced off the UI thread.
pub struct Listing {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    pub error: Option<String>,
}

/// Read one directory. Never follows symlinks for the directory itself.
pub fn read_dir(path: &Path) -> Listing {
    let mut entries = Vec::new();
    let mut error = None;
    match std::fs::read_dir(path) {
        Ok(rd) => {
            for item in rd {
                match item {
                    Ok(de) => {
                        let name = de.file_name().to_string_lossy().to_string();
                        entries.push(Entry::from_path(de.path(), name));
                    }
                    Err(e) => {
                        error.get_or_insert_with(|| format!("Some entries could not be read: {e}"));
                    }
                }
            }
        }
        Err(e) => error = Some(describe_io_error(&e, path)),
    }
    Listing { path: path.to_path_buf(), entries, error }
}

pub fn describe_io_error(err: &std::io::Error, path: &Path) -> String {
    use std::io::ErrorKind::*;
    let what = match err.kind() {
        PermissionDenied => "Permission denied",
        NotFound => "No longer exists",
        AlreadyExists => "Already exists",
        InvalidInput => "Invalid name or path",
        _ => "Could not access",
    };
    format!("{}: {}", what, path.display())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SortField {
    Name,
    Type,
    Size,
    Modified,
    Created,
    Permissions,
    Owner,
    Tags,
}

impl SortField {
    pub fn label(&self) -> &'static str {
        match self {
            SortField::Name => "Name",
            SortField::Type => "Type",
            SortField::Size => "Size",
            SortField::Modified => "Modified",
            SortField::Created => "Created",
            SortField::Permissions => "Permissions",
            SortField::Owner => "Owner",
            SortField::Tags => "Tags",
        }
    }

    pub fn all() -> [SortField; 8] {
        [
            SortField::Name,
            SortField::Type,
            SortField::Size,
            SortField::Modified,
            SortField::Created,
            SortField::Permissions,
            SortField::Owner,
            SortField::Tags,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SortSpec {
    pub field: SortField,
    pub ascending: bool,
    pub folders_first: bool,
}

impl Default for SortSpec {
    fn default() -> Self {
        SortSpec { field: SortField::Name, ascending: true, folders_first: true }
    }
}

/// Deterministic natural-ish comparison: case-insensitive first, then exact.
pub fn compare_names(a: &str, b: &str) -> Ordering {
    let la = a.to_lowercase();
    let lb = b.to_lowercase();
    la.cmp(&lb).then_with(|| a.cmp(b))
}

pub fn sort_entries(entries: &mut [Entry], spec: SortSpec, tags_of: &dyn Fn(&Entry) -> String) {
    entries.sort_by(|a, b| {
        if spec.folders_first && a.is_dir != b.is_dir {
            return if a.is_dir { Ordering::Less } else { Ordering::Greater };
        }
        let ord = match spec.field {
            SortField::Name => compare_names(&a.name, &b.name),
            SortField::Type => {
                a.type_label().to_lowercase().cmp(&b.type_label().to_lowercase())
                    .then_with(|| compare_names(&a.name, &b.name))
            }
            SortField::Size => a.size.cmp(&b.size),
            SortField::Modified => a.modified.cmp(&b.modified),
            SortField::Created => a.created.cmp(&b.created),
            SortField::Permissions => a.mode.cmp(&b.mode),
            SortField::Owner => a.uid.cmp(&b.uid),
            SortField::Tags => tags_of(a).cmp(&tags_of(b)),
        };
        let ord = ord.then_with(|| compare_names(&a.name, &b.name));
        if spec.ascending {
            ord
        } else {
            ord.reverse()
        }
    });
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else if value >= 10.0 {
        format!("{:.1} {}", value, UNITS[unit])
    } else {
        format!("{:.2} {}", value, UNITS[unit])
    }
}

pub fn total_size(entries: &[Entry]) -> u64 {
    entries.iter().map(|e| e.size).sum()
}

/// Format a timestamp using glib's locale-aware date formatting (no chrono
/// dependency), falling back to the Unix epoch rendering.
pub fn format_time(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return "—".into() };
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let dt = gtk4::glib::DateTime::from_unix_local(dur.as_secs() as i64).ok();
    match dt {
        Some(dt) => dt
            .format("%Y-%m-%d %H:%M")
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "—".into()),
        None => "—".into(),
    }
}

pub fn format_time_long(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return "Unknown".into() };
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    gtk4::glib::DateTime::from_unix_local(dur.as_secs() as i64)
        .ok()
        .and_then(|dt| dt.format("%A %d %B %Y, %H:%M:%S").ok().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown".into())
}

/// Aggregate size of a directory tree, bounded by a deadline-independent
/// entry limit so the UI never hangs on pathological trees.
pub struct DirSize {
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    pub truncated: bool,
}

pub fn dir_size(path: &Path, cancel: &AtomicBool, limit: u64) -> DirSize {
    let mut out = DirSize { bytes: 0, files: 0, folders: 0, truncated: false };
    let mut stack = vec![path.to_path_buf()];
    let mut seen = 0u64;
    while let Some(dir) = stack.pop() {
        if cancel.load(AtomicOrdering::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for item in rd.flatten() {
            seen += 1;
            if seen > limit {
                out.truncated = true;
                return out;
            }
            let Ok(meta) = item.file_type() else { continue };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                out.folders += 1;
                stack.push(item.path());
            } else if let Ok(m) = item.metadata() {
                out.files += 1;
                out.bytes += m.len().max(m.blocks() * 512);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Recursive scans (shared by Recursive, Treemap and Graph views)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ScanNode {
    pub entry: Entry,
    pub depth: u32,
    pub parent: Option<usize>,
    pub child_count: usize,
    /// Bytes of this file, or aggregated bytes for folders.
    pub total: u64,
    pub direct_files: u64,
    pub direct_folders: u64,
    pub denied: bool,
    pub truncated: bool,
}

pub struct ScanResult {
    pub root: PathBuf,
    pub nodes: Vec<ScanNode>,
    pub truncated: bool,
    pub cancelled: bool,
    pub errors: u32,
    pub total_bytes: u64,
    pub total_files: u64,
    pub total_folders: u64,
    pub elapsed_ms: u128,
}

pub struct ScanConfig {
    pub max_depth: u32,
    pub max_entries: usize,
    pub follow_symlinks: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig { max_depth: 8, max_entries: 150_000, follow_symlinks: false }
    }
}

/// Progress callback: (nodes scanned, current path).
pub type ScanProgress = Arc<dyn Fn(usize, &Path) + Send + Sync>;

/// Breadth-first recursive scan that never follows symlinks by default, detects
/// unreadable folders, and enforces hard safety limits.
pub fn scan_tree(root: &Path, cfg: ScanConfig, cancel: Arc<AtomicBool>, progress: Option<ScanProgress>) -> ScanResult {
    let start = std::time::Instant::now();
    let root_entry = Entry::from_path(root.to_path_buf(), file_name_of(root));
    let mut nodes: Vec<ScanNode> = vec![ScanNode {
        entry: root_entry,
        depth: 0,
        parent: None,
        child_count: 0,
        total: 0,
        direct_files: 0,
        direct_folders: 0,
        denied: false,
        truncated: false,
    }];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(0usize);
    let mut errors = 0u32;
    let mut truncated = false;
    let scanned_paths = AtomicU64::new(0);

    while let Some(idx) = queue.pop_front() {
        if cancel.load(AtomicOrdering::Relaxed) {
            return ScanResult {
                root: root.to_path_buf(),
                nodes,
                truncated,
                cancelled: true,
                errors,
                total_bytes: 0,
                total_files: 0,
                total_folders: 0,
                elapsed_ms: start.elapsed().as_millis(),
            };
        }
        let depth = nodes[idx].depth;
        if depth >= cfg.max_depth || !nodes[idx].entry.is_dir {
            continue;
        }
        let dir_path = nodes[idx].entry.path.clone();
        let rd = match std::fs::read_dir(&dir_path) {
            Ok(rd) => rd,
            Err(_) => {
                nodes[idx].denied = true;
                errors += 1;
                continue;
            }
        };
        let mut children: Vec<Entry> = Vec::new();
        for item in rd {
            let Ok(item) = item else { errors += 1; continue };
            let name = item.file_name().to_string_lossy().to_string();
            if name == "." || name == ".." {
                continue;
            }
            let path = item.path();
            let is_link = item.file_type().map(|t| t.is_symlink()).unwrap_or(false);
            if is_link && !cfg.follow_symlinks {
                // Record the link but never descend into it.
            }
            children.push(Entry::from_path(path, name));
            if nodes.len() >= cfg.max_entries {
                truncated = true;
                break;
            }
        }
        children.sort_by(|a, b| compare_names(&a.name, &b.name));
        for child in children {
            let child_is_dir = child.is_dir;
            let child_size = if child_is_dir { 0 } else { child.size.max(child.allocated) };
            let node_idx = nodes.len();
            nodes.push(ScanNode {
                entry: child,
                depth: depth + 1,
                parent: Some(idx),
                child_count: 0,
                total: child_size,
                direct_files: 0,
                direct_folders: 0,
                denied: false,
                truncated: false,
            });
            nodes[idx].child_count += 1;
            if child_is_dir {
                nodes[idx].direct_folders += 1;
                queue.push_back(node_idx);
            } else {
                nodes[idx].direct_files += 1;
            }
        }
        nodes[idx].truncated = truncated;
        let n = scanned_paths.fetch_add(1, AtomicOrdering::Relaxed) + 1;
        if n % 64 == 0 {
            if let Some(cb) = &progress {
                cb(nodes.len(), &dir_path);
            }
        }
        if truncated {
            break;
        }
    }

    // Aggregate sizes upward (children are always pushed after parents, so a
    // reverse pass is sufficient).
    for i in (1..nodes.len()).rev() {
        let total = nodes[i].total;
        if let Some(parent) = nodes[i].parent {
            nodes[parent].total = nodes[parent].total.saturating_add(total);
        }
    }
    let total_files = nodes.iter().skip(1).filter(|n| !n.entry.is_dir).count() as u64;
    let total_folders = nodes.iter().skip(1).filter(|n| n.entry.is_dir).count() as u64;
    let total_bytes = nodes.first().map(|n| n.total).unwrap_or(0);
    if let Some(cb) = &progress {
        cb(nodes.len(), root);
    }

    ScanResult {
        root: root.to_path_buf(),
        nodes,
        truncated,
        cancelled: false,
        errors,
        total_bytes,
        total_files,
        total_folders,
        elapsed_ms: start.elapsed().as_millis(),
    }
}

pub fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Collect all descendants of a folder for copy/delete/compress, without
/// following symlink directories (prevents infinite recursion).
pub fn collect_tree(root: &Path, limit: usize) -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for item in rd.flatten() {
            if out.len() >= limit {
                return out;
            }
            let path = item.path();
            let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
            out.push((path.clone(), is_dir));
            if is_dir {
                stack.push(path);
            }
        }
    }
    out
}

/// Validate a user-supplied filename (no separators, not empty, no NUL).
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("The name cannot be empty.".into());
    }
    if name.contains('/') {
        return Err("The name cannot contain '/'.".into());
    }
    if name.contains('\0') {
        return Err("The name contains an invalid character.".into());
    }
    if name == "." || name == ".." {
        return Err("That name is reserved.".into());
    }
    Ok(())
}

/// Produce a non-colliding sibling name: `photo.png` -> `photo (1).png`.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.into());
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 1..10_000 {
        let candidate = dir.join(format!("{} ({}){}", stem, n, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{} ({}){}", stem, std::process::id(), ext))
}

pub fn is_descendant(parent: &Path, child: &Path) -> bool {
    let Ok(parent) = parent.canonicalize() else { return false };
    let Ok(child) = child.canonicalize() else { return false };
    child != parent && child.starts_with(&parent)
}

pub fn uri_for(path: &Path) -> String {
    gtk4::gio::File::for_path(path).uri().to_string()
}

pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let file = gtk4::gio::File::for_uri(uri);
    file.path()
}

/// Trash contents, shaped as entries for a read-only browsing view.
pub struct TrashEntry {
    pub path: PathBuf,
    pub name: String,
    pub original_parent: PathBuf,
    pub deleted: Option<SystemTime>,
    pub is_dir: bool,
    pub size: Option<u64>,
}

pub fn trash_entries() -> Result<Vec<TrashEntry>, String> {
    let items = trash::os_limited::list().map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .map(|it| {
            let path = PathBuf::from(&it.id);
            let size = trash::os_limited::metadata(&it).ok().and_then(|m| m.size.size());
            TrashEntry {
                name: it.name.to_string_lossy().to_string(),
                original_parent: it.original_parent.clone(),
                deleted: if it.time_deleted >= 0 {
                    SystemTime::UNIX_EPOCH
                        .checked_add(std::time::Duration::from_secs(it.time_deleted as u64))
                } else {
                    None
                },
                is_dir: path.is_dir(),
                path,
                size,
            }
        })
        .collect())
}

pub fn restore_trash(items: Vec<trash::TrashItem>) -> Result<(), String> {
    trash::os_limited::restore_all(items).map_err(|e| e.to_string())
}
