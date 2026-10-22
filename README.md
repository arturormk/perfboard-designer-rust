# Perfboard Designer

Perfboard Designer is a local-first desktop application for designing small
circuits on isolated-pad perfboard. The project is under active development;
Milestone 0 establishes the development skeleton and data contracts.

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

## Current limitations

This bootstrap does not yet provide board editing. Passing validation will mean
that project data is internally consistent; it will never be a guarantee of
electrical correctness or safety.

## License

MIT
