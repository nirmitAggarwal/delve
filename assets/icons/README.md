# Bundled icons

Delve ships a complete `Delve` icon theme so the application does not depend on
an icon theme being installed on the host. The theme is embedded in the binary as
GResources and is also installed under `share/icons/Delve` for desktop menus and
other applications.

## Contents

- `Delve/` — the bundled fallback icon theme. It contains the full Adwaita
  symbolic icon set plus scalable Places, MIME, Devices and Status artwork.
- `hicolor/` — Delve application icons in the sizes required by the
  freedesktop.org icon theme specification.
- `COPYING.ADWAITA` — copyright and licensing information for the derived
  theme artwork.

The Delve logo (`assets/delve.svg` and the application icons in `hicolor/`) is
original artwork for this project and is distributed under the project's MIT
license.

## Updating the icon theme

The bundled theme is deliberately checked in so release builds and offline
builds do not download anything. To refresh it from an Adwaita installation:

```sh
rm -rf assets/icons/Delve
mkdir -p assets/icons/Delve
cp -a /usr/share/icons/Adwaita/16x16 assets/icons/Delve/
cp -a /usr/share/icons/Adwaita/symbolic assets/icons/Delve/
cp -a /usr/share/icons/Adwaita/scalable assets/icons/Delve/
cp /usr/share/icons/Adwaita/index.theme assets/icons/Delve/index.theme
sed -i \
  -e 's/^Name=Adwaita$/Name=Delve/' \
  -e 's/^Comment=.*/Comment=Bundled icon theme for Delve/' \
  -e 's/^Inherits=.*/Inherits=Adwaita,hicolor/' \
  -e 's/^Hidden=true$/Hidden=false/' \
  assets/icons/Delve/index.theme
scripts/render-app-icons.sh
```

The checked-in `index.theme` lists only the directories that are actually
shipped. If you copy a different subset of Adwaita, trim the `Directories=`
line and remove the matching `[directory]` sections; do not list directories
that were not copied. The theme intentionally inherits `Adwaita,hicolor` for
icons outside the bundled subset.

Review the diff, update `COPYING.ADWAITA` if needed, and rebuild. Do not add an
icon theme that is not redistributable.

## Licensing

The Adwaita-derived artwork is distributed under CC-BY-SA-3.0 or LGPL-3.0; the
full copyright notice and license texts are in `COPYING.ADWAITA`. Attribution:
GNOME Project, <https://www.gnome.org>. These terms apply only to the bundled
theme artwork, not to Delve's source code, which is MIT licensed.
