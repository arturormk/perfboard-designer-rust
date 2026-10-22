# BLUEPRINT.md — Perfboard Designer

## 1. Project overview

**Perfboard Designer** is an open-source, local-first desktop application for designing and assembling small electronic circuits on perforated prototype boards.

The application combines:

- A graphical editor for placing components and routing wires.
- A deterministic geometry and routing engine.
- A simple circuit connectivity model.
- A component library with structured geometry and human-readable documentation.
- A revision-controlled design workflow.
- Assembly instructions and construction-order planning.
- An MCP server allowing an LLM to perform the same operations available to a human user.

The intended user is an electronics hobbyist or technically competent beginner building small projects using Arduino-compatible boards, breakout modules, connectors, resistors, capacitors, transistors, and similar components.

**The application should be practical rather than ambitious.**

A large project might contain a dozen components and several dozen wires. There is no need to compete with professional electronic design automation software.

The goal is to help users produce designs that are electrically sensible, physically realizable, easy to solder, and easy to understand.

The application must be fully usable without an LLM.

### 1.1 Core philosophy

1. Human users and LLMs are equal participants in editing a design.
2. Both operate through the same underlying Rust design model and validation engine.
3. Geometry and connectivity are deterministic.
4. The LLM supplies electronic common sense, interpretation, suggestions, and design judgment.
5. The user retains final control over electrical and physical design decisions.
6. The application should prefer easily assembled designs over geometrically optimal but impractical ones.
7. All design data should be portable, inspectable, and stored locally.
8. Keep the application small, comprehensible, and maintainable.

### 1.2 Explicit non-goals

The initial application is not:

- A professional PCB design package.
- A schematic capture or electronic simulation environment.
- A multi-layer PCB autorouter.
- A general-purpose 3D mechanical CAD application.
- An electrical rules checker comparable to KiCad.
- An autonomous circuit designer requiring no user approval.
- A system for manufacturing PCBs.

Avoid implementing features merely because they exist in conventional EDA software.

---

## 2. Technology and architecture

### 2.1 Technology choices

| Layer | Technology |
|---|---|
| Core engine | Rust |
| Desktop application | Tauri |
| Graphical interface | HTML, CSS, TypeScript |
| Board rendering | SVG |
| MCP interface | Rust MCP SDK, preferably `rmcp` |
| Component definitions | TOML and Markdown |
| Project persistence | Human-readable JSON or TOML |
| Revision history | Local snapshot-based history |
| Testing | Rust tests and frontend integration tests |

The exact frontend framework is an implementation choice. A small application does not necessarily need a large UI framework.

Prefer mature, actively maintained dependencies.

### 2.2 Suggested repository organization

```text
perfboard-designer/
├── BLUEPRINT.md
├── README.md
├── LICENSE
├── Cargo.toml
├── crates/
│   ├── core/
│   │   ├── geometry
│   │   ├── components
│   │   ├── circuit
│   │   ├── routing
│   │   ├── constraints
│   │   ├── validation
│   │   └── assembly
│   ├── project/
│   │   ├── persistence
│   │   ├── revisions
│   │   └── operations
│   └── mcp-server/
├── src-tauri/
├── frontend/
├── components/
├── examples/
└── tests/
```

This is a suggested organization, not a mandatory structure.

The key requirement is that the core engine must not depend on Tauri or the graphical frontend.

### 2.3 Shared design engine

All modifications must pass through the same Rust operation layer.

The graphical UI invokes operations through Tauri commands.

The MCP server invokes equivalent operations through its tool handlers.

Both receive the same results, including validation errors and warnings.

There must not be a separate MCP design representation or an independent AI-only editor.

The core engine owns the authoritative design state.

---

## 3. Design model

A project contains three related models.

### 3.1 Circuit

Describes what should be electrically connected.

Contains:

- Component instances.
- Pin identities.
- Electrical nets.
- Proposed and approved connections.
- Electrical annotations.
- Circuit-level constraints.

The circuit model does not specify where wires physically run.

A net may contain more than two pins.

Component pins have stable identifiers.

### 3.2 Layout

Describes the physical board.

Contains:

- Board geometry.
- Component placement and orientation.
- Occupied holes.
- Component outlines and overhangs.
- Keepout and access areas.
- Wire paths.
- Wire colors.
- Wire sides.
- Wire crossings.
- Physical routing constraints.

Multiple physical wires may implement one electrical net.

### 3.3 Assembly

Describes how to build the finished design.

Contains:

- Component installation instructions.
- Wire installation order.
- Crossing dependencies.
- Wire colors and estimated lengths.
- Jumper recommendations.
- Assembly warnings.
- User notes.
- Inspection and testing checklist.

Assembly information is derived from the circuit and layout but may include user-approved overrides.

Derived assembly information should be regenerated or marked stale when the relevant design changes.

