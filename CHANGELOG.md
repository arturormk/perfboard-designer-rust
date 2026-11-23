# Changelog

Notable changes to Perfboard Designer. Development builds (odd minor
versions, such as `v0.1.26`) are not listed individually; their number is the
commit count. Releases (even minor versions) get a section here. See
[ADR 0010](docs/adr/0010-versioning.md).

## Unreleased

Development line `v0.1.x`: the first public development builds.

- Multimeter checks with expected readings (ADR 0022). The assembly plan
  has a new "Check resistances" step: each resistor measured in place, with
  the reading and range its tolerance allows, such as "R3 (4.7 kΩ): expect
  4.7 kΩ (4.65–4.75 kΩ)". Where other parts change the reading, it names
  them. Where a diode, transistor or IC could change it, the reading is not
  predicted and a note says why. The supply-to-ground short check gives the
  expected resistance too, when the design proves it. The Breadboard view
  gets a Checks section of its own: continuity measured at the parts' leads
  (which catches a lead in the wrong row or not pushed home), neighbouring
  rows with different nets that must read open, each rail end to end (for
  boards whose rails break at the middle), resistances, and shorts. Pick a
  measurement to ring its test points on the board. Agents see the same
  checks in `get_assembly_plan` and `get_breadboard_layout`.

- The Breadboard view turns a part so its pins face their nets' rails. A
  diode between two supplies, each with a rail down one side, used to go
  down the wrong way round, with a jumper across the whole board from each
  end. Now each end takes a short jumper to the rail on its own side. A part
  with a cable end, such as a Nano's USB, keeps its turn.

- A part's lead that the perfboard joins to a net without the net listing
  it (it shares a hole with a bridge end or another pin of the net, sits at
  the far end of a run of bridges, or lies under a rail) is now wired in on
  the Breadboard. Before, it was left unconnected there while soldered in on
  the perfboard. The board also warns about such a lead: add it to the net,
  or move the part or bridge so they no longer touch.

- Each resistor can show a 4-band (5%) or 5-band (1%) colour code, to match
  the parts in hand: choose it under the value in the inspector. Five stays
  the default. The board, Assembly and Breadboard views, the colour-code card
  and the assembly plan's "Check the colour bands" line all follow it, and
  the parts list keeps 4- and 5-band parts of one value apart. A value that
  needs three digits (such as 4.99 kΩ) has no 4-band code and is shown as
  text. Agents set it with the `set_resistor_bands` operation.

- On the solder side, a pin whose lead is laid across component-side
  bridges no longer shows a joint in its own hole, since the lead does not go
  in there. The joint is drawn instead at the far end of the run, where the
  lead goes through the board.

- A rail on the solder side may run over pins of its own net, such as a GND
  rail laid across a row of GND pins in place of a chain of bridges. It is
  joined to every pin it passes over (and to bridges chained from them), so
  those pins need no wire to it. Drawing or reshaping it adds a node on
  each pin it passes, so its template has a bare window to solder there; a
  pin left under a rail without a node (such as a part moved there) is
  reported. Its assembly step comes after the parts
  whose pins it covers and the wires that end in those holes (a wire ending
  on one of its nodes on a whole hole is soldered first and the rail laid
  over it), and names the pins to solder it to. A rail over a pin of another
  net, a part pin on no net, or a bridge that reaches another net is still
  an error, and so is a rail on the component side over any pin.

- Rails are labelled `BB1`, `BB2`… (bus bar) instead of `R1`, `R2`…, which
  read as resistors. `B` stays with bridges.

- The project has a description: what the board is meant to do. With
  nothing selected, the inspector shows it as a text box under the project
  name; it saves when the box loses focus and Escape restores it. It is saved
  with the file (an optional top-level `description`). Agents see it in
  `get_project` and `get_design`, and can change it with the new
  `set_project_description` MCP tool, so the design intent no longer has to
  be repeated in every prompt. Like the name, it is revision-checked but not
  an undo step.

- Diodes are drawn with their cathode band and electrolytic capacitors with
  a `+` by the positive lead, on every drawing: the editor (both sides, faint
  on the side opposite the part), assembly, breadboard, printed sheets, the
  component designer preview and the agent's rendered design. The mark comes
  from the pin IDs (`cathode`/`anode`, `positive`/`negative`), so library
  parts that follow that naming get it too; it replaces the corner notch on
  those parts.

- A part whose body stops short of its pins, such as a diode, is drawn with
  a lead from each end of the body out to the rim of its pad, so the pins
  read as the part's own. The lead takes the outline's colour (orange when
  the part is selected), on the board, the component designer preview and
  the agent's rendered design.

