# Project format v1

Status: implemented through Milestone 4. Superseded by
[version 2](project-v2.md), which changes only how revision history is
stored; version 1 files are still read and upgraded on load.

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
properties, notes, and `shunted` (written only when `true`). `shunted` means a
shunt is fitted on a `jumper` part: its two pins count as connected. When both
are on the same net they need no wire between them (an extra, parallel path is
also fine). When they are on two different nets the shunt joins those nets
while it is fitted, which is what a selector jumper is for: each net stays its
own net in the file, is still routed and checked for completeness on its own
copper, and the join is reported as a `circuit.shunt_joins_nets` advisory.
`get_net_connectivity` lists, per net, the other nets it currently reaches
through shunts (`joined_nets`, transitively) and the jumpers doing it
(`shunts`). It is set with `set_jumper_shunt`, which refuses parts of other kinds.
`update_component_metadata` also adopts a library change between `standard`
and `jumper` when the pins are identical; going back to `standard` clears
`shunted`. A cached `validation` array is stored when non-empty and
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
derived. Paths are polylines whose nodes lie on the half-hole grid; each
segment must have non-zero length. A geometric
crossing never joins two wires; connectivity exists only through shared
terminal references.

Rails extend that circuit model as net-owned polylines with no terminal
requirement at either end. Like wires, their ordered vertices use canonical
half-grid (`x2`, `y2`) coordinates and belong to one board side. A wire can use
a `rail_node` terminal containing a rail ID and position; the position must be
an explicit stored rail vertex and must belong to the wire's net. A node on a
whole hole is soldered through that hole, so a wire on either side may connect
to it; a node between holes can only be reached by wires on the rail's side.
All explicit nodes on one rail are electrically connected; adding a connection
point is a `modify_rail_path` that inserts a vertex on an existing segment.
This deliberately makes connection points stable and editable rather than
inferring connectivity from geometric crossings.

Saddles are mechanical wire ties stored in the optional `saddles` map, keyed by
typed UUID. Each records `side` (the face the strap lies on, i.e. the face of
the wires it holds), whole-hole `start` and `end` positions (`x`, `y`), a
`color` (default `black`) and optional `notes`. A saddle belongs to no net,
has no terminals and never contributes to connectivity. Its two holes must
differ, lie on the board, and be free: no component pin or covered hole,
junction, wire or rail path, other saddle, mounting hole or forbidden area.
Adding or updating a saddle that violates this, or whose strap passes over a
component pin, is rejected. Later edits that take a saddle hole are reported as
`saddle.hole_conflict` warnings, and a saddle that crosses no wire or rail on
its side is reported as a `saddle.holds_nothing` advisory. The router treats
saddle holes as obstacles on both faces, because the legs are soldered on the
far face.

The permanent minimal fixture is
[`tests/fixtures/project-v1/empty-1x1.perfboard.json`](../../tests/fixtures/project-v1/empty-1x1.perfboard.json).

Milestone 3 adds optional snapshot-based revision fields without changing the
schema version. `working_base` is the design at the last commit (or at the
first edit for an uncommitted legacy document). `undo_stack` and `redo_stack`
contain atomic working-copy changes with before/after design snapshots,
operation payloads, origin, rationale, validation, and a human-readable
summary. They are persisted so undo and redo continue to work after reopen.

Named commits are stored in the `commits` map and ordered by `commit_order`.
Each commit has a stable UUID, optional parent, timestamp in Unix milliseconds,
description, source document revision, full design and validation snapshots,
and the operations included in the commit. `head_commit` identifies the newest
commit. Restoring a commit copies its snapshot into the working copy and does
not move the head or delete later commits.

Document revisions are monotonically increasing compare-and-swap tokens. Every
mutation, transaction, undo, redo, discard, commit, and restore checks the
caller's expected revision. A stale request returns the stable
`revision.stale` transport code together with expected and actual revisions.
Transactions apply to a temporary document and replace the active document
only if every operation succeeds.

Milestone 4 adds optional `constraints`, `jumper_inventory`, and
`crossing_choices` maps. Constraints have a typed payload plus severity,
origin, rationale, and an uninterpreted annotation. Required constraint
violations cannot be accepted through the manual-placement override;
preferred and advisory violations remain visible warnings. Jumper
`span_holes` counts intervals, quantity is informational, and route generation
never consumes it. Crossing choices record the two wire IDs, canonical
half-grid position, and which wire passes over the other.

Route proposals are derived transport values rather than project state. They
contain canonical vertices, side, measurements, crossings, jumper match,
named integer cost breakdown, and soft warnings. Only the explicit
`accept_route_proposal` operation creates or replaces a wire. Its payload
preserves the proposal in working history, and any replacement wire ID and
over/under choice are explicit.
