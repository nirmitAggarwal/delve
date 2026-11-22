# Changelog

All notable changes to Delve are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Delve uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
