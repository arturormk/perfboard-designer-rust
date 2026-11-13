# Perfboard Designer development plan

Status: active
Source of truth: [`docs/BLUEPRINT.md`](../BLUEPRINT.md)  
Scope: version 1, implemented as a sequence of demonstrable vertical slices

## 1. Outcome and delivery strategy

Version 1 will be a local-first Tauri desktop application in which a hobbyist can place simple through-hole components on an isolated-pad perfboard, define electrical nets, route insulated wires, review warnings, preserve revisions, and generate an assembly plan. An external MCP client will be able to inspect and edit the same working copy through the same validated Rust operations used by the UI.

Development will follow the blueprint's milestone order. Each milestone must leave the repository in a runnable, tested state and add one end-to-end workflow to a permanent example project. Core behavior will be built before UI polish, automatic routing, or MCP integration.

The main architectural rule is:

> Every state change is a typed operation applied by the Rust engine to an expected document revision. The UI and MCP server are adapters; neither owns design rules or an independent design representation.

## 2. Version 1 boundaries

### Included

- Rectangular isolated-pad perfboards with configurable pitch and margins.
- Integer board coordinates and fixed-resolution routing coordinates.
- Rectangular components, stable pins, 90-degree rotations, overhangs, keepouts, and access annotations.
- Named multi-pin nets and explicit physical wires.
- Component-side and correctly mirrored solder-side views.
- Manual routing and deterministic route suggestions in eight directions.
- Crossings, warnings, preferred U-jumper inventory, and route locks.
- Snapshot-based commits, a shared working copy, undo/redo, compare, and restore.
- TOML/Markdown component library and project-contained component snapshots.
- Local MCP access to the active editor session.
- Derived wire lists and assembly steps.

### Explicitly deferred

- Schematic capture, simulation, professional ERC, PCB manufacture, copper traces, and solder bridges.
- Arbitrary rotations, non-rectangular board outlines, and full 3D mechanical models.
- Cloud accounts, collaboration over a network, branching, and merge workflows.
- Globally optimal multi-net routing, a dedicated bus router, or automatic circuit design.
- Silent consumption of physical jumper inventory.
- A built-in chat UI.

## 3. Initial technical decisions

These choices should be recorded as short ADRs if implementation experience requires changing them.

| Area | Initial choice | Reason |
|---|---|---|
| Workspace | Cargo workspace with `core`, `project`, and `mcp-server` crates | Keeps domain logic independent of Tauri and transport code. |
| Core state | Immutable-looking `Design` values changed only through operations | Makes validation, undo/redo, transactions, and conflicts predictable. |
| Serialization | Serde JSON for project files; TOML for component definitions | JSON handles nested revision data well; TOML remains pleasant for library authors. |
| IDs | Typed UUID newtypes serialized as strings | Stable across saves without mixing component, pin, net, and wire identifiers. |
| Geometry | Integer hole coordinates plus integer half-grid routing units | Supports holes and between-hole paths without floating-point topology errors. Millimeters are derived values. |
| Frontend | Vite + TypeScript + SVG, starting without a UI framework | Keeps the first editor small; introduce a framework only if state synchronization proves cumbersome. |
| Desktop shell | Current stable Tauri major version chosen at bootstrap and pinned | Provides local filesystem access and Rust command integration. |
| Async boundary | Session actor serializing commands | Gives UI and MCP one ordering point and makes expected-revision conflicts explicit. |
| Routing | Deterministic A* with fixed neighbor ordering and integer costs | Reproducible suggestions and tests. |
| Persistence | Schema-versioned project document with atomic replace and backup | Human-inspectable, portable, and resilient to interrupted writes. |
| Revisions | Full snapshots initially | Projects are small; simplicity and recoverability are more valuable than storage optimization. |
| Errors | Stable error/warning codes plus human-readable messages | UI and MCP can behave consistently without parsing prose. |

Do not commit to a frontend framework, IPC implementation, or exact routing weights without a small proof of concept. Those are replaceable adapters or policy values; the serialized domain model and operation semantics are harder to change.

## 4. Proposed repository layout