---

## 4. Board geometry

### 4.1 Supported boards

Version 1 supports rectangular perfboards with isolated through-hole pads.

The default pitch is 2.54 mm (0.1 inch), configurable at project level.

The user specifies:

- Number of holes horizontally.
- Number of holes vertically.
- Grid pitch.
- Optional board-edge margins.
- Optional mounting holes and forbidden areas.
- Optional notes concerning unusual board construction.

For example:

```toml
[board]
name = "25x18 perfboard"
holes_x = 25
holes_y = 18
pitch_mm = 2.54
```

The application must distinguish the number of holes from the number of intervals between holes.

Hole coordinates are zero-based integer pairs.

Physical dimensions are derived from the pitch and configured margins.

### 4.2 Board faces

Support:

- Component side.
- Solder side.

The graphical editor must allow switching between these views.

When viewed from the solder side, the board must be mirrored correctly.

Pin and component labels should remain readable in the UI, even when the board itself is mirrored.

All component and wire coordinates remain in a single canonical board coordinate system. Mirroring is a presentation transformation.

### 4.3 Coordinate representation

Use integer grid coordinates wherever possible.

Wire routing may use finer coordinates, such as half-grid units, for paths between holes.

For example, one grid interval could be represented internally as two or more routing units.

Avoid accumulating floating-point errors when determining occupancy, crossings, and grid alignment.

Real-valued physical dimensions in millimeters remain useful for measurements, component bodies, wire lengths, and clearances.

---

## 5. Components

### 5.1 General requirements

Version 1 represents components as rectangles with annotated pins and orientation markers.

A component definition may describe:

- An Arduino-compatible module.
- A breakout PCB.
- A connector.
- A resistor.
- A capacitor.
- A transistor.
- A DIP IC.
- A socket.
- A generic custom component.

Complex artwork and exact 3D models are unnecessary.

A rectangle with text labels and accurately positioned pins is sufficient.

### 5.2 Structured component definition

Each component should have a machine-readable definition, preferably TOML.

Example directory:

```text
components/
  arduino-nano/
    component.toml
    README.md
```

The structured definition contains:

- Component ID.
- Human-readable name.
- Optional manufacturer and variant information.
- Physical body dimensions.
- Pin positions.
- Pin identifiers.
- Pin electrical functions, where known.
- Body overhang.
- Occupied mounting holes.
- Keepout regions.
- Access requirements.
- Allowed orientations.
- Optional height or clearance annotations.

All component coordinates are relative to a defined local origin.

The body outline and pin locations must be independent.

Negative local coordinates are permitted to represent overhang.

### 5.3 Component orientations

Support rotation in 90° increments.

For version 1, arbitrary component rotations are unnecessary.

A definition may restrict allowed rotations.

The component model should be compatible with future support for mounting on either board side.

For the initial implementation, conventional component-side installation may be the default.

### 5.4 Pins

Pins are first-class objects.

Each pin should have:

- Stable ID.
- Display name.
- Local position.
- Optional aliases.
- Optional electrical functions.
- Optional direction or signal role.
- Optional electrical metadata.

A pin may serve multiple purposes.

For example, an Arduino pin may function as GPIO, UART, PWM, or another peripheral function.

Structured function annotations may assist the application, but complex interpretation of those capabilities is primarily an LLM responsibility.

### 5.5 Access constraints

Some components require access from a particular direction.

Examples include:

- USB connectors.
- Screw terminals.
- Buttons.
- Adjustment potentiometers.
- Programming connectors.

An access constraint should specify:

- Component or component feature.
- Direction.
- Clearance region.
- Whether overhang beyond the board edge is permitted.
- Whether the constraint is required or advisory.

An annotation such as `USB SIDE` or `SCREW ACCESS` should appear on the graphical component.

A component may extend beyond the board boundary if its mounting pins fit and the overhang is explicitly allowed.

### 5.6 Markdown documentation

Each component may have an accompanying Markdown file.

The Markdown provides information intended for human users and LLMs.

Examples:

- Pinout explanations.
- Voltage limitations.
- Current limitations.
- Dual-use pins.
- Important configuration details.
- Connector orientation.
- Decoupling recommendations.
- Placement considerations.
- Assembly advice.
- Links to authoritative datasheets.

The documentation need not be understandable by the Rust engine.

The LLM should be able to retrieve it through MCP.

Component-specific documentation should be distinguished from the LLM's general knowledge.

For example, generic Arduino Nano information must not automatically be treated as authoritative for every Nano-compatible board.

### 5.7 Component editor

Users must be able to create or modify component definitions.

The editor should support:

- Rectangle dimensions.
- Pin placement on a grid.
- Pin names.
- Body overhang.
- Orientation.
- Access-direction annotations.
- Editing Markdown documentation.

An LLM should have equivalent MCP capabilities to create proposed component definitions.

