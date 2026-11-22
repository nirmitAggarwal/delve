# Delve support guide

This guide explains how to diagnose a Delve problem, collect useful information,
recover from a bad configuration, and report an issue without leaking private
data.

## Before reporting

1. Confirm the installed version: open **Settings → Advanced → Copy
   diagnostics**. The copied text includes the Delve version, GTK version,
   config/data/cache paths, theme, tab count, and thumbnail cache size.
2. Try the same action with a clean, temporary configuration. This tells you
   whether the problem is in Delve or in local settings:

   ```sh
   rm -rf /tmp/delve-clean
   mkdir -p /tmp/delve-clean/{cfg,data,cache,home}
   HOME=/tmp/delve-clean/home \
   XDG_CONFIG_HOME=/tmp/delve-clean/cfg \
   XDG_DATA_HOME=/tmp/delve-clean/data \
   XDG_CACHE_HOME=/tmp/delve-clean/cache \
   delve /tmp
   ```

   If the problem disappears with clean XDG directories, the report should
   mention that. Do not delete your real config to test this.
3. Check whether the problem also happens in a different view, with a different
   folder, or after restarting Delve.
4. Read the relevant section below before filing. Several common symptoms have a
   known cause and a direct fix.

## Diagnostics and logs

Delve has no telemetry and does not upload logs automatically. Everything stays
on the machine.

- **Copy diagnostics** — copies a plain-text summary to the clipboard. This is
  the best first attachment for a report.
- **Reveal log file** — opens the log location in your default file manager.
- Default log path: `${XDG_DATA_HOME:-~/.local/share}/delve/delve.log`.
- Settings: `${XDG_CONFIG_HOME:-~/.config}/delve/settings.json`.
- Tags and colours: `${XDG_DATA_HOME:-~/.local/share}/delve/metadata.json`.

The log records application events and errors. It may include folder names and
paths you opened. Review it before sharing and redact anything private.

## Recovering from a bad settings file

Delve loads settings defensively. If `settings.json` cannot be parsed:

1. Delve logs the parse error.
2. It copies the unreadable file to `settings.json.invalid` beside the original.
3. It starts with default settings.

To recover deliberately:

```sh
cd "${XDG_CONFIG_HOME:-$HOME/.config}/delve"
mv settings.json settings.json.broken
delve
```

A new defaults file is written on the next settings change. To restore the old
one after fixing it by hand, stop Delve first and replace the new file. Never
edit `settings.json` while Delve is running; the application writes the file
atomically and will overwrite concurrent edits.

Settings can also be exported and imported from **Settings → Advanced**. The
export contains settings and metadata, including machine-specific paths and
file identities. Treat it as personal data.

## Resetting parts of Delve

Do these with Delve closed.

| Goal | Action |
| --- | --- |
| Reset settings only | Delete `settings.json`, or use **Settings → Advanced → Reset all settings**. |
| Reset tags and folder colours | Delete `metadata.json`. This cannot be undone unless you exported a bundle first. |
| Remove a custom theme | Delete its directory under `~/.config/delve/themes/<name>/`, then choose another theme. |
| Remove custom icons | Delete `~/.local/share/delve/icons/` and clear the matching rules in Settings. |
| Rebuild thumbnails | Delete `~/.cache/delve/thumbnails/`; it is a cache and is regenerated. |
| Reset shortcuts | **Settings → Keyboard → Reset shortcuts**. |
| Fully reset Delve | Remove all three XDG directories listed above. This deletes settings, tags, themes, icons, thumbnails, and the log. |

Uninstalling the application does not remove these directories.

## Common problems

### The window does not open

Run Delve from a terminal and capture the output:

```sh
delve /tmp
```

Check that `DISPLAY` or `WAYLAND_DISPLAY` is set and that another application
can open a GTK4 window in the same session. If the process exits immediately,
include the terminal output in the report.

### Application menu entry or icon is missing