```text
Cargo.toml
crates/
  core/
    src/
      geometry.rs
      board.rs
      component.rs
      circuit.rs
      layout.rs
      constraints.rs
      validation.rs
      routing/
      assembly.rs
  project/
    src/
      document.rs
      operations.rs
      transaction.rs
      history.rs
      persistence.rs
      diff.rs
      session.rs
  mcp-server/
    src/
      main.rs
      tools.rs
      client.rs
  print/
    src/
      lib.rs        page SVGs -> true-scale PDF (ADR 0011)
src-tauri/
  src/
    commands.rs
    events.rs
    session_host.rs
frontend/
  src/
    api/
    editor/
    renderer/
    inspectors/
    revisions/
    assembly/
components/
  arduino-nano-like/
    component.toml
    README.md
  rs485-breakout/
    component.toml
    README.md
examples/
  nano-rs485.perfboard.json
tests/
  fixtures/
docs/
  adr/
  format/
  plan/
```

Crate dependency direction:

```text
core <- project <- Tauri adapter
              <- MCP adapter
print <- Tauri adapter
frontend --Tauri commands/events--> project session
```

`print` turns page-sized SVGs into PDF and depends on neither `core` nor Tauri. `core` must not depend on Tauri, MCP, filesystem access, or frontend types. `project` may depend on `core` and implements orchestration, history, persistence, and the authoritative session. The adapters translate requests and responses but do not reimplement validation.

## 5. Domain contracts to establish first

Before building substantial UI, write and test the following contracts.

### 5.1 Coordinates and transforms

- `HolePos { x, y }` uses zero-based integers.
- `RoutePos { x2, y2 }` uses half-grid units, so a hole at `(x, y)` maps to `(2x, 2y)`.
- Top/component-side coordinates are canonical. Solder-side mirroring is a presentation transform around the board's hole extents.
- Rotations are an enum: `Deg0`, `Deg90`, `Deg180`, `Deg270`.
- Length is calculated from routing geometry and board pitch in millimeters; it is never used to decide topological equality.
- Board dimensions distinguish hole count from intervals: the grid span is `(holes_x - 1) * pitch` by `(holes_y - 1) * pitch`, plus configured margins.

### 5.2 Stable identity and component snapshots

- Library definitions have a stable component definition ID and a definition version or content hash.
- Instances, pins, nets, wires, constraints, operations, and commits have distinct typed IDs.
- A project stores the exact component definitions needed by its instances, or equivalent immutable snapshots, so library edits cannot alter an existing project silently.
- Updating an instance to a newer library definition is an explicit, validated operation with a preview of pin and geometry changes.

### 5.3 Circuit versus layout

- Accepted and proposed connectivity are represented separately.
- A pin belongs to at most one accepted net.
- Wires refer to a net and explicit endpoints; geometric intersection never creates connectivity.
- A multi-pin net is complete only when the graph of its terminals and explicit junctions is connected.
- Moving geometry may invalidate routes, but never changes accepted connectivity.

### 5.4 Operations and validation

Each mutation request contains:

- operation ID;
- expected document revision;
- typed payload;
- origin (`user`, `mcp`, `import`, or `system-derived`);
- optional rationale.

Applying an operation returns either a typed error with no state change, or:

- new document revision;
- normalized operation/result;
- affected object IDs;
- validation delta;
- derived-data invalidations;
- change summary.

Transactions validate against a temporary state and commit atomically. Required violations reject acceptance; preferred and advisory findings are returned as warnings. Some manual editing operations may create a working-copy state with warnings, but never a structurally corrupt state.

### 5.5 Derived data

Crossings, route measurements, general validation reports, revision diffs, and assembly plans should be reproducible from authoritative state. Cache them only behind explicit invalidation. Assembly completion progress is user session data and is not part of immutable design revisions.

## 6. Cross-cutting engineering rules

