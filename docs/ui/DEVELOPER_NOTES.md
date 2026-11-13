# Frontend developer notes

How the UI in `frontend/src` is put together, and the conventions to keep.
The reasons behind it are in [FINDINGS.md](FINDINGS.md) and
[ADR 0009](../adr/0009-preact-signals-and-project-view.md).

## Running it

```sh
npm --prefix frontend run dev          # browser, preview backend
cargo tauri dev                        # desktop, real engine
```

In a plain browser tab the app installs `dev/mock-backend.ts` and shows a
**Browser preview** badge. Choose a fixture with `?fixture=<name>`: files in
`examples/*.json` and the git-ignored `frontend/dev-fixtures/*.json` are found
by name prefix. From the page's own console:

```js
__mock.agentEdit();        // an MCP-style move with a rationale
__mock.agentProposeNet();  // a proposed net needing approval
```

The preview backend is deliberately naive: L-shaped route suggestions, no
validation beyond net completeness, and part drafts edited in memory. Check
behaviour that matters against `cargo tauri dev`.

## Layout

```
src/
  main.tsx            boot: optional preview backend, render, start sync
  app/App.tsx         shell layout and global keyboard shortcuts
  api/project.ts      typed Tauri commands (mirror of Rust types, ADR 0004)
  state/
    store.ts          all signals: engine data, UI state, derived values
    actions.ts        every engine call: mutate, undo, files, routing…
    sync.ts           project-changed events + revision polling
    toasts.ts         notification queue
  model/              pure, tested: geometry, naming, ratsnest
  board/
    BoardScene.tsx    pure SVG scene (no event handlers), reused for print
    BoardCanvas.tsx   camera, gestures, tools, overlays
    CanvasHud.tsx     floating tool options, side switch, route card, view
    HoverCard.tsx     pointer tooltips
    hit.ts            pure pointer helpers (tested)
  panels/             Parts, Nets, Inspector, Problems, History
  shell/              TopBar, ToolRail, StatusBar, Toasts, dialogs, Welcome
  component-designer/ part editor; preview.ts and helpers predate the rebuild
  print/
    PrintSheet.tsx    build drawing for @media print (not true to scale)
    wire-templates.ts pure, tested: wire shapes, dimensions, cut columns, bends
    pdf-pages.ts      true-scale cut guide and bending templates as page SVGs
  editor/             older pure helpers still used (routing, drag, sizing)
  styles/             tokens, app, board, designer, print
  dev/mock-backend.ts preview backend (dev only, tree-shaken from builds)
```

## State and data flow

- **Read path.** `refresh()` (coalesced) loads `get_project_view` and
  `get_net_connectivity`, then the comparison diff and the compared commit's
  design. `sync.ts` calls it on `project-changed` events, on window focus,
  and when a 1.2 s revision poll sees a new revision while visible.
- **Write path.** Components never call `api/project.ts` directly. They call
  `actions.ts`, which goes through `mutate(ops, {rationale})`: one operation,
  or a transaction for several, always with the current revision. A
  `revision.stale` error refreshes and tells the user that something
  (usually the agent) changed the board first.
- **Agent awareness.** After each refresh, `announceExternalChanges` compares
  undo operation ids. New entries with origin `mcp` raise an agent toast and
  pulse the touched objects (`flashIds`).
- **Derived values** live in `store.ts` as `computed`: diagnostics counts,
  wire labels, airwires, change kinds, proposals, the emphasised net. Keep
  them presentational; if a rule decides validity, it belongs in Rust.

## Coordinates

- Hole units everywhere in the scene: hole `(x, y)` is drawn at `(x, y)`.
- Routes use half-grid integers `{x2, y2}`. Convert with `toRoute` and
  `fromRoute`.
- The solder side is a presentation transform only:
  `translate(holes_x-1 0) scale(-1 1)` on the geometry group. Text is drawn
  outside that group at `viewX(x)`, so it is never mirrored. Stored
  coordinates are always canonical.