- A part's pins can be renamed for that part only: click a name in the
  inspector's **Pins** table and type, such as `TRIG` for pin 2 of one 555
  while another 555 keeps its names. A renamed pin shows in copper with a
  button to go back to the definition's name; clearing the field does the
  same. The new name shows on board pin labels, in the pin hover card (with
  the definition's name beside it), net pin lists, the status bar, the
  assembly plan and exported drawings. Agents set it with `set_pin_label`.
  Labels are saved on the part as `pin_labels`, only when it has some.

- A new net can no longer take the name of an existing one (case and
  surrounding spaces are ignored, as when renaming). Before, defining a net
  twice made a second, empty net with the same name, which was then reported
  as having fewer than two pins while the real net looked complete.

- Bridges can be made on the component side: a part's lead bent flat across
  the holes instead of a solder-side link. Use the bridge tool (**B**) while
  viewing the component side, or switch an existing bridge under **Made on**
  in the inspector. They are drawn thin and gray, like a lead, still conduct
  and take up the holes they cover (a pin in a hole the lead passes over is
  an error). The bridge tool no longer flips to the solder side; it places
  on the face you are viewing. In the assembly plan a run of component-side
  bridges has no step of its own: the step of the part with a pin at its
  start says to leave that lead long, bend it over the holes and push it
  through the last one, and a solder-side bridge continuing from there comes
  after. A run with no part at either end gets its own step and a warning.

- The solder side can be mirrored (**M**, or the tag under the side switch).
  In the mirror view each part and wire stays where it is on the component
  side, so you can switch faces without hunting for the matching spot, and
  every label reads backwards so the view cannot be mistaken for the real
  underside. The usual solder view is now labelled "from below" rather than
  "mirrored", since it shows the board as it looks when turned over.

- Hovering over a bridge end that shares its hole with a part's pin shows that
  pin in the tooltip and status bar (its number, name, part and net), not
  the bridge end.

- The parts library keeps its search text and scroll position when you switch
  to Nets or History, hide the left panel or change modes, so placing several
  parts of the same kind no longer means finding it again each time.

- Resistor and ceramic capacitor leads are named **1** and **2** instead of
  "Terminal 1" and "Terminal 2", so pin labels on the board stay short. The
  standing resistor and both ceramic capacitors move to version 1.2.0, the
  3- and 4-hole resistors to 1.3.0. Pin ids are unchanged, so existing
  connections still hold.

- Parts can be placed, dragged and nudged off the board on any side, not only
  to the right and below, so a new part can be parked beside the board and
  rotated before it goes on. Hole coordinates are now signed (ADR 0021);
  positions left of or above the board are negative. Existing projects open
  unchanged, but a project with a part parked left of or above the board
  will not open in older builds.

- The Notes tab has two lists: **Notes**, the agent's review notes, and
  **Issues**, notes you keep across reviews. **Keep as issue** on a note
  moves it to Issues with its reply and the agent's answer; **Back to
  Notes** returns it. Agents see issues in `get_review_notes` but cannot
  change, move or remove them, so a fresh review cannot clear what you
  chose to keep. Issues are saved with the project.

- Bridges are named B1, B2… (definition version 1.2.0) instead of W1, W2…,
  which clashed with the numbering of wires. Bridges already placed keep
  their names; rename them in the inspector if you like.

- Screw terminals cover the hole beyond each end pin again (definition
  version 1.2.0): seen along the row a block takes X P X P X, so two blocks
  butted together keep the 5.08 mm pitch without sharing a hole. New 4-, 6-,
  7- and 8-position blocks join the 2-, 3- and 5-position ones; from 4
  positions up, a part also stands for a run of smaller blocks butted
  together, as its description explains. Projects keep the definitions they
  embed until you update them.

- Parts have a value. Resistors and capacitors understand theirs: `4k7`,
  `4.7k` and `4700` all become `4.7 kΩ`, `0.1u` and the code `104` become
  `100 nF`, and a value that cannot be read (such as `2m2` for 2.2 MΩ) is
  refused with a hint; other parts take free text such as `1N4148`. Set it in
  the inspector, or with `set_component_value` from MCP. **View → Part
  values** shows values on the board (off by default). Breadboard draws
  resistors beige with their five-band colour code at 1% tolerance, and so
  does Assembly for the resistor being fitted; hovering a resistor on the
  breadboard shows its code. A colour-code card, with each band named, appears in the
  inspector when a resistor is selected and beside the instructions when one
  is fitted in Assembly; the Breadboard panel can list every resistor's code
  (**Resistor colour codes**). Each part step says which bands to look for, a new parts list
  groups parts by type and value, and a note names resistors and capacitors
  without one. The built-in resistor and capacitor parts declare their value
  kind (`value_kind` in `component.toml`); projects pick it up when a part's
  definition is updated. See
  [ADR 0019](docs/adr/0019-component-values.md).

- A new Breadboard tab lays the design out on standard solderless
  breadboards (protoboards), so the circuit can be tried before anything is
  soldered. Parts straddle the channel or sit on one side with their own
  rows; ground and supply take the rails; every connection is a numbered
  Dupont jumper in the colour of the net's wires. More boards are added side
  by side when one is not tall enough, and parts that cannot sit on a board,
  such as 2×N headers, go beside it with male–female leads. Required access
  areas, such as a Nano's USB end, are drawn with their label and kept clear
  of parts and jumpers. Each side's rails are separate copper, so a jumper
  just below the parts joins a net's rails, and the Rails list names it. The
  view is read-only with pan and zoom, follows every design change, and checks its
  own connectivity against the nets. Available to MCP clients as
  `get_breadboard_layout`. See
  [ADR 0018](docs/adr/0018-breadboard-layout.md).

- While drawing a wire, each click adds one straight segment from the last
  point to the click, at any angle, so diagonal runs can be drawn directly.
  Before, every click was split into a horizontal and a vertical segment.

- Click again on the same spot to select the next object down when parts,
  wires or rails overlap, for example a resistor fitted under a module.
  Pressing on the selected object and dragging moves it, even where
  something else covers it. Airwires no longer catch clicks meant for what
  lies under them, except on the dashed line itself, which still asks for a
  suggested route.

- Bridges can be deleted once no wire ends on them. A bridge end joins a net
  when a wire is drawn to it and stayed listed there after the wire was
  gone, which blocked deleting the bridge with "object is still referenced
  by circuit or routing data". Its ends now leave their nets with it. A
  bridge that still has a wire is refused with a message that says so.

- A pin, bridge end or rail node under an airwire now takes the click, so you
  can start or end a wire there. Before, the airwire drawn over it took the
  click and asked for route suggestions instead.

- Wires drawn on the component side can end on a bridge. Bridge ends now
  show there as dashed through-hole points, like rail nodes on the other
  face. Before, clicking a bridge end on the component side only added a
  bend to the wire.

- An Arduino Nano (30 pins, USB side marked) is now in the standard library
  as `dev.perfboard.arduino-nano`.

- The Part designer's part list scrolls again. Its library sections were
  squashed to fit the window, which clipped their text (the Project section
  most visibly) and left nothing to scroll.

- Screw terminals: a new 5-position block joins the 2- and 3-position ones.
  All three have measured bodies (definition version 1.1.0): half a hole
  past the end pins, so the holes beyond them stay free, and a whole hole in
  front and behind. Wires still enter from the marked south side.

- The Welcome card over an empty board closes once you create or open a
  project, and has a close button. Before, it stayed over every empty
  board, including one you had just made.

- Answer review notes in the editor. Each note has Acknowledge and Request
  review buttons and room for a short reply, so you can accept findings,
  dispute others, and then ask the agent to look again at the ones you sent
  back (the Notes tab has a request ready to copy into the agent's chat).
  Agents find those with `get_review_notes` filtered by
  `status: review_requested` and answer with the new `update_review_note`
  tool. The answer shows under your reply, and the tool can also revise the
  note. A toast tells you when the agent has answered
  ([ADR 0016](docs/adr/0016-review-note-replies.md)).

- Notes on wires, rails, nets and saddles can now be edited, in the
  inspector and by agents through the new `set_wire_notes`, `set_rail_notes`
  and `set_net_notes` operations (saddles use `update_saddle`). Agents can
  also edit junction notes with `set_junction_notes`; the editor does not
  show junctions. Before, notes could only be given when the object was
  created, and the inspector showed them read-only, or not at all for rails
  and saddles. Editing notes is an undo step, and works on locked wires and
  rails.

- Closing the window with unsaved changes now asks whether to save them
  (Save, Don't save, Cancel), and so do opening another project and starting
  a new board. Unsaved review notes and an unsaved part in the Part designer
  count. Before, closing discarded them without a word. The unsaved-changes
  dot next to the file name now also covers review notes and projects that
  were never saved.

- An application icon: a perfboard with a green jumper, after the top bar's
  logo. It now shows on the window in the dock and Alt+Tab on Linux, where
  the previous placeholder never appeared because the window icon was too
  large for X11 (see `src-tauri/icons/README.md`). Packages get it in all
  sizes, plus `.ico` and `.icns`.

- Review notes: agents can leave findings in the project with the new
  `add_review_notes` MCP tool, each marked critical, warning, suggestion or
  info, optionally pointing at parts, nets or wires. They appear in a new
  Notes tab beside Inspect and Problems (History moved to the left panel),
  where you delete them as you address them (Undo is in the toast) and drag
  them, or use the arrow keys on the grip, to reorder them. New notes are
  placed by severity: each goes in front of the first less severe note, so a
  new critical note appears before the first warning, without re-sorting an
  order you chose. Clicking a part chip shows it on the board. Each note
  shows the revision it describes and says when the design has changed
  since; adding notes does not change the revision. Notes are saved in the project file (`review_notes`) but are not
  undo steps or part of commits. Agents can also read them with
  `get_review_notes`, reorder them with `move_review_note` and remove them
  with `remove_review_notes`
  ([ADR 0015](docs/adr/0015-review-notes.md)).

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