- Use stable machine-readable diagnostic codes, object references, severity, and plain-language explanations.
- Make deterministic iteration explicit: sort IDs and use fixed A* tie-breaking rather than depending on hash-map order.
- Validate at deserialization boundaries and again when applying state-changing operations.
- Keep TypeScript types generated from, or contract-tested against, Rust transport types; do not manually duplicate domain rules.
- Treat component Markdown and imported data as untrusted display content. Sanitize rendered Markdown and never interpret it as a permission grant.
- Restrict project and component-library file access to user-selected roots.
- Write project saves to a sibling temporary file, flush, and atomically replace the target where supported. Retain a recoverable backup after migration or replacement.
- Log operation IDs and revision IDs, not full project contents by default.
- Add accessibility labels and non-color wire identifiers as editor controls are introduced.
- Keep accepted routes stable. Revalidation may mark them invalid or stale, but only an explicit operation may replace them.

## 7. Milestone plan

### Milestone 0 — Bootstrap and contracts

Status: complete (2026-10-22)

Goal: create a reliable development skeleton and settle the formats that Milestone 1 needs.

Work:

- Create the Cargo workspace and empty crates with enforced dependency direction.
- Bootstrap Tauri and the Vite/TypeScript frontend.
- Add formatting, linting, Rust tests, frontend tests, and a CI workflow.
- Define typed IDs, coordinate primitives, rotations, diagnostic shape, and schema version wrapper.
- Draft `docs/format/project-v1.md`, `docs/format/component-v1.md`, and the first ADRs.
- Add fixture helpers and deterministic snapshot conventions.
- Add a minimal Tauri command and frontend smoke test proving the Rust/frontend boundary.

Exit criteria:

- A clean checkout can run the desktop development build using documented commands.
- Formatting, linting, unit tests, and frontend tests run in CI.
- Coordinate and mirror transform tests pass for rectangular boards including edge holes.
- A serialized empty project round-trips without loss.

### Milestone 1 — Core model and simple graphical board

Status: complete (2026-10-23)

Goal: create, render, edit, save, and reopen a board with placed components.

Core and project work:

- Implement board geometry, margins, mounting holes, forbidden areas, component definitions, pins, keepouts, access regions, placement, and rotation.
- Parse and validate component TOML, including duplicate IDs, invalid rotations, and malformed pin locations.
- Implement occupancy, bounds, overhang, and required access checks.
- Implement the first operations: create project, add/remove/move/rotate component, and change instance properties.
- Store component definition snapshots in the project document.
- Implement schema-versioned JSON load/save and atomic replacement.

Frontend work:

- Render board outline, holes, components, pins, labels, orientation markers, overhangs, access arrows, selection, and invalid regions in SVG.
- Add zoom, pan, selection, drag placement, rotate action, component list, properties inspector, and validation panel.
- Keep view transforms separate from canonical coordinates.

Fixtures and demonstration:

- Add `arduino-nano-like` and `rs485-breakout` component definitions with documentation.
- Create a 25 x 18, 2.54 mm project; place, move, and rotate both modules; save and reopen it.

Exit criteria:

- All edits go through Rust operations; direct frontend state mutation cannot become authoritative.
- Invalid placement is either rejected with an identified required constraint or retained only where the operation explicitly permits a warned manual override.
- Save/reopen preserves IDs, geometry, definitions, notes, and validation results.
- Golden SVG or browser screenshot tests cover unrotated and rotated components.

### Milestone 2 — Nets and manual routing

Status: complete (2026-10-23)

Goal: describe intended connectivity and manually route insulated wires on either face.

Core and project work:

- Implement named nets, accepted/proposed connection state, and pin-to-net invariants.
- Implement wires with stable IDs, explicit terminals, net, one board side, color, vertices, lock state, and notes.
- Define explicit junction and through-hole transition semantics, even if automatic routes remain single-sided.
- Validate eight-direction segments, endpoint references, grid rules, keepouts, bounds, side transitions, short segments, and basic clearance.
- Calculate geometric length and bend information.
- Build actual-connectivity graph checks for unrouted and incomplete multi-pin nets.
- Add operations for nets, proposal acceptance/rejection, wire creation/removal/path editing/color/side/lock.

Frontend work:

- Add net creation and pin assignment UI, with proposals visibly distinct from accepted connectivity.
- Add manual wire mode: choose endpoints, insert/move/remove bend points, select side and color, and inspect warnings.
- Add component-side/solder-side toggle with presentation-only mirroring and readable labels.
- Show wire IDs and patterns/labels so color is not the sole discriminator.

