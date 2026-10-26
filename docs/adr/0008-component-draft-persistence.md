# ADR 0008: Component draft persistence

## Status

Accepted for milestone CE3.

## Decision

Component authoring uses an in-memory Rust draft with typed operations and its
own undo/redo history. A draft may be invalid so that intermediate geometry is
visible, but every operation is normalized and validated by Rust. Duplicate
pin identities and numbers are rejected at the operation boundary.

`component.toml` is serialized deterministically with `toml` from the
authoritative component model. Saving requires the content revision captured
when the draft was opened (or last saved) to still match disk. A mismatch is a
conflict and never overwrites either file.

For replacement, the existing file is copied to `component.toml.bak`, the new
content is written to a sibling temporary file, flushed and reparsed, and the
temporary file is renamed over `component.toml`. Documentation follows the
same procedure with `README.md.bak`. One recoverable backup per file is kept.
The definition version remains an explicitly authored field; a save updates
the canonical content hash but never invents a version.

## Consequences

- Draft history is independent from project history and cannot mutate an open
  project.
- Invalid drafts can be inspected and corrected but cannot be saved.
- External editor changes are detected at save time even if watcher events are
  delayed.
- TOML comments and hand formatting are normalized on save; backups retain the
  immediately preceding bytes for recovery and review.
