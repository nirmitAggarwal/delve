# Delve documentation

Delve is a GTK4 file manager for Linux written in Rust. This directory contains
the complete documentation set for version 1.0.1.

| Guide | What it covers |
| --- | --- |
| [User guide](user-guide.md) | Every view, operation, setting, and default keyboard shortcut. |
| [Install on Linux](install-linux.md) | Prebuilt tarballs, per-user and system installs, source builds, and uninstalling. |
| [Build and release](build-and-release.md) | Developer setup, release builds, versioning, metadata checks, and producing a tarball. |
| [Support](support.md) | Diagnostics, log files, recovery, common problems, and what information to include in a report. |
| [Folder commands](folder-commands.md) | Command sets: scopes, argv rules, placeholders, execution modes, quick slots, and safety. |
| [Theme format](theme-format.md) | Theme tokens, metrics, typography, extra CSS, validation, and live reload. |

Project-level files:

- [README](../README.md) — overview and quick start.
- [CHANGELOG](../CHANGELOG.md) — release history.
- [LICENSE](../LICENSE) — MIT license for Delve source and original artwork.
- [Bundled icon notes](../assets/icons/README.md) — icon provenance, licensing,
  and refresh instructions.

## Requirements at a glance

- Linux with a working XDG desktop and a display server supported by GTK4
  (Wayland or X11).
- GTK 4.10 or newer at runtime.
- Rust 1.80 or newer and the GTK development headers only when building from
  source.
- No network connection is required. Delve bundles its interface icons in the
  executable and never downloads a theme or asset at runtime.

## Data and privacy

Delve has no telemetry, analytics, crash uploader, or automatic network access.
It reads and writes local files only when you ask it to. The optional LocalSend
transfer panel opens a local multicast socket and contacts the device you choose;
it does nothing until you open the panel and start a transfer.

Settings, tags, themes, thumbnails, and logs stay under the XDG directories
listed in the [user guide](user-guide.md#storage-and-files). Uninstalling the
application preserves them.