See [Install on Linux → Verify the installation](install-linux.md#verify-the-installation).
The common cause is installing the binary and the desktop files under different
prefixes, or a stale desktop/icon cache.

### Icons are blank or fall back to generic shapes

Delve embeds its icon theme. If the interface is blank, confirm the installed
binary matches the release (not a manually copied partial binary). If only
custom icons are wrong, clear the custom icon rules or choose **Delve** in
**Settings → Icons & theme**.

### A theme is rejected

Delve validates a theme in a throwaway CSS provider before applying it. An
invalid theme leaves the previous theme active and reports an error. Check:

- `theme.json` is valid JSON.
- Colour values are valid GTK colours (`#rrggbb`, `#rrggbbaa`, `rgb(...)`,
  `rgba(...)`, or a named colour).
- Length metrics include a unit such as `px` or `pt`.
- `style.css`, if present, is valid GTK CSS.

See [Theme format](theme-format.md).

### A folder does not open or is slow

Delve scans folders on worker threads, but a very large directory can still take
time. The Treemap and Graph views deliberately bound their scans. Check that the
filesystem is mounted and reachable, then try another folder. For network
mounts, a hung server can make any file manager wait; Delve cannot bypass the
kernel's mount timeout.

### Move to Trash fails

Trash requires a writable trash directory on the same filesystem, following the
freedesktop trash specification. Removable media, network shares, and some
container filesystems may not support it. Delve reports the error and does not
silently delete. Use permanent delete only after reviewing the confirmation.

### Copy or move reports conflicts

Open **Settings → Files** and review the conflict policy. The default is to ask.
A batch operation applies the chosen policy to nested conflicts. If a destination
changed during the operation, the report lists skipped or failed items.

### Terminal action is disabled

Delve detects terminals from `PATH`. Set an explicit command template in
**Settings → Terminal** using `{dir}` for the working directory, then use the
Test button. The template is argv-based; shell syntax is not interpreted.

### A command set does not appear

Check that it is enabled, that its folder and scope match the current folder,
and that it has at least one non-empty program step. See
[Folder commands](folder-commands.md#troubleshooting).

### A command set ran in the wrong directory

Review **Working directory** in the set editor: current folder, Home, or the
first selected folder. Placeholders expand inside arguments only, not in the
program name. `{folder}` is the profile's folder, not necessarily the shell's
current directory.

### LocalSend cannot find a device

- Install and launch LocalSend on both devices.
- Confirm both devices are on the same local network and multicast traffic is
  allowed. Guest Wi-Fi and isolated VLANs often block discovery.
- Discovery runs only while the transfer panel is open.
- Some LocalSend peers use HTTPS with a self-signed certificate. Delve refuses
  to disable certificate verification; the transfer fails with an error instead.

Receiving is not implemented in 1.0.0.

### The application crashes

Collect:

- the terminal output, if launched from a terminal
- the last lines of `delve.log`
- **Copy diagnostics**
- the exact steps that reproduce the crash
- whether a clean XDG configuration also crashes
- distribution, desktop session (Wayland or X11), GTK version, and architecture

If the crash leaves the settings file unreadable, keep
`settings.json.invalid`.

## Reporting an issue

Use the issue tracker of the release or source repository you obtained Delve
from. Include:

1. **Summary** — one sentence describing the symptom.
2. **Version and environment** — paste the diagnostics block, plus distribution
   and desktop session.
3. **Steps to reproduce** — start from a fresh folder and list exact clicks or
   keys.
4. **Expected and actual behaviour**.
5. **Evidence** — log excerpt, terminal output, and a screenshot if it is not
   sensitive.
6. **Scope** — whether a clean XDG configuration reproduces it, and whether it
   happens in more than one view.

Redact usernames, home paths, file names, network addresses, and anything else
you do not want public. Do not attach a full configuration export unless the
maintainer asks for it.

## Security reports

Delve runs with the privileges of the user who launches it. It does not run a
local daemon and does not listen for inbound connections. LocalSend discovery
uses an outbound multicast announce while the transfer panel is open.

If you find a security issue, report it privately to the maintainer rather than
opening a public issue with exploit details. Include the affected version, the
impact, and a minimal reproduction. Do not test against systems or data you do
not own.

## What support cannot promise

- Recovery of files deleted permanently outside Trash.
- Recovery of tags or folder colours after `metadata.json` is deleted without an
  export.
- Compatibility with every terminal emulator, desktop shell, or icon theme.
- Transfers with peers that require disabled TLS verification.
- Decoding of video or PDF previews, which are not implemented in 1.0.0.
