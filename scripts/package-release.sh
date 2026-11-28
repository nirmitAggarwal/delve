#!/usr/bin/env bash
# Build a self-contained Linux release tarball and its checksum.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP_ID="io.github.delve.FileManager"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
ARCH="${ARCH:-$(uname -m)}"
NAME="delve-$VERSION-linux-$ARCH"
DIST="$ROOT/dist"
STAGE="$DIST/$NAME"
TARBALL="$DIST/$NAME.tar.gz"

command -v cargo >/dev/null 2>&1 || {
  echo "cargo is required to build a release." >&2
  exit 1
}
command -v sha256sum >/dev/null 2>&1 || {
  echo "sha256sum is required to write checksums." >&2
  exit 1
}
command -v desktop-file-validate >/dev/null 2>&1 || {
  echo "desktop-file-validate is required (desktop-file-utils)." >&2
  exit 1
}
command -v appstreamcli >/dev/null 2>&1 || {
  echo "appstreamcli is required (appstream package)." >&2
  exit 1
}

echo "==> Validating packaging metadata"
desktop-file-validate "packaging/$APP_ID.desktop"

# appstreamcli exits non-zero for warnings as well as errors. The only accepted
# warning for 1.0.0 is the absence of a public project homepage; a release must
# not invent a URL. Any other warning or error blocks packaging.
set +e
appstream_output="$(appstreamcli validate --no-net "packaging/$APP_ID.metainfo.xml" 2>&1)"
appstream_status=$?
set -e
printf '%s\n' "$appstream_output"
if [[ $appstream_status -ne 0 ]]; then
  unexpected="$(printf '%s\n' "$appstream_output" | grep -E '^[EWP]:' | grep -v 'url-homepage-missing' || true)"
  if printf '%s\n' "$appstream_output" | grep -qE '^E:' || [[ -n "$unexpected" ]]; then
    echo "AppStream validation contains errors or unexpected warnings." >&2
    exit 1
  fi
  echo "Accepted AppStream warning: no public homepage URL is configured."
fi

echo "==> Building release binary"
cargo build --release --locked

for required in \
  "target/release/delve" \
  "assets/delve.svg" \
  "assets/icons/Delve/index.theme" \
  "assets/icons/hicolor/scalable/apps/$APP_ID.svg" \
  "assets/icons/COPYING.ADWAITA"; do
  [[ -e "$required" ]] || {
    echo "Missing release file: $required" >&2
    exit 1
  }
done

rm -rf "$STAGE"
mkdir -p \
  "$STAGE/share/applications" \
  "$STAGE/share/metainfo" \
  "$STAGE/share/icons" \
  "$STAGE/share/licenses/delve" \
  "$STAGE/docs" \
  "$STAGE/assets" \
  "$STAGE/assets/icons"

install -m 0755 target/release/delve "$STAGE/delve"
install -m 0755 scripts/install.sh "$STAGE/install.sh"
install -m 0755 scripts/uninstall.sh "$STAGE/uninstall.sh"
install -m 0644 "packaging/$APP_ID.desktop" "$STAGE/share/applications/"
install -m 0644 "packaging/$APP_ID.metainfo.xml" "$STAGE/share/metainfo/"
install -m 0644 LICENSE CHANGELOG.md README.md "$STAGE/"
install -m 0644 assets/icons/COPYING.ADWAITA "$STAGE/share/licenses/delve/"
install -m 0644 assets/delve.svg "$STAGE/assets/"
install -m 0644 assets/icons/README.md "$STAGE/assets/icons/"
install -m 0644 assets/icons/COPYING.ADWAITA "$STAGE/assets/icons/"
cp -a assets/icons/hicolor "$STAGE/share/icons/"
cp -a assets/icons/Delve "$STAGE/share/icons/"

# Raster app icons are committed for platforms that do not render SVG. Rebuild
# them from assets/delve.svg in the staging tree so a release can never ship an
# older logo than the SVG it was built from.
if command -v rsvg-convert >/dev/null 2>&1; then
  for size in 16 24 32 48 64 128 256 512; do
    dir="$STAGE/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    rsvg-convert -w "$size" -h "$size" assets/delve.svg -o "$dir/$APP_ID.png"
  done
  echo "==> Refreshed raster application icons from assets/delve.svg"
else
  echo "Warning: rsvg-convert not found; packaged PNG icons may be stale." >&2
fi
cp -a docs/. "$STAGE/docs/"

echo "==> Creating $TARBALL"
tar -C "$DIST" -czf "$TARBALL" "$NAME"
(
  cd "$DIST"
  sha256sum "$NAME.tar.gz" > SHA256SUMS
)

echo
echo "Release artifacts:"
ls -lh "$TARBALL" "$DIST/SHA256SUMS"
echo
echo "Verify with: cd dist && sha256sum -c SHA256SUMS"
