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
- Rails: change their colour in the inspector (`set_rail_color`), add
  connection nodes by double-clicking a rail or clicking it with the Wire tool,
  and remove unused ones. A node on a whole hole acts like a pin, so wires on
  either side of the board can connect to it.
- A wire started on a bridge end takes the net the bridge is soldered to
  (across chained bridges too) instead of the tool bar's active net. A wire
  from an unconnected terminal adopts the net of the rail or pin it ends on.
- A bridge no longer keeps the net of a wire that has been removed: changing
  the net of a pin next to it works, and the bridge follows. It is only refused
  when the bridge would join two different nets, for example when its other
  end still carries a wire or pin on another net.
- Revisioned working copy with undo, commits, comparison, and restore.
- Deterministic automatic routing with route proposals.
- Validation with problems that frame the affected objects.
- Component Designer for authoring component definitions.
- Local MCP server so Codex and Claude Code can read and edit the open design.
- Printable build drawing with a wire cut list.
