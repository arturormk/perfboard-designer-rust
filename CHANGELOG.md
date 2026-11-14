# Changelog

Notable changes to Perfboard Designer. Development builds (odd minor
versions, such as `v0.1.26`) are not listed individually; their number is the
commit count. Releases (even minor versions) get a section here. See
[ADR 0010](docs/adr/0010-versioning.md).

## Unreleased

Development line `v0.1.x`: the first public development builds.

- Much smaller project files. Undo and redo history is now saved as JSON
  patches between neighbouring designs instead of full copies, and commits no
  longer copy the undo stack with its snapshots. A 40.7 MB project with three
  commits now saves as 778 KB. This is project format v2
  ([ADR 0013](docs/adr/0013-patch-encoded-history.md)): v1 files open and are
  upgraded on the next save, but builds from before this change cannot open
  v2 files.
- `quickstart.sh` builds the executable or `.deb`, `.rpm` and AppImage
  bundles, and the release workflow uses it. `.deb` and `.rpm` packages also
  install the MCP adapter as `/usr/bin/perfboard-mcp-server`.
- The standard component library is compiled into the application, so
  packages and copied executables always have it, and MCP's
  `list_components`, `get_component_definition` and
  `get_component_documentation` now offer the same parts as the palette
  (before, MCP saw three of them). Debug builds read `components/` from the
  checkout, and `PERFBOARD_STANDARD_COMPONENTS` points any build at another
  folder ([ADR 0014](docs/adr/0014-component-library-layers.md)).
- Project components: a saved project also offers the parts in the folder
  named after it (`board.perfboard/` beside `board.perfboard.json`), ahead of
  the standard library, to both the palette and MCP. The optional
  `component_paths` project field lists other folders under the project's
  folder. The palette labels project parts and flags those that override a
  standard part, and reports library problems below the list.
- The bundled library now holds only general-purpose parts (bridge, Dupont
  jumper and pin, two resistors). The sample project's Nano, RS-485 breakouts
  and RJ45 connector moved to `examples/nano-rs485.perfboard/` and were
  renamed from `dev.perfboard.*` to `local.*`, a prefix the library now warns
  about outside the standard library. Projects that placed them keep working
  from their embedded snapshots.
- User component libraries: File → Component libraries… lists the folders a
  project takes new parts from, in search order, and lets you add, reorder
  and remove them. A folder of your own parts (`${USER_COMPONENTS}`) can be
  granted to each project; grants are kept on your computer, never in the
  project, so a project you send to someone else carries none. File →
  Preferences… lists every grant. Library problems, such as a user library
  that is not granted, appear in the Problems panel and in MCP's validation
  report.
- The Part designer opens on a list of the project's parts, layer by layer.
  Project and user-library parts open for editing; a standard part is copied
  into the project's folder or the user library first (the copy keeps its
  `id`, so it overrides the standard part for that project). "New part here"
  creates a part in the project's folder or user library, and opening a
  folder that holds several parts lists them instead of reporting an error.
  The designer asks before discarding unsaved changes.
- Placed parts are named by reference designator: a component definition's
  optional `designator` (such as `R`) names new parts `R1`, `R2`, … using the
  lowest free number, in the UI and for MCP agents that leave the name empty.
  The bundled parts have designators (R, W, JP, J, U), and the Part designer
  edits it. Parts already placed keep their names. Placing a part whose
  library definition changed only in name, designator, description,
  documentation or version now refreshes the project's copy instead of
  failing.
- Alternative routes: a suggested route now comes with up to four
  alternatives, shown as tabs on the route card (keys 1–5): a different path
  that keeps off the balanced route, fewest bends, shortest, and the other
  board side, without repeats. MCP's `suggest_routes` returns them with
  `alternatives: true`.
- 41 new standard parts: a standing resistor; ceramic capacitors (2.54 and
  5.08 mm); radial electrolytics (5, 6.3, 8 and 10 mm); DO-35 and DO-41
  diodes; 3 and 5 mm LEDs; TO-92 and standing TO-220; DIP-8 to DIP-28 and
  wide DIP-28 and DIP-40; male and female headers 1x2 to 1x8, 2x3 and 2x5;
  and 2- and 3-position screw terminals. Families are generated from one
  description, so every size follows the same rules.
