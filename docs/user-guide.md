# Delve user guide

This guide describes Delve 1.0.1. It covers the interface, file operations,
views, command sets, themes, settings, keyboard shortcuts, and the files Delve
writes on disk.

## Starting Delve

```sh
delve                 # open Home
delve ~/Downloads     # open a folder
delve ~/notes.txt     # open the containing folder
```

Delve accepts a folder or file path as its startup argument. If the path is a
file, the containing folder opens. There is no option parser in 1.0.1, so use a
path as the only argument.

Delve uses a single application instance. Launching it again presents the
existing window; opening a path from the desktop or a file manager navigates the
active tab.

## The window

The main window has four regions:

- **Sidebar** — Places, Tags, Library (Recent and Trash), Network, and System.
  Its width and collapsed sections are remembered.
- **Toolbar and tab strip** — navigation, location entry, search, view switcher,
  sort and filter controls, and independent tabs.
- **File area** — the active view. Selection, sort order, filter, tags, and file
  operations are shared when you switch views.
- **Inspector** — preview, metadata, tag chips, and folder colour controls for
  the current selection.

The sidebar and inspector collapse into overlay drawers in narrow windows. Use
`F9` and `F10`, or the toolbar buttons, to toggle them.

## Views

Switch views with the toolbar or `Ctrl+1` through `Ctrl+5`. Switching views does
not change the selection.

### Icon view — `Ctrl+1`

A flow grid with thumbnails or file-type icons, configurable icon size,
rubber-band selection, and drag-and-drop. Use it for image folders and mixed
content.

### Detail view — `Ctrl+2`

A sortable list with columns for name, type, size, modified time, permissions,
owner, and tags. Columns can be hidden in **Settings → Layout**. Folders-first
sorting and byte-size display are configurable.

### Recursive view — `Ctrl+3`

A flattened tree of the current folder with depth control, expand/collapse, and
the same rows as Detail. It is useful for seeing a project or download tree
without opening each folder.

### Treemap view — `Ctrl+4`

A squarified size map of the folder. Zoom into a subfolder, move back out, and
filter to files, folders, or media. Large trees use a bounded scan so the
interface stays responsive.

### Graph view — `Ctrl+5`

A radial overview of the folder tree with pan, zoom, and search. It is a
navigation overview, not an editor for graph data.

Folder scans run on worker threads. Starting another scan or closing the tab
cancels the old one where possible. Treemap and Graph stop at their scan limits
rather than freeze the window.

## Tabs

Each tab keeps its own history, view, sort and filter, selection, and scroll
position.

- `Ctrl+T` — new tab at the current folder.
- `Ctrl+W` — close the current tab.
- `Ctrl+Shift+T` — reopen the most recently closed tab.
- `Ctrl+Tab` / `Ctrl+Shift+Tab` — next or previous tab.
- Drag a tab to reorder it.
- Middle-click a tab to close it.
- The tab menu duplicates a tab, closes other tabs, or closes tabs to the right.

## Navigation

- `Alt+Left` / `Alt+Right` — back and forward in the tab's history.
- `Alt+Up` — parent folder.
- `Ctrl+L` — edit the location.
- Click the location bar to type a path, `~`, or `$HOME/...`. Relative paths
  are resolved against Home.
- Double-click a folder, or press `Enter` on it, to open it.
- Use the sidebar to jump to Home, Desktop, Documents, Downloads, Pictures,
  Videos, Music, mounted volumes, network mounts, Recent, Trash, or the
  filesystem root.
- Right-click a folder for **Open in new tab** and terminal actions where
  available.

Delve records visited folders so frequently used locations can be suggested.
This history stays in the local settings file.

### Home dashboard

The **Home** sidebar entry opens an overview page, not the contents of your home
folder. It shows the space used on the volume that holds your home directory, a
ring chart of that usage, mounted and unmounted volumes, pinned folders, quick
access to your user folders, and recent places. The ring and cards animate in
briefly when the page opens.

Use **Open folder** on the dashboard (or the `~` breadcrumb) to browse the real
`/home/<user>` directory. Home and the real folder are separate pages, so the
dashboard never gets in the way of normal file management.

### Pinning folders

Right-click a folder in any view, on the location bar, or on a sidebar row and
choose **Pin to sidebar**. Pinned folders appear in their own sidebar section and
on the Home dashboard; choose **Unpin from sidebar** to remove them.

### Removable media and partitions

