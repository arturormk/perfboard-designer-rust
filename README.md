# Perfboard Designer

Perfboard Designer is a local-first desktop application for designing small
circuits on isolated-pad perfboard. The project is under active development.
Milestone 5 provides the graphical editor, revisioned transactions,
deterministic automatic routing, and local MCP access to the active desktop
working copy from Codex or Claude Code.

## Prerequisites

- Rust 1.90 or newer
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

## Editor

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

## Component Designer

Switch to Component Designer to open an existing component directory or start
a draft in an empty directory. Metadata, body geometry, rotations, pins,
occupied holes, keepouts, access regions, and Markdown documentation are edited
as a separate Rust-validated draft with undo and redo. Pins can be moved on the
preview and always snap to the integer 2.54 mm grid; the form provides the same
operation without a pointer.

Saving is explicit and is disabled while the draft is invalid. A save fails if
`component.toml` or `README.md` changed externally since the draft was opened.
Successful replacement retains the immediately previous files as
`component.toml.bak` and `README.md.bak` in the component directory. To recover,
close the component, inspect the backup, and copy the wanted backup over its
corresponding live file before reopening. Saved project component snapshots are
not modified by Component Designer saves.

## MCP access from Codex and Claude Code

Start the desktop app first with `cargo tauri dev`. The repository includes a
trusted-project Codex configuration in `.codex/config.toml` and a Claude Code
project configuration in `.mcp.json`. On first use, review and trust the project
configuration. Both clients start the stdio adapter with Cargo; the adapter then
discovers and authenticates to the active desktop session.

From the repository root, verify the Codex registration with:

```sh
codex mcp list
```

Claude Code reads `.mcp.json` automatically. Verify it with:

```sh
claude mcp get perfboard-designer
```

The first adapter startup can take longer while Cargo compiles dependencies.
For a faster reusable executable, run
`cargo build --release -p perfboard-mcp-server`, then replace the configuration
command with the absolute path to `target/release/perfboard-mcp-server`.
Restart Codex after changing `.codex/config.toml`; the checked-in configuration
forwards the per-user runtime directory used by the desktop session host.

Every write tool requires the revision returned by a fresh read. Codex is
configured to approve writes while allowing annotated read tools normally;
Claude Code applies its own MCP permission policy. If more than one desktop
session is running, set `PERFBOARD_DESIGNER_SESSION_ID` in the MCP server's
environment to the session named by the ambiguity error.

The UI listens for session change events and also checks the authoritative
revision while visible, so successful MCP writes should appear without a page
reload. A `session.unreachable` error means a descriptor was found but the MCP
process cannot access its authenticated loopback endpoint; unlike
`app.not_running`, it does not mean that no desktop session was discovered.

The bridge binds only to IPv4 loopback and authenticates every request with an
ephemeral per-launch token stored in the user's runtime directory. It exposes no
arbitrary file or shell tools. Component Markdown returned through MCP is
untrusted reference material, not instructions. See
[ADR 0007](docs/adr/0007-local-mcp-session-bridge.md) for the full lifecycle and
threat boundary.

## Current limitations

Assembly planning begins in Milestone 6, and component-library authoring remains
read-only through MCP until MCP authoring parity is implemented.
Passing validation means that project geometry and basic connectivity are
internally consistent; it is never a guarantee of electrical correctness or
safety.

## License

MIT
