//! Terminal integration: runtime detection, safe argument passing, no shell.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct TerminalInfo {
    pub name: String,
    pub program: String,
    /// Template where `{dir}` is replaced by a single, safely quoted argument.
    pub template: String,
}

/// Terminals known to accept a working-directory flag. Detection is by PATH
/// lookup; none of these are required for Delve to run.
const KNOWN: &[(&str, &str, &str)] = &[
    // Kitty is listed first: it is the editor's terminal of choice whenever it
    // is installed, ahead of generic system defaults that may pick something
    // else entirely.
    ("Kitty", "kitty", "--directory={dir}"),
    ("Ghostty", "ghostty", "--working-directory={dir}"),
    ("Alacritty", "alacritty", "--working-directory={dir}"),
    ("WezTerm", "wezterm", "start --cwd {dir}"),
    ("Foot", "foot", "--working-directory={dir}"),
    ("Ptyxis", "ptyxis", "--new-window --working-directory={dir}"),
    ("GNOME Terminal", "gnome-terminal", "--working-directory={dir}"),
    ("Konsole", "konsole", "--workdir={dir}"),
    ("Xfce Terminal", "xfce4-terminal", "--working-directory={dir}"),
    ("Tilix", "tilix", "--working-directory={dir}"),
    ("Mate Terminal", "mate-terminal", "--working-directory={dir}"),
    ("LXTerminal", "lxterminal", "--working-directory={dir}"),
    ("Terminator", "terminator", "--working-directory={dir}"),
    ("XTerm", "xterm", ""),
    ("urxvt", "urxvt", ""),
    ("st", "st", ""),
    // Generic launchers last so a specific terminal always wins.
    ("xdg-terminal-exec", "xdg-terminal-exec", ""),
];

pub fn detect_terminals() -> Vec<TerminalInfo> {
    let mut out = Vec::new();
    for (name, program, template) in KNOWN {
        if crate::thumbnails::which(program).is_some() {
            out.push(TerminalInfo {
                name: (*name).to_string(),
                program: (*program).to_string(),
                template: (*template).to_string(),
            });
        }
    }
    // Debian's alternatives symlink is a portable fallback, never preferred
    // over a terminal that was found explicitly.
    if crate::thumbnails::which("x-terminal-emulator").is_some() {
        out.push(TerminalInfo {
            name: "System default terminal".into(),
            program: "x-terminal-emulator".into(),
            template: String::new(),
        });
    }
    out
}

/// Parse a command template into arguments, honouring single and double quotes
/// and backslash escapes. No shell is ever involved.
pub fn split_args(template: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = template.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut has_token = false;
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    has_token = true;
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() || has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if !current.is_empty() || has_token {
        args.push(current);
    }
    args
}

fn substitute(args: &[String], dir: &Path) -> Vec<String> {
    let dir_str = dir.to_string_lossy().to_string();
    let mut out = Vec::new();
    for arg in args {
        out.push(arg.replace("{dir}", &dir_str));
    }
    out
}

/// Build the command for a directory. When the template has no `{dir}`
/// placeholder the process is started in that directory instead.
pub fn build_command(settings: &crate::settings::Settings, dir: &Path) -> Result<Command, String> {
    let template = settings.terminal.trim();
    let mut argv: Vec<String> = if template.is_empty() {
        let terminals = detect_terminals();
        let info = terminals
            .into_iter()
            .next()
            .ok_or_else(|| "No terminal emulator was detected. Set one in Settings → Terminal.".to_string())?;
        let mut v = vec![info.program.clone()];
        v.extend(split_args(&info.template));
        v
    } else {
        split_args(template)
    };
    if argv.is_empty() {
        return Err("The terminal command is empty.".into());
    }
    let program = argv.remove(0);
    let program_path = crate::thumbnails::which(&program).ok_or_else(|| {
        format!("The terminal “{program}” was not found. Choose another in Settings → Terminal.")
    })?;
    let mut cmd = Command::new(program_path);
    if argv.iter().any(|a| a.contains("{dir}")) {
        cmd.args(substitute(&argv, dir));
    } else {
        if !argv.is_empty() {
            cmd.args(&argv);
        }
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

/// Expand a profile step into a direct argv command. No shell is involved.
pub fn step_command(
    profile: &crate::settings::RunCommand,
    step: &crate::settings::RunStep,
    folder: &Path,
    selected: &[PathBuf],
) -> Result<Command, String> {
    let program = crate::thumbnails::which(&step.program)
        .ok_or_else(|| format!("“{}” was not found on this system.", step.program))?;
    let mut cmd = Command::new(program);
    let folder_str = folder.to_string_lossy().to_string();
    for arg in &step.args {
        match arg.as_str() {
            "{selected}" | "{files}" => {
                for path in selected {
                    cmd.arg(path);
                }
            }
            _ => {
                let expanded = arg
                    .replace("{folder}", &folder_str)
                    .replace("{dir}", &folder_str)
                    .replace("{home}", &crate::paths::home().to_string_lossy())
                    .replace("{name}", &crate::fs::file_name_of(folder));
                cmd.arg(expanded);
            }
        }
    }
    for (key, value) in &profile.env {
        cmd.env(key, value);
    }
    let cwd = match profile.cwd_rule {
        crate::settings::CwdRule::Folder => folder.to_path_buf(),
        crate::settings::CwdRule::Home => crate::paths::home(),
        crate::settings::CwdRule::Selected => selected
            .first()
            .filter(|p| p.is_dir())
            .cloned()
            .unwrap_or_else(|| folder.to_path_buf()),
    };
    if cwd.is_dir() {
        cmd.current_dir(cwd);
    }
    Ok(cmd)
}

/// Run one step of a profile inside a new terminal window. Terminal-specific
/// exec flags are chosen from the detected emulator; a custom terminal uses
/// `-e`, the most widely supported form.
pub fn launch_program_in_terminal(
    settings: &crate::settings::Settings,
    dir: &Path,
    program: &str,
    args: &[String],
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let mut cmd = build_command(settings, dir)?;
    for (key, value) in env {
        cmd.env(key, value);
    }
    let terminal = if settings.terminal.trim().is_empty() {
        detect_terminals()
            .into_iter()
            .next()
            .map(|t| t.program)
            .unwrap_or_default()
    } else {
        split_args(settings.terminal.trim())
            .into_iter()
            .next()
            .unwrap_or_default()
    };
    let base = Path::new(&terminal)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| terminal.clone());
    match base.as_str() {
        "kitty" | "xterm" | "urxvt" | "st" | "xdg-terminal-exec" => {}
        "wezterm" | "gnome-terminal" | "ptyxis" => {
            cmd.arg("--");
        }
        _ => {
            cmd.arg("-e");
        }
    };
    cmd.arg(program);
    cmd.args(args);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not launch the terminal: {e}"))
}

pub fn launch_terminal(settings: &crate::settings::Settings, dir: &Path) -> Result<(), String> {
    let mut cmd = build_command(settings, dir)?;
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not launch the terminal: {e}"))
}
