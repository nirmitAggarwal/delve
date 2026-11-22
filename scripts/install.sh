#!/usr/bin/env bash
# Install Delve from a release directory or from a source checkout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ID="io.github.delve.FileManager"
PREFIX="${PREFIX:-/usr/local}"
DESTDIR="${DESTDIR:-}"

usage() {
  cat <<'EOF'
Usage: install.sh [OPTIONS]

Install Delve, its desktop entry, AppStream metadata, application icons, the
bundled icon theme, and documentation.

Options:
  --prefix PATH    Install under PATH (default: /usr/local, or $HOME/.local
                   with --user)
  --destdir PATH   Stage the installation under PATH (for packaging). Desktop
                   and icon caches are not updated when this is set.
  --user           Install for the current user under $HOME/.local
  -h, --help       Show this help

Environment:
  PREFIX, DESTDIR  Same as the corresponding options.
  DELVE_BINARY     Source-tree install only: binary to install. Defaults to
                   target/release/delve.

Examples:
  sudo ./install.sh
  ./install.sh --user
  sudo ./install.sh --prefix /opt/delve
  ./install.sh --prefix /usr --destdir "$PWD/pkg"
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

# A packaged release has ./delve and ./share next to this script. A source
# checkout has Cargo.toml one level up and uses the release build.
if [[ -x "$SCRIPT_DIR/delve" && -d "$SCRIPT_DIR/share/icons" ]]; then
  ROOT="$SCRIPT_DIR"
  BINARY="$SCRIPT_DIR/delve"
  ICONS="$SCRIPT_DIR/share/icons"
  DESKTOP="$SCRIPT_DIR/share/applications/$APP_ID.desktop"
  METAINFO="$SCRIPT_DIR/share/metainfo/$APP_ID.metainfo.xml"
  DOCS="$SCRIPT_DIR/docs"
elif [[ -f "$SCRIPT_DIR/../Cargo.toml" ]]; then
  ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
  BINARY="${DELVE_BINARY:-$ROOT/target/release/delve}"
  ICONS="$ROOT/assets/icons"
  DESKTOP="$ROOT/packaging/$APP_ID.desktop"
  METAINFO="$ROOT/packaging/$APP_ID.metainfo.xml"
  DOCS="$ROOT/docs"
else
  echo "Could not find a Delve release layout or source checkout next to $SCRIPT_DIR." >&2
  exit 1
fi

for required in "$BINARY" "$ICONS/Delve/index.theme" "$ICONS/hicolor" "$DESKTOP" "$METAINFO"; do
  if [[ ! -e "$required" ]]; then
    echo "Missing installation file: $required" >&2
    if [[ "$required" == "$BINARY" ]]; then
      echo "Build it first with: cargo build --release --locked" >&2
    fi
    exit 1
  fi
done

if [[ -z "$DESTDIR" && ! -w "$PREFIX" && ! -w "$(dirname "$PREFIX")" ]]; then
  echo "No write permission for $PREFIX. Re-run with sudo, choose --user," >&2
  echo "or select a writable --prefix." >&2
  exit 1
fi

BIN_DIR="$DESTDIR$PREFIX/bin"
DATA_DIR="$DESTDIR$PREFIX/share"
ICON_DIR="$DATA_DIR/icons"
DOC_DIR="$DATA_DIR/doc/delve"
LICENSE_DIR="$DATA_DIR/licenses/delve"

echo "Installing Delve to $DESTDIR$PREFIX"

install -d "$BIN_DIR" "$DATA_DIR/applications" "$DATA_DIR/metainfo" \
  "$ICON_DIR/hicolor" "$ICON_DIR/Delve" "$DOC_DIR/docs" \
  "$DOC_DIR/assets/icons" "$LICENSE_DIR"
install -m 0755 "$BINARY" "$BIN_DIR/delve"
install -m 0644 "$DESKTOP" "$DATA_DIR/applications/$APP_ID.desktop"
install -m 0644 "$METAINFO" "$DATA_DIR/metainfo/$APP_ID.metainfo.xml"

# Install every application icon size and the complete fallback theme.
cp -a "$ICONS/hicolor/." "$ICON_DIR/hicolor/"
cp -a "$ICONS/Delve/." "$ICON_DIR/Delve/"

if [[ -d "$DOCS" ]]; then
  cp -a "$DOCS/." "$DOC_DIR/docs/"
fi
if [[ -f "$ROOT/assets/delve.svg" ]]; then
  install -m 0644 "$ROOT/assets/delve.svg" "$DOC_DIR/assets/delve.svg"
fi
if [[ -f "$ROOT/assets/icons/README.md" ]]; then
  install -m 0644 "$ROOT/assets/icons/README.md" "$DOC_DIR/assets/icons/README.md"
fi
if [[ -f "$ROOT/assets/icons/COPYING.ADWAITA" ]]; then
  install -m 0644 "$ROOT/assets/icons/COPYING.ADWAITA" "$DOC_DIR/assets/icons/COPYING.ADWAITA"
fi
for file in README.md CHANGELOG.md LICENSE; do
  if [[ -f "$ROOT/$file" ]]; then
    install -m 0644 "$ROOT/$file" "$DOC_DIR/$file"
  fi
done
for notice in "$ROOT/share/licenses/delve/COPYING.ADWAITA" \
              "$ROOT/assets/icons/COPYING.ADWAITA"; do
  if [[ -f "$notice" ]]; then
    install -m 0644 "$notice" "$LICENSE_DIR/COPYING.ADWAITA"
    break
  fi
done

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
  if [[ -n "$icon_cache" ]]; then
    "$icon_cache" -q -t -f "$ICON_DIR/hicolor" >/dev/null 2>&1 || true
    "$icon_cache" -q -t -f "$ICON_DIR/Delve" >/dev/null 2>&1 || true
  fi
fi

echo
echo "Delve installed."
echo "Run: ${PREFIX}/bin/delve"
if [[ -z "$DESTDIR" && "$PREFIX" == "$HOME/.local" ]]; then
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "Add $HOME/.local/bin to PATH if your desktop does not find the command." ;;
  esac
fi
