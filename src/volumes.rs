//! Removable media and partitions.
//!
//! Volumes are read from GLib's volume monitor so a pendrive appears the moment
//! it is plugged in (the monitor emits `mount-added`/`volume-added`). Mounted
//! filesystems that GLib does not expose as a volume — internal partitions for
//! example — are merged in from `/proc/self/mountinfo` so the dashboard is
//! never emptier than `df`.

use crate::app::App;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// One row in the storage UI. Cheap to clone: the GLib handles are refcounted.
#[derive(Clone)]
pub struct VolumeInfo {
    pub name: String,
    pub icon: String,
    pub mount_point: Option<PathBuf>,
    pub can_mount: bool,
    pub can_eject: bool,
    pub removable: bool,
    /// Set for volumes GLib knows about; `None` for plain mountinfo entries.
    volume: Option<gio::Volume>,
    mount: Option<gio::Mount>,
}

impl VolumeInfo {
    /// A sensible symbolic icon for the volume, falling back to a drive glyph.
    pub fn icon_name(&self) -> String {
        let raw = self
            .icon
            .trim()
            .trim_start_matches("file:")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if raw.is_empty() || raw.contains('.') {
            if self.removable {
                "drive-removable-media-usb-symbolic".to_string()
            } else {
                "drive-harddisk-symbolic".to_string()
            }
        } else if raw.ends_with("-symbolic") {
            raw
        } else {
            format!("{raw}-symbolic")
        }
    }
}

fn volume_from_gio(volume: &gio::Volume) -> VolumeInfo {
    let mount = volume.get_mount();
    let mount_point = mount
        .as_ref()
        .and_then(|m| m.root().path())
        .filter(|p| p.is_absolute());
    let drive = volume.drive();
    let removable = drive.as_ref().map(|d| d.is_removable()).unwrap_or(false);
    let can_eject = volume.can_eject() || mount.as_ref().map(|m| m.can_eject()).unwrap_or(false);
    VolumeInfo {
        name: volume.name().to_string(),
        icon: volume.icon().to_string().map(|s| s.to_string()).unwrap_or_default(),
        mount_point,
        can_mount: volume.can_mount(),
        can_eject,
        removable,
        volume: Some(volume.clone()),
        mount,
    }
}

