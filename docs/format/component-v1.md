# Component definition format v1

Status: implemented through Milestone 2

A library component lives in its own directory:

```text
component-name/
  component.toml
  README.md
```

`component.toml` is authoritative structured data. `README.md` is untrusted
human-readable documentation and must be sanitized before rich rendering. A
project embeds an immutable snapshot (or content-equivalent representation) of
every definition it uses; library changes never silently alter a project.

## Minimum TOML shape

```toml
schema_version = 1
kind = "standard"
id = "com.example.part.nano-like"
definition_version = "1.0.0"
name = "Nano-like controller"
designator = "U"
origin_x = 0
origin_y = 0
allowed_rotations = ["deg0", "deg90", "deg180", "deg270"]

[body]
x = -1.5
y = -1.5
width = 18.0
height = 45.0

[[pins]]
id = "d0"
number = "1"
name = "D0 / RX"
x = 0
y = 0
aliases = ["RX"]
functions = ["gpio", "uart_rx"]

[[keepouts]]
label = "Module body"
min = { x = 0, y = 0 }
max = { x = 6, y = 14 }

[[wire_keepouts]]
label = "Left pin header"
min = { x = 0, y = 0 }
max = { x = 0, y = 14 }

[[access]]
label = "USB connector"
min = { x = 1, y = -2 }
max = { x = 5, y = -1 }
direction = "north"
required = true
allow_board_overhang = true
```

## Contract

- `schema_version` is required and initially equals `1`.
- `kind` defaults to `standard`. The special `bridge` kind has exactly two pins
  in orthogonally adjacent holes and may not declare `occupied_holes`,
  `keepouts`, `wire_keepouts`, or `access` regions. Bridge endpoints must remain
  on the board, but they may share component pin holes and ignore component
  keepouts.
- The `jumper` kind is a two-pin header that takes a removable shunt. It must
  have exactly two pins and is otherwise placed like a standard part. Each
  placed jumper records whether its shunt is fitted (`shunted` on the
  instance); a fitted shunt joins the two pins electrically, including when
  they sit on two different nets.
- `designator` is optional: one to eight ASCII letters, by convention the
  reference designator prefix (`R` resistor, `C` capacitor, `D` diode, `Q`
  transistor, `U` IC or module, `J` connector, `JP` jumper, `W` wire link).
  A part placed without a name is named with it and the lowest unused
  number, `R1`, `R2`, …; without a designator it is named after `name`
  (`Bridge 1`). Like `name`, `description` and `documentation`, it is
  metadata: placing a definition that differs from a project's stored copy
  only in these fields and `definition_version` refreshes the stored copy.
- `id` is a stable library identifier; `definition_version` or a canonical
  content hash identifies the exact definition revision.
- Body measurements use millimetres. Pin and mounting positions use signed
  integer local grid coordinates, allowing negative overhangs.
- Pin IDs and authored physical pin numbers are stable and unique within a
  definition. `number` is a string because markings may be alphanumeric and
  must never be inferred from array order. Body geometry and pin locations are
  independent.
- Rotations are restricted to `deg0`, `deg90`, `deg180`, and `deg270`.
- `occupied_holes` adds local grid positions covered by the component beyond
  its pins. They block placement and component-side routing and cannot anchor a
  bend, but a solder-side wire may run straight beneath them. Only `pins`
  describe leads that physically obstruct a route through the board.
  `keepouts` are inclusive local-grid rectangles which constrain placement.
  `wire_keepouts` use the same coordinates but constrain only component-side
  wire routing, including half-grid route points between their grid bounds;
  component bodies do not implicitly block wires. `access` tables are inclusive
  regions with a direction, label, required flag, and explicit board-overhang
  policy.
- Unknown fields may be retained by migration tooling, but authoritative
  readers must not silently reinterpret an unknown schema version.

The bundled definitions under `components/` are permanent parsing and example
fixtures. Definition IDs, pin IDs, and pin numbers must be unique in their
respective scope.
