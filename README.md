# Perfboard Designer

Perfboard Designer is a local-first desktop application for designing small
circuits on isolated-pad perfboard. The project is under active development.
Milestone 2 provides the board and component placement editor, named electrical
nets, accepted/proposed connectivity, and manual insulated-wire routing on
either board face through validated Rust operations.

## Prerequisites

- Rust 1.85 or newer
- Node.js 22 or newer and npm
- The [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/)
  for your operating system
- The Tauri CLI (`cargo install tauri-cli --version '^2' --locked`)

## Setup and development

```sh
npm --prefix frontend ci
cargo tauri dev
```

The Tauri command starts Vite automatically. For browser-only frontend work,
run `npm --prefix frontend run dev`; the bridge status will report that Tauri
is unavailable, as expected.

To install the locked frontend dependencies and build an optimized desktop
executable in one step:

```sh
./quickstart.sh
```

## Quality checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix frontend run format:check
npm --prefix frontend run lint
npm --prefix frontend test
npm --prefix frontend run build
```

The authoritative product scope is in [the blueprint](docs/BLUEPRINT.md), and
the milestone sequence is in [the development plan](docs/plan/DEVELOPMENT_PLAN.md).

## Milestone 2 editor

The desktop editor renders a canonical SVG board and supports component
selection, drag placement, rotation, property editing, zoom/pan, validation,
and save/reopen. Bundled Nano-like and RS-485 breakout definitions are
available from the component palette. The populated example lives at
`examples/nano-rs485.perfboard.json`.

Create an accepted net (or a proposal requiring explicit acceptance), select it,
then click component pins to assign them. Manual wire mode connects assigned
pins with half-grid bend points. Existing bends can be edited numerically or
removed from the wire inspector. The component/solder view toggle mirrors only
presentation geometry, so all saved and edited coordinates remain canonical.

Manual edits may temporarily leave a component out of bounds, overlapping, or
blocking a required access region. The editor preserves the edit and reports
the placement problem until a later move or rotation resolves it.

## Current limitations

Automatic route suggestions, crossing-order decisions, and jumper-stock
matching begin in Milestone 4. Passing validation means that project geometry
and basic connectivity are internally consistent; it is never a guarantee of
electrical correctness or safety.

## License

MIT
