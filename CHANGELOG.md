# Changelog

Notable changes to Perfboard Designer. Development builds (odd minor
versions, such as `v0.1.26`) are not listed individually; their number is the
commit count. Releases (even minor versions) get a section here. See
[ADR 0010](docs/adr/0010-versioning.md).

## Unreleased

Development line `v0.1.x`: the first public development builds.

- Graphical perfboard editor with component and solder side views, parts,
  nets, wires, bridges, and rails.
- Saddles: net-free wire ties that hold wires and bundles to the board through
  two free holes (`S`).
- Revisioned working copy with undo, commits, comparison, and restore.
- Deterministic automatic routing with route proposals.
- Validation with problems that frame the affected objects.
- Component Designer for authoring component definitions.
- Local MCP server so Codex and Claude Code can read and edit the open design.
- Printable build drawing with a wire cut list.
