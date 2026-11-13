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
  either side of the board can connect to it. Drag a node with the Select or
  Rail tool to move it: a node in a straight run slides along the rail, while
  ends and corners reshape it. Double-click an unconnected node to remove it,
  with either tool. Nodes with a wire attached stay put, with a message
  saying why. Wire and rail labels no longer block the pointer, so a node
  under a rail's name label can be reached too.
- Nets: rename them by editing the name in the inspector (`rename_net`; names
  must be unique, ignoring case), and remove unused ones from the Nets panel or
  the inspector. A net cannot be removed while wires or rails use it.
- Merge nets: pick **Merge into…** in a net's inspector, or rename a net to
  the name of an existing one (`merge_nets`). Pins, wires, rails, junctions and
  net constraints move to the target net, which keeps its name, and the source
  net is removed. Both nets must be accepted. Undo reverses it.
- A wire started on a bridge end takes the net the bridge is soldered to
  (across chained bridges too) instead of the tool bar's active net. A wire
  from an unconnected terminal adopts the net of the rail or pin it ends on.
- A bridge no longer keeps the net of a wire that has been removed: changing
  the net of a pin next to it works, and the bridge follows. It is only refused
  when the bridge would join two different nets, for example when its other
  end still carries a wire or pin on another net.
- Jumpers: the Dupont Jumper is now a `jumper` part that takes a shunt. Set
  it to Open or Shunted in the inspector (`set_jumper_shunt`). A shunted
  jumper joins its two pins, so a net through it no longer reports "not
  completely routed". A shunt may also join two different nets, as a
  selector jumper does: each net is still routed and checked on its own, the
  validation report notes the join as an advisory, and the net inspector
  lists the nets it is currently joined with.
  Parts placed before this change pick it up with **Update metadata**.
- Recent projects: the last 10 projects opened or saved are listed in the
  File menu and on the empty-board welcome card, each with its folder. Files
  that have been moved or deleted are marked and can be removed from the list
  one by one, or all at once with **Clear**. The list is kept in
  `recent-projects.json` in the app's configuration folder (on Linux,
  `~/.config/dev.perfboarddesigner.desktop/`), so it no longer lives in the
  webview's storage; an existing list is adopted on first start.
- Revisioned working copy with undo, commits, comparison, and restore.
- Deterministic automatic routing with route proposals.
- Validation with problems that frame the affected objects.
- Component Designer for authoring component definitions.
- Local MCP server so Codex and Claude Code can read and edit the open design.
- Printable build drawing with a wire cut list.
- Wire templates as a true-scale PDF (**File → Export wire templates…**). A
  cut guide comes first: every wire straightened at 1:1 in its own column over
  a millimetre grid, with its bare ends, insulation colour, cut lines, strip
  marks, and a triangle with the angle at every bend, so cuts and bends can be
  marked while the wire is still straight off the spool. Bending templates follow: every wire shape drawn at 1:1 over
  the board's hole grid, grouped by insulation colour, with each run
  dimensioned in millimetres and hole pitches. Wires of the same shape share
  one template. Pages are A4 (or US Letter where that is the local paper),
  each with a 50 mm calibration bar and a red reminder to set Page Scaling to
  None in the print dialog, so they print at actual size. See
  [ADR 0011](docs/adr/0011-true-scale-pdf-output.md).