Changes to a component definition must not silently corrupt existing layouts.

Projects should preserve sufficient versioned component information to remain reproducible.

---

## 6. Electrical connectivity

### 6.1 Nets

The user defines electrical connections as named nets.

Examples:

```text
GND:
  Nano.GND
  RS485.GND
  C1.2

VCC:
  Nano.5V
  RS485.VCC
  C1.1

UART_TX:
  Nano.D1
  RS485.DI
```

The examples are illustrative, not an endorsement of a particular circuit.

A net may connect two or more pins.

Each physical pin belongs to at most one electrical net within a design.

### 6.2 Proposed connections

LLMs may suggest electrical connections, pin assignments, and additional components.

However, such changes must be clearly identified as proposals until accepted.

The user must explicitly approve proposed circuit changes before they become part of the accepted circuit.

Routing already-approved nets is a layout operation and does not require approval of the electrical connectivity again.

### 6.3 Basic electrical checking

Version 1 should perform simple checks where reliable structured data exists.

Examples:

- Missing required connections.
- Pins assigned to conflicting nets.
- Duplicate or invalid pin references.
- Connections involving incompatible voltage annotations.
- Unrouted or incompletely routed nets.

The application must not claim that passing these checks establishes electrical safety or correctness.

More complicated electronic considerations should be presented as suggestions or warnings, not fabricated guarantees.

---

## 7. Wire model

### 7.1 General assumptions

**All wire routes use insulated wire.**

This applies to both hand-cut stock wire and preformed jumper wires.

The application does not need to support bare-wire traces or solder-bridge routing in version 1.

Users may ultimately choose:

- Hard wire.
- Semi-hard wire.
- Soft wire.

The default router should favor designs that are practical with hard, solid-core wire.

This is a routing preference rather than a requirement that users employ a specific material.

### 7.2 Wire properties

Each wire contains:

- Stable ID.
- Associated electrical net.
- Starting terminal.
- Ending terminal.
- Board side.
- Color.
- Ordered path segments or vertices.
- Calculated length.
- Calculated bends.
- Crossings.
- Over/under relationships.
- Optional user instructions.
- Optional jumper-stock match.

Connections to an intermediate pad or other intentional junction require explicit electrical junction semantics.

A geometric crossing is not an electrical connection.

### 7.3 Wire colors

Default palette:

| Color | Suggested use |
|---|---|
| Red | Positive power |
| Black | Ground |
| Yellow | Clock or control |
| White | General signals |
| Green | Data or other signals |

The application must not treat these suggestions as mandatory standards.

Users can assign colors manually.

The LLM may propose colors based on circuit meaning, consistency, or practical assembly considerations.

Color is a property of an individual physical wire, not necessarily an entire net.

The graphical editor should provide labels and identifiers so color is not the only means of distinguishing wires.

### 7.4 Wire sides

Wires can be routed on either board face.

The application should prefer simple, accessible routes rather than always preferring one particular side.

Physical passage through a board hole must be represented explicitly when a wire changes sides.

A wire should not magically change sides at a graphical crossing.

Version 1 may restrict automatic routing to one side per wire, while supporting multiple wires and explicit through-hole transitions to implement a net.

---

## 8. Routing engine

### 8.1 Purpose

The router helps both humans and LLMs find practical wire paths.

It must be deterministic, independent of the LLM, and capable of reporting why a route is invalid or awkward.

It should generate suggestions, not irrevocable decisions.

Users and LLMs can override automatically generated routes.

### 8.2 Allowed geometry

Automatically generated routes may use eight directions:

- Horizontal.
- Vertical.
- The four 45° diagonals.

Thus changes in direction are normally 45°, 90°, or, when unavoidable, larger turns composed from those directions.

Avoid arbitrary precision angles and tiny corrective bends.

Prefer:

1. Straight routes.
2. Routes with one simple bend.
3. Routes with a small number of well-separated bends.
4. More complicated routes only when necessary.

### 8.3 Bend constraints

The router should consider:

- Total bend count.
- Bend angles.
- Distance between adjacent bends.
- Distance from terminals to nearby bends.
- Very short segments.
- Ease of forming a wire before installing it.

Use a configurable preferred minimum bend spacing.

An initial default of approximately 5 mm is acceptable, but should not be treated as a physical law.

Manual routes may violate soft bend preferences with warnings.

Automatic routing should avoid such violations where reasonably possible.

### 8.4 Grid-aware routing

Because perfboard holes normally follow a 2.54 mm grid, orthogonal routes should preferentially align with convenient grid positions.

45° diagonal runs have lengths of:

```text
length = steps * pitch * sqrt(2)
```

The router must calculate actual geometric length, not simply count grid units.

Routes may pass between holes if clearances permit.

Insulated wires can cross other insulated wires, but intersections and physical accessibility still matter.

