# ADR 0002: Desktop and frontend tooling

- Status: accepted (framework clause amended by ADR 0009)
- Date: 2026-10-22

## Decision

Use Tauri 2, Vite, strict TypeScript, and framework-free DOM/SVG code for the
bootstrap. Use Vitest for frontend unit tests, ESLint for linting, and Prettier
for deterministic formatting. Pin major versions in lockfiles and let routine
dependency maintenance update compatible releases.

## Consequences

The UI starts with a small dependency and conceptual footprint. A framework may
be introduced by a later ADR if editor state synchronization demonstrates a
concrete need. The production frontend build type-checks before bundling.
