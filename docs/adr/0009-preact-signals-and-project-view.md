# ADR 0009: Preact with signals, and a summarized project read model

- Status: accepted
- Date: 2026-11-11
- Amends: [ADR 0002](0002-desktop-and-frontend-tooling.md) (framework-free DOM)

## Context

ADR 0002 started the UI without a framework and allowed one later "if editor
state synchronization demonstrates a concrete need". It did: the framework-free
editor grew into one 3,400-line module with global mutable state and full
re-renders, and every edit re-read the whole project document, including
undo snapshots (16 MB for the working example). See
[the UI findings](../ui/FINDINGS.md).

## Decision

1. **Preact + `@preact/signals`** for the frontend. Preact is about 4 kB,
   renders SVG well, and uses JSX type-checked by `tsc`. Signals keep
   application state outside components and update only what reads them,
   which suits a large SVG scene with frequent hover changes. No router, no
   CSS framework, no state library beyond signals.
2. **Rust remains authoritative.** Signals hold the latest *view* of the
   engine state plus presentation state (tool, selection, camera, panels).
   Every change is an engine operation with an expected revision; derived
   values in the UI (airwires, labels, counts) are display aids only.
3. **`get_project_view` read model.** The UI reads `ProjectView`: the design,
   validation, and *summaries* of commits and undo/redo entries (origin,
   rationale, semantic diff), never the full snapshots. A commit's design is
   fetched on demand (`get_commit_design`) for comparison.
4. **`get_net_connectivity`** exposes which pins of each net are already
   joined (wires, rails, bridges), so the UI never re-implements electrical
   connectivity to draw unrouted hints.
5. **Preview backend for browser development.** In `vite dev` without Tauri,
   `src/dev/mock-backend.ts` installs an in-memory imitation of the commands
   so the UI can be developed and screenshotted without Rust. It is excluded
   from production builds and performs no validation.

## Consequences

- Two runtime dependencies are added (`preact`, `@preact/signals`).
- The old `main.ts`, `styles.css`, and string renderer are deleted; tested pure
  helpers are kept.
- `get_project` stays for MCP and file operations but is no longer called on
  every UI refresh.
- The preview backend can drift from Rust behaviour. It is a development aid
  only; behaviour must be verified against the real engine.