### 8.5 Routing algorithm

A* or another suitable shortest-path algorithm is an acceptable foundation.

A routing state may include:

- Current coordinate.
- Board side.
- Current direction.
- Distance since the last bend.
- Occupancy and clearance context.

This permits bend penalties and minimum-segment constraints.

The router should distinguish:

**Hard constraints**

- Impossible terminal references.
- Geometrically invalid paths.
- Prohibited clearance regions.
- Invalid side transitions.
- Unintended electrical junctions.

**Soft preferences**

- Short wire length.
- Few bends.
- Adequate bend spacing.
- Low crossing count.
- Good access.
- Parallel organization.
- Standard jumper compatibility.

The exact routing cost function is an implementation detail.

All important weights should be adjustable or at least centralized in configuration.

### 8.6 Preserving routes

Accepted wire paths are explicitly stored in the project.

Do not reroute previously accepted wires merely because the project is reopened or another component is moved.

If a change invalidates an existing route, mark it accordingly and offer rerouting.

Automatic routing should normally preserve already committed or locked routes unless the user requests broader optimization.

### 8.7 Manual wire editing

The user must be able to:

- Select a starting pin.
- Select an ending pin.
- Insert bend points.
- Move or remove bend points.
- Select a board side.
- Change color.
- Request automatic routing.
- Accept or reject an automatic route.
- Lock a wire against automatic changes.
- Inspect warnings.

The interface should snap to sensible geometry while permitting deliberate overrides.

The LLM should have equivalent route modification operations through MCP.

---

## 9. Preferred preformed U-shaped jumpers

### 9.1 Motivation

Many users have assortments of rigid, insulated U-shaped jumper wires with terminal spacing in multiples of 2.54 mm.

Such jumpers are convenient because they are precut and preformed.

The router should know which lengths the user has available and favor designs that use them.

### 9.2 Jumper inventory

Support a user-configurable inventory of available jumpers.

Each jumper type may specify:

- Name or identifier.
- Leg-to-leg spacing in hole pitches.
- Optional color.
- Optional quantity.
- Optional physical wire diameter.
- Optional approximate exposed leg length.
- Optional notes.

For example:

```toml
[[jumpers]]
name = "U-3"
span_holes = 3
quantity = 20

[[jumpers]]
name = "U-5"
span_holes = 5
quantity = 15

[[jumpers]]
name = "U-8"
span_holes = 8
quantity = 10
```

Define `span_holes` unambiguously as the number of grid intervals between terminal centers, not the number of holes counted inclusively.

For 2.54 mm pitch, a 5-interval jumper has a terminal separation of 12.70 mm.

A quantity may be omitted to mean that stock is effectively unrestricted.

Inventory should be a preference and planning aid. Generating a design must not silently consume physical inventory.

### 9.3 Jumper matching

The simplest matching case is a straight connection between two holes separated by an available jumper span.

This should receive a strong routing preference.

For bent or reshaped jumpers, distinguish:

- Terminal separation.
- Developed wire length.
- Location of bends.
- Available stock length.
- Terminal-leg requirements.

Merely matching the total path length to a multiple of 2.54 mm does not guarantee that a preformed U-jumper can be reshaped into the route.

The router may suggest reshaping when the geometry appears reasonable.

Such suggestions should be advisory rather than guarantees of fit.

### 9.4 Tolerance

Support configurable fit tolerance for approximate jumper matches.

A default around 0.5 mm is a reasonable starting point.

A relaxed tolerance around 1 mm may be appropriate for some users and longer wires.

Tolerance should not authorize moving pins off the board grid.

It represents an acceptable discrepancy that the human may be able to accommodate through physical forming.

Warn when a proposed adjustment may create an awkward or stressed fit.

### 9.5 Priorities

When `prefer_standard_jumpers` is enabled, routing should favor:

1. Exact straight matches to available U-jumpers.
2. Simple routes achievable with minor adjustments.
3. Custom routes with few bends.
4. More complicated routes only if justified.

The preference must not override hard physical constraints.

### 9.6 Future extensions

Possible later features:

- Wire cut lists.
- Detailed bend-position guides.
- Stock usage summaries.
- Inventory-aware route optimization.
- Reusable jumper layout templates.

These are not all required in the first milestone.

---

## 10. Wire crossings and parallel buses

### 10.1 Crossings

Two insulated wires may cross without being electrically connected.

The router should discourage unnecessary crossings but not prohibit them.

Every same-side crossing should be detectable and displayed.

Where relevant, store or derive:

- Crossing location.
- Participating wires.
- Which wire passes over the other.
- Whether the crossing presents an assembly dependency.

### 10.2 Assembly dependencies

If wire W2 must be installed over wire W1, the assembly plan should generally install W1 first.

Represent this as:

```text
W1 -> W2
```

