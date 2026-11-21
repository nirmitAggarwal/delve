//! Optional LocalSend integration.
//!
//! The integration is capability-detected and entirely optional: when LocalSend
//! is not installed (or the network is unavailable) Delve still launches and all
//! other features keep working. Discovery uses LocalSend's documented multicast
//! announce; transfers use the LocalSend v2 HTTP API.

use crate::fs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const LOCAL_SEND_PORT: u16 = 53317;
const MULTICAST_ADDR: &str = "224.0.0.167:53317";

#[derive(Clone, Debug)]
pub enum Availability {
    Available { detail: String },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceInfo {
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub device_model: String,
    #[serde(default)]
    pub device_type: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub download: bool,
}

fn default_protocol() -> String {
    "https".into()
}

#[derive(Clone, Debug)]
pub struct Device {
    pub info: DeviceInfo,
    pub host: String,
}

fn fingerprint() -> String {
    let host = std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "delve-host".into());
    let user = std::env::var("USER").unwrap_or_default();
    crate::util::hash_key(&[&host, &user, "delve"])
}

fn local_ip_for(addr: &str) -> Option<String> {
    // Determine the interface address used to reach the multicast group.
    let probe = UdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect(addr).ok()?;
    let local = probe.local_addr().ok()?;
    Some(local.ip().to_string())
}

pub fn availability() -> Availability {
    let mut found: Vec<String> = Vec::new();
    for program in ["localsend_app", "localsend", "localsend_appimage"] {
        if let Some(path) = crate::thumbnails::which(program) {
            found.push(format!("{}", path.display()));
        }
    }
    if crate::thumbnails::which("flatpak").is_some() {
        if let Ok(out) = std::process::Command::new("flatpak")
            .args(["list", "--app", "--columns=application"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.lines().any(|l| l.trim() == "org.localsend.localsend_app") {
                found.push("Flatpak org.localsend.localsend_app".into());
            }
        }
    }
    if found.is_empty() {
        Availability::Unavailable {
            reason: "LocalSend was not detected on this system. Install LocalSend to enable \
                     device-to-device transfers; all other Delve features work without it."
                .into(),
        }
    } else {
        Availability::Available {
            detail: format!("LocalSend detected ({})", found.join(", ")),
        }
    }
}

/// Announce and collect nearby devices for a short, bounded window.
pub fn discover(timeout: Duration) -> (Vec<Device>, Vec<String>) {
    let mut errors = Vec::new();
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => return (Vec::new(), vec![format!("Could not open a discovery socket: {e}")]),
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(400)));
    let _ = socket.set_multicast_ttl_v4(1);
    let local_ip = local_ip_for(MULTICAST_ADDR).unwrap_or_else(|| "0.0.0.0".into());
    let announce = serde_json::json!({
        "alias": device_alias(),
        "version": "2.0",
        "deviceModel": "Linux",
        "deviceType": "desktop",
        "fingerprint": fingerprint(),
        "port": LOCAL_SEND_PORT,
        "protocol": "http",
        "download": false
    });
    let payload = announce.to_string();
    if let Err(e) = socket.send_to(payload.as_bytes(), MULTICAST_ADDR) {
        errors.push(format!("Multicast announce failed: {e}"));
    }
    let mut devices: Vec<Device> = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 16 * 1024];
    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Ok(info) = serde_json::from_str::<DeviceInfo>(&text) {
                    if info.fingerprint == fingerprint() {
                        continue;
                    }
                    let host = from.ip().to_string();
                    if devices.iter().any(|d| d.info.fingerprint == info.fingerprint) {
                        continue;
                    }
                    if info.port == 0 {
                        continue;
                    }
                    devices.push(Device { info, host });
                }
            }
            Err(_) => {
                if deadline.saturating_duration_since(Instant::now()) > Duration::from_millis(400) {
                    let _ = socket.send_to(payload.as_bytes(), MULTICAST_ADDR);
                }
            }
        }
    }
    devices.sort_by(|a, b| a.info.alias.cmp(&b.info.alias));
    let _ = local_ip;
    (devices, errors)
}

pub fn device_alias() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    format!("Delve on {}", user)
}

struct FileEntry {
    id: String,
    file_name: String,
    size: u64,
    file_type: String,
}

