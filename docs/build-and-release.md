# Build and release Delve

This guide is for developers producing a checked, installable Delve release. It
covers the toolchain, local builds, versioning, metadata validation, the release
tarball, and the checks that must pass before publishing.

## Toolchain

Delve is a Rust 2021 project using GTK4 through the `gtk4` crate. It requires:

| Tool | Purpose |
| --- | --- |
| Rust 1.80 or newer | Compiler and Cargo. |
| GTK 4.10 development headers | `gtk4` crate build dependency. |
| `pkg-config` | Finds GTK and GLib. |
| `glib-compile-resources` | `build.rs` compiles `assets/icons` into the binary. |
| `desktop-file-validate` | Validates the desktop entry. |
| `appstreamcli` | Validates AppStream metadata. |
| `rsvg-convert` | Optional; regenerates PNG application icons. |
| `sha256sum` | Release checksums. |

Debian / Ubuntu:

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libglib2.0-dev-bin \
  desktop-file-utils appstream librsvg2-bin
```

Fedora:

```sh
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config gtk4-devel glib2-devel \
  desktop-file-utils appstream librsvg2-tools
```

Arch Linux:

```sh
sudo pacman -S base-devel pkgconf gtk4 glib2 desktop-file-utils appstream librsvg
```

The build reads all icon assets from the checkout. It does not download
anything. The release binary embeds the icon theme as a GResource, so the build
works fully offline once the Rust toolchain and system packages are present.

## Get the source

Use the project's release source archive or a clone of the repository. The build
needs the whole checkout, including `assets/icons`; a source archive must not
strip assets.

```sh
cd delve
cargo metadata --no-deps --format-version 1 >/dev/null
```

The last command is a quick sanity check that Cargo can read the manifest.

## Development build

```sh
cargo build
./target/debug/delve ~/Downloads
```

Debug builds are unstripped and faster to compile. The current dead-code
warnings are known and do not affect behaviour; do not suppress them with crate
attributes just to obtain a quiet build. Treat any error, and any new warning
category, as a release blocker.

Useful checks while developing:

```sh
cargo fmt --check
cargo check
cargo build --message-format short
```

There is no automated test suite in 1.0.1. Do not claim a test suite exists.
Manual runtime verification is part of the release checklist below.

## Release build

Always build with the checked-in lockfile:

```sh
cargo build --release --locked
```

The release profile uses `opt-level = 2`, thin LTO, and symbol stripping. The
result is `target/release/delve`.

The build embeds:

- every file under `assets/icons/Delve/`
- every file under `assets/icons/hicolor/`
- `assets/delve.svg`
- the bundled icon index and licensing notice

The generated manifest and GResource live in Cargo's `OUT_DIR`; they are build
artifacts, not source files. Changing an icon or `build.rs` triggers a rebuild.

To regenerate application icons after changing the logo:

```sh
./scripts/render-app-icons.sh
cargo build --release --locked
```

The icon script uses `assets/delve.svg`, writes PNGs at 16, 24, 32, 48, 64,
128, 256, and 512 pixels, and updates the scalable application icon. Review
`assets/icons/README.md` before changing the bundled theme or its licensing.

## Versioning

Delve uses semantic versioning. A release changes:

1. `version` in `Cargo.toml`.
2. The `delve` package version in `Cargo.lock` (Cargo updates it when the
   manifest changes).
3. The new release entry in `packaging/io.github.delve.FileManager.metainfo.xml`
   (`version` and `date`).
4. A new heading in `CHANGELOG.md`.

Keep the AppStream release date in ISO `YYYY-MM-DD` form and make it the actual
release date, not the date the code was written.

If the settings schema changes, increment `SETTINGS_VERSION` in
`src/settings.rs` and document migration behaviour. If the application ID
changes, update all of these together:

- `Cargo.toml` binary name, if applicable
- `src/main.rs` `application_id`
- `src/app.rs` window icon name
- desktop file name and `Icon=`
- AppStream `<id>` and launchable
- install, uninstall, and package scripts
- icon file names

Do not change the application ID after publishing without providing a migration
path; desktop environments key application identity, notifications, and icon
lookups to it.

## Validate metadata

```sh
desktop-file-validate packaging/io.github.delve.FileManager.desktop
appstreamcli validate --no-net packaging/io.github.delve.FileManager.metainfo.xml
```

`desktop-file-validate` must exit 0. The current entry produces only an optional
category hint; the required categories are present.

`appstreamcli` currently reports one warning for a missing public project
homepage URL. If the project has a canonical public URL, add it to the
metainfo:

```xml
<url type="homepage">https://github.com/OWNER/REPO</url>
```

Then validate again with no warnings. Do not invent or ship a dead homepage
link. The packaged desktop application still installs and launches with the
warning present.

## Produce the release tarball

```sh
./scripts/package-release.sh
```

The script validates the metadata, builds with `--locked`, stages the release,
and writes:

```text
dist/delve-1.0.1-linux-x86_64.tar.gz
dist/SHA256SUMS
```

The tarball contains:

```text
delve-1.0.1-linux-x86_64/
├── delve
├── install.sh
├── uninstall.sh
├── README.md
├── CHANGELOG.md
├── LICENSE
├── docs/
├── assets/
│   ├── delve.svg
│   └── icons/
│       ├── README.md
│       └── COPYING.ADWAITA
├── share/applications/io.github.delve.FileManager.desktop
├── share/metainfo/io.github.delve.FileManager.metainfo.xml
├── share/icons/hicolor/...
├── share/icons/Delve/...
└── share/licenses/delve/COPYING.ADWAITA
```

The binary is already stripped and does not depend on the source tree at
runtime. The installed copy of the icon theme is for desktop integration; the
running application uses the embedded copy.

Build for the target architecture on that architecture, or set `ARCH` only when
the artifact genuinely matches it. Cross-compilation is not configured. The
tarball name must not claim an architecture the binary was not built for.

## Release checklist

Run these in order. A release is not ready until every item passes or a written
exception is recorded.

### 1. Clean source state

```sh
git status --short
```

Review the diff. The release must include the intended changes and no generated
`target/` or `dist/` files. Update `.gitignore` if a new artifact directory was
introduced.

### 2. Compile

```sh
cargo build --release --locked
```

Require exit status 0. Do not release with compiler errors. Inspect warnings;
the known dead-code warnings are acceptable, new behavioural warnings are not.

### 3. Validate metadata

```sh
desktop-file-validate packaging/io.github.delve.FileManager.desktop
appstreamcli validate --no-net packaging/io.github.delve.FileManager.metainfo.xml
```

Fix errors. Record any accepted warning, including the missing homepage, in the
release notes or tracking issue.

### 4. Verify the bundle contents

```sh
find assets/icons/Delve -type f | wc -l          # expected: 756
find assets/icons/hicolor -type f | wc -l        # expected: 9
test -f assets/icons/Delve/index.theme
test -f assets/icons/COPYING.ADWAITA
```

Expected counts for 1.0.1 are 756 bundled-theme files and 9 application icon
files. If you intentionally change the theme, update these counts and the notes
in `assets/icons/README.md`.

### 5. Test a staged install

```sh
rm -rf /tmp/delve-stage
./scripts/install.sh --prefix /usr --destdir /tmp/delve-stage
test -x /tmp/delve-stage/usr/bin/delve
test -f /tmp/delve-stage/usr/share/applications/io.github.delve.FileManager.desktop
test -f /tmp/delve-stage/usr/share/icons/hicolor/512x512/apps/io.github.delve.FileManager.png
test -f /tmp/delve-stage/usr/share/icons/Delve/index.theme
test -f /tmp/delve-stage/usr/share/doc/delve/README.md
test -f /tmp/delve-stage/usr/share/licenses/delve/COPYING.ADWAITA
```

Then remove the staged tree with `./scripts/uninstall.sh --prefix /usr --destdir
/tmp/delve-stage` and confirm the executable and metadata are gone while no user
data was touched.

### 6. Runtime smoke test

Launch the release binary with isolated XDG directories so the test does not
touch your real configuration:

```sh
rm -rf /tmp/delve-release-test
mkdir -p /tmp/delve-release-test/{cfg,data,cache,home}
HOME=/tmp/delve-release-test/home \
XDG_CONFIG_HOME=/tmp/delve-release-test/cfg \
XDG_DATA_HOME=/tmp/delve-release-test/data \
XDG_CACHE_HOME=/tmp/delve-release-test/cache \
./target/release/delve /tmp
```

Check, at minimum:

- The window opens and the Delve icon is visible in the title bar or task
  switcher where the desktop shows one.
- Icons render in the toolbar and sidebar without an installed Delve theme.
- Icon, Detail, Recursive, Treemap, and Graph views switch without losing the
  selection.
- A new tab opens and closes; history back and forward work.
- Search, Settings, and Quick Spotlight open.
- A harmless copy and rename can be undone.
- The run panel opens; a command set with a harmless command (`printf`) runs if
  the test configuration includes one.
- Closing the window exits cleanly and writes settings under the isolated
  config directory.

Do not test destructive operations on real data. Use a scratch directory.

### 7. Package and checksum

```sh
./scripts/package-release.sh
cd dist
sha256sum -c SHA256SUMS
```

Extract the tarball into a fresh directory and repeat the staged install and a
short launch test from the extracted copy. This catches a release that works
only from the source tree.

### 8. Archive the evidence

Record in the release notes:

- version, date, architecture, and distribution used
- exact build and validation commands
- any accepted warnings or skipped checks
- checksum of the tarball
- known limitations from the README and changelog

## Publishing

Publishing is an external, irreversible action. Do not push tags, upload
artifacts, or announce a release until the owner has reviewed the checklist.

A typical release consists of:

1. Commit the version, changelog, metadata, and any documentation changes.
2. Tag the commit `v1.0.1` (annotated tag).
3. Build and validate from that exact commit.
4. Upload the tarball and `SHA256SUMS` to the project's release page.
5. Publish the changelog text with the release.
6. Keep the previous release available for rollback.

Do not rebuild an artifact after publishing under the same version. If a fix is
needed, publish a new patch version.

## Rollback

If a release is broken:

1. Mark the release as withdrawn or clearly note the issue on the release page.
2. Point users to the last known-good version.
3. Do not silently replace the tarball under the same version and checksum.
4. Publish a patch release with the fix and a changelog entry describing the
   regression.

Local rollback is simply reinstalling the previous tarball; user configuration
is preserved. If a settings schema change is at fault, keep the
`settings.json.invalid` backup created by the loader and report it with the
support bundle.

## Reproducibility and offline builds

- `Cargo.lock` is committed; use `--locked`.
- All icons, metadata, and desktop files are checked in.
- `build.rs` invokes only `glib-compile-resources`.
- Runtime never fetches network assets.
- LocalSend is optional and never blocks startup.
- The release binary embeds its icon theme, so package correctness does not
  depend on the host's icon packages.

Build timestamps and toolchain versions can still differ between machines.
Record the Rust and GTK versions used for an official build.