Mounted volumes appear under **System** in the sidebar with their free space in
the tooltip. Unmounted partitions and pendrives are listed too and mount with one
click. **Eject safely** is available in the right-click menu of a mounted volume
and next to it on the Home dashboard. Delve watches the system volume monitor, so
plugging in a drive updates the sidebar and dashboard immediately.

## Selecting files

- Click to select, `Ctrl+Click` to toggle, and `Shift+Click` to extend.
- Drag on empty space in Icon view for rubber-band selection.
- `Ctrl+A` selects everything in the current view.
- `Escape` clears the selection or dismisses a transient popover.
- The selection is shared across views and survives a view change.

## File operations

### Copy, cut, paste

`Ctrl+C`, `Ctrl+X`, and `Ctrl+V` use Delve's internal clipboard. Paste into the
current folder, another tab, or another window. When a destination item already
exists, Delve asks how to resolve it:

- **Keep both** — rename the new item, preserving both files.
- **Replace** — overwrite the destination.
- **Skip** — leave the existing item alone.
- **Ask every time** — the default conflict policy in Settings.

Nested conflicts use the same choice. A cancelled copy or move reports what was
completed.

### Rename and duplicate

- `F2` opens a rename popover anchored to the current item. The filename stem is
  selected so the extension is easy to keep. Invalid names and conflicts are
  rejected before anything changes.
- **Duplicate** creates a copy in the same folder with a non-conflicting name.

### New folder

`Ctrl+Shift+N` creates a folder. The name is validated before creation, and an
existing name is rejected.

### Move to Trash and delete

- `Delete` moves the selection to the freedesktop Trash.
- Restore items from **Library → Trash** in the sidebar.
- `Shift+Delete` permanently deletes after an explicit confirmation.
- Emptying Trash is a separate confirmed action.

Trash support comes from the freedesktop trash specification. Files on
filesystems without a usable trash directory may not be movable to Trash; Delve
reports the error instead of silently deleting them.

### Drag and drop

Drag files or folders onto a folder row, another tab, or another Delve window.
The drop target indicates copy, move, or link where the platform provides that
information. Holding the platform's modifier keys follows GTK's drag-action
conventions.

### Properties

`Alt+Enter` opens Properties for the selection. It shows type, MIME type,
location, timestamps, permissions, owner and group, link target and status, and
disk usage. For folders, **Calculate recursive folder size** runs off the main
thread and reports file and folder counts.

### Organise

**Organise folder** (`Ctrl+Shift+O`) builds a move plan from one rule:

- file extension
- file-type category
- modified month
- size range
- first letter of the name

The dialog shows every source and destination before applying. Only files are
moved; folders are left in place. Choose **Preview only** to inspect the plan
without changing files. Applying is a real move operation with progress and
cancellation. The preview is the safety net: review it before applying.

## Archives

- **Compress** (`Ctrl+Shift+C`) creates ZIP, TAR, or TAR.GZ from the selection,
  or from the current folder when nothing is selected.
- **Extract** opens an archive preview and extracts to a chosen destination.
- **Extract here** and **Extract to…** are available from the context menu.
- Extraction rejects absolute paths and entries that would escape the
  destination through `..` or symlink traversal. Unsafe entries are skipped and
  reported.
- Existing destinations follow the selected conflict policy.

Archive support is intentionally limited to ZIP, TAR, and TAR.GZ. Other formats
are not promised.

## Tags and folder colours

- Add, remove, and filter tags from the inspector or the context menu.
- Tags are stored against a stable file identity (device and inode) when the
  filesystem provides one, with a path fallback. Replacing a file can therefore
  lose its tag, while moving it within the same filesystem usually preserves it.
- Tag dots can be shown or hidden in **Settings → Tags & colours**.
- Folder colours use a built-in palette and appear in the sidebar, icon view,
  and detail rows.
- Custom file and folder icons can be assigned in **Settings → Icons & theme**.

Tags and colours are local metadata. Delve does not write extended attributes or
sidecar files into your folders.

## Sidebar and Library

- **Places** — Home, Desktop, Documents, Downloads, Pictures, Videos, Music.
- **Tags** — one entry per tag in use.
- **Library** — Recent files/locations and Trash.
- **Network** — detected NFS, CIFS/SMB, SSHFS, DAVFS, rclone, 9p, Ceph, Gluster,
  and AFS mounts. Detection does not imply that the mount is reachable.
- **Pinned** — folders you pinned, in the order you pinned them.
- **System** — filesystem root, mounted local volumes, and unmounted removable
  media that can be mounted from Delve.

