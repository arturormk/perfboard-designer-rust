# ADR 0003: Identifiers, coordinates, and schema envelope

- Status: accepted
- Date: 2026-10-22

## Decision

Use distinct Rust newtypes around UUIDs for project, definition, instance, pin,
net, wire, constraint, operation, and commit identities. Serialize them as UUID
strings. Use unsigned integer hole coordinates and signed integer half-grid
route coordinates. Use a four-value rotation enum. Store canonical
component-side coordinates and mirror the horizontal axis around the board's
hole extents only for solder-side presentation.

Project JSON begins with integer `schema_version: 1`; readers validate the
version and invariants after deserialization. Deterministic snapshots are
pretty-printed JSON with ordered maps and one final newline.

## Consequences

IDs cannot be accidentally mixed in Rust. Geometric topology avoids floating
point comparisons, while physical measurements remain real-valued derived
data. The envelope supports explicit future migrations and readable diffs.
