# Application icons

`icon.svg` is the source: the top bar's brand mark (a board with 3 × 3 holes
and a stepped wire) drawn as a tan perfboard with a green jumper, which stays
legible down to 16 px. Every other file here is generated from it.

## Regenerating

```sh
cargo tauri icon src-tauri/icons/icon.svg -o /tmp/perfboard-icons
cp /tmp/perfboard-icons/{32x32,64x64,128x128,128x128@2x,icon}.png \
   /tmp/perfboard-icons/icon.{icns,ico} src-tauri/icons/
```

The Android, iOS and Windows Store images it also writes are not used.

## Why `128x128.png` is listed first

Tauri uses the first PNG in `bundle.icon` (`tauri.conf.json`) as the window
icon on Linux. On X11, GTK sets it as the window's `_NET_WM_ICON` property,
which must fit in one X request of 65,535 32-bit units. A 256 or 512 px icon
does not fit, and GTK then sets no icon at all, without a warning; the
window shows a generic icon in the dock and Alt+Tab. 128 px fits. The bundlers
pick the other files by size or extension, so the order does not affect
packages.
