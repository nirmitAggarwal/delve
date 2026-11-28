# Maintainer: Delve contributors <noreply@codebuff.com>
pkgname=delve
pkgver=1.0.1
pkgrel=1
pkgdesc="A fast, adaptable Linux file manager built for everyday file work"
arch=('x86_64' 'aarch64')
url="https://github.com/io.github/delve/delve"
license=('MIT')
depends=('gtk4' 'glib2' 'cairo' 'pango' 'libxkbcommon' 'libcolord-gtk' 'json-glib' 'libzip' 'libarchive')
makedepends=('cargo' 'rustup' 'pkg-config' 'glib2-devel')
source=("delve-1.0.1.tar.gz::https://github.com/io.github/delve/delve/archive/refs/tags/1.0.1.tar.gz"
        "delve.desktop"
        "delve.appdata.xml")
sha256sums=('SKIP'
            'SKIP'
            'SKIP')

prepare() {
  cd "$pkgname"
  # The source tarball already contains the built binary; we just need to
  # repackage it. If building from git, run:
  #   cargo build --release --locked
  true
}

build() {
  cd "$pkgname"
  # Source tarballs from releases contain the pre-built binary.
  # For AUR, we build from source:
  cargo build --release --locked
}

package() {
  cd "$pkgname"
  # Install to a temporary directory
  ./scripts/install.sh --prefix /usr --destdir "$pkgdir"

  # Install license
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/delve/LICENSE"

  # Desktop file
  install -Dm644 "$srcdir/delve.desktop" "$pkgdir/usr/share/applications/delve.desktop"

  # AppStream metainfo
  install -Dm644 "$srcdir/delve.appdata.xml" "$pkgdir/usr/share/metainfo/delve.appdata.xml"

  # The binary is already stripped; no further steps needed.
}