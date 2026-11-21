//! Asynchronous thumbnail and preview generation with a rebuildable cache.
//!
//! Thumbnails are keyed by file identity (path, size, modification time) and
//! stored under the XDG cache directory. Video thumbnails use an optional,
//! runtime-detected backend; when unavailable, Delve falls back to a file icon.

use crate::fs::Entry;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub enum Preview {
    Image { texture: gtk4::gdk::Texture },
    Text(String),
    None,
}

fn cache_key(entry: &Entry, size: u32) -> String {
    let mtime = entry
        .modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::util::hash_key(&[
        &entry.path.to_string_lossy(),
        &entry.size.to_string(),
        &mtime.to_string(),
        &size.to_string(),
    ])
}

fn png_path(key: &str) -> PathBuf {
    crate::paths::thumbnails_dir().join(format!("{key}.png"))
}

/// Build a thumbnail, or return an existing cached one. Returns `None` when the
/// file type has no available thumbnail backend.
pub fn thumbnail_for(entry: &Entry, size: u32) -> Option<PathBuf> {
    if entry.is_dir || !entry.is_image() && !entry.is_video() && !entry.is_pdf() {
        return None;
    }
    let key = cache_key(entry, size);
    let path = png_path(&key);
    if path.is_file() {
        return Some(path);
    }
    let _ = std::fs::create_dir_all(crate::paths::thumbnails_dir());
    if entry.is_image() {
        if entry.ext() == "svg" {
            // Rasterise vector images when librsvg's CLI is available.
            let tool = which("rsvg-convert")?;
            let tmp = path.with_extension("tmp.png");
            let status = Command::new(tool)
                .args(["-w", &size.to_string(), "-h", &size.to_string(), "-o"])
                .arg(&tmp)
                .arg(&entry.path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if status.map(|s| s.success()).unwrap_or(false) && tmp.is_file() {
                std::fs::rename(&tmp, &path).ok()?;
                return Some(path);
            }
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        if entry.ext() == "ico" || entry.ext() == "avif" {
            // These need a renderer backend that is not guaranteed to exist.
            return None;
        }
        let img = image::open(&entry.path).ok()?;
        let thumb = img.thumbnail(size, size);
        // The temporary file keeps a .png suffix so the encoder can infer the
        // format; a bare .tmp extension made every save fail silently.
        let tmp = path.with_extension("tmp.png");
        // Flatten transparency over the canvas colour so previews stay legible.
        let mut rgba = thumb.to_rgba8();
        if rgba.pixels().any(|p| p.0[3] < 255) {
            for pixel in rgba.pixels_mut() {
                let a = pixel.0[3] as u32;
                for c in 0..3 {
                    pixel.0[c] = ((pixel.0[c] as u32 * a) / 255) as u8;
                }
                pixel.0[3] = 255;
            }
        }
        rgba.save(&tmp).ok()?;
        std::fs::rename(&tmp, &path).ok()?;
        return Some(path);
    }
    if entry.is_pdf() {
        if pdf_thumbnail(&entry.path, &path, size) {
            return Some(path);
        }
        return None;
    }
    if entry.is_video() {
        if video_thumbnail(&entry.path, &path, size) {
            return Some(path);
        }
    }
    None
}

/// Render a video frame. `ffmpegthumbnailer` has its own CLI; plain `ffmpeg`
/// needs different flags, so each backend is invoked explicitly.
fn video_thumbnail(input: &Path, out: &Path, size: u32) -> bool {
    if which("ffmpegthumbnailer").is_some() {
        let status = Command::new("ffmpegthumbnailer")
            .args(["-i"])
            .arg(input)
            .args(["-o"])
            .arg(out)
            .args(["-s", &size.to_string(), "-t", "10%"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if status.map(|s| s.success()).unwrap_or(false) && out.is_file() {
            return true;
        }
    }
    if which("ffmpeg").is_some() {
        let tmp = out.with_extension("tmp.png");
        let scale = format!("scale={size}:-1");
        let status = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-ss", "0.8", "-i"])
            .arg(input)
            .args(["-frames:v", "1", "-vf", &scale])
            .arg(&tmp)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if status.map(|s| s.success()).unwrap_or(false) && tmp.is_file() {
            if std::fs::rename(&tmp, out).is_ok() {
                return true;
            }
        }
        let _ = std::fs::remove_file(&tmp);
    }
    false
}

/// Render the first PDF page with whatever poppler/mutool renderer is around.
fn pdf_thumbnail(input: &Path, out: &Path, size: u32) -> bool {
    let prefix = out.with_extension("");
    if which("pdftoppm").is_some() {
        let ok = Command::new("pdftoppm")
            .args(["-png", "-f", "1", "-l", "1", "-singlefile", "-scale-to"])
            .arg(size.to_string())
            .arg(input)
            .arg(&prefix)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && out.is_file() {
            return true;
        }
    }
    if which("mutool").is_some() {
        let ok = Command::new("mutool")
            .args(["draw", "-o"])
            .arg(out)
            .args(["-w"])
            .arg(size.to_string())
            .arg(input)
            .arg("1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && out.is_file() {
            return true;
        }
    }
    false
}

pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let p = PathBuf::from(program);
        return if p.is_file() { Some(p) } else { None };
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Load a preview for the inspector. Never executes or trusts file contents.
pub fn preview_for(entry: &Entry, settings: &crate::settings::Settings) -> Preview {
    if entry.is_dir || entry.broken_link {
        return Preview::None;
    }
    if entry.is_image() || entry.is_pdf() || entry.is_video() {
        if let Some(path) = thumbnail_for(entry, 1000) {
            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                return Preview::Image { texture };
            }
        }
    }
    if settings.text_preview && entry.is_text() {
        if let Some(text) = read_text_preview(&entry.path, 64 * 1024) {
            return Preview::Text(text);
        }
    }
    Preview::None
}

fn read_text_preview(path: &Path, limit: u64) -> Option<String> {
    use std::io::Read;
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 8 * 1024 * 1024 {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; limit as usize];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    // Only render valid UTF-8; binary content is never displayed.
    let text = String::from_utf8(buf).ok()?;
    if text.chars().any(|c| c == '\0') {
        return None;
    }
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        if i >= 160 {
            out.push_str("…\n");
            break;
        }
        let trimmed: String = line.chars().take(300).collect();
        out.push_str(&trimmed);
        out.push('\n');
    }
    Some(out)
}

/// Remove cached thumbnails. Returns the number of removed files.
pub fn clear_cache() -> usize {
    let dir = crate::paths::thumbnails_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return 0 };
    let mut removed = 0;
    for item in rd.flatten() {
        if std::fs::remove_file(item.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn cache_size() -> u64 {
    let dir = crate::paths::thumbnails_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return 0 };
    rd.flatten()
        .filter_map(|i| i.metadata().ok())
        .map(|m| m.len())
        .sum()
}
