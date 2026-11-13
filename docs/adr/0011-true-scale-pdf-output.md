# ADR 0011: True-scale print sheets are exported as PDF

- Status: accepted
- Date: 2026-11-13

## Context

The wire cut guide and bending templates are only useful at exactly 1:1: a
hobbyist lays wire on the paper to cut and bend it. They were first printed
from HTML with `window.print()` and SVGs sized in CSS millimetres.

On Linux the desktop shell is WebKitGTK, and its print path is not
true-scale. It ignores `@page` size and margins and fits the page to the paper
by its own rules. Printed through WebKitGTK 2.50 to a PDF file, 80 mm of CSS
came out as 86.3 mm on A4 landscape, 90.4 mm on A4 portrait and 91.4 mm on
JIS B5. Window width made no difference, and wider content shrank everything.
On a real printer, a user measured 80 mm printing at 85.5 mm. Headless Chromium
does honour CSS millimetres, so checks there gave false confidence. Other
platforms' web views have their own print scaling, and print dialogs often
default to "fit to page".

Tools that print reliably at 1:1, such as Inkscape and sewing-pattern
software, draw onto a page of known physical size in real units, and the user
prints that document at "Actual size".

## Decision

1. **Sheets that must be 1:1 are never printed from HTML.** They are exported
   as a PDF (**File → Export wire templates…**) and printed from a PDF viewer
   at "Actual size" (100 %). The build drawing, which needs no exact scale,
   still uses `window.print()`.
2. **The frontend lays out pages.** `frontend/src/print/pdf-pages.ts` builds
   complete page-sized SVG documents in millimetres (A4, or US Letter for
   locales that use it), with fixed margins, its own text wrapping and page
   breaks, and presentation attributes only (no CSS). It reuses the existing
   template geometry in `wire-templates.ts`.
3. **Rust writes the PDF.** The `perfboard-print` crate converts the pages with
   `svg2pdf` (usvg resolves `mm` at 96 px per inch, written as 0.75 pt per
   px) and embeds system fonts, preferring DejaVu Sans. It has no Tauri
   dependency, so it can be tested on its own. The `export_pdf` Tauri command
   asks where to save, writes the file atomically and opens it in the system
   viewer.
4. **Every page proves its scale.** Each page carries a 50 mm calibration bar
   and a red warning that uses the print dialog's own wording: "set Page
   Scaling to None". In testing, the Ubuntu document viewer (Evince 42)
   remembered "Fit" as its page scaling and printed the 50 mm bar at 48 mm.
   Evince ignores a PDF's `/PrintScaling` preference, so the warning is what
   protects the user.

## Consequences

The PDF's page geometry is exact: A4 pages are 595.28 × 841.89 pt, and
measured renders show the bar at 50.0 mm and 10.0 mm grid squares. The printout
is right as long as the user prints at actual size, and the bar shows when
they did not. The file can also be kept or shared.

Page layout now lives in our code rather than the browser's, so text
measurement is an estimate (a generous average glyph width) and wrapping is
simpler than CSS. A wire shape too wide for the paper is turned a quarter,
which is the same piece of wire. One that is still too large is not scaled
down, because a template must never silently change scale. To verify any
print output, measure the PDF (for example with `pdftoppm` at 254 dpi, which is
10 px per mm), not a browser render.
