# Install Delve on Linux

Delve is distributed as a self-contained binary tarball. The tarball contains
the executable, desktop entry, AppStream metadata, all application icon sizes,
the complete bundled icon theme, installed documentation, and the install and
uninstall scripts. No package manager, network connection, icon package, or
desktop-specific integration is required after download.

The application is built for Linux and GTK4. It runs on Wayland and X11 through
GTK's normal backends.

## Requirements

### Runtime

- A 64-bit Linux system matching the release architecture (the filename says
  `linux-x86_64`, `linux-aarch64`, and so on).
- GTK 4.10 or newer.
- A working XDG desktop session. Delve does not require GNOME, KDE, a specific
  compositor, DMS, or a shell extension.
- `glibc` compatible with the build host. A binary built on a newer distribution
  may not start on an older one; build from source in that case.

Check the installed GTK version with your package manager or:

```sh
pkg-config --modversion gtk4
```

This command is only available when the development package is installed. At
runtime, Delve reports the GTK version in **Settings → Advanced → Copy
diagnostics**.

### Build dependencies

Only needed when building from source:

| Distribution | Packages |
| --- | --- |
| Debian / Ubuntu | `build-essential pkg-config libgtk-4-dev libglib2.0-dev-bin` |
| Fedora | `gcc gcc-c++ make pkgconf-pkg-config gtk4-devel glib2-devel` |
| Arch Linux | `base-devel pkgconf gtk4 glib2` |
| openSUSE | `gcc gcc-c++ make pkg-config gtk4-devel glib2-devel` |