Assembly dependencies form a directed graph.

A topological sort can produce a valid installation sequence when no cycles exist.

Cycles should generate warnings and may motivate alternative routing.

Crossing geometry does not always establish a mandatory installation order. The engine should avoid inventing strict dependencies where the wires can be installed in either order.

### 10.3 Parallel buses

The router should favor neat parallel runs where appropriate.

Adjacent wires should have sufficient spacing to lie beside one another without needing to be stacked.

A full bus-routing subsystem is not required in version 1.

Independent wires with parallelism preferences are sufficient.

More advanced grouped routing may be added later.

### 10.4 Practical flexibility

The application is intended for small handmade projects.

A little untidiness is acceptable.

The router should not reject a usable layout simply because it contains a crossing or an aesthetically imperfect route.

Warnings should describe practical consequences rather than merely declaring a design suboptimal.

---

## 11. Constraints

### 11.1 Constraint categories

Support structured constraints including:

- Fixed placement.
- Allowed orientation.
- Component proximity.
- Maximum preferred wire length.
- Minimum bend spacing.
- Connector access.
- Edge placement.
- Reserved holes or areas.
- Wire side preference.
- Locked routing.
- Assembly accessibility.

Each constraint should have:

- Stable ID.
- Type.
- Target objects.
- Parameters.
- Severity.
- Human-readable rationale.
- Origin.

### 11.2 Severity

Use three levels:

**Required:** A violation makes the proposed operation invalid or prevents normal acceptance.

**Preferred:** A violation is allowed but produces a warning or routing penalty.

**Advisory:** An observation or recommendation that requires human interpretation.

### 11.3 Natural-language constraints

The user may provide instructions such as:

```text
Keep the capacitor close to the transceiver.

Leave the Arduino USB connector accessible.

Prefer short wires for the serial lines.

Reserve room for another connector.

Make this easy to hand solder.
```

An LLM may interpret these and propose structured constraints.

Where the meaning cannot be represented structurally, retain the original language as an annotation.

The application should not pretend that every natural-language instruction has been checked algorithmically.

### 11.4 Constraint origin and approval

Record whether a constraint was:

- Added by the user.
- Imported with a component.
- Suggested by an LLM.
- Accepted from an LLM proposal.

An LLM may propose changes to constraints.

User-defined hard constraints should not be silently weakened or removed.

---

## 12. Revision-controlled editing

### 12.1 Two-pane model

The main interface contains:

**Committed design**

The latest accepted revision.

**Working copy**

The current editable design, including human and LLM modifications.

Both panes show the same project using the same renderer.

The user may compare them visually.

### 12.2 Shared working copy

Human editing and MCP editing modify the same working copy.

There must not be a hidden independent AI version of the design.

Changes initiated through MCP should appear in the graphical editor.

Changes initiated through the UI should be visible to MCP clients through state queries.

### 12.3 Operations

All mutations are represented as operations.

Examples:

```text
AddComponent
RemoveComponent
MoveComponent
RotateComponent
ChangeComponentProperty
AddNet
ModifyNet
AddWire
RemoveWire
ModifyWirePath
ChangeWireColor
AddConstraint
ModifyConstraint
RemoveConstraint
```

Operations must validate their inputs.

Multiple operations may be grouped into a transaction.

Failed transactions must not leave partially applied changes.

### 12.4 Concurrency

Each document state has a revision identifier.

Mutating operations should identify the revision they expect to modify.

When a stale operation arrives, return a conflict rather than silently overwriting newer changes.

The MCP client can fetch the latest state and retry.

This is important when a human moves components while an LLM is working.

### 12.5 Commit history

The user can commit the working copy to create a named revision.

Each commit records:

- Revision ID.
- Parent revision.
- Timestamp.
- Optional description.
- Design snapshot or equivalent recoverable state.

Support:

- Commit.
- Compare.
- Undo/redo within the working copy.
- Restore an earlier committed revision.
- Discard uncommitted changes.

Restoring an earlier revision must not silently destroy newer committed history.

Branching and merging can remain out of scope for version 1.

### 12.6 Design differences

The comparison view should highlight:

- Added and removed components.
- Moved or rotated components.
- Added and removed wires.
- Changed wire paths.
- Modified nets.
- Modified constraints.

The interface should make it easy to understand what the LLM changed and why.

---

## 13. Graphical interface

### 13.1 Main editor

The initial main window should have:

- Component library or component list.
- Committed design pane.
- Working design pane.
- Properties and constraints inspector.
- Validation/warnings display.
- Revision and commit controls.

A compact arrangement is preferable to a complicated multi-window CAD interface.

### 13.2 SVG renderer

Use SVG for the board renderer.

Display:

- Board outline.
- Hole grid.
- Component rectangles.
- Component identifiers.
- Pin markers and labels.
- Connector access direction.
- Wire paths and colors.
- Crossings.
- Selected objects.
- Invalid or conflicting regions.