Exit criteria:

- The example project's required nets can be defined and routed manually.
- Solder-side rendering is correctly mirrored while selection and editing still target canonical coordinates.
- A crossing does not create a junction, and connectivity uses the explicit graph.
- Route length tests cover horizontal, vertical, and 45-degree segments at non-default pitch.
- Proposed electrical changes cannot become accepted without an explicit approval operation.

### Milestone 3 — Operations, working copy, and revisions

Status: complete (2026-10-23)

Goal: make all editing recoverable, comparable, atomic, and safe under concurrent clients.

Project work:

- Complete the operation registry and ensure every existing mutation uses it.
- Add grouped transactions with all-or-nothing application.
- Add working-copy undo/redo based on reversible operations or snapshots; clear redo only after a divergent edit.
- Add named commits with parent, timestamp, description, and full design snapshot.
- Add non-destructive restore that creates a new working copy from an older commit without deleting later history.
- Implement semantic diffs for components, nets, wires, constraints, and routes.
- Add expected-revision checks and a structured stale-revision response.
- Introduce the session actor as the only owner of the active project state and publish change events.

Frontend work:

- Add committed and working panes using the same renderer component.
- Add commit, discard, undo, redo, history, compare, and restore controls.
- Highlight additions, removals, movement/rotation, route changes, and modified circuit data.
- Display operation origin and optional rationale in change history.

Exit criteria:

- A failed transaction leaves bytes and in-memory state unchanged.
- Undo/redo and save/reopen preserve consistent working and committed state.
- Restoring an old commit does not remove its descendants from history.
- Two requests with the same expected revision result in one success and one conflict.
- The milestone demonstration in the blueprint passes as an automated integration scenario.

### Milestone 4 — Constraints, automatic routing, and jumper preferences

Status: complete (2026-10-24)

Goal: produce reproducible route proposals optimized for hand assembly, without disturbing accepted work.

Constraint work:

- Implement typed constraints for fixed placement, orientation, proximity, preferred maximum length, bend spacing, access, edge placement, reserved areas, wire side, locked routes, and assembly access.
- Preserve severity, rationale, origin, and any unstructured natural-language annotation.
- Add CRUD operations and an inspector/editor for constraints.

Router work:

- Build an occupancy/clearance map from board geometry, components, keepouts, terminals, and accepted wires.
- Implement deterministic A* over position, side, direction, and distance-since-bend.
- Centralize cost configuration: length, bend, cramped bend, crossing, access, side transition, parallel run, and jumper match.
- Use hard constraints to exclude states and soft preferences only to adjust cost and produce warnings.
- Detect crossings after routing and record user-selected over/under relationships where relevant.
- Return proposals containing path, length, bend count, crossings, jumper match, cost breakdown, and warnings.
- Never replace accepted or locked routes unless the invoking operation identifies them explicitly.

Jumper work:

- Implement inventory entries with interval-based `span_holes`, optional quantity, color, dimensions, and tolerance.
- Strongly prefer exact straight matches; report approximate/reshaped matches as advisory.
- Do not decrement inventory when generating or accepting a design.

Frontend work:

- Add suggest/evaluate/accept/reject route actions and expose cost reasons without overwhelming the user.
- Add jumper inventory preferences and route-warning visualization.
- Allow manual overrides that preserve hard validity and visibly acknowledge soft warnings.

Exit criteria:

- Repeated routing of identical state and configuration returns byte-equivalent proposals.
- Tests cover obstacle avoidance, eight directions, minimum bend spacing, crossings, locked routes, no-path explanations, exact jumper spans, and tolerance boundaries.
- Accepting one proposal does not reroute an unrelated accepted wire.
- The sample project can be routed automatically, manually adjusted, and reopened without route drift.

### Milestone 5 — MCP integration

Status: complete (2026-10-25)

Goal: let an external MCP client inspect and edit the active working copy with identical semantics to the UI.

Architecture spike first:

- Prototype a local endpoint owned by the Tauri session host and an `rmcp` stdio process that connects to it.
- Choose the simplest secure local IPC supported on the target platforms; document endpoint discovery, authentication/token lifetime, startup, reconnect, and app-closed behavior.
- Keep the session protocol transport-neutral and reuse the same request/response DTOs as Tauri where practical.

Initial MCP tools:

- Read: `get_project`, `get_design`, `get_validation_report`, `get_revision_history`, `compare_revisions`, `list_components`, `get_component_definition`, `get_component_documentation`, `list_constraints`, `get_jumper_inventory`, `get_assembly_plan`, `get_rendered_design`.
- Write: one coherent `apply_operations` transaction tool plus focused helpers only where they improve discoverability, such as `suggest_routes`, `evaluate_route`, `accept_proposed_connections`, and `commit_design`.
- Component creation should write only to explicitly selected library locations and should validate before replacement.

Implementation note: `get_assembly_plan` arrived with Milestone 6, once the
authoritative derived plan was defined. `rename_project` was added later; like
the inspector rename, it is revision-checked but not an undo step, because the
project name is document metadata outside the design snapshots.
Component-library access is read-only in this milestone; safe creation and
replacement remain Milestone 7 work.

Behavior:

- Every write supplies the expected revision and origin/rationale.
- MCP returns the same normalized result, diagnostics, affected objects, and revision IDs as the UI.
- Session change events cause the UI to refresh promptly after MCP operations.
- SVG rendering is produced from authoritative design data through a defined renderer boundary; structured coordinates remain the source of truth.
- Markdown is returned as untrusted documentation content, not executable instructions.

Exit criteria:

- Contract tests run the same operation cases through direct project calls, Tauri commands, and MCP, and compare outcomes.
- A human edit racing an MCP edit yields an explicit stale-revision conflict, never last-writer-wins data loss.
- MCP cannot read or write arbitrary filesystem paths or execute commands.
- An MCP client can complete the blueprint demonstration and the UI shows each change in the shared working copy.

### Milestone 6 — Assembly planning

Status: in progress. Done: 1:1 wire cut guide and bending templates grouped by colour and shape, exported as a true-scale PDF ([ADR 0011](../adr/0011-true-scale-pdf-output.md)); the assembly planning engine with dependency ordering, cycle warnings, wire list and staleness, exposed through Tauri and MCP `get_assembly_plan`, and checkmark storage outside the project file ([ADR 0012](../adr/0012-assembly-planning.md)). Assembly mode in the editor: one step at a time over the board as built so far, with checkmarks reconciled by step fingerprint and a banner when the plan changes. The printed build drawing carries the plan's wire list in build order and a build-order checklist. All planned work is in; to close the milestone, build the sample project from the plan in the desktop app.

Goal: turn a validated design into a practical, step-by-step construction aid.

Engine work:

- Derive crossing relationships and explicit assembly dependencies without treating every crossing as mandatory ordering.
- Build a dependency graph for components and wires; topologically sort it with deterministic heuristic tie-breaking.
- Warn on cycles and show the involved objects.
- Add heuristics for low-profile parts, sockets, access-blocking components, wire layering, and inaccessible joints.
- Generate a wire list with endpoints, net, color, face, estimated length, bends, jumper suggestion, and assembly order.
- Attach a source document revision to every generated plan and mark it stale when relevant state changes.

Frontend work:

- Add assembly mode with one highlighted step, faded context, correct board face, terminal markers, instructions, and next/previous controls.
- Store completion checkmarks separately from design revisions and reset or reconcile them when the plan changes.
- Add printable/exportable wire list and construction checklist after the in-app flow works.

Exit criteria:

- Stable inputs produce stable plans.
- Dependency tests cover simple chains, independent steps, crossing order, access constraints, and cycles.
- Editing relevant placement or routing marks an existing plan stale.
- The sample project produces a usable step sequence and wire list with estimates clearly labeled.

### Milestone 7 — Component editor and release hardening

Goal: make the complete workflow dependable for ordinary hobby projects and package a version 1 release.

Work:

- Add the component editor for body, pins, overhang, rotations, keepouts, access annotations, and Markdown.
- Add preview, validation, safe save, version/hash update, and explicit project-instance upgrade flow.
- Improve constraint editing, routing explanations, keyboard operation, accessibility, and error recovery.
- Add schema migration tests, corrupt-file recovery tests, and backup restoration guidance.
- Profile the intended upper bound: approximately 12 components and several dozen wires.
- Add onboarding, limitation/safety language, component-authoring documentation, and example projects.
- Package and smoke-test supported desktop platforms; document support based on available CI runners.
  Progress: `quickstart.sh` builds Linux `.deb`, `.rpm` and AppImage bundles with the MCP adapter and the component library (`src-tauri/tauri.bundle.conf.json`), and the release workflow uses it. Not yet smoke-tested from an installed package; macOS and Windows are not packaged.
- Perform a complete acceptance-criteria audit against the blueprint.

Required before 1.0 (noted 2026-11-13, with 1.0 now in sight):

- **A basic component library.** The bundled `components/` holds nine parts,
  mostly those of the sample project. Ship a starter set of common
  through-hole parts for ordinary hobby builds: resistors, diodes, capacitors,
  LEDs, transistors, pin headers, and DIP sockets, each checked against a real
  part. The template generators of Milestone CE5 in
  [the Component Editor plan](DEVELOPMENT_PLAN-ComponentEditor.md) can produce
  most of them. MCP must see the same library as the palette: today
  `list_components` and `get_component_definition` still read three
  definitions compiled into the app (`bundled_component_library`).
- **Project-specific components.** Today the palette loads only the bundled
  `components/` directory, which is read-only in an installed package, so a
  user has nowhere to keep a part of their own. Add a supported place for
  components that belong to one project, such as a folder next to the project
  file, that the palette loads alongside the bundled library, that travels
  with the project, and that never mixes into the shared library. Placed
  instances keep their embedded snapshots as today.

Exit criteria:

- All 18 version 1 acceptance criteria in the blueprint have an automated test, a documented manual test, or both.
- A component library edit cannot silently alter an existing project.
- A new user can place common through-hole parts from the bundled library,
  and can add a part of their own to a project without editing the
  application's installed files.
- The packaged application completes create -> place -> connect -> route -> commit -> MCP edit -> assemble -> save/reopen.
- Known limitations and the non-guarantee of electrical safety are visible in product documentation.

## 8. Test strategy

### Test layers

1. **Core unit/property tests:** transforms, rotations, bounds, pin identity, connectivity graphs, route geometry, crossing classification, jumper math, dependency ordering, and deterministic ordering.
2. **Project state-machine tests:** generated operation sequences, transactions, undo/redo, expected-revision conflicts, commit/restore, and persistence round trips.
3. **Fixture regression tests:** small human-readable projects with expected diagnostics, semantic diffs, route proposals, and assembly plans.
4. **Adapter contract tests:** identical requests through direct Rust, Tauri, and MCP boundaries produce equivalent results.
5. **Frontend component tests:** coordinate conversion, hit testing, editor gestures, inspector behavior, and warning presentation.
6. **Visual tests:** SVG/golden screenshots for both faces, rotations, selections, crossings, comparisons, and assembly highlighting.
7. **End-to-end tests:** milestone demonstrations and the complete version 1 workflow.

### Required permanent fixtures

- Empty 1 x 1 and typical 25 x 18 boards.
- Component with negative overhang and directional access.
- Rotated asymmetric pin layout.
- Multi-pin net with and without explicit junctions.
- Crossing wires that are not connected.
- Obstacle course for deterministic eight-direction routing.
- Exact and approximate U-jumper matches at multiple pitches.
- Revision conflict and failed transaction scenarios.
- Assembly dependency chain and cycle.
- Project using an older supported schema for migration.

### Quality gates

Every pull request should pass:

- Rust formatting, linting with warnings denied for project code, and all Rust tests.
- TypeScript formatting/type checking, frontend unit tests, and production build.
- Schema/fixture round-trip tests.
- Changed visual goldens reviewed intentionally.
- No new dependency without a short justification and license/security review.

## 9. Implementation sequence inside each milestone