- [Component libraries](docs/component-libraries.md): a guide to where parts
  live, user-library grants, making and changing parts, identifiers and
  designators, and library problems.
- Delete a part from the project's folder or the user library with the trash
  button in the Part designer's list. The folder goes to the system trash, so
  it can be restored. Placed parts keep working from the definition stored
  in the project, and a standard part the deleted one overrode is offered
  again.
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
- Projects can be renamed: with nothing selected, edit the project name at the
  top of the inspector. Enter or leaving the field saves it, Escape cancels,
  and blank names are refused. The name is saved with the file; a rename is
  not an undo step. The header now shows only the file name. Agents can
  rename it too, with the `rename_project` MCP tool.
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
  Its reads (`get_project`, `get_design`, `get_revision_history` and the
  others) no longer send every undo and commit snapshot, so they keep working
  on projects with a long history; before, a project past 16 MiB made them
  fail with "unexpected end of file". `get_project` and `get_revision_history`
  now list working changes as summaries with the objects they touched. A
  response that is still too large is reported as `session.response_too_large`.
- Printable build drawing (**File → Print build drawing…**): both board
  faces, the wire list, the parts, and the build order. The wire list now
  comes from the assembly plan: wires and rails in the order they are
  installed, with the step number, an estimated cut length (path plus 5 mm
  leads, labelled ≈) next to the routed path length, and a suggested
  inventory jumper where one fits. The build order starts on a new page:
  every step with a box to tick, its instructions and notes, the board side
  and the steps it comes after. Check steps list each measurement with its
  own box. Steps already ticked in the app print ticked. If the plan cannot
  be derived, the sheet prints the previous wire table by label.
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
- Rails in the wire templates: each rail gets a column in the cut guide and
  a 1:1 bending template, after the wires. Bends are marked as for wires.
  Rail ends on a hole get a 5 mm lead bent down into it, like a wire's, so
  a rail used only at its taps can have its ends clipped while installing.
  An end between holes gets a 1.5 mm bare tip instead. Connection points
  (every node in a straight run, any corner a wire is attached to, and ends
  between holes) are drawn as tan bare windows, 3 mm long and centred on
  the node, with lines where the insulation is sliced. Windows that overlap
  merge into one. Templates dimension every run between nodes, and
  consecutive short dimensions are staggered so their labels stay readable.
- Assembly plans: a step-by-step build order derived from the design, with
  one step per part, wire, rail, bridge and saddle, then inspection,
  continuity and power checks. Wires under a part's body come before the part,
  bridges after the leads they join, the lower wire of a recorded crossing
  first, and otherwise low parts before tall ones. Each step says why it comes
  after others. Cycles and unordered crossings are reported as warnings
  instead of blocking the plan. A wire list gives endpoints, net, colour,
  side, approximate cut length and bends. The plan records the revision it
  came from and goes stale when an edit changes what a step asks for.
  Available to MCP clients as `get_assembly_plan`. See
  [ADR 0012](docs/adr/0012-assembly-planning.md).
- Assembly mode (**Assembly** tab): build the board one step at a time. The
  board shows what is installed so far, with earlier steps faded, the
  current step highlighted, its holes ringed, and the face it is done on
  (switchable). Beside it: the step's instructions, notes, the steps it
  comes after (click one to go there) and, for wires and rails, colour, net,
  approximate cut length and bends. **Done, next** (Enter) ticks the step
  and moves on; ← and → move between steps; Space ticks or unticks. The step
  list shows progress, and notes about the order can be expanded. Checkmarks
  are kept in `assembly-progress.json` in the app's configuration folder,
  not in the project, so ticking a step never changes the design. When the
  design changes, the plan is updated and a banner says how many steps were
  added, changed or removed; a step ticked before it changed is marked with
  **!** until it is checked and ticked again.
- Check steps list their measurements one by one: each net to test for
  continuity, and each supply-to-ground pair to test for shorts, with an
  example pair of pins. Click a measurement to ring its test points on the
  board and light up that net's wires and rails; click it again to clear.
  MCP clients get the same data as `probes` on the step.
