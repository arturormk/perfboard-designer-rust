# Project format v1

Status: draft contract for Milestone 0

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

The only v1 content currently implemented is an empty design with a rectangular
board. Later milestones will extend the `design` object with component
snapshots, instances, circuit data, layout data, constraints, and authoritative
user notes. Additions to version 1 must remain backward-compatible or be
introduced through a documented migration.

All coordinates stored in a project are canonical component-side coordinates.
A solder-side view is a presentation transform and is never persisted as
mirrored geometry.

The permanent minimal fixture is
[`tests/fixtures/project-v1/empty-1x1.perfboard.json`](../../tests/fixtures/project-v1/empty-1x1.perfboard.json).