Use the same order to keep changes reviewable:

1. Write or update the domain contract and fixtures.
2. Add failing Rust tests for the rule.
3. Implement the core type or algorithm.
4. Expose it through project operations and session responses.
5. Add the Tauri adapter and TypeScript contract.
6. Build the smallest usable editor interaction.
7. Add integration and visual coverage.
8. Update the sample project and milestone demonstration.
9. Document limitations and close the milestone only after save/reopen verification.

Prefer small vertical pull requests. Avoid landing a large domain model with no runnable interaction, or a polished UI backed by temporary frontend-only behavior.

## 10. Risks and planned mitigations

| Risk | Mitigation |
|---|---|
| Frontend and Rust models drift | Rust owns validation; generate or contract-test transport types; adapter equivalence tests. |
| Solder-side edits write mirrored coordinates | One canonical coordinate system; inverse view transform at the input boundary; asymmetric golden tests. |
| Router state space grows unexpectedly | Fixed half-grid resolution, intended small-board limits, bounded search, profiling fixture, and clear no-route diagnostics. |
| Routing weights produce surprising paths | Centralized named costs, deterministic cost breakdowns, regression fixtures, and manual override. |
| Library changes break old projects | Embed immutable component snapshots and make upgrades explicit. |
| Undo/history semantics become inconsistent | All changes are operations; transaction tests; full revision snapshots for version 1. |
| UI and MCP race | Single session actor plus expected-revision compare-and-swap semantics. |
| Local MCP endpoint is exposed too broadly | Local-only IPC, per-session endpoint/token, narrow operation protocol, no general filesystem API. |
| Assembly planner overstates certainty | Label lengths as estimates, distinguish required dependencies from heuristics, and explain warnings. |
| Project corruption on save/migration | Atomic replacement, backup before migration, round-trip checks, recovery tests. |
| Scope expands toward EDA/CAD features | Check every feature against the version 1 boundaries and acceptance criteria before adding it. |

## 11. Decision checkpoints

These decisions should be made at the indicated point using a small prototype and recorded in `docs/adr/`:

- **Milestone 0:** exact Tauri major version, build tooling, test runner, typed-ID/UUID crate, JSON schema-version envelope, and Rust-to-TypeScript type generation approach.
- **Before Milestone 2 UI:** editing and hit-testing strategy for direct SVG manipulation.
- **Before Milestone 3:** reversible operations versus snapshots for working-copy undo; commits remain full snapshots either way.
- **Before Milestone 4:** routing clearance model, maximum search bounds, and default cost/spacing configuration.
- **At Milestone 5 spike:** local IPC mechanism and MCP process lifecycle on supported platforms.
- **Before Milestone 7 packaging:** supported operating systems based on actual CI and smoke-test capacity.

No checkpoint should block earlier domain work whose contracts do not depend on that choice.

## 12. Backlog organization

Track work by milestone and tag each issue as one of: `domain`, `operation`, `persistence`, `frontend`, `routing`, `mcp`, `assembly`, `test`, or `docs`.

Every implementation issue should state:

- user-visible outcome;
- authoritative Rust types/operations affected;
- validation and diagnostic behavior;
- persistence/schema impact;
- tests and fixtures required;
- explicit non-goals;
- milestone demonstration impact.

The first implementation issues should be:

1. Bootstrap Cargo/Tauri/Vite workspace and CI.
2. Define coordinates, rotations, typed IDs, and mirror transforms with tests.
3. Define the versioned empty project envelope and lossless round trip.
4. Define/parse the component TOML format with one Nano-like fixture.
5. Implement board/component placement operations and diagnostics.
6. Render the 25 x 18 board and placed fixture component through the first end-to-end Tauri command.

## 13. Version 1 definition of done

Version 1 is done only when the blueprint's complete acceptance workflow works in a packaged desktop build, all authoritative mutations pass through the Rust operation layer, the sample project survives save/reopen with history intact, MCP and UI edits share a conflict-safe working copy, and the generated assembly plan is clear enough to build the sample board by hand.

Passing validation must be described as consistency checking, not proof of electrical correctness or safety.
