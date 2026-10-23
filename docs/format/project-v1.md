# Project format v1

Status: implemented through Milestone 2

Perfboard Designer project files are UTF-8 JSON. The suggested suffix is
`.perfboard.json`. Writers emit two-space-indented JSON with one trailing
newline. Domain maps introduced by later milestones will be serialized in key
order so fixtures and revisions remain deterministic.

## Envelope

```json
{
  "schema_version": 1,
  "project_id": "018f1000-0000-7000-8000-000000000001",
  "name": "Example",
  "revision": 0,
  "design": {
    "board": {
      "name": "25x18 perfboard",
      "holes_x": 25,
      "holes_y": 18,
      "pitch_mm": 2.54
    }
  }
}
```

- `schema_version` is an integer format version. Version 1 readers reject
  unknown versions rather than guessing.
- `project_id` is a stable UUID string.
- `revision` is a non-negative, monotonically increasing document revision.
  Revision semantics become operational in Milestone 1.
- `design.board.holes_x` and `holes_y` count holes, not intervals, and must both
  be at least one.
- `pitch_mm` must be positive and finite. JSON is not permitted to contain
  non-finite numeric values.

Milestone 1 adds optional board `margins_mm`, `mounting_holes`,
`forbidden_areas`, and `notes`; a `component_definitions` map keyed by stable
definition ID; and a `components` map keyed by instance UUID. Every used
definition is embedded in full so external library edits cannot change the
project. Instance data includes canonical anchor position, rotation, user name,
properties, and notes. A cached `validation` array is stored when non-empty and
must match deterministic validation on load. Placement diagnostics may be
stored in an otherwise structurally valid project: the graphical editor uses
this to allow temporary out-of-bounds or overlapping states while a component
is being rotated and repositioned.

The permanent populated example is
[`examples/nano-rs485.perfboard.json`](../../examples/nano-rs485.perfboard.json).
It records the result of the Milestone 1 add, move, and rotate demonstration.

All coordinates stored in a project are canonical component-side coordinates.
A solder-side view is a presentation transform and is never persisted as
mirrored geometry.

Milestone 2 adds `nets`, `junctions`, and `wires`, all deterministic maps keyed
by typed UUIDs. A net records its accepted/proposed state and a set of stable
component-pin references. A pin can belong to at most one net. Junctions name a
hole and explicitly state whether they form a through-hole transition. Wires
reference two terminals, one net and one board side; their ordered `vertices`
use canonical half-grid coordinates (`x2`, `y2`). Wire color, lock state and
notes are authoritative, while length, bends and connectivity diagnostics are
derived. Paths accept horizontal, vertical and 45-degree segments. A geometric
crossing never joins two wires; connectivity exists only through shared
terminal references.

The permanent minimal fixture is
[`tests/fixtures/project-v1/empty-1x1.perfboard.json`](../../tests/fixtures/project-v1/empty-1x1.perfboard.json).