fn gather_files(paths: &[PathBuf]) -> (Vec<FileEntry>, u64) {
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut counter = 0u64;
    for path in paths {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let base_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if meta.is_dir() {
            for (child, is_dir) in fs::collect_tree(path, 20_000) {
                if is_dir {
                    continue;
                }
                let Ok(cm) = std::fs::metadata(&child) else { continue };
                let rel = child.strip_prefix(path).unwrap_or(&child);
                let name = format!("{}/{}", base_name, rel.to_string_lossy().replace('\\', "/"));
                counter += 1;
                total += cm.len();
                files.push(FileEntry {
                    id: format!("{counter}"),
                    file_name: name,
                    size: cm.len(),
                    file_type: fs::mime_for_path(&child),
                });
            }
        } else {
            counter += 1;
            total += meta.len();
            files.push(FileEntry {
                id: format!("{counter}"),
                file_name: base_name,
                size: meta.len(),
                file_type: fs::mime_for_path(path),
            });
        }
    }
    (files, total)
}

/// Send paths to a discovered device. Files only; folders are expanded into
/// their files with relative paths, which LocalSend displays as a folder.
pub fn send_paths(
    device: &Device,
    paths: &[PathBuf],
    cancel: Arc<AtomicBool>,
    progress: Arc<dyn Fn(f64, String) + Send + Sync>,
) -> Result<usize, String> {
    let (files, total_bytes) = gather_files(paths);
    if files.is_empty() {
        return Err("There are no files to send in the current selection.".into());
    }
    let scheme = if device.info.protocol.is_empty() { "https" } else { &device.info.protocol };
    let base = format!("{}://{}:{}", scheme, device.host, device.info.port);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout_read(Duration::from_secs(30))
        .build();

    let mut map = serde_json::Map::new();
    for file in &files {
        map.insert(
            file.id.clone(),
            serde_json::json!({
                "id": file.id,
                "fileName": file.file_name,
                "size": file.size,
                "fileType": file.file_type,
            }),
        );
    }
    let body = serde_json::json!({
        "info": {
            "alias": device_alias(),
            "version": "2.0",
            "deviceModel": "Linux",
            "deviceType": "desktop",
            "fingerprint": fingerprint(),
            "port": LOCAL_SEND_PORT,
            "protocol": "http",
            "download": false
        },
        "files": map
    });
    let response = agent
        .post(&format!("{base}/api/localsend/v2/prepare-upload"))
        .send_json(&body)
        .map_err(|e| format!("The receiver did not accept the transfer: {e}"))?;
    #[derive(Deserialize)]
    struct PrepareResponse {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(default)]
        files: BTreeMap<String, String>,
    }
    let prepared: PrepareResponse = response
        .into_json()
        .map_err(|e| format!("Unexpected response from the receiver: {e}"))?;
    let mut sent = 0usize;
    let mut bytes_sent = 0u64;
    // Build a flat list in the same order as `files` so ids can be looked up.
    let mut flat: Vec<(String, PathBuf)> = Vec::new();
    for path in paths {
        let Ok(meta) = std::fs::symlink_metadata(path) else { continue };
        if meta.is_dir() {
            for (child, is_dir) in fs::collect_tree(path, 20_000) {
                if !is_dir {
                    flat.push((child.to_string_lossy().to_string(), child));
                }
            }
        } else {
            flat.push((path.to_string_lossy().to_string(), path.clone()));
        }
    }
    for (file, (_, path)) in files.iter().zip(flat.iter()) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Transfer cancelled.".into());
        }
        let Some(token) = prepared.files.get(&file.id) else {
            return Err("The receiver did not provide a token for a file.".into());
        };
        let mut data = Vec::new();
        std::fs::File::open(path)
            .and_then(|mut f| f.read_to_end(&mut data))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let url = format!(
            "{base}/api/localsend/v2/upload?sessionId={}&fileId={}&token={}",
            urlencode(&prepared.session_id),
            urlencode(&file.id),
            urlencode(token)
        );
        agent
            .post(&url)
            .set("Content-Type", "application/octet-stream")
            .send_bytes(&data)
            .map_err(|e| format!("Sending “{}” failed: {e}", file.file_name))?;
        sent += 1;
        bytes_sent += data.len() as u64;
        let fraction = if total_bytes > 0 { bytes_sent as f64 / total_bytes as f64 } else { 0.0 };
        progress(fraction, format!("Sending {}", file.file_name));
    }
    Ok(sent)
}

fn urlencode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn device_summary(device: &Device) -> String {
    let kind = if device.info.device_type.is_empty() {
        "device".to_string()
    } else {
        device.info.device_type.clone()
    };
    format!("{} · {} · {}", device.info.alias, kind, device.info.device_model)
}

#[allow(dead_code)]
fn parse_addr(host: &str, port: u16) -> Option<SocketAddr> {
    format!("{host}:{port}").parse().ok()
}

#[allow(dead_code)]
fn is_local_path(path: &Path) -> bool {
    path.is_absolute()
}
