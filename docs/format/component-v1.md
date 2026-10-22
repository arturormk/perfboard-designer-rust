# Component definition format v1

Status: implemented in Milestone 1

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
id = "com.example.part.nano-like"
definition_version = "1.0.0"
name = "Nano-like controller"
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
name = "D0 / RX"
x = 0
y = 0
aliases = ["RX"]
functions = ["gpio", "uart_rx"]

[[keepouts]]
label = "Module body"
min = { x = 0, y = 0 }
max = { x = 6, y = 14 }

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
- `id` is a stable library identifier; `definition_version` or a canonical
  content hash identifies the exact definition revision.
- Body measurements use millimetres. Pin and mounting positions use signed
  integer local grid coordinates, allowing negative overhangs.
- Pin IDs are stable and unique within a definition. Body geometry and pin
  locations are independent.
- Rotations are restricted to `deg0`, `deg90`, `deg180`, and `deg270`.
- `occupied_holes` adds occupied local grid positions beyond pins. `keepouts`
  are inclusive local-grid rectangles. `access` tables are inclusive regions
  with a direction, label, required flag, and explicit board-overhang policy.
- Unknown fields may be retained by migration tooling, but authoritative
  readers must not silently reinterpret an unknown schema version.

The bundled definitions under `components/` are permanent parsing and example
fixtures. Definition IDs and pin IDs must be unique in their respective scope.
