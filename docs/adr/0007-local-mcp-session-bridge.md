# ADR 0007: Authenticated local MCP session bridge

Status: accepted (2026-10-24)

## Context

Milestone 5 lets Codex and Claude Code inspect and edit the same working copy
as the desktop UI. MCP clients launch local servers over standard input/output,
while the authoritative `ProjectSession` must remain owned by the Tauri process.
Starting another project engine inside the MCP process would create divergent
state, duplicate validation policy, and allow last-writer-wins data loss.

The bridge must work on the initially supported desktop platforms without
exposing a network service beyond the local machine. It must also distinguish
stale discovery files from live sessions and avoid giving the MCP adapter
general filesystem or command-execution capabilities.

## Decision

- The MCP adapter is an `rmcp` stdio server. Codex or Claude Code owns its
  lifecycle and communicates with it only through MCP standard I/O.
- Tauri remains the sole owner of `ProjectSession`. UI commands and MCP requests
  call that same revision-checked session and publish the same
  `project-changed` event after successful mutations.
- A transport-neutral `session-ipc` crate defines the request, response, and
  structured error DTOs. Its initial transport is length-prefixed JSON over an
  ephemeral IPv4 loopback TCP socket, chosen for portability across Linux,
  macOS, and Windows.
- At application startup, Tauri creates a random session ID and a bearer
  token made of two random UUIDv4 values (244 random bits), binds only to
  `127.0.0.1`, and writes a per-session descriptor in the current user's
  runtime directory. Unix directories and files are restricted
  to modes `0700` and `0600`. The descriptor is deleted on clean shutdown.
- Discovery authenticates a `Ping`, so malformed files, dead listeners, and
  unrelated processes that inherited a reused port are not treated as live.
  A parsed descriptor is retained when its endpoint is unreachable because the
  caller may be in a different network namespace; the resulting diagnostic
  identifies the session and descriptor without exposing its token. Every
  subsequent request carries the same token and protocol version.
  On Linux, a descriptor is removed without probing when `/proc` proves that its
  recorded owner PID is no longer present.
- With one live session, discovery is automatic. With multiple live sessions,
  the adapter fails closed and reports their IDs. The user selects one with
  `PERFBOARD_DESIGNER_SESSION_ID`. `PERFBOARD_DESIGNER_RUNTIME_DIR` exists for
  isolated tests and advanced launch environments.
- If the app is closed, the adapter returns a structured `app.not_running` or
  `session.unavailable` tool error. Each request reconnects and rediscovers, so
  restarting the desktop app does not require restarting a long-lived client.
- Project-scoped Codex configuration forwards the runtime-directory and session
  selector environment variables into the stdio adapter. The frontend installs
  its event listener before initial reads and uses a lightweight revision poll
  as recovery for missed desktop events.
- The MCP surface accepts domain DTOs and invokes existing project operations.
  It exposes no arbitrary path, shell, process, or network tools. Bundled
  component definitions are read-only in Milestone 5.
- Every mutation requires an expected document revision. Related operations are
  submitted as one atomic transaction with MCP origin and a rationale. Stale
  writes report expected and actual revisions and never retry automatically.
- Component Markdown is returned with an explicit `untrusted_reference` trust
  marker. Rendered SVG is derived from authoritative Rust design data.
- Tools carry MCP annotations. Read tools are read-only and closed-world;
  mutating tools are marked accordingly so Codex can apply write-focused
  approvals. Claude Code consumes the same standard MCP schemas and metadata.

## Consequences

The desktop application must be running before tools can inspect a project.
The stdio process is a small adapter rather than a second application server,
and reconnects cheaply per call. Loopback plus an unguessable per-launch token
prevents another local web page or unauthenticated process from controlling the
session, although a process running as the same OS user with permission to read
that user's runtime files remains inside the trust boundary.

The protocol is versioned independently of MCP and can move to Unix sockets or
named pipes later without changing tool contracts. Component-library writes
remain release-hardening work. Developer configuration uses `cargo run` from
this checkout.

Update (2026-11-13): Linux packages ship the adapter. `.deb` and `.rpm`
bundles install it as `/usr/bin/perfboard-mcp-server` (a Tauri sidecar built
from the same commit as the app, so the session protocol versions match);
users register that path with their MCP client once. The AppImage carries the
adapter inside the image only, so it is not usable as an MCP command.