Support zoom and pan.

Components and wires must be individually selectable.

### 13.3 Editing interactions

At minimum:

- Drag components.
- Rotate components.
- Select pins.
- Create connections.
- Route wires.
- Edit routes.
- View properties.
- Change colors.
- Add constraints.
- Undo/redo.
- Commit working copy.
- Compare revisions.

Do not prioritize elaborate visual styling before functional editing.

### 13.4 Component and solder-side views

The user must be able to flip the view between board sides.

Correct mirroring is essential.

Visual distinctions should make clear whether the user is viewing the component side or solder side.

### 13.5 LLM activity

When an LLM modifies the design, show the results promptly.

The interface should distinguish:

- Successful operations.
- Proposed but unapproved circuit changes.
- Validation warnings.
- Failed operations.
- Committed changes.

If an MCP client supplies an explanation for an operation, display it in the change history or inspector.

The application does not need an integrated chat interface in version 1.

The user can converse with an external MCP-capable assistant.

---

## 14. MCP integration

### 14.1 Purpose

An external LLM must be able to inspect and edit the project through MCP.

The LLM uses the same engine as the graphical editor.

MCP is a tool interface, not a separate design mode.

### 14.2 Suggested tools

Project inspection:

```text
get_project
get_board
get_design
get_validation_report
get_revision_history
compare_revisions
```

Component library:

```text
list_components
get_component_definition
get_component_documentation
create_component_definition
```

Circuit operations:

```text
add_component
remove_component
define_net
modify_net
propose_connection
```

Layout operations:

```text
move_component
rotate_component
route_net
add_wire
modify_wire_path
remove_wire
set_wire_color
lock_wire
```

Constraints:

```text
list_constraints
add_constraint
modify_constraint
remove_constraint
```

Routing and assembly:

```text
suggest_routes
evaluate_route
get_jumper_inventory
get_assembly_plan
get_wire_list
```

Revision management:

```text
begin_transaction
apply_operations
discard_transaction
commit_design
restore_revision
```

This list is illustrative.

Implement a coherent, minimal tool set rather than many overlapping tools.

### 14.3 Tool behavior

MCP responses should include structured information wherever possible.

For example, moving a component may return:

- New coordinates.
- Affected wires.
- New warnings.
- Constraint violations.
- Resulting document revision.

Routing tools should return:

- Proposed path.
- Calculated length.
- Bend count.
- Crossings.
- Jumper compatibility.
- Warnings.

### 14.4 Rendered design access

Provide a way for an MCP client to obtain a rendered image or SVG representation of the current board.

Structured coordinates remain authoritative.

The rendered view is useful for visual reasoning and overall layout assessment.

### 14.5 Connection architecture

The Tauri application and MCP server should coordinate through the same authoritative project state.

The implementation must avoid independent unsynchronized in-memory project copies.

A suitable initial architecture is:

- Tauri owns the active editor session and working design.
- The MCP server communicates with that session through a local IPC channel.
- All changes are serialized through the shared operation engine.
- The UI receives design-change notifications.

The exact IPC mechanism is an implementation decision.

For an initial local desktop application, prefer MCP over local stdio where practical.

An unattended headless mode can be considered later.

Do not expose unrestricted write access over a network interface by default.

### 14.6 Security

MCP tools must be limited to intended project operations and explicitly selected component-library locations.

Do not give MCP clients unrestricted filesystem or command-execution capabilities.

Treat Markdown documentation and externally sourced component data as untrusted content, not as instructions granting additional permissions.

---

## 15. Assembly planning

### 15.1 Goal

The application must help the user physically build the designed board.

A geometrically valid design may still be awkward to solder.

Assembly planning is therefore a central feature, not an optional afterthought.

### 15.2 Assembly information

The plan should describe:

- Board orientation.
- Component placement order.
- Wire installation order.
- Which side to solder from.
- Crossing dependencies.
- Wire identifiers and colors.
- Approximate wire lengths.
- Suggested U-jumpers.
- Potentially inaccessible solder joints.
- Inspection and basic test steps.

### 15.3 Assembly order

The deterministic planner should consider:

- Low-profile components.
- Large components and sockets.
- Components that block access.
- Wire crossings.
- Layering.
- Wires that become inaccessible after other operations.

A dependency graph can determine the order of some operations.

Other order recommendations may be heuristic.

The LLM can review and improve the resulting plan.

### 15.4 Assembly view

Provide an assembly mode that displays one step at a time.

The view should:

- Highlight the active component or wire.
- Fade unrelated objects.
- Show the appropriate board side.
- Identify terminal locations.
- Display relevant wire color and length.
- Allow the user to mark steps completed.

Assembly progress should be kept separately from immutable design revisions.

