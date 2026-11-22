#!/usr/bin/env bash
# Remove Delve while preserving user settings, tags, themes, and caches.
set -euo pipefail

APP_ID="io.github.delve.FileManager"
PREFIX="${PREFIX:-/usr/local}"
DESTDIR="${DESTDIR:-}"

usage() {
  cat <<'EOF'
Usage: uninstall.sh [OPTIONS]

Remove the Delve binary, desktop entry, AppStream metadata, application icons,
bundled icon theme, and installed documentation. User configuration, tags,
themes, and caches are deliberately preserved.

Options:
  --prefix PATH    Installation prefix (default: /usr/local, or $HOME/.local
                   with --user)
  --destdir PATH   Remove from a staged installation under PATH
  --user           Remove a per-user installation under $HOME/.local
  -h, --help       Show this help

Environment:
  PREFIX, DESTDIR  Same as the corresponding options.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      [[ $# -ge 2 ]] || { echo "--prefix needs a path" >&2; exit 2; }
      PREFIX="$2"
      shift 2
      ;;
    --destdir)
      [[ $# -ge 2 ]] || { echo "--destdir needs a path" >&2; exit 2; }
      DESTDIR="$2"
      shift 2
      ;;
    --user)
      PREFIX="$HOME/.local"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$PREFIX" != /* ]]; then
  echo "PREFIX must be an absolute path: $PREFIX" >&2
  exit 2
fi

if [[ -z "$DESTDIR" && ! -w "$PREFIX" && ! -w "$(dirname "$PREFIX")" ]]; then
  echo "No write permission for $PREFIX. Re-run with sudo or choose --user." >&2
  exit 1
fi

BIN_DIR="$DESTDIR$PREFIX/bin"
DATA_DIR="$DESTDIR$PREFIX/share"
ICON_DIR="$DATA_DIR/icons"

echo "Removing Delve from $DESTDIR$PREFIX"
rm -f "$BIN_DIR/delve"
rm -f "$DATA_DIR/applications/$APP_ID.desktop"
rm -f "$DATA_DIR/metainfo/$APP_ID.metainfo.xml"
rm -rf "$ICON_DIR/Delve"
rm -rf "$DATA_DIR/doc/delve"
rm -rf "$DATA_DIR/licenses/delve"

for size in 16 24 32 48 64 128 256 512; do
  rm -f "$ICON_DIR/hicolor/${size}x${size}/apps/$APP_ID.png"
  rmdir "$ICON_DIR/hicolor/${size}x${size}/apps" 2>/dev/null || true
  rmdir "$ICON_DIR/hicolor/${size}x${size}" 2>/dev/null || true
done
rm -f "$ICON_DIR/hicolor/scalable/apps/$APP_ID.svg"
rmdir "$ICON_DIR/hicolor/scalable/apps" 2>/dev/null || true
rmdir "$ICON_DIR/hicolor/scalable" 2>/dev/null || true

if [[ -z "$DESTDIR" ]]; then
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DATA_DIR/applications" >/dev/null 2>&1 || true
  fi
  icon_cache=""
  for candidate in gtk4-update-icon-cache gtk-update-icon-cache; do
    if command -v "$candidate" >/dev/null 2>&1; then
      icon_cache="$candidate"
      break
    fi
  done
  if [[ -n "$icon_cache" && -d "$ICON_DIR/hicolor" ]]; then
    "$icon_cache" -q -t -f "$ICON_DIR/hicolor" >/dev/null 2>&1 || true
  fi
fi

echo
echo "Delve removed."
echo
echo "Your settings, tags, themes, thumbnails, and log were preserved:"
printf '  %s\n' \
  "${XDG_CONFIG_HOME:-$HOME/.config}/delve" \
  "${XDG_DATA_HOME:-$HOME/.local/share}/delve" \
  "${XDG_CACHE_HOME:-$HOME/.cache}/delve"
