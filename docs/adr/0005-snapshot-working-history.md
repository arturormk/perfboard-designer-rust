# ADR 0005: Snapshot-based working-copy history

Status: accepted; persistence amended by [ADR 0013](0013-patch-encoded-history.md)
Date: 2026-10-23

## Context

Milestone 3 requires atomic transactions, undo/redo that survives project
reopen, full commit snapshots, and non-destructive restore. The current design
is small, but several operations have domain-specific validation and inverse
operations would duplicate that logic.

## Decision

Store before/after `Design` snapshots for each atomic working-copy change.
Persist the undo and redo stacks in the project document. Transactions produce
one history entry even when they contain several operation payloads. Named
commits store a full design snapshot and the operation metadata being committed.

The active `ProjectSession` exclusively owns the document. Adapters call its
compare-and-swap methods and publish the resulting change event. Restoring an
old commit creates a working-copy change while leaving `head_commit` and every
descendant intact.

## Consequences

- Undo, redo, discard, transaction rollback, and persistence are simple and
  deterministic.
- Operation origin and rationale remain inspectable without reconstructing an
  inverse operation stream.
- Project files grow with edit history. This is acceptable for the intended
  board sizes; compact history can be introduced later behind the same API.