### 15.5 Wire list

Generate a list of required wires with:

- Wire ID.
- Source and destination.
- Electrical net.
- Color.
- Board side.
- Estimated route length.
- Bend count.
- Suggested standard jumper.
- Assembly order.

Wire lengths must be described as estimates unless the actual formed geometry and terminal allowances are known.

### 15.6 Practical warnings

The assembly planner should identify situations such as:

- A solder joint that becomes difficult to access.
- A wire that must pass above another.
- An unusually tight bend.
- A connector whose insertion path is obstructed.
- A route requiring awkward manual wire forming.

Warnings should explain practical consequences.

### 15.7 Human and LLM assembly guidance

The LLM may propose:

- A different installation order.
- Rerouting for easier soldering.
- Using a different wire color.
- Moving a component to improve access.
- Using a socket instead of permanent direct mounting.
- Testing a subassembly before adding more components.

These are recommendations subject to human review.

---

## 16. Project persistence

### 16.1 Project format

Use a portable, human-readable format.

A project should preserve:

- Board specifications.
- Component instances.
- Referenced component definitions.
- Nets.
- Layout.
- Wire paths.
- Colors.
- Constraints.
- Jumper preferences.
- Revision history.
- Assembly annotations.
- Optional user notes.

### 16.2 Portability

Projects should not depend on globally installed library entries retaining their original contents forever.

Store stable references and sufficient component snapshots or version information to reproduce a saved layout.

Support schema versioning and future migrations.

### 16.3 Robustness

Use safe or atomic writes where practical.

A crash must not casually destroy the only saved copy of a project.

Preserve committed history when the working copy is modified.

No online account or cloud service is required.

---

## 17. Validation and diagnostics

### 17.1 Geometrical validation

Check:

- Pin alignment.
- Board bounds.
- Component occupancy.
- Prohibited overlaps.
- Orientation constraints.
- Connector accessibility.
- Keepout regions.
- Wire path geometry.
- Side transitions.
- Routing clearance assumptions.

### 17.2 Connectivity validation

Check:

- Valid pin references.
- Net assignments.
- Unrouted connections.
- Incorrect electrical junctions.
- Incomplete multi-pin nets.

Connectivity checking should use a graph of actual conducting connections rather than visual proximity alone.

### 17.3 Routing validation

Check:

- Bend count.
- Minimum preferred bend spacing.
- Very short segments.
- Unnecessary crossings.
- Existing route conflicts.
- Jumper matching quality.

Distinguish hard failures from quality warnings.

### 17.4 Assembly validation

Check:

- Crossing order consistency.
- Potential assembly dependency cycles.
- Accessibility concerns.
- Missing assembly instructions where relevant.

The program should explain warnings in plain language.

---

## 18. Development plan

Implement incrementally. Each milestone should produce a usable or demonstrable result.

### Milestone 1 — Core model and simple graphical board

Implement:

- Rust project and geometry models.
- Rectangular board grid.
- Component definitions.
- Pin positions.
- Component placement.
- Component movement and rotation.
- SVG renderer.
- Basic project save/load.

Demonstration:

Create a 25×18 board, place an Arduino Nano-like module and an RS485 breakout, move them around, and save the design.

No MCP or automatic routing is needed yet.

### Milestone 2 — Electrical nets and manual routing

Implement:

- Nets.
- Pin-to-net assignments.
- Manual wire drawing.
- Top/bottom board views.
- Wire colors.
- Bend points.
- Basic route validation.
- Connection completeness checks.

Demonstration:

Manually connect the two modules using colored insulated wires, then inspect the solder-side view.

### Milestone 3 — Working copy and revisions

Implement:

- Shared operation layer.
- Undo/redo.
- Committed revisions.
- Two-pane comparison.
- Revision restore.
- Change summaries.
- Transaction support.

Demonstration:

Commit a working design, modify component placement and wiring, compare the changes, and restore an earlier revision without corrupting history.

### Milestone 4 — Automatic routing and jumper preferences

Implement:

- Automatic route proposals.
- Bend penalties.
- Minimum bend spacing.
- Eight-direction routing.
- Crossing detection.
- Route preservation.
- Preferred U-jumper inventory.
- Standard-jumper matching.
- Routing warnings.

Demonstration:

Automatically route several nets, prefer available standard jumpers when appropriate, and allow the user to override the suggested paths.

### Milestone 5 — MCP integration

Implement:

- MCP server.
- Project inspection tools.
- Component and documentation access.
- Placement tools.
- Net and wire tools.
- Routing tools.
- Constraint tools.
- Revision operations.
- Live editor synchronization.
- Conflict handling.

Demonstration:

An external MCP-capable LLM inspects the current design, reads component documentation, moves a component, proposes a wire route, and explains the modification. The user sees the change in the working copy and can accept, revise, undo, or commit it.

