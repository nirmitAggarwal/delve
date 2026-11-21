//! Small shared utilities: logging and helpers.

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Append a line to Delve's log file and stderr. Logging never panics and never
/// stores file contents or personal paths beyond the operation being reported.
pub fn log(message: &str) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{stamp}] {message}\n");
    eprint!("{line}");
    let path = crate::paths::log_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    // Keep the log bounded so diagnostics never grow without limit.
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 512 * 1024 {
            let _ = std::fs::remove_file(&path);
        }
    }
}

pub fn pretty_path(path: &Path) -> String {
    crate::paths::display_path(path)
}

/// Truncate a string for display without splitting a UTF-8 char.
pub fn ellipsize(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Escape text for use as a CSS string value.
pub fn css_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Parse `#rgb`, `#rrggbb`, `rgb(r,g,b)` or `rgba(...)` into (r,g,b,a).
pub fn parse_color(value: &str) -> Option<(u8, u8, u8, u8)> {
    let v = value.trim().to_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        let digits: Vec<char> = hex.chars().collect();
        let byte = |s: &[char]| -> Option<u8> {
            let text: String = s.iter().collect();
            u8::from_str_radix(&text, 16).ok()
        };
        return match digits.len() {
            3 => Some((
                byte(&digits[0..1])? * 17,
                byte(&digits[1..2])? * 17,
                byte(&digits[2..3])? * 17,
                255,
            )),
            6 => Some((byte(&digits[0..2])?, byte(&digits[2..4])?, byte(&digits[4..6])?, 255)),
            8 => Some((
                byte(&digits[0..2])?,
                byte(&digits[2..4])?,
                byte(&digits[4..6])?,
                byte(&digits[6..8])?,
            )),
            _ => None,
        };
    }
    if let Some(inner) = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb(")) {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if parts.len() >= 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            let a = if parts.len() > 3 {
                (parts[3].parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0) as u8
            } else {
                255
            };
            return Some((r, g, b, a));
        }
    }
    None
}

pub fn color_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Relative luminance, used to pick readable text on coloured chips.
pub fn is_light_color(value: &str) -> bool {
    match parse_color(value) {
        Some((r, g, b, _)) => (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) > 150.0,
        None => false,
    }
}

/// Stable cache key for a path + fingerprint.
pub fn hash_key(parts: &[&str]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    format!("{:016x}", h.finish())
}
