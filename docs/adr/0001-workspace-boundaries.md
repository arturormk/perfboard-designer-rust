# ADR 0001: Workspace and dependency boundaries

- Status: accepted
- Date: 2026-10-22

## Decision

Use one Cargo workspace with `perfboard-core`, `perfboard-project`, the future
`perfboard-mcp-server`, and the Tauri package. Dependencies point from core to
project to adapters. Core owns deterministic domain contracts and has no
filesystem, Tauri, frontend, or MCP dependency. Project owns documents and,
later, operations, persistence, history, and sessions. Adapters translate at
the boundary and do not own design rules.

## Consequences

The desktop application can be compiled and tested alongside the domain model,
while core tests remain independent of platform APIs. The empty MCP executable
makes its future dependency direction explicit without prematurely choosing an
SDK or transport.