Mounts come from GLib's volume monitor and `/proc/self/mountinfo`;
pseudo-filesystems are filtered out.

## Search and Quick Spotlight

- `Ctrl+F` focuses search for the current folder.
- Search filters the visible view by name and can include tags.
- `Super+Space` opens Quick Spotlight. If the desktop reserves that combination,
  `Ctrl+Space` is the fallback.
- Quick Spotlight searches folders, recent locations, files, and command sets.
  Selecting a folder navigates to it; selecting a command set opens its
  confirmation or run flow.

Search does not index the entire filesystem. It searches the loaded folder and
Delve's recorded recents/visits.

## Terminal

Delve does not assume a terminal. It scans `PATH` for supported emulators and
offers **Open Terminal Here** only when one is found. Set an explicit terminal
template in **Settings → Terminal** if auto-detection chooses the wrong one; use
`{dir}` for the working directory. The template is parsed as argv, not through a
shell.

## Folder command sets

Folder command sets attach one or more programs to a folder. They are always
explicit: navigation and selection never run them. See
[Folder commands](folder-commands.md) for the full reference.

In short:

- Create a set under **Settings → Commands** or from a folder's context menu.
- Choose a scope: this folder, this folder and subfolders, or every folder.
- Commands run as argv, never through an implicit shell.
- Sequential, parallel, and mixed-group modes are available.
- `Shift+S, 1` through `Shift+S, 9` start assigned quick slots.
- `Shift+S, 0` opens the multi-set picker.
- Background runs stream output in the run panel; **Open command runs in a
  window** detaches it without stopping the process.
- A separate terminal window is available for interactive commands. Delve cannot
  receive that terminal's exit status.

Quick slots are ignored while a text field has focus, so typing a capital S in
search, path, rename, or command fields works normally.

## Transfers

**Send to device** uses LocalSend when it is installed. Discovery runs only when
you open the transfer panel. Folders are expanded to their files with relative
names. Sending is cancelled from the operation window.

LocalSend support covers discovery and sending only. Receiving is not
implemented. If a peer advertises HTTPS with a certificate Delve cannot verify,
the send fails with an error; Delve does not disable certificate verification.

Open **Settings → Transfers** to enable or disable the feature. Disabling it
removes the entry point without affecting the rest of the application.

## Settings

Open Settings with `Ctrl+,` or the toolbar. Settings are written immediately to
`settings.json` and apply without restarting, except where a reload is inherent.

### Appearance

- Theme: Delve Dark, Delve Light, Delve Ember, or a custom theme.
- Live reload for file-backed themes.
- Density: Compact, Comfortable, or Spacious.
- UI font and monospace font overrides. Empty means use the theme/system font.
- Background opacity from 35% to 100%. Only window and panel backgrounds fade;
  text, icons, and interaction states stay opaque.
- Translucent panels.
- Accent colour override.
- Duplicate a built-in theme, open the theme directory, and reload the theme
  after editing.

### Icons & theme

- Icon theme selection. Delve ships its own **Delve** theme and uses it as the
  fallback, so a minimal system still shows every interface icon.
- File-type icon overrides by MIME type, MIME wildcard, or extension.
- Folder icon overrides by folder colour, plus a default folder icon.
- Icon overrides can name an installed icon or a `file:` path. Delve copies
  uploaded icons into its data directory so they survive cache cleanup.

### Layout

- Default view and icon size.
- Sidebar and inspector visibility and widths.
- Detail column visibility.
- Folders first, hidden files, default sort field and direction, and byte sizes.
- Tag dots.

### Files

- Show hidden files.
- Folders first.
- Default sort field and direction for new tabs.
- Human-readable or exact byte sizes.
- Conflict policy: ask, replace, skip, or keep both.

### Thumbnails

- Enable or disable thumbnails.
- Thumbnail size (48–512 px).
- Text preview toggle.
- Thumbnail cache size and a **Clear thumbnail cache** button.

### Tags & colours

- Show or hide tag dots.
- Manage the folder colour palette and remove stale metadata.

### Terminal

- Auto-detected terminal status.
- Explicit terminal command template with `{dir}`.
- Test button that launches the configured terminal in the current folder.

### Commands

Create, edit, enable, disable, run, and delete folder command sets, assign quick
slots, and edit the sequence, environment, scope, and launch mode of each set.
See [Folder commands](folder-commands.md).

### Keyboard

