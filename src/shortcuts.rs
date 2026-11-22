//! Editable keyboard shortcuts with conflict detection helpers.

/// (action id, default accelerator, label)
pub const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("undo", "<Control>z", "Undo"),
    ("redo", "<Control><Shift>z", "Redo"),
    ("copy", "<Control>c", "Copy"),
    ("cut", "<Control>x", "Cut"),
    ("paste", "<Control>v", "Paste"),
    ("trash", "Delete", "Move to Trash"),
    ("delete", "<Shift>Delete", "Delete permanently"),
    ("rename", "F2", "Rename"),
    ("select-all", "<Control>a", "Select all"),
    ("new-tab", "<Control>t", "New tab"),
    ("close-tab", "<Control>w", "Close tab"),
    ("reopen-tab", "<Control><Shift>t", "Reopen closed tab"),
    ("location", "<Control>l", "Edit location"),
    ("search", "<Control>f", "Search"),
    ("back", "<Alt>Left", "Back"),
    ("forward", "<Alt>Right", "Forward"),
    ("up", "<Alt>Up", "Parent folder"),
    ("hidden", "<Control>h", "Toggle hidden files"),
    ("new-folder", "<Control><Shift>n", "New folder"),
    ("refresh", "F5", "Refresh"),
    ("view-icon", "<Control>1", "Icon view"),
    ("view-detail", "<Control>2", "Detail view"),
    ("view-recursive", "<Control>3", "Recursive view"),
    ("view-treemap", "<Control>4", "Treemap view"),
    ("view-graph", "<Control>5", "Graph view"),
    ("next-tab", "<Control>Tab", "Next tab"),
    ("prev-tab", "<Control><Shift>Tab", "Previous tab"),
    ("settings", "<Control>comma", "Settings"),
    ("properties", "<Alt>Return", "Properties"),
    ("terminal", "<Control><Shift>e", "Open in terminal"),
    ("runs", "<Control><Shift>r", "Toggle run panel"),
    ("spotlight", "<Super>space", "Quick Spotlight"),
    ("organise", "<Control><Shift>o", "Organise folder"),
    ("compress", "<Control><Shift>c", "Compress selection"),
    ("sidebar", "F9", "Toggle sidebar"),
    ("inspector", "F10", "Toggle inspector"),
];

pub fn default_accel(action: &str) -> &'static str {
    SHORTCUTS
        .iter()
        .find(|(id, _, _)| *id == action)
        .map(|(_, accel, _)| *accel)
        .unwrap_or("")
}

pub fn label(action: &str) -> &str {
    SHORTCUTS
        .iter()
        .find(|(id, _, _)| *id == action)
        .map(|(_, _, label)| *label)
        .unwrap_or(action)
}

/// Normalise an accelerator to GTK syntax (`<Control><Shift>k`). Accepts both
/// `Ctrl+Shift+K` and `<Control><Shift>k` forms.
pub fn normalize(accel: &str) -> String {
    let trimmed = accel.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut mods: Vec<&'static str> = Vec::new();
    let mut rest = trimmed.to_string();
    loop {
        if let Some(stripped) = rest.strip_prefix('<') {
            if let Some(end) = stripped.find('>') {
                let name = stripped[..end].to_lowercase();
                if let Some(m) = map_modifier(&name) {
                    if !mods.contains(&m) {
                        mods.push(m);
                    }
                    rest = stripped[end + 1..].trim_start_matches('+').to_string();
                    continue;
                }
            }
        }
        break;
    }
    let mut parts: Vec<String> = rest
        .split('+')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    let key = parts.pop().unwrap_or_default();
    for part in parts {
        if let Some(m) = map_modifier(&part.to_lowercase()) {
            if !mods.contains(&m) {
                mods.push(m);
            }
        }
    }
    if key.is_empty() {
        return String::new();
    }
    let key = if key.chars().count() == 1 {
        key.to_lowercase()
    } else {
        key
    };
    let mut out = String::new();
    for m in ["Control", "Shift", "Alt", "Super"] {
        if mods.contains(&m) {
            out.push_str(&format!("<{}>", m));
        }
    }
    out.push_str(&key);
    out
}

fn map_modifier(name: &str) -> Option<&'static str> {
    match name {
        "ctrl" | "control" | "primary" => Some("Control"),
        "shift" => Some("Shift"),
        "alt" | "meta" | "mod1" => Some("Alt"),
        "super" | "win" | "cmd" | "mod4" => Some("Super"),
        _ => None,
    }
}

/// Find a conflict: two actions sharing an accelerator.
pub fn conflict_for(
    shortcuts: &std::collections::HashMap<String, String>,
    action: &str,
    accel: &str,
) -> Option<String> {
    if accel.trim().is_empty() {
        return None;
    }
    for (id, default, _) in SHORTCUTS {
        if *id == action {
            continue;
        }
        let existing = shortcuts.get(*id).map(|s| s.as_str()).unwrap_or(default);
        if strings_equal(existing, accel) {
            return Some((*id).to_string());
        }
    }
    None
}

fn strings_equal(a: &str, b: &str) -> bool {
    normalize(a).eq_ignore_ascii_case(&normalize(b))
}

pub fn effective<'a>(
    settings: &'a crate::settings::Settings,
    action: &str,
) -> std::borrow::Cow<'a, str> {
    match settings.shortcuts.get(action) {
        Some(v) => std::borrow::Cow::Borrowed(v.as_str()),
        None => std::borrow::Cow::Borrowed(default_accel(action)),
    }
}
