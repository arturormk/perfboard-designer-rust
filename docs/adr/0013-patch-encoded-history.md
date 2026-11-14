# ADR 0013: Patch-encoded history in the project file

- Status: accepted
- Date: 2026-11-14
- Amends: [ADR 0005](0005-snapshot-working-history.md)

## Context

ADR 0005 stores before/after design snapshots for every working-copy change
and accepted that "project files grow with edit history". In practice they
grew far faster than expected. A real 25×18 project with a 47 KB design
reached 40.7 MB after three commits and a few hundred edits:

- every commit copied the whole undo stack, snapshots included (about 14 MB
  of the 18 MB of compact JSON), although only each change's origin,
  rationale and summary were ever read from it;
- every undo entry held two full designs plus two validation reports, and
  each entry's `after` repeated the next entry's `before`;
- pretty-printing roughly doubled all of it.

Compressed with `xz`, the same file is 36 KB: almost all of it is repetition.

Splitting history into a sidecar file was considered and rejected. It makes
nothing smaller, a sidecar is easily lost when a project is emailed, copied or
committed, and saving two files atomically is harder than one. The MCP adapter
is not affected either way: it reaches projects only through the desktop
session (ADR 0007), never the file.

## Decision

Keep ADR 0005's in-memory model (full snapshots, so undo, redo, discard and
restore stay trivial) and change only the serialized form, which becomes
project schema version 2:

- A commit's `changes` record each change's operation ID, origin, rationale,
  summary and payloads (`CommittedChange`), without snapshots. The commit's
  own `design` is the result of all of them.
- Undo and redo entries store their snapshots as RFC 6902 JSON patches
  (`before_patch`, `after_patch`), chained from the current design: the undo
  stack from the newest entry back, the redo stack from the next entry to
  redo onward. In a contiguous history only the patch that crosses each
  change is non-empty; empty patches are omitted. Arbitrary stacks still
  round-trip exactly.
- Validation reports inside undo/redo entries are not stored. They are
  deterministic and recomputed on load, where they were already being checked.
- The working base is stored as `working_base_patch`, a patch from the
  current design.
- Commits still store their full design and validation.

The encoding is implemented as `Serialize`/`Deserialize` for
`ProjectDocument`, so the session transport (`GetProject`, Tauri
`get_project`) also carries the compact form. Version 1 files are read and
upgraded in memory; they are written back as version 2. Older builds refuse
version 2 files with the unsupported-schema error rather than misreading them.

## Consequences

- The 40.7 MB project above saves as 778 KB and reloads identical to the
  in-memory document. Its undo stack is now 51 KB instead of 4 MB.
- Saving diffs neighbouring snapshots and loading applies patches and
  recomputes validation; for that project both take well under a second.
- Memory use is unchanged. If it becomes a problem, the in-memory stacks can
  adopt the same encoding behind the existing API.
- Commit snapshots still grow linearly with the number of commits. Encoding
  each commit against its parent is the next step if that matters.
