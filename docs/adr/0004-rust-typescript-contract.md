# ADR 0004: Rust-to-TypeScript boundary contracts

- Status: accepted for bootstrap; revisit in Milestone 1
- Date: 2026-10-22

## Decision

For Milestone 0, keep transport DTOs deliberately tiny and contract-test every
command at both sides of the boundary. The smoke command returns the project
schema version from the Rust project crate, and the TypeScript test verifies
the exact command name and response shape.

Before Milestone 1 adds substantial DTOs, evaluate generated bindings using a
small placement-operation prototype. Adopt generation if it keeps serialized
names, tagged enums, and error shapes stable without leaking internal domain
types. Manual duplication of domain rules is not permitted.

## Consequences

The bootstrap avoids committing to a generator before representative types
exist, while leaving a concrete checkpoint that prevents long-term DTO drift.