/// Every volume the session knows about: mounted drives, unmounted removable
/// media, and mounted filesystems that only exist in mountinfo.
pub fn snapshot() -> Vec<VolumeInfo> {
    let monitor = gio::VolumeMonitor::get();
    let mut out: Vec<VolumeInfo> = monitor.volumes().iter().map(volume_from_gio).collect();
    let known: Vec<PathBuf> = out.iter().filter_map(|v| v.mount_point.clone()).collect();
    let home_mount = crate::paths::home();
    for (label, path) in crate::paths::mounts() {
        if path == crate::paths::root() || home_mount.starts_with(&path) {
            continue;
        }
        if known.iter().any(|k| k == &path) {
            continue;
        }
        if out.iter().any(|v| v.mount_point.as_deref() == Some(path.as_path())) {
            continue;
        }
        out.push(VolumeInfo {
            name: label,
            icon: String::new(),
            mount_point: Some(path),
            can_mount: false,
            can_eject: false,
            removable: false,
            volume: None,
            mount: None,
        });
    }
    // Mounted volumes first, then removable media ready to mount.
    out.sort_by(|a, b| {
        b.mount_point
            .is_some()
            .cmp(&a.mount_point.is_some())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// Mount a volume, then hand the resulting mount point to `done` on the main
/// loop. Errors are reported through `done` as well.
pub fn mount_volume(app: &Rc<App>, volume: &VolumeInfo, done: impl Fn(Option<PathBuf>, Option<String>) + 'static) {
    let Some(handle) = volume.volume.clone() else {
        done(None, Some("This volume cannot be mounted from Delve.".into()));
        return;
    };
    if let Some(mount) = handle.get_mount() {
        if let Some(path) = mount.root().path() {
            done(Some(path), None);
        }
        return;
    }
    let _ = app;
    let name = volume.name.clone();
    let callback_handle = handle.clone();
    handle.mount(
        gio::MountMountFlags::NONE,
        gio::MountOperation::NONE,
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(()) => {
                let path = callback_handle
                    .get_mount()
                    .and_then(|m| m.root().path())
                    .filter(|p| p.is_absolute());
                match path {
                    Some(path) => done(Some(path), None),
                    None => done(None, Some(format!("Mounted “{name}” but could not find its path."))),
                }
            }
            Err(err) => done(None, Some(format!("Could not mount “{name}”: {err}"))),
        },
    );
}

/// Unmount (and power off when possible) the volume at `mount_point`.
pub fn unmount(app: &Rc<App>, mount_point: &Path, done: impl Fn(Result<(), String>) + 'static) {
    let monitor = gio::VolumeMonitor::get();
    let mount = monitor
        .mounts()
        .into_iter()
        .find(|m| m.root().path().as_deref() == Some(mount_point));
    let Some(mount) = mount else {
        done(Err("That volume is no longer mounted.".into()));
        return;
    };
    let _ = app;
    let path = mount_point.to_path_buf();
    mount.unmount_with_operation(
        gio::MountUnmountFlags::NONE,
        gio::MountOperation::NONE,
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(()) => done(Ok(())),
            Err(err) => done(Err(format!("Could not eject {}: {err}", path.display()))),
        },
    );
}

fn dispatch_refresh() {
    // Rebuild outside the GIO signal emission: the sidebar and dashboard own
    // widgets whose handlers must not be torn down mid-dispatch.
    crate::dispatch::post_simple(|app| {
        app.refresh_sidebar();
        app.refresh_home_dashboard();
    });
}

/// A cheap fingerprint of the current storage set. Two snapshots with the same
/// fingerprint describe the same drives, so the poller can stay quiet.
fn signature() -> String {
    let mut out = String::new();
    for volume in snapshot() {
        out.push_str(&volume.name);
        out.push('|');
        if let Some(path) = &volume.mount_point {
            out.push_str(&path.to_string_lossy());
        }
        out.push('|');
        out.push(if volume.can_mount { 'm' } else { '-' });
        out.push(if volume.removable { 'r' } else { '-' });
        out.push('\n');
    }
    out
}

/// Watch for hot-plugged media for the lifetime of the process. GLib keeps the
/// monitor alive itself; the signal handlers make the sidebar and dashboard
/// update immediately when a drive is plugged in, mounted or removed.
pub fn start_monitor(app: &Rc<App>) {
    let monitor = gio::VolumeMonitor::get();
    {
        let weak = Rc::downgrade(app);
        monitor.connect_mount_added(move |_, _| {
            if weak.upgrade().is_some() {
                dispatch_refresh();
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        monitor.connect_mount_removed(move |_, _| {
            if weak.upgrade().is_some() {
                dispatch_refresh();
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        monitor.connect_volume_added(move |_, _| {
            if weak.upgrade().is_some() {
                dispatch_refresh();
            }
        });
    }
    {
        let weak = Rc::downgrade(app);
        monitor.connect_volume_removed(move |_, _| {
            if weak.upgrade().is_some() {
                dispatch_refresh();
            }
        });
    }
    // Not every session has a volume monitor backend that emits hot-plug
    // signals (containers, minimal desktops, some udisks setups). Poll the
    // mount table as a safety net so media still appears and disappears on its
    // own while Delve sits in the background.
    {
        let weak = Rc::downgrade(app);
        let mut last = signature();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let current = signature();
            if current != last {
                last = current;
                if weak.upgrade().is_some() {
                    dispatch_refresh();
                }
            }
            glib::ControlFlow::Continue
        });
    }
    // VolumeMonitor::get() is a process-wide singleton; keep a reference so the
    // signals above cannot outlive their owner.
    glib::timeout_add_local_once(std::time::Duration::from_millis(0), move || {
        let _ = &monitor;
    });
}
