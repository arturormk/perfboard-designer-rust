# TODO: Fix MCP desktop-session connectivity and UI synchronization

Status: resolved and verified with a live MCP-to-running-desktop component edit
(2026-10-25).

## Resolution

- The Codex project configuration now explicitly inherits `XDG_RUNTIME_DIR`,
  `PERFBOARD_DESIGNER_RUNTIME_DIR`, and `PERFBOARD_DESIGNER_SESSION_ID` into the
  stdio server. This keeps descriptor discovery in the same per-user runtime
  directory as the Tauri process.
- Discovery no longer deletes a validly parsed descriptor merely because its
  endpoint cannot be reached from the caller's environment. It distinguishes no
  application, a missing selected session, multiple live sessions, and found but
  unreachable authenticated endpoints. Unreachable diagnostics include session
  IDs, descriptor paths, and transport errors, but never authentication tokens.
  On Linux, descriptors whose recorded owner PID conclusively no longer exists
  are removed before probing, which handles abrupt `cargo tauri dev` restarts.
- The frontend installs its `project-changed` listener before its initial state
  reads, records changes that arrive during startup, and reports event-emission
  failures in the Tauri log.
- The frontend also polls only the authoritative revision once per second while
  visible and on window focus. It fetches and renders the complete project only
  when that revision changes, providing recovery if an event is missed.
- Dependent MCP calls must still await the preceding JSON-RPC response. The
  integration test does so and verifies that a post-write read sees the new
  revision; concurrently submitted independent JSON-RPC requests are not given
  an artificial ordering guarantee.

Manual retest succeeded: a real MCP stdio client discovered the running desktop
session, read revision 0, added an RS-485 breakout through `apply_operations`,
received revision 1 with no validation diagnostics, and confirmed the component
with a dependent `get_design` read. The project-scoped Codex registration was
subsequently confirmed working as well.

## Context

This note records problems encountered while using the Perfboard Designer MCP
server against a desktop application started with `cargo tauri dev` on
2026-10-24. The bundled Codex MCP connector could not reach the application,
although a manually started copy of the repository's MCP server could reach the
same desktop session when run outside the command sandbox.

No session tokens are included below. Session IDs are ephemeral and abbreviated
where useful.

## Observed problems

### 1. The configured MCP connector reported that the app was not running

Calls through the configured `mcp__perfboard_designer` connector, including
`list_components`, `get_design`, and `get_project`, consistently returned:

```json
{
  "ok": false,
  "error": {
    "code": "app.not_running",
    "message": "Perfboard Designer is not running",
    "retryable": false
  }
}
```

This happened while the development desktop app was running and had created a
session descriptor under:

```text
/run/user/1000/perfboard-designer/session-<uuid>.json
```

At one point two descriptor files existed. One belonged to an earlier packaged
build and was stale; the newer descriptor was created by the development app.
The configured connector reported `app.not_running`, rather than identifying
the live descriptor or reporting multiple sessions.

### 2. Sandboxed and unsandboxed MCP clients saw different connectivity

Running `target/debug/perfboard-mcp-server` manually with
`PERFBOARD_DESIGNER_SESSION_ID` set to the newer session reproduced a related
failure inside the normal command sandbox:

```text
selected Perfboard Designer session `<session-id>` was not found
```

Running the same command with permission to access the host environment worked.
The MCP server then successfully served `get_design`,
`get_component_definition`, and `apply_operations` over stdio JSON-RPC.

This suggests that descriptor discovery and the loopback TCP probe are crossing
different runtime, process, or network namespaces. The descriptor directory was
readable from the sandbox, but the loopback listener was not reachable there.
The configured connector may have a similar isolation or environment mismatch.

Areas to investigate:

- Confirm which `XDG_RUNTIME_DIR`, `HOME`, and
  `PERFBOARD_DESIGNER_RUNTIME_DIR` values are visible to the configured MCP
  process and to the Tauri process.
