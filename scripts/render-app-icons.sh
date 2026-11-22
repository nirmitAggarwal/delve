#!/usr/bin/env bash
# Render the Delve application icon at every freedesktop icon size.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v rsvg-convert >/dev/null 2>&1 || {
  echo "rsvg-convert is required (librsvg2-bin on Debian/Ubuntu)." >&2
  exit 1
}

for size in 16 24 32 48 64 128 256 512; do
  dir="assets/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  rsvg-convert -w "$size" -h "$size" assets/delve.svg \
    -o "$dir/io.github.delve.FileManager.png"
  echo "rendered ${size}x${size}"
done

mkdir -p assets/icons/hicolor/scalable/apps
cp assets/delve.svg assets/icons/hicolor/scalable/apps/io.github.delve.FileManager.svg
mkdir -p assets/icons/Delve/scalable/apps
cp assets/delve.svg assets/icons/Delve/scalable/apps/io.github.delve.FileManager.svg
