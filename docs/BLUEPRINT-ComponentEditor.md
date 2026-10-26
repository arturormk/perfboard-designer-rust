# BLUEPRINT — Component Designer

## 1. Purpose

The Component Designer is an integrated mode of Perfboard Designer for
inspecting, creating, and maintaining component-library definitions. It uses
the same component model, validation rules, and rendering semantics as the
perfboard editor, but its workspace and interaction model are independent.

The first release is deliberately a **read-only live component viewer**. A
user edits `component.toml` in a text editor, by hand or with an LLM, while the
application continuously validates and previews the result. Later releases
add graphical authoring, safe persistence, reusable templates, and equivalent
MCP operations.

This document supplements [`BLUEPRINT.md`](BLUEPRINT.md). The main blueprint
remains authoritative for shared component geometry, identifiers, project
snapshots, validation, security boundaries, and application architecture.

### 1.1 Goals

- Make physical component definitions easy to inspect and verify.
- Show immediately whether pins are on the intended 2.54 mm grid positions.
- Keep pin number, name, aliases, and electrical functions visible and
  unambiguous.
- Support hand-authored and LLM-authored TOML equally well.
- Reuse one authoritative Rust parser and validator across the viewer, future
  graphical editor, library loader, and MCP tools.
- Prevent library edits from silently changing existing projects.
- Grow from a small useful viewer into an editor without replacing the file
  format or creating a second component model.

### 1.2 Non-goals

The Component Designer is not:

- A general-purpose vector drawing application.
- A PCB footprint or schematic-symbol editor comparable to a professional EDA
  package.
- A datasheet scraper or a guarantee that copied dimensions and pinouts are
  correct.
- A 3D mechanical or enclosure-design tool.
- A mechanism for silently updating component snapshots already embedded in
  projects.
- An autonomous component generator whose output bypasses human review.

## 2. Product integration

The application provides a top-level mode switch between **Perfboard
Designer** and **Component Designer**. Switching modes changes the workspace,
tools, inspector, and current selection, but does not start another process or
open another application.

The Component Designer operates on a component-library directory selected by
the user. A component directory contains:

```text
component-name/
  component.toml
  README.md          # optional
```

The component workspace is distinct from an active perfboard project:

- Opening or reloading a component never mutates the project.
- Editing a library definition never alters a definition snapshot embedded in
  an existing project.
- Adopting a revised definition in a project is always an explicit upgrade
  operation with a change preview.

## 3. First release: live component viewer

### 3.1 Opening a component

The user selects a component directory, not an arbitrary implicit filesystem
location. The application reads `component.toml` and, when present,
`README.md` from that directory at runtime.

The viewer must also offer an explicit **Reload** action. Automatic file
watching is a convenience and must not be the only way to refresh the preview.

The current bundled library may continue to be compiled into the application
for placement until runtime library management is implemented. The viewer's
runtime loader is a separate read-only boundary in the first release.

### 3.2 Live reload

While a component is open, the application watches `component.toml` and
`README.md` for relevant filesystem changes. Watching must accommodate editors
that save by writing a temporary file and renaming it over the original.

File events are debounced before reading. A burst of changes results in one
logical reload after writes settle. Each reload reads a coherent file, parses
it, validates it, and only then publishes a new preview state.

The watcher is scoped to the selected component directory. It is stopped when
the component is closed, another component is opened, or the application
exits.

### 3.3 Last-known-good behavior

Invalid edits must not blank the canvas or replace a valid definition.

```text
file change or Reload
        |
        v
read, parse, and validate
        |
        +-- valid ----> replace preview and clear stale diagnostics
        |
        +-- invalid --> retain last valid preview and show diagnostics
```

When no valid definition has yet been loaded, the canvas shows an empty error
state with the diagnostics. When a subsequent edit becomes valid, the viewer
recovers automatically.

The UI visibly distinguishes the current on-disk state from the last valid
preview. It must not imply that stale preview geometry represents the invalid
file currently on disk.

### 3.4 Diagnostics

Parsing and validation are authoritative Rust behavior. Diagnostics should
include, where the underlying parser can provide them:

- A stable diagnostic code.
- Severity.
- Plain-language message.
- File name.
- Line and column or source span.
- The affected field or pin identifier.

The viewer displays all useful diagnostics from a load attempt rather than
hiding them behind a generic failure. TypeScript must not reimplement Rust
validation rules.

## 4. Preview canvas

### 4.1 Coordinate system and scale

The canvas uses the component definition's canonical local coordinate system.
Pin and occupied-hole positions use signed integer grid coordinates. Adjacent
grid positions represent one component pitch, normally 2.54 mm in the
perfboard application.

The preview shows:

- A visible hole grid at the active pitch.
- The local origin and coordinate axes.
- The component body using its millimetre dimensions and offset.
- Pins at their exact local grid coordinates.
- Additional occupied holes.
- Keepout regions.
- Access regions, directions, and overhang policy.
- Body and feature overhang outside the pin grid.
- An orientation or pin-one marker where available.

