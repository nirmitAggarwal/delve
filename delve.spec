Name: delve
Version: 1.0.1
Release: 1%{?dist}
Summary: A fast, adaptable Linux file manager
License: MIT
URL: https://github.com/io.github/delve/delve

Source0: %{name}-%{version}.tar.gz
Source1: %{name}.desktop
Source2: %{name}.appdata.xml

BuildArch: x86_64
BuildRoot: %{_tmppath}/%{name}-%{version}-root

Requires: gtk4, glib2, cairo, pango, libxkbcommon, colord-gtk, json-glib, libzip, libarchive
Requires: desktop-file-utils, appstream-glib

%description
Delve is a distribution-neutral GTK4 file manager built for everyday file work.
It combines five folder views, independent tabs, real file operations, archive
support, tags, previews, and reusable command sets in one native Linux application.

%prep
%setup -q -n %{name}-%{version}

%build
cargo build --release --locked

%install
rm -rf $RPM_BUILD_ROOT
./scripts/install.sh --prefix /usr --destdir $RPM_BUILD_ROOT

# Install license
install -Dm644 LICENSE $RPM_BUILD_ROOT/usr/share/licenses/delve/LICENSE

# Install desktop file
install -Dm644 %{SOURCE1} $RPM_BUILD_ROOT/usr/share/applications/%{name}.desktop

# Install AppStream metainfo
install -Dm644 %{SOURCE2} $RPM_BUILD_ROOT/usr/share/metainfo/%{name}.appdata.xml

# The binary is already stripped in release mode

%files
%doc README.md CHANGELOG.md
/usr/bin/delve
/usr/share/applications/delve.desktop
/usr/share/metainfo/delve.appdata.xml
/usr/share/licenses/delve/LICENSE
/usr/share/icons/hicolor/*/apps/io.github.delve.FileManager*
/usr/share/icons/Delve/

%changelog
* Thu Nov 28 2026 - Version 1.0.1
- Initial package release with responsive Icon view selection and distro packages.
- Right-clicking anywhere in the Icon view canvas opens the folder menu.
- Clicks in the gaps between icons no longer select a neighbour.
- Native Debian, Fedora, and Arch packages available; each pulls in GTK 4.10+
  and ships the complete bundled icon theme.