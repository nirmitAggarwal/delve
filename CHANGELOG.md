# Changelog

All notable changes to Delve are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Delve uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.1] - 2026-11-28

### Added

- Native packages for Debian/Ubuntu (`.deb`), Fedora/RHEL (`.rpm`), and Arch
  Linux (`.pkg.tar.zst`). Each package pulls in GTK 4.10+ and its runtime
  dependencies and ships the complete Delve icon theme, so the application is
  fully themed and usable immediately after installation.
- A dedicated Home dashboard: an animated storage ring for the home volume,
  mounted and unmounted volumes, pinned folders, quick access, and recent
  places. `Location::Home` is now this overview; `/home/<user>` remains a normal
  folder listing.
- Pinned folders, available from the file, sidebar, and location-bar context
  menus and shared by the sidebar and dashboard.
- Removable media and partition management: unmounted volumes mount with one
  click, mounted volumes can be ejected safely, and the sidebar and dashboard
  update immediately when media is plugged in or removed.
- Letter-based quick select in Icon view (type a name prefix to jump to it).
- Right-click menus on sidebar rows and the location bar.
- The Home dashboard is a responsive bento grid: cards reflow between three,
  two, and one column as the window changes width, and every workspace view
  (including Treemap, Graph, and the dashboard) now has a right-click menu on
  empty space.
- Open with now has an advanced bar for entering a command or choosing an
  application location. Custom commands support `%f`/`%u`, can be remembered
  per file type, and appear as suggestions next to installed applications.

### Fixed

- The dashboard no longer forces the window wider than the screen: hint text
  wraps, cards have small minimum widths, and the bento tracks the real
  viewport so a narrow window settles at one column without clipping.
- Narrow windows are responsive: the sidebar becomes the icon rail
  automatically, secondary command-bar actions move into the More menu (which
  now also offers Cut, Copy, Paste, Trash, Undo and Redo), and the view
  switcher drops its label when space is tight.
- Removable media is also polled, so drives appear and disappear even when the
  session's volume monitor does not emit hot-plug signals.
- Ctrl+click selects individual items again in Icon view. A per-cell shortcut
  toggled the item after FlowBox had already done so, cancelling the selection.
- Icon view hit-testing follows the visible tile: right-clicking anywhere in the
  canvas (including below the last row, where the grid has no surface) opens the
  folder menu, and clicks in the gaps between icons no longer select a
  neighbour.
- Application icons are re-rendered from `assets/delve.svg` during install and
  packaging, and icon caches are forcibly refreshed, so an updated logo no
  longer leaves stale icons on the desktop.
- Tag management is reliable: the sidebar keeps its Tags section after
  navigation, tag rows no longer leave stale hover popovers behind, and rename,
  recolour and delete in the tag manager are reachable again (their dialogs now
  open above the manager instead of behind it). Metadata is also saved when a
  tagged path is not valid UTF-8, so tag changes no longer silently fail to
  persist and deleted tags do not return on the next launch.

### Packaging

- The native packages install the complete bundled Delve icon theme under
  `/usr/share/icons/Delve`, the hicolor application icons, the desktop entry,
  and the AppStream metadata. No separate icon package is required; users can
  still switch the icon theme in Settings.
- `scripts/package-native.sh` builds the `.deb`, and the `.rpm` and
  `.pkg.tar.zst` where the Arch and Fedora build tools are available, from the
  release tarball.

## [1.0.0] - 2026-11-22

### Added

- Five folder views — Icon, Detail, Recursive, Treemap, and Graph — sharing
  selection, sort order, filter, tags, and file operations.
- Independent tabs with per-tab history, view state, scroll position, drag
  reordering, and reopen-closed-tab.
- File operations: copy, cut, paste with conflict handling, rename, duplicate,
  create folder, move to Trash, restore, permanent delete, and drag and drop.
- Organise rules with a preview-before-apply move plan.
- ZIP, TAR, and TAR.GZ creation and path-traversal-safe extraction.
- Tags keyed to stable file identity, folder colours, sidebar sections, Recent
  and Trash library locations.
- Image thumbnails, text previews, metadata inspector, and properties dialog
  with off-thread recursive size calculation.
- Editable keyboard shortcuts with conflict detection and reset-to-defaults.
- Editable JSON themes with live reload and safe CSS validation; built-in
  Delve Dark, Delve Light, and Delve Ember themes.
- Folder command sets with sequential, parallel, and mixed execution, scoped
  availability, environment variables, argument placeholders, quick slots,
  batch picker, streamed background output, and a detached run window.
- Optional LocalSend discovery and file sending.
- A complete bundled icon theme embedded in the binary, so the interface works
  offline and on systems without an icon package.
- Desktop entry, AppStream metadata, application icons, and Linux install and
  packaging scripts.

### Documentation

- User guide, Linux install guide, build and release guide, support guide,
  folder command reference, theme format reference, and icon provenance notes.

### Known limitations

- LocalSend supports discovery and sending only; receiving is not implemented.
- Video and PDF previews are shown as icons rather than decoded frames.
- There is no automated test suite yet; releases are verified with build,
  metadata, install, and manual runtime checks.

## [0.1.0] - 2026-11-22

- Initial development snapshot.
