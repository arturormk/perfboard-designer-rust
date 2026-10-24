# ADR 0006: Deterministic half-grid routing policy

Status: accepted (2026-10-23)

## Context

Milestone 4 needs reproducible route proposals that respect required constraints without changing accepted routes. The router must remain practical for the version 1 project size and must explain failure.

## Decision

- Search uses deterministic A* on half-grid coordinates with eight fixed-order directions.
- Search state contains position, board side, incoming direction, and capped distance since the last bend. Capping keeps the state space finite because values beyond the applicable spacing threshold are equivalent.
- Required reserved areas and bend spacing exclude states. Preferred and advisory rules add named costs or warnings.
- Cost configuration is one serializable Rust type. Integer search costs avoid platform-dependent ordering, while physical length remains a derived `f64` measurement.
- Existing wires remain accepted state. Suggestion and evaluation are read-only; accepting a proposal is an explicit revision-checked operation and replacement requires a wire ID.
- Search is bounded at 250,000 visited states by default. A failed search returns the enforced exclusions and visited-state count.
- Jumper spans count board intervals. Exact straight matches are preferred over tolerance-bound approximate or reshaped matches, and inventory quantities are never decremented.

## Consequences

Identical design, request, and configuration values serialize to identical proposals. Cost weights remain policy and can change without changing project topology. Version 1 wires remain single-sided; the search state includes side so later explicit through-hole route segments can extend the algorithm without replacing its ordering contract.
