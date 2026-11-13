# ADR 0012: Assembly plans are derived, ordered by hard dependencies, then by heuristics

- Status: accepted
- Date: 2026-11-13

## Context

Milestone 6 turns a design into a build sequence a hobbyist can follow at the
bench, one step at a time. Several things pull against each other:

- Some orderings are physical necessities. A wire that runs under a module's
  body cannot be laid once the module is soldered in. A bridge that solders
  several leads together needs those leads in their holes first.
- Most orderings are only good practice: low parts before tall ones, so the
  board lies flat on the bench while soldering; component-side wiring before
  parts; solder-side bridges last.
- Treating every crossing as a hard rule produces cycles on ordinary boards.
  Two wires crossing on the same side only have an order when the designer
  has recorded which one goes on top (a `CrossingChoice`).
- The plan is derived from the design and must never be stored in it, or the
  two would drift apart. Progress at the bench is not a design change either:
  ticking a step must not create a revision or dirty the file.
- A design edited midway through a build must not silently invalidate, or
  silently keep, what the builder has already done.

## Decision

1. **The plan is a pure function of the design.**
   `perfboard_project::assembly_plan(&Design, DocumentRevision)` returns an
   `AssemblyPlan` with ordered steps, a wire list and warnings. It is computed
   on request (Tauri `get_assembly_plan`, MCP `get_assembly_plan`) and never
   saved. The same design always yields the same plan, byte for byte.

2. **Steps.** One step per placed component, wire, rail, bridge and saddle,
   followed by check steps: a visual inspection, a continuity check over
   accepted nets with at least two component pins, and a power check when a
   ground net and a supply net can be recognised by name. Step IDs are
   `<kind>:<object id>`, so they survive renumbering and edits elsewhere.
   Check steps carry their measurements as `probes`: the text, the nets
   measured and the pin holes to touch, so a viewer can show each one on the
   board.

3. **Hard dependencies** become graph edges, each with a reason shown to the
   user:
   - a component-side wire or rail whose interior runs inside a component's
     body goes before that component;
   - a component whose body covers another part's pins goes after that part;
   - a bridge goes after the parts with pins in its holes and the wires that
     end in them;
   - a solder-side wire goes after the parts it is soldered to (a
     component-side wire sharing a pin's hole only gets a note, since it can
     go in with the part);
   - a rail goes before the wires soldered to its nodes and the same-side
     wires that cross it;
   - for a recorded crossing choice, the wire underneath goes first;
   - a saddle goes after the same-side wires and rails it holds down.

   Unrecorded same-side crossings are not ordered. They get a note on both
   steps and an `assembly.unordered_crossing` advisory.

4. **Heuristic order breaks ties.** Kahn's algorithm with a priority queue
   places, among the steps whose dependencies are met, the one with the lowest
   `(phase, rank, y, x, label, id)`. Phases: component-side rails, wires and
   saddles; small components (body up to 12 square pitches), smallest first;
   larger components; solder-side rails; solder-side wires; bridges;
   solder-side saddles; checks.

5. **Cycles never block a plan.** When no step is ready, one cycle is
   reported as an `assembly.cycle` warning naming its objects, and the
   lowest-priority remaining step is placed anyway. Other warnings flag
   construction hazards such as 135° bends.

6. **Staleness by fingerprint.** The plan carries the document revision it
   was computed from and a fingerprint (SHA-256 over steps, wire list and
   warnings). `AssemblyPlan::is_current` compares fingerprints, so edits that
   change nothing physical (notes, names that do not appear in steps) keep a
   plan current, while moving or rerouting anything that alters a step makes
   it stale. Each step has its own fingerprint over its content, excluding its
   number and dependency list.

7. **Progress lives outside the project.** Checkmarks are stored by the
   desktop shell in `assembly-progress.json` in the app's configuration
   folder, keyed by project ID and step ID, with the step's fingerprint when
   it was ticked. A mark whose fingerprint still matches is done; one that no
   longer matches is shown as changed since it was done, for the builder to
   check and re-tick. A missing or corrupt file means no progress.

8. **Estimates are labelled as such.** Wire-list lengths are path length plus
   a 5 mm lead per end into its hole (1.5 mm bare tips for rail ends between
   holes), rounded, and presented as approximate cut lengths.

## Consequences

- Plans need no migration: changing the rules changes the plan, and
  fingerprints make checkmarks for altered steps show as changed rather than
  vanish or lie.
- Adding a rule means adding an edge or a priority term with a test in
  `crates/project/tests/assembly.rs`; hard edges must be physical
  necessities, or cycles become common.
- Progress is per machine. Moving a build to another computer starts its
  checklist afresh; the printed checklist covers that case.
- The ordering does not model tool access in three dimensions (an iron
  reaching a joint between tall parts). Body footprints over pins and wires
  are the access rules for now.