- Confirm whether the MCP process and Tauri process share a loopback network
  namespace.
- Make discovery diagnostics distinguish "no descriptor", "descriptor found
  but endpoint unreachable", "selected session not found", and "multiple live
  sessions".
- Include the descriptor path and session ID in debug logs, but never log the
  authentication token.
- Decide whether stale descriptors should be removed more eagerly or retained
  with an explicit stale/unreachable diagnostic.
- Document the required Codex MCP launch environment for a development build.

### 3. Back-to-back stdio requests can race

For the manual workaround, initialization, `get_design`, `apply_operations`,
and a second `get_design` were written to stdin as consecutive JSON-RPC lines.
The first read returned revision 3. The write succeeded and returned revision 4,
but the immediately queued verification read returned revision 3. A fresh MCP
connection made after the write returned revision 4 and showed the new
component.

The server appears to handle requests concurrently, so input order does not
guarantee completion order. This is valid for independent JSON-RPC requests,
but clients must await each response before issuing a dependent request.

Potential improvements:

- Add an integration test that sends a write and dependent read close together
  and documents the expected ordering behavior.
- Ensure examples and debugging instructions wait for the write response before
  verifying state.
- Consider whether session-mutating requests should be serialized at the MCP
  adapter boundary, even if read-only requests remain concurrent.

### 4. An MCP write changed backend state but was not visible in the open UI

The manual MCP client added an `RS-485 breakout` at `(2, 10)`, rotation `deg0`.
`apply_operations` returned success with revision 4 and no validation
diagnostics. A later, independent `get_design` call confirmed that the sole live
desktop session contained both:

- `Arduino Nano-like 1` at `(20, 1)`, rotation `deg90`
- `RS-485 breakout 2` at `(2, 10)`, rotation `deg0`

The user did not see the newly added component in the already open project.
This was not a separate headless project: the MCP server is a stateless adapter,
and the write was stored in the live Tauri session backend.

The intended notification path already exists:

- `DesktopSessionHandler::handle(SessionRequest::ApplyTransaction)` calls
  `self.publish(&event)`.
- `publish` emits the Tauri `project-changed` event.
- `frontend/src/main.ts` listens for `project-changed` and calls `refresh()`.

Investigate why that event did not refresh the visible window. Possibilities
include an event-listener startup race, delivery to a different window/app
handle, a frontend exception during `refresh()` or `render()`, or a second UI
window retaining cached state despite sharing the same backend.

Suggested tests:

1. Start `cargo tauri dev` and wait until the frontend has installed its event
   listener.
2. Apply an operation through the MCP server.
3. Assert that the frontend displays the new revision and component without a
   manual reload.
4. Repeat with an MCP write performed during frontend startup.
5. Repeat after opening/replacing a project and with more than one app window.
6. Log event emission and frontend receipt with the revision number to locate
   where notification delivery stops.

## Manual workaround that succeeded

The successful path was:

1. Locate the newest descriptor in
   `/run/user/1000/perfboard-designer/`.
2. Set `PERFBOARD_DESIGNER_SESSION_ID` to that descriptor's session ID.
3. Run `target/debug/perfboard-mcp-server` outside the restricted network
   sandbox.
4. Communicate with it using MCP stdio JSON-RPC.
5. Await the response to each state-dependent request before sending the next
   one.

The normal configured connector should make this workaround unnecessary.

## Proposed acceptance criteria

- With one `cargo tauri dev` instance running, configured MCP calls discover it
  without manual environment variables or elevated execution.
- If no live instance exists, the error explains whether no descriptor was
  found or descriptors were found but unreachable.
- If multiple live instances exist, the error lists their session IDs and
  explains how to select one.
- Stale descriptors do not hide or invalidate a healthy live session.
- An MCP mutation is visible in the already open frontend without a reload.
- A dependent read made after awaiting a successful write observes the returned
  revision.
- Authentication tokens never appear in logs, diagnostics, or test output.