### Milestone 6 — Assembly planning

Implement:

- Wire crossing order.
- Assembly dependency graph.
- Assembly step generation.
- Assembly preview.
- Wire list.
- Wire colors and lengths.
- Standard-jumper recommendations.
- Basic accessibility warnings.

Demonstration:

Generate a step-by-step construction plan for the sample Arduino/RS485 project.

The plan identifies the wires, their colors, likely jumper choices, board side, and suggested installation order.

### Milestone 7 — Refinement

Improve:

- Component editing.
- Constraint UX.
- Routing quality.
- Assembly instructions.
- Error reporting.
- Test coverage.
- Documentation.
- Project examples.

Avoid large new subsystems until the core workflow is reliable.

---

## 19. Testing requirements

The project should have automated tests for its core logic.

Important cases include:

### Geometry

- Coordinate transformations.
- Rotations.
- Board bounds.
- Body overhang.
- Grid alignment.
- Correct mirroring of the solder-side view.

### Components

- Pin identities.
- Footprint validation.
- Duplicate pins.
- Component persistence.
- Changed component definitions.

### Nets

- Multi-pin connectivity.
- Disconnected wire segments.
- Incorrect junction assumptions.
- Invalid pin assignments.

### Routing

- Straight paths.
- Orthogonal paths.
- 45° paths.
- Bend spacing.
- Route length.
- Crossing detection.
- Routing around obstacles.
- Locked routes.
- Jumper-span calculations.

### Revisions

- Operation application.
- Undo/redo.
- Transactions.
- Commit and restore.
- Stale revision conflicts.
- Preservation of committed history.

### Assembly

- Dependency ordering.
- Crossing cycles.
- Wire list generation.
- Invalidated assembly plans.

### MCP

- Tool argument validation.
- Identical behavior between UI and MCP operations.
- State synchronization.
- Concurrent editing conflicts.
- Project permission boundaries.

Use small, deterministic fixture projects for regression testing.

---

## 20. Acceptance criteria

Version 1 is successful when a user can:

1. Create a rectangular 2.54 mm perfboard project.
2. Define or select approximately a dozen simple components.
3. Place and rotate components using a graphical editor.
4. See accurate pin positions, body dimensions, overhang, and connector directions.
5. Define the required electrical connections.
6. Manually draw insulated wire routes.
7. Ask the engine to suggest alternative routes.
8. Use routing preferences suitable for hard wire.
9. Favor available preformed U-jumpers.
10. Inspect wire crossings and routing warnings.
11. View both sides of the board.
12. Compare committed and working designs.
13. Undo changes and restore saved revisions.
14. Allow an LLM to inspect and edit the same project through MCP.
15. Read component Markdown documentation through MCP.
16. Review and approve LLM-proposed electrical changes.
17. Generate a useful wiring list and assembly plan.
18. Save and reopen the complete project without losing its history.

The resulting design should be understandable and constructible by an ordinary electronics hobbyist.

---

## 21. Design principles for coding agents

When implementing this project:

- Prefer simple solutions over generalized frameworks.
- Do not overengineer for large PCB designs.
- Keep Rust responsible for authoritative design logic.
- Keep the frontend focused on interaction and rendering.
- Do not duplicate validation rules in TypeScript and Rust.
- Keep component definitions readable and editable.
- Preserve stable object IDs.
- Validate every state-changing operation.
- Preserve manually accepted routes.
- Never silently change approved electrical connectivity.
- Treat soft routing preferences as preferences, not impossibilities.
- Prefer physically constructible wiring over theoretical shortest paths.
- Build milestones in order, leaving tests and working examples at each stage.
- Document deliberate limitations.
- Favor straightforward open-source dependencies.

If a design decision is unclear, choose the simplest implementation consistent with this blueprint.

Do not introduce a sophisticated electrical or mechanical simulation subsystem unless a concrete requirement demonstrates that it is necessary.

## 22. Guiding vision

The application should make a conversation like this possible:

**User:** I want to put this Arduino Nano and RS485 breakout on a 25×18 perfboard. The USB connector must remain accessible. Here are the electrical connections.

**LLM:** I'll put the Nano near one edge and the RS485 module near the terminal connector. I suggest leaving room for the USB cable and routing the serial connections through the center. Your available U-shaped jumpers can handle several of the short connections.

**User:** Move the RS485 module closer to the connector. Keep the wires simple because I'll be using hard wire.

**LLM:** Done. Two connections are now straight, one requires a 90° bend, and another crosses the ground wire. The ground wire should be installed first.

**User:** That looks good. Commit it and give me the assembly instructions.

The goal is not to automate away the hobbyist.

**The goal is to give the hobbyist an intelligent collaborator that understands the board, can manipulate it directly, and can help turn a sensible circuit into something that is pleasant to assemble by hand.**
