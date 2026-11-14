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

## Upgrading version 1

A version 1 document is read with its full snapshots, its undo/redo
validation reports are recomputed, `schema_version` becomes 2, and the next
save writes the encoding above. Versions other than 1 and 2 are rejected with
the unsupported-schema error.