`glib-compile-resources` from `libglib2.0-dev-bin` / `glib2-devel` is required by
`build.rs`; it compiles the bundled icon theme into the executable. Rust 1.80 or
newer is required. Install Rust with [rustup](https://rustup.rs/) or your
distribution's `rust` package.

Optional tools:

- `librsvg2-bin` (`rsvg-convert`) — only to regenerate application PNG icons.
- `desktop-file-utils` — `desktop-file-validate`.
- `appstream` — `appstreamcli validate`.
- `gtk4-update-icon-cache` or `gtk-update-icon-cache` — refresh icon caches after
  a manual install. The install script uses either when present.
- A supported terminal emulator — only for **Open Terminal Here** and command
  sets configured to launch in a terminal.
- LocalSend — only for the optional transfer panel.

## Install from a release tarball

Download the tarball and `SHA256SUMS` from the release page, then verify and
extract:

```sh
sha256sum -c SHA256SUMS
tar -xzf delve-1.0.1-linux-x86_64.tar.gz
cd delve-1.0.1-linux-x86_64
```

The checksum file must list the exact tarball name. If it does not, download both
files again from the same release.

### System-wide install

```sh
sudo ./install.sh
```

This installs under `/usr/local`:

| Item | Path |
| --- | --- |
| Executable | `/usr/local/bin/delve` |
| Desktop entry | `/usr/local/share/applications/io.github.delve.FileManager.desktop` |
| AppStream metadata | `/usr/local/share/metainfo/io.github.delve.FileManager.metainfo.xml` |
| Application icons | `/usr/local/share/icons/hicolor/<size>/apps/io.github.delve.FileManager.*` |
| Bundled icon theme | `/usr/local/share/icons/Delve/` |
| Documentation | `/usr/local/share/doc/delve/` |
| Third-party icon notice | `/usr/local/share/licenses/delve/COPYING.ADWAITA` |

The script updates the desktop database and icon caches when the tools are
available. Launch Delve from the application menu or run `delve`.

To install somewhere other than `/usr/local`:

```sh
sudo ./install.sh --prefix /opt/delve
```

Then run `/opt/delve/bin/delve`. A non-standard prefix may not be on `PATH` or in
the desktop search path; add it to `XDG_DATA_DIRS` if desktop integration is
needed.

### Per-user install

```sh
./install.sh --user
```

This installs under `~/.local`. No root permission is required, and it takes
precedence over a system-wide install for the current user when `~/.local/bin`
is on `PATH`.

If your shell cannot find `delve` after a user install, add the directory to
`PATH`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

For desktop menus, `~/.local/share` is already an XDG data directory on normal
Linux sessions. Log out and back in if the new entry does not appear.

### Staged install for packaging

`--destdir` installs into a staging tree without touching system caches:

```sh
./install.sh --prefix /usr --destdir "$PWD/pkg"
```

The resulting tree can be copied into a package build root. `DESTDIR` and
`PREFIX` environment variables work as well. Packagers should use their normal
distribution packaging process for dependencies, file ownership, and post-install
hooks.

## Install from source

Use the source archive or clone the project repository, then build from the
checkout:

```sh
cd delve
cargo build --release --locked
sudo ./scripts/install.sh
```

If you built without `--locked`, the install still uses
`target/release/delve`, but release builds should always use `--locked` so the
checked-in `Cargo.lock` is honoured.

Choose a different prefix or a per-user install:

```sh
sudo ./scripts/install.sh --prefix /usr/local
./scripts/install.sh --user
```

Set `DELVE_BINARY` to install a binary from another path:

```sh
DELVE_BINARY="$PWD/target/release/delve" ./scripts/install.sh --user
```

To run without installing:

```sh
./target/release/delve ~/Downloads
```

The debug build is also usable for development:

```sh
cargo run -- ~/Downloads
```

## Verify the installation

1. Check that the installed command resolves, then open Delve:

   ```sh
   command -v delve
   delve
   ```

2. Open **Settings → Advanced**. The page shows the config, data, cache, and log
   paths, plus a **Copy diagnostics** button.
3. Confirm the window icon and the application menu icon appear. The executable
   carries the app icon as a resource, while the desktop entry uses the
   `hicolor` icons installed under the prefix.
4. Open a folder with many images and switch to Icon view. Thumbnails should
   appear without installing an icon theme or downloading anything.
5. If the menu entry is missing, refresh the desktop database and icon cache
   manually:

   ```sh
   sudo update-desktop-database /usr/local/share/applications
   sudo gtk4-update-icon-cache -f -t /usr/local/share/icons/hicolor
   sudo gtk4-update-icon-cache -f -t /usr/local/share/icons/Delve
   ```

   Replace `gtk4-update-icon-cache` with `gtk-update-icon-cache` if that is the
   name on your system.

## Native packages

Release 1.0.1 ships native packages in addition to the tarball. They declare
their runtime dependencies, so the package manager installs GTK 4.10+ for you,
and they include the complete bundled Delve icon theme, so no separate icon
package is required.

| Distribution | Package file |
| --- | --- |
| Debian / Ubuntu | `delve_1.0.1_amd64.deb` |
| Fedora / RHEL | `delve-1.0.1-1.x86_64.rpm` |
| Arch Linux | `delve-1.0.1-1-x86_64.pkg.tar.zst` |

```sh
sudo dpkg -i delve_1.0.1_amd64.deb
sudo dnf install ./delve-1.0.1-1.x86_64.rpm
sudo pacman -U ./delve-1.0.1-1-x86_64.pkg.tar.zst
```

The package's post-install script refreshes the desktop database and icon
caches. Users can still switch the icon theme in **Settings → Icons & theme**.

To build the packages yourself from a release tarball, see
`docs/build-and-release.md` and `packaging/README.md`.

## Uninstall

```sh
# system-wide install
sudo ./uninstall.sh

# per-user install
./uninstall.sh --user

# custom prefix or staged tree
sudo ./uninstall.sh --prefix /opt/delve
./uninstall.sh --prefix /usr --destdir "$PWD/pkg"
```

Uninstalling removes the executable, desktop entry, AppStream metadata,
application icons, bundled icon theme, and installed documentation.

It deliberately keeps user data:

```text
${XDG_CONFIG_HOME:-~/.config}/delve
${XDG_DATA_HOME:-~/.local/share}/delve
${XDG_CACHE_HOME:-~/.cache}/delve
```

Remove those directories yourself only if you also want to discard settings,
tags, custom themes, custom icons, thumbnails, and the log.

## Troubleshooting installation

**`delve: command not found`**

The binary directory is not on `PATH`. For a user install, add
`$HOME/.local/bin` to `PATH`. For a custom prefix, add `<prefix>/bin`.

**The application menu entry does not appear**

Check that the desktop file is in an XDG application directory and that
`TryExec=delve` resolves. Refresh the desktop database as shown above. On
desktop environments that cache menus, log out and back in.

**The icon is a generic question mark**

Install the `hicolor` and `Delve` icon directories under the same prefix as the
desktop file, then refresh the icon cache. A per-user install needs
`~/.local/share/icons`; a system install needs `/usr/local/share/icons` or
`/usr/share/icons`.

**GTK cannot be found at startup**

The runtime GTK4 library is missing or is older than 4.10. Install the GTK4
runtime package for your distribution. Delve's binary is dynamically linked to
the system GTK4 and does not bundle GTK itself.

**The binary fails with a GLIBC symbol error**

The release was built against a newer glibc than the target system provides.
Build from source on the older distribution, or use a release built for that
distribution.

**The application starts but has no icons**

Delve embeds its fallback theme, so missing host icons should not blank the
interface. If a custom icon theme is selected, switch to **Delve** or the
automatic fallback in **Settings → Icons & theme**. Confirm the binary is the
release build and not a stripped-down copy missing its GResource.

**A terminal action is disabled**

No supported terminal emulator was detected. Set one in **Settings → Terminal**
or install a common terminal such as GNOME Terminal, Konsole, Alacritty,
Kitty, Foot, WezTerm, or Xfce Terminal.

**LocalSend is unavailable**

The transfer panel requires LocalSend to be installed and discoverable. All
other features work without it. See the [user guide](user-guide.md#transfers).
