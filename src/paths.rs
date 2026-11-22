//! XDG base-directory handling and standard user locations.
//!
//! Delve never writes into its installation directory and never hard-codes
//! machine-specific paths. Every location is derived from the environment with
//! well-defined fallbacks, as required by the specification.

use std::path::{Path, PathBuf};

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn config_home() -> PathBuf {
    env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

fn data_home() -> PathBuf {
    env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"))
}

fn cache_home() -> PathBuf {
    env_path("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache"))
}

/// `$XDG_CONFIG_HOME/delve` (default `~/.config/delve`).
pub fn config_dir() -> PathBuf {
    config_home().join("delve")
}

/// `$XDG_DATA_HOME/delve` (default `~/.local/share/delve`).
pub fn data_dir() -> PathBuf {
    data_home().join("delve")
}

/// `$XDG_CACHE_HOME/delve` (default `~/.cache/delve`).
pub fn cache_dir() -> PathBuf {
    cache_home().join("delve")
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn metadata_file() -> PathBuf {
    data_dir().join("metadata.json")
}

/// Uploaded icon assets. Kept beside metadata rather than in the cache so
/// custom icons survive cache cleanup and configuration export.
pub fn icons_dir() -> PathBuf {
    data_dir().join("icons")
}

pub fn thumbnails_dir() -> PathBuf {
    cache_dir().join("thumbnails")
}

pub fn log_file() -> PathBuf {
    data_dir().join("delve.log")
}

/// Create the XDG directories Delve uses. Failures are tolerated: the app still
/// runs, features that need a directory report their own errors.
pub fn ensure_dirs() {
    for dir in [
        config_dir(),
        data_dir(),
        cache_dir(),
        themes_dir(),
        icons_dir(),
        thumbnails_dir(),
    ] {
        let _ = std::fs::create_dir_all(&dir);
    }
}

/// Entry point for portable configuration export.
pub fn export_file() -> PathBuf {
    let desktop = user_dir(UserDir::Desktop);
    if desktop.is_dir() {
        desktop.join("delve-config.json")
    } else {
        config_dir().join("delve-config.json")
    }
}

#[derive(Clone, Copy)]
pub enum UserDir {
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Videos,
    Music,
}

/// Resolve an XDG user directory, honouring `user-dirs.dirs`, with sensible
/// English fallbacks that are only used when the file is missing.
pub fn user_dir(kind: UserDir) -> PathBuf {
    let key = match kind {
        UserDir::Desktop => "XDG_DESKTOP_DIR",
        UserDir::Documents => "XDG_DOCUMENTS_DIR",
        UserDir::Downloads => "XDG_DOWNLOAD_DIR",
        UserDir::Pictures => "XDG_PICTURES_DIR",
        UserDir::Videos => "XDG_VIDEOS_DIR",
        UserDir::Music => "XDG_MUSIC_DIR",
    };
    let file = config_home().join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(&file) {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            if k.trim() != key {
                continue;
            }
            let v = v.trim().trim_matches('"');
            let expanded = if let Some(rest) = v.strip_prefix("$HOME") {
                if rest.is_empty() || rest.starts_with('/') {
                    format!("{}{}", home().display(), rest)
                } else {
                    continue;
                }
            } else {
                v.to_string()
            };
            let p = PathBuf::from(expanded);
            if p.is_absolute() {
                return p;
            }
        }
    }
    let name = match kind {
        UserDir::Desktop => "Desktop",
        UserDir::Documents => "Documents",
        UserDir::Downloads => "Downloads",
        UserDir::Pictures => "Pictures",
        UserDir::Videos => "Videos",
        UserDir::Music => "Music",
    };
    home().join(name)
}

/// Filesystem root, used as the top of the navigation tree.
pub fn root() -> PathBuf {
    PathBuf::from("/")
}

/// Mount points parsed from `/proc/self/mountinfo`, with the most useful
/// (real, writable-ish) volumes first. Pseudo filesystems are filtered out.
pub fn mounts() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return out;
    };
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let _id = fields.next();
        let _parent = fields.next();
        let _dev = fields.next();
        let Some(root_field) = fields.next() else { continue };
        let Some(mount_point) = fields.next() else { continue };
        let mount = unescape_mount(mount_point);
        if !mount.starts_with('/') {
            continue;
        }
        if root_field != "/" && mount.starts_with("/var/lib/docker") {
            continue;
        }
        let pseudo = [
            "/proc", "/sys", "/dev/pts", "/dev/shm", "/run/user", "/sys/fs/cgroup",
            "/proc/sys/fs/binfmt_misc", "/dev/mqueue", "/dev/hugepages", "/run/lock",
            "/snap", "/var/lib/snapd", "/boot/efi",
        ];
        if pseudo.iter().any(|p| mount == *p || mount.starts_with(&format!("{}/", p))) {
            continue;
        }
        if mount == "/" || mount.starts_with("/media/") || mount.starts_with("/mnt/")
            || mount.starts_with("/run/media/") || mount.starts_with("/home/")
        {
            let label = mount_label(Path::new(&mount));
            out.push((label, PathBuf::from(mount)));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

/// Remote/network mounts detected in mountinfo, without pretending any
/// unsupported protocol works.
pub fn network_mounts() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return out;
    };
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let mount = unescape_mount(fields[4]);
        if !mount.starts_with('/') {
            continue;
        }
        let Some(sep) = fields.iter().position(|f| *f == "-") else { continue };
        let fs_type = fields.get(sep + 1).copied().unwrap_or("");
        let remote = matches!(
            fs_type,
            "nfs" | "nfs4" | "cifs" | "smb3" | "smbfs" | "sshfs" | "fuse.sshfs" | "davfs"
                | "fuse.davfs" | "fuse.rclone" | "9p" | "ceph" | "glusterfs" | "afs"
        );
        if remote {
            let label = mount
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(&mount)
                .to_string();
            out.push((label, PathBuf::from(mount)));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

fn mount_label(path: &Path) -> String {
    if path == Path::new("/") {
        return "File System".into();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn unescape_mount(s: &str) -> String {
    // mountinfo escapes space, tab, newline and backslash as \ooo.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let oct = &s[i + 1..i + 4];
            if let Ok(v) = u8::from_str_radix(oct, 8) {
                out.push(v as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// A short, human-friendly display name for a location: home becomes `~`.
pub fn display_path(path: &Path) -> String {
    let home = home();
    if path == home {
        return "~".into();
    }
    if let Ok(rest) = path.strip_prefix(&home) {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// Expand a leading `~` or `$HOME` typed by the user without invoking a shell.
pub fn expand_user_path(input: &str) -> PathBuf {
    let t = input.trim();
    if t == "~" {
        return home();
    }
    if let Some(rest) = t.strip_prefix("~/") {
        return home().join(rest);
    }
    if let Some(rest) = t.strip_prefix("$HOME/") {
        return home().join(rest);
    }
    if Path::new(t).is_absolute() {
        PathBuf::from(t)
    } else {
        // Relative paths are resolved against home, matching file-manager
        // location-entry conventions.
        home().join(t)
    }
}
