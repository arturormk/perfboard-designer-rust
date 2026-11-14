# UI assessment and redesign rationale

Date: 2026-11-11 · Branch: `ui-redesign`

## Summary

The Rust side (domain model, operations, revisions, routing, validation, MCP
bridge) is sound and well tested. The original frontend was not: it was a
single 3,400-line `main.ts` with global mutable state and full `innerHTML`
re-renders, and its interaction design exposed the data model rather than the
task. Incremental repair would have meant touching nearly every line, so **the
UI was rebuilt from scratch** on Preact + signals (see
[ADR 0009](../adr/0009-preact-signals-and-project-view.md)). The tested pure
helpers (`editor/*`, `component-designer/*`) and the typed command layer
(`api/project.ts`) were kept.

## What was wrong

### Architecture

- **Monolith.** `main.ts` mixed rendering, event wiring, state, and command
  calls. About 60 module-level `let` variables held state; any change
  re-rendered whole regions through template strings, losing focus and scroll.
- **Static markup in `index.html`.** Every control existed at all times and was
  shown, hidden, or disabled imperatively, so states drifted (buttons enabled
  for the wrong selection, stale inspector fields).
- **Heavy reads.** `get_project` returned the full document after every edit,
  including each undo entry's before/after design snapshots. The working
  example is 16 MB, so every click re-serialized 16 MB across the bridge.
- **Dropped information.** Diagnostics carry `objects` (what they refer to),
  but the TypeScript type omitted it, so problems could not be located.

### Interaction design

- **Too much at once.** Around 60 controls were permanently visible: net
  forms, constraint JSON, wire bend tables, history, and component palette
  shared one long sidebar.
- **The board hid the circuit.** Opaque component bodies covered pins and
  wiring, parts had no names on the board, and wires were labelled only `W12`.
- **Surprising clicks.** Clicking a pin silently toggled its membership of the
  selected net. Nothing showed which pins still needed connecting.
- **Raw data entry.** Constraints were edited as JSON; Open and Save took a
  typed file path; the board size could only be set numerically.
- **List noise.** The 44 bridges in the working design flooded the parts list.
- **Invisible agent.** MCP edits appeared silently. Nothing distinguished what
  the agent did from what the user did, although the history records it, and
  proposed nets needing approval were easy to miss. This is the product's
  headline feature.
- **`window.confirm`** was used for destructive actions; it is unreliable in
  Tauri's webview.

## What the redesign does

| Problem | Response |
| --- | --- |
| 16 MB reads | New `get_project_view` returns the design plus *summaries* of history (`ProjectView`). The full document never crosses the bridge during editing. |
| No unrouted display | New `get_net_connectivity` (Rust decides what is joined); the UI draws dashed hints between islands and offers "click to route". |
| Hidden circuit | Pads drawn above bodies, part names centred on bodies, pin numbers in pads, per-net emphasis on hover, see-through toggle, real solder-side copper rendering. |
| Pin-click surprises | Pins are assigned by drawing a wire or from the part's pin table. Alt+click toggles for power users. A wire from an unassigned pin prompts for a net name. |
| Raw JSON | Typed constraint forms for all 11 constraint types; native file dialogs (`pick_project_path`); drag-to-resize board; new-board presets. |
| Invisible agent | Violet is reserved for agent activity: toasts with the agent's rationale and Review/Undo, a "to review" pill, an approval block for proposed nets, origin badges in history, and changed objects pulse on the board. |
| Bridges flooding lists | Parts grouped by type, bridges collapsed last. |
| Locating problems | Diagnostics list with plain-language advice; click to select and frame the object. |
| Comparing | Highlight changes against the head commit or any earlier commit; side-by-side split view. |
| Printing | A build sheet: both faces plus a wire cut list with colour, length, bends and a tick column. Wire cut guides and bending templates are exported as a true-scale PDF, because the web view cannot print at 1:1 (ADR 0011). |

## Things found along the way (not UI)

- **Failing tests at HEAD.** `cargo test --workspace` fails three tests on the
  base commit, independent of this branch:
  `milestone1::place_move_rotate_save_and_reopen_example_components`
  (bundled RS-485 part now placed out of bounds),
  `milestone2::non_octilinear_wire_is_rejected_atomically`, and
  `mcp-server milestone5::real_stdio_server_reads_and_mutates_the_shared_session`.
  They had drifted from the component library and wire-rule changes in
  `06c79c5`. *Resolved in `029c460`.*
- **Clippy baseline.** `cargo clippy -- -D warnings` fails on three lints from
  newer Clippy (`too_many_lines` in `component_draft.rs`, plus lints in
  `routing.rs`, `constraints.rs`, `connectivity.rs`). None are in new code.
  *Resolved in `8f267a8`.*
- **Contract drift risk (ADR 0004).** TypeScript types in `api/project.ts` are
  hand-written mirrors of Rust types. Generated types (for example `ts-rs` or
  `specta`) would have caught the missing `Diagnostic.objects`.
- **Undo snapshots dominate file size.** Each undo entry stores two full
  designs. The History dialog shows the size and can trim it, but a delta
  representation in Rust would fix the cause.
  *Resolved in project format v2 ([ADR 0013](../adr/0013-patch-encoded-history.md)):
  the file stores undo/redo snapshots as JSON patches and commits no longer
  copy them; a 40.7 MB project now saves as 778 KB.*
