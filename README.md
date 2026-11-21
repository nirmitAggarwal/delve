# Delve

Delve is a GTK4 file manager for Linux written in Rust. It aims to be a complete,
daily-driver file browser: five ways to look at a folder, independent tabs, real
file operations, archives, tags, previews, and a theme system you can edit while
the app is running.

Delve is distribution-neutral. It does not depend on GNOME, KDE, Hyprland, DMS,
a particular icon pack, terminal, or desktop shell CSS. It needs GTK 4.10 or
newer and a normal XDG desktop.

## Build and run

System packages (Debian/Ubuntu names):

```sh
sudo apt install build-essential pkg-config libgtk-4-dev
```

Then:

```sh
cargo build --release
./target/release/delve
```

Optionally pass a path to open it at startup:

```sh
./target/release/delve ~/Downloads
```

Rust 1.80 or newer is required. No network access is needed at runtime; the only
optional network feature is LocalSend (see below).

## What it does

### Views

All views share the same selection, sort order, filter, tags, and file
operations. Switching views never changes what is selected.

- **Icon** — flow grid with thumbnails, configurable icon size, rubber-band
  selection, and drag and drop.
- **Detail** — sortable columns (name, type, size, modified, permissions, owner,
  tags), per-column visibility, inline tag and colour indicators.
- **Recursive** — flattened tree of the current folder with depth control,
  expand/collapse, and the same rows as Detail.
- **Treemap** — squarified size map of the folder, with zoom into subfolders and
  filters for files, folders, or media.
- **Graph** — radial layout of the folder tree with pan, zoom, and search.

Folder scans run on worker threads and can be cancelled. Treemap and Graph use
bounded scans so a pathological tree cannot freeze the UI.

### Tabs

- Independent history, view, sort/filter, selection, and scroll position per tab.
- New, duplicate, close, close others, close to the right, reopen closed tab.
- Drag to reorder tabs.
- Middle-click closes a tab.

### Files and folders

- Copy, cut, paste (with conflict prompt: replace, skip, keep both, or ask).
- Rename via a small popover anchored to the current item (no full-window
  dialog), with validation and conflict checks.
- Move to Trash and restore from Trash; permanent deletion always requires an
  explicit confirmation.
- Create folder, duplicate, bulk rename preview, and multi-selection operations.
- Drag and drop between folders, panes, and tabs, with copy/move/link feedback
  from the drop target.
- **Organise** builds a move plan from rules (extension/category, date, size,
  name). You see a full preview before anything moves, then apply once.
- Properties dialog with permissions and ownership display, recursive size
  calculation off the main thread.

### Archives

- Compress to `.zip`, `.tar`, and `.tar.gz`.
- Extract archives from the toolbar or context menu, or extract a single entry.
- Extraction refuses entries that would escape the destination (path traversal
  and absolute-path defence) and reports skipped entries.

### Tags and folder colours

- Tags are stored against a stable file identity (device + inode) where the
  filesystem provides one, with a path fallback.
- Tags are searchable and can be shown as dots in all views.
- Folder colours come from a built-in palette and render in the sidebar, icon
  view, and list rows.
- Sidebar sections for Places, Tags, Library (Recent, Trash), Network, and
  System; sidebar collapse state is persisted.

### Previews

- Image previews and thumbnails (PNG, JPEG, GIF, WebP, BMP), generated in the
  background and cached under `~/.cache/delve/thumbnails`.
- Text preview with a size cap and binary detection.
- File metadata grid, tag chips, and folder colour controls in the inspector.

### Terminal, shortcuts, commands

- Terminal detection scans `PATH` for a supported emulator; nothing is
  hard-coded to a desktop.
- "Open Terminal Here" and other terminal actions are available from the
  context menu once a terminal is detected.
- Folder run commands are a single feature: define a command with a shortcut or
  toolbar/context action, and it appears exactly once in the UI.
- Every shortcut is editable in Settings, with conflict detection and reset to
  defaults. Shortcuts are stored in XDG config, not compiled in.
- Hover help shows the active binding.

### Themes and icons

- Delve ships its own token-based design system. It does not import desktop
  shell CSS.
- Built-in themes: Delve Dark, Delve Light, Delve Ember.
- Custom themes live in `~/.config/delve/themes/<name>/` and are plain JSON plus
  an optional `style.css`. See [docs/theme-format.md](docs/theme-format.md).
- A custom theme is validated in a throwaway CSS provider before it replaces the
  active theme, so a typo never leaves the app unstyled.
- Live reload: while a file-backed theme is active, changes to `theme.json` or
  `style.css` are applied automatically.
- Icon theme selection lists installed themes and changes icons live. When no
  theme is chosen, a sensible installed fallback is used.
- Density, UI font, monospace font, accent colour, and transparency are all
  configurable and applied at runtime.

### Transfers

Delve can send files to LocalSend devices. Support is optional: when LocalSend
is not available, the panel explains that and the rest of the app is unaffected.
Delve never blocks startup on discovery.

- Multicast discovery of LocalSend devices on the local network.
- Sends files using the LocalSend v2 prepare-upload/upload protocol.
- Folders are expanded to their files with relative names.
- Device discovery runs only when you open the transfer panel.

### Storage

Delve follows the XDG base directory specification:

| Purpose | Path |
| --- | --- |
| Settings | `~/.config/delve/settings.json` |
| Tags and folder colours | `~/.local/share/delve/metadata.json` |
| Thumbnail cache | `~/.cache/delve/thumbnails` |
| Themes | `~/.config/delve/themes/` |
| Log | `~/.local/share/delve/delve.log` |

Settings are schema-versioned and written atomically. Missing or partial files
fall back to defaults rather than failing to start.

## Design notes

- GTK4 only, no libadwaita requirement, no shell extensions.
- No Git or GitHub integration.
- No hard-coded home directory, username, terminal, font, icon theme, or screen
  resolution.
- Destructive actions are explicit: permanent delete and risky bulk operations
  ask first; Trash is the default.
- Long work runs on threads and reports progress in a cancellable operation
  window; the GTK main loop is never blocked by scans or file copies.
- High-DPI friendly: sizes are in GTK units, the layout is responsive, and
  narrow windows move the sidebar and inspector into overlay drawers.

## Known limitations

- LocalSend support covers discovery and sending only; receiving and TLS
  certificate pinning for self-signed peers are not implemented. Some peers
  advertise HTTPS with a self-signed certificate; if the certificate cannot be
  verified the send fails with an error rather than disabling verification.
- The Graph view is a navigation overview, not a force-directed editor; very
  large trees are bounded by the scan limits.
- Organise applies real moves. A "group visually without moving files" mode is
  not implemented; the preview-only mode shows exactly what would move.
- Rename uses a popover anchored to the item rather than an inline editable
  cell.
- Archive creation supports ZIP, TAR, and TAR.GZ; extraction also accepts those
  formats.
- Video and PDF previews are represented by icons rather than decoded frames.

## Tests

There is no automated test suite yet. The application is verified by building
with `cargo build`, launching it on a real GTK display, and exercising the views
and operations manually.