Every action in the default shortcut list can be rebound. Enter an accelerator
such as `<Control><Shift>k` or `Ctrl+Shift+K`; both forms are normalised. Delve
detects conflicts with other actions before accepting a binding. **Reset
shortcuts** restores the defaults.

### Transfers

Enable or disable LocalSend integration and open the transfer panel.

### Advanced

- Copy diagnostics to the clipboard.
- Reveal the log file.
- Export configuration and metadata as a JSON bundle. Paths are machine-specific.
- Import a Delve export bundle.
- Reset all settings. Tabs, tags, and folder colours are not deleted.

## Default keyboard shortcuts

All of these can be changed in **Settings → Keyboard**.

| Action | Shortcut |
| --- | --- |
| Undo | `Ctrl+Z` |
| Redo | `Ctrl+Shift+Z` |
| Copy | `Ctrl+C` |
| Cut | `Ctrl+X` |
| Paste | `Ctrl+V` |
| Move to Trash | `Delete` |
| Delete permanently | `Shift+Delete` |
| Rename | `F2` |
| Select all | `Ctrl+A` |
| New tab | `Ctrl+T` |
| Close tab | `Ctrl+W` |
| Reopen closed tab | `Ctrl+Shift+T` |
| Edit location | `Ctrl+L` |
| Search | `Ctrl+F` |
| Back | `Alt+Left` |
| Forward | `Alt+Right` |
| Parent folder | `Alt+Up` |
| Toggle hidden files | `Ctrl+H` |
| New folder | `Ctrl+Shift+N` |
| Refresh | `F5` |
| Icon view | `Ctrl+1` |
| Detail view | `Ctrl+2` |
| Recursive view | `Ctrl+3` |
| Treemap view | `Ctrl+4` |
| Graph view | `Ctrl+5` |
| Next tab | `Ctrl+Tab` |
| Previous tab | `Ctrl+Shift+Tab` |
| Settings | `Ctrl+,` |
| Properties | `Alt+Enter` |
| Open in terminal | `Ctrl+Shift+E` |
| Toggle run panel | `Ctrl+Shift+R` |
| Quick Spotlight | `Super+Space` (fallback `Ctrl+Space`) |
| Organise folder | `Ctrl+Shift+O` |
| Compress selection | `Ctrl+Shift+C` |
| Toggle sidebar | `F9` |
| Toggle inspector | `F10` |

Command-set quick slots use `Shift+S` followed by `1`–`9`; `Shift+S, 0` opens
the picker. These are separate from the editable shortcut table.

## Storage and files

Delve follows the XDG base directory specification. When an XDG variable is
unset, the standard fallback is used.

| Purpose | Default path |
| --- | --- |
| Settings | `~/.config/delve/settings.json` |
| Settings backup after a parse failure | `~/.config/delve/settings.json.invalid` |
| Tags and folder colours | `~/.local/share/delve/metadata.json` |
| Custom icon assets | `~/.local/share/delve/icons/` |
| Themes | `~/.config/delve/themes/` |
| Thumbnail cache | `~/.cache/delve/thumbnails/` |
| Log | `~/.local/share/delve/delve.log` |
| Configuration export | Desktop `delve-config.json`, or the config directory if no Desktop exists |

Settings are schema-versioned and written atomically through a temporary file.
An invalid settings file is copied aside and defaults are used instead of
preventing startup. Tag metadata is also saved atomically.

Delve does not write into its installation directory. Removing the application
does not remove your settings, tags, themes, or caches.

## Accessibility and display

- The interface uses GTK4 widgets, so the system screen reader, keyboard focus,
  and high-contrast settings apply where GTK exposes them.
- Every primary action has a keyboard binding or is reachable from the menu and
  context menu.
- Sizes are expressed in GTK units and scale with the display scale factor.
- Narrow windows move the sidebar and inspector into drawers rather than
  clipping them.
- Reduced-motion desktop settings are respected by GTK's transitions; Delve does
  not add its own animation loop.

## Known limitations in 1.0.1

- LocalSend receiving and TLS certificate pinning are not implemented.
- Video and PDF previews show file-type icons rather than decoded frames.
- Archive support is limited to ZIP, TAR, and TAR.GZ.
- Graph and Treemap use bounded scans; extremely large trees may not show every
  node.
- Organise performs real moves, not virtual grouping.
- There is no automated test suite yet. Release verification is documented in
  [Build and release](build-and-release.md).
- The project does not have a built-in update checker or telemetry.
