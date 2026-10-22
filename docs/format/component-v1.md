# Component definition format v1

Status: draft for implementation in Milestone 1

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

## Proposed minimum TOML shape

```toml
schema_version = 1
id = "com.example.part.nano-like"
definition_version = "1.0.0"
name = "Nano-like controller"
origin_x = 0
origin_y = 0
body_width_mm = 18.0
body_height_mm = 45.0
allowed_rotations = ["deg0", "deg90", "deg180", "deg270"]

[[pins]]
id = "d0"
name = "D0 / RX"
x = 0
y = 0
aliases = ["RX"]
functions = ["gpio", "uart_rx"]
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
- Future `keepouts` and `access` tables will carry geometry, severity, labels,
  direction, and whether explicit board-edge overhang is allowed.
- Unknown fields may be retained by migration tooling, but authoritative
  readers must not silently reinterpret an unknown schema version.

The exact tables for overhang, occupied holes, keepouts, access annotations,
and electrical metadata will be settled with parser tests in Milestone 1.
