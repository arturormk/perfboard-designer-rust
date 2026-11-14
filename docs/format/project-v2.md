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

## Upgrading version 1

A version 1 document is read with its full snapshots, its undo/redo
validation reports are recomputed, `schema_version` becomes 2, and the next
save writes the encoding above. Versions other than 1 and 2 are rejected with
the unsupported-schema error.