The initial component schema assumes the application's standard pitch for pin
coordinates. If support for another pitch is later required, it must be an
explicit schema capability rather than a display-only assumption.

### 4.2 View controls

The viewer provides:

- Zoom in and out.
- Pan.
- Fit component to view.
- Reset to canonical orientation.
- A control for each allowed 90-degree rotation.
- Optional overlays for dimensions, keepouts, access regions, occupied holes,
  and labels.

Rotation changes only the preview. It does not modify the definition file.
Disallowed rotations are not offered as normal choices.

### 4.3 Rendering consistency

The component preview and perfboard canvas must share rendering primitives or
be covered by equivalence tests. Body bounds, rotation, pins, keepouts, access
regions, and label semantics must not drift between the two views.

The structured definition is the source of truth. SVG output is derived and is
not stored as authoritative component data.

## 5. Labels and pin information

### 5.1 Label policy

Labels are intentionally constrained:

- The component label is generated from the definition's `name`.
- Pin labels are generated from structured pin metadata.
- Helper labels identify structured physical features, such as an access
  region labeled `USB port` or `Screw terminals`.
- Arbitrary decorative text and unrestricted drawing annotations are excluded
  from the first release.

Pin labels are anchored to their pins and cannot be independently moved away
from the pin geometry. This prevents a visual label from accidentally
identifying the wrong hole.

The first viewer uses existing access-region labels for helper labels. A new
annotation schema is added only if real component definitions demonstrate a
need that cannot be expressed as a named body, pin, keepout, or access feature.

### 5.2 Pin label modes

The canvas supports these display modes:

- **Number** — the authored physical pin marking.
- **Name** — the primary human-readable pin name.
- **Function** — one or more structured electrical functions.
- **Combined** — number, name, and functions where space permits.
- **Hidden** — pin markers remain visible without canvas text.

Dense definitions may make the combined mode visually crowded. The renderer
may use compact labels, collision avoidance, leader lines, hover details, or
selection highlighting, but it must never silently substitute pin identity or
change the pin's coordinates.

### 5.3 Pin inspector

A pin table remains available regardless of canvas label mode. It lists at
least:

- Pin number.
- Stable pin ID.
- Name.
- Aliases.
- Functions.
- Local `(x, y)` grid position.

Selecting a pin on the canvas selects the corresponding table row and vice
versa. The inspector is the unambiguous detailed view when canvas labels are
abbreviated or overlap.

Electrical functions are descriptive metadata. Displaying a function does not
constitute electrical-rule checking or proof that a connection is safe.

## 6. Documentation view

When present, `README.md` is displayed alongside the structured preview. It is
untrusted reference content and must be sanitized before rich rendering.

The documentation view may show pinout explanations, voltage and current
limits, orientation notes, assembly advice, and datasheet links. Statements in
Markdown do not override structured geometry or grant filesystem, network, or
application permissions.

A missing README is a normal state, not a component-validation failure.

## 7. Validation and correctness boundaries

The viewer applies all validation required by the current component format,
including:

- Supported schema version.
- Nonblank stable definition identity and version.
- Valid finite body dimensions.
- Unique allowed rotations.
- Unique, nonblank pin IDs and physical pin numbers.
- Nonblank pin names.
- Valid keepout and access rectangles.

Future validation should include any new invariants introduced by authoring,
such as invalid feature references or unsupported label placement.

Passing validation means that a definition is internally consistent. It does
not prove that dimensions, markings, functions, or pinouts match a physical
part. The UI must remind users to compare critical definitions with the exact
part and an authoritative datasheet before assembly.

## 8. Filesystem and security model

- Component access is limited to roots explicitly selected by the user.
- Paths are canonicalized and checked against the selected root before reads
  or future writes.
- The viewer does not expose a general-purpose filesystem API to the frontend
  or MCP server.
- Markdown and externally sourced component data are treated as untrusted.
- File contents are not interpreted as application instructions.
- Opening a component does not execute scripts, fetch remote assets, or follow
  arbitrary references.
- Symlink and rename behavior must not allow a watcher or save operation to
  escape the selected root.

## 9. Future graphical editor

After the viewer is reliable, the Component Designer may edit:

- Component identity, version, name, and description.
- Body dimensions and offset.
- Allowed rotations.
- Pin identity, number, name, aliases, functions, and grid position.
- Additional occupied holes.
- Keepout regions.
- Access regions and helper labels.
- Markdown documentation.

All changes pass through typed Rust operations and validation. The frontend
may hold transient form state but is not the authoritative component model.

The graphical editor requires:

- Undo and redo for the current component draft.
- Explicit dirty and valid/invalid state.
- A preview driven by the same draft being edited.
- Atomic save using a sibling temporary file and replacement where supported.
- A recoverable backup when replacing an existing definition.
- Explicit definition version or content-hash updates.
- Protection against overwriting external edits made after the file was
  loaded.

Saving a library definition does not update existing project snapshots.

## 10. Generic component templates

Common through-hole parts should normally be created from constrained
templates rather than duplicated freehand definitions. Candidate families
include:

- Axial resistors with compact and standard lead spans.
- Axial diodes with compact and standard lead spans.
- Radial capacitors with common body sizes and pin spacing.
- 3 mm and 5 mm LEDs.
- Transistor package templates such as TO-92.
- Simple pin headers, sockets, and DIP packages.

Templates produce ordinary component definitions and do not create a second
runtime component type. Package appearance must not imply a pinout. Parts with
different lead ordering require explicit variants and human verification.

## 11. LLM and MCP authoring

An LLM may draft or revise a component through the same operations used by the
graphical editor. Useful tasks include transcribing pin tables, generating
variants, proposing geometry from supplied measurements, and drafting
documentation.

LLM authoring follows this workflow:

```text
authoritative source or user measurements
        |
        v
LLM creates a proposed definition
        |
        v
Rust parser and validator
        |
        v
human preview and dimensional/pinout review
        |
        v
explicit save or replacement
```

MCP tools are restricted to selected component-library roots. They must not
write arbitrary paths or silently replace a definition. Proposed writes expose
the resulting definition, diagnostics, and version change before acceptance.

An LLM is an authoring assistant, not an authority on physical dimensions,
connector orientation, voltage limits, or pin ordering.

## 12. Definition upgrades in projects

Projects retain immutable snapshots of the component definitions used by their
instances. When a newer library definition is available, the application may
offer an explicit upgrade preview showing at least:

- Added, removed, and renamed pins.
- Pin ID, number, function, or coordinate changes.
- Body, keepout, access, occupied-hole, or rotation changes.
- Placement and route diagnostics that the upgrade would introduce.

An upgrade is applied through the normal project operation and revision
system. It is never triggered merely because a watched library file changed.

## 13. Accessibility and interaction

- Every canvas feature has a textual equivalent in the inspector.
- Pins and diagnostics are distinguishable without relying on color alone.
- Keyboard users can select pins, change preview rotation, toggle overlays,
  reload, and move between the canvas and table.
- Zoom does not make text permanently unreadable; detailed metadata remains
  available outside the canvas.
- Watcher status, stale preview state, and validation severity are announced
  accessibly.

## 14. Testing requirements

### Core and format tests

- Valid and invalid component TOML parsing.
- Deterministic validation and diagnostic ordering.
- Signed grid coordinates and negative body overhang.
- All allowed rotations of an asymmetric definition.
- Duplicate and blank pin identity cases.
- Keepout and access rectangle validation.

### Runtime loading and watcher tests

- Opening a selected component directory.
- Missing TOML and optional missing README behavior.
- Explicit reload.
- In-place writes and rename-and-replace saves.
- Event burst debouncing.
- Invalid edit retains the last valid preview.
- Invalid-to-valid recovery.
- Watcher cleanup when switching or closing components.
- Permission-root and symlink escape rejection.

### Frontend and visual tests

- Exact alignment of pins to the preview grid.
- Body dimensions relative to the configured pitch.
- Rotation, origin, overhang, keepout, access, and helper-label rendering.
- Each pin-label mode, including dense layouts.
- Canvas/table selection synchronization.
- Empty, loading, current, stale, and invalid states.
- Accessible keyboard interaction and non-color diagnostic cues.

### Authoring and upgrade tests

- Undo/redo and dirty-state behavior.
- Atomic save and external-change conflicts.
- Backup and recovery after an interrupted or invalid replacement.
- Definition version/hash updates.
- Project snapshots remain unchanged after library edits.
- Explicit upgrade previews and rollback through project history.
- UI and MCP operation equivalence.

## 15. Acceptance criteria

### Live viewer release

The first release is complete when a user can:

1. Switch from the perfboard workspace to the Component Designer.
2. Select a permitted directory containing a component definition.
3. See the body, origin, 2.54 mm grid, pins, occupied holes, keepouts, access
   regions, overhang, and allowed rotations accurately rendered.
4. Inspect every pin by number, stable ID, name, aliases, functions, and local
   coordinates.
5. Toggle useful canvas label modes without losing access to full pin details.
6. Edit `component.toml` externally and see a valid change appear without
   restarting the application.
7. See actionable diagnostics for an invalid edit while retaining a clearly
   marked last-known-good preview.
8. Correct the file and have the preview recover automatically.
9. Reload manually when desired.
10. View sanitized optional Markdown documentation.

The viewer must not mutate the component definition, component library, active
project, or any embedded project snapshot.

### Complete component-authoring workflow

The later editor is complete when a user or MCP client can create or revise a
component, validate and preview every allowed rotation, save it safely with an
explicit version change, and deliberately upgrade a project instance without
silently changing existing layouts or electrical connectivity.

## 16. Guiding principles

- Build the viewer before the editor.
- Keep component definitions portable, inspectable, and human-editable.
- Keep Rust authoritative for parsing, validation, and state-changing
  operations.
- Prefer structured features over arbitrary artwork.
- Keep labels tied to semantic objects.
- Preserve exact pin identity and location across every view.
- Treat validation as consistency checking, not proof of physical correctness.
- Require explicit user approval for writes, replacements, and project
  upgrades.

