# Project format v2

Status: implemented. Current writers emit version 2; readers also accept
[version 1](project-v1.md) and upgrade it on load.

Version 2 is [version 1](project-v1.md) with a compact encoding of revision
history ([ADR 0013](../adr/0013-patch-encoded-history.md)). The design,
validation cache, commit map, commit order and head commit are unchanged.
Only the following differ.

## Commit changes

`commits.<id>.changes` lists the changes the commit absorbed as records of
`operation_id`, `origin`, optional `rationale`, `summary` and `payloads`.
They carry no `before`/`after` snapshots or validation reports; the commit's
`design` and `validation` describe its result. Version 1 readers stored full
undo entries here; version 2 readers ignore their snapshots.

## Undo and redo stacks

Entries in `undo_stack` and `redo_stack` have the same metadata fields as
commit changes plus two optional
[RFC 6902](https://www.rfc-editor.org/rfc/rfc6902) JSON patches,
`before_patch` and `after_patch`. An absent patch is the empty patch. The
patches are applied to the JSON form of a design, starting from the
document's `design`:

- `undo_stack` is processed from its last (newest) entry to its first. For
  each entry, apply `after_patch` to the running design to get the entry's
  `after` snapshot, then `before_patch` to that to get its `before` snapshot,
  which becomes the running design for the next entry.
- `redo_stack` is processed from its last entry (the next one to redo) to its
  first. For each entry, apply `before_patch` to the running design to get
  `before`, then `after_patch` to get `after`, which becomes the running
  design.

When the history is contiguous, an undo entry's `after` equals the running
design and a redo entry's `before` does too, so writers omit that patch.
Validation reports for the snapshots are not stored; readers recompute them.

## Working base

`working_base` is replaced by `working_base_patch`: a JSON patch from
`design` to the design at the last commit (or at the first edit of an
uncommitted document). It is absent when there is no working base and an
empty array when the working copy is unchanged.

## Component paths

The optional top-level `component_paths` is an array of strings naming the
folders that new placements take components from, in priority order
([ADR 0014](../adr/0014-component-library-layers.md)). It sits beside `name`,
outside `design`, so it is not part of revision history.

```json
"component_paths": [
  "${PROJECT_DIR}/${PROJECT_STEM}.perfboard",
  "${USER_COMPONENTS}/rs485"
]
```

- Each entry starts with `${PROJECT_DIR}` (the folder holding the project
  file) or `${USER_COMPONENTS}` (the user library granted to the project on
  this machine), optionally followed by `/`-separated segments.
  `${PROJECT_STEM}`, the file name without `.perfboard.json`, may appear in
  those segments. `..`, `.`, empty segments, backslashes and other variables
  are not allowed.
- Absent means `["${PROJECT_DIR}/${PROJECT_STEM}.perfboard"]`. An empty array
  means no project layers. The standard library always follows the listed
  layers.
- Readers load a document with malformed entries and report them as library
  diagnostics; a bad entry never makes a project unopenable.

The field was added after version 2 was introduced and needs no version
change: readers that predate it ignore it, and drop it when they save.

## Review notes

The optional top-level `review_notes` is an array of findings about the
project, usually written by an agent, in the order the user keeps them
([ADR 0015](../adr/0015-review-notes.md)). Like `component_paths` it sits
outside `design`, so it is not part of revision history or commits. It is
omitted when empty.

```json
"review_notes": [
  {
    "id": "6f0c2f9e-3b7e-4f5e-9d61-0a8f3c1d2b47",
    "severity": "critical",
    "title": "RS-485 A and B are swapped at the RJ45",
    "detail": "Swap the wires on pins 4 and 5.",
    "objects": [{ "kind": "component", "id": "…" }],
    "created_ms": 1794614400000,
    "revision": 41,
    "status": "review_requested",
    "reply": "Both ends use pin 4 for B; see the bus drawing."
  }
]
```

- `severity` is `critical`, `warning`, `suggestion` or `info`.
- Array order is the display order. Editors add a note in front of the first
  note that is less severe (or at the end), so the list starts in severity
  order; the user and agents can then move notes anywhere.
- `revision` is the document `revision` when the note was added: the design
  it describes. Adding, removing or reordering notes does not change the
  document revision. It is optional, for notes written before it was recorded.
- `status` is `acknowledged` or `review_requested`, and is omitted while the
  note is `open`. `reply` is the user's answer and `response` the agent's,
  each plain text up to 2000 characters and omitted when empty
  ([ADR 0016](../adr/0016-review-note-replies.md)).
- `detail` and `objects` are optional. `objects` uses the diagnostics'
  `{ kind, id }` references; a reference to an object that no longer exists
  is kept and shown as missing.

The field was added after version 2 was introduced and needs no version
change: readers that predate it ignore it, and drop it when they save.

## Upgrading version 1

A version 1 document is read with its full snapshots, its undo/redo
validation reports are recomputed, `schema_version` becomes 2, and the next
save writes the encoding above. Versions other than 1 and 2 are rejected with
the unsupported-schema error.