- Bridges are components with `kind: "bridge"` on the solder side.

## Conventions

- **Colour means something.** Copper (`--copper`) is the single accent and
  selection colour. Violet (`--agent`) is only for agent activity. Error,
  warning, and OK colours are only for diagnostics and status.
- **SVG hit targets** are wide invisible strokes (`.wire-hit`,
  `.airwire-hit`) identified by `data-*` attributes (`data-pin`,
  `data-wire`, `data-component`…). Pointer logic reads those; the scene stays
  free of handlers so print and compare views can reuse it.
- **No `window.confirm`.** Use `confirmAction()` from `shell/dialogs.ts`.
- **Markdown** from parts is untrusted. Render it only through
  `renderSanitizedMarkdown`, which escapes HTML.
- **Keyboard:** global shortcuts are in `App.tsx` and ignored while typing in
  a field. Canvas-specific keys (Esc, Enter, R while placing, Space-pan) are
  handled in `BoardCanvas` in the capture phase.

## New Tauri commands (this branch)

| Command | Purpose |
| --- | --- |
| `get_project_view` | Design + validation + history summaries (`ProjectView`) |
| `get_net_connectivity` | Per-net islands of electrically joined pins |
| `get_commit_design` | One commit's design, for comparison |
| `rename_project` | Rename the project (revision-checked, not an undo step; also an MCP tool) |
| `get_undo_history_bytes` | Size of stored undo snapshots |
| `pick_project_path` | Native open/save dialog (`tauri-plugin-dialog`) |
| `export_pdf` | Save page SVGs as a true-scale PDF and open it (`tauri-plugin-opener`) |

Rust side: `crates/project/src/view.rs` and `net_connectivity()` in
`connectivity.rs`, each with a unit test. `export_pdf` uses the
`perfboard-print` crate (`crates/print`), which converts the page SVGs with
`svg2pdf`.

## Component library location

`get_component_library` reads `components/` from the app's resource directory
when it exists, which is where packages install it
(`/usr/lib/Perfboard Designer/components`). Otherwise it falls back to the
checkout the binary was compiled from, so `cargo tauri dev` and
`./quickstart.sh exe` builds see edits to `components/` directly. Packages copy
the library at build time without Component Designer `*.bak` backups.

## Printing at true scale

WebKitGTK (the Linux web view) rescales whatever `window.print()` prints, and
ignores `@page` size and margins: 80 mm of CSS printed as 86–91 mm depending on
the paper. Headless Chromium honours CSS millimetres, so a check there proves
nothing. Anything that must measure 1:1 is therefore built as page-sized SVGs
in millimetres (`print/pdf-pages.ts`, presentation attributes only, no CSS)
and exported as a PDF. See
[ADR 0011](../adr/0011-true-scale-pdf-output.md).

To check scale, measure the PDF, not a browser render. In the preview
backend, export from the File menu, then read the pages with
`window.__mock.pdfPages()`, save them as files, and convert them as the app
does:

```sh
cargo run -p perfboard-print --example svg_pages_to_pdf -- out.pdf page*.svg
pdftoppm -r 254 -png out.pdf page   # 254 dpi = 10 px per mm
```

## Checks

```sh
npm --prefix frontend run format:check
npm --prefix frontend run lint
npm --prefix frontend test
npm --prefix frontend run build
```

The Rust checks in the [README](../../README.md#quality-checks) pass as well;
the test and Clippy failures recorded in FINDINGS.md have been resolved.

## Known gaps

- Junctions are not drawn on the board and there is no junction tool.
- The router's crossing order is chosen per route; there is no global
  assembly-order view yet (Milestone 6).
- Part designer region editing covers labels and direction; resizing keepouts
  and access regions is done on the preview.
- No automated UI tests yet. The preview backend makes Playwright tests
  straightforward to add.
