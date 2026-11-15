# ADR 0017: Rule checks, batch edit and loading broken projects

- Status: accepted
- Date: 2026-11-15

## Context

Every edit was checked against the whole design, and refused if the result
had any error. Many design rules were also structural invariants, checked
the same way when a file was opened. Three problems followed:

- A person edits one step at a time. Reworking part of a board (moving a
  bridge chain, re-netting pins) often passes through states that break a
  rule, so the edit was refused. The only way through was to delete
  everything involved and start over.
- An agent's transaction was checked after each step, not at the end, so
  even a bulk change could not pass through a rule-breaking state.
- When a rule changed, or a file was saved in a state the current rules
  reject, the project could not be opened at all. The saved validation
  report had the same effect: any difference from the recomputed one
  refused the file.

## Decision

Checks fall into two tiers.

- **Integrity** is what the data model cannot represent: dangling
  references, map keys that do not match ids, degenerate routes, a pin in
  two nets, invalid component definitions. These still refuse an edit or a
  file (`connectivity::validate_structure` and `ProjectDocument::validate`).
- **Design rules** are what a correct board must also meet: no bridge
  shorts (following runs of touching bridges), wires ending on their own
  net, routes on the board, routes not crossing unrelated pins, junctions
  and rail nodes reachable from the wire's face, placement, required
  constraints. These are error diagnostics (`connectivity::rule_diagnostics`
  and the existing placement and constraint diagnostics).

An edit is refused only if it **adds** a design-rule error that was not
there before (`operations::check_new_violations`). A project that already
breaks a rule can still be edited and repaired step by step. Placement
errors are exempt when the request sets `allow_invalid_placement`, as the
desktop app always does.

A transaction is checked once, on its result. Its steps may pass through
states that break rules. This is how agents make bulk changes.

**Batch edit** (`defer_checks` on a request) skips the design-rule check
for that change. It is for the person at the desktop app: the engine
refuses it for any origin but `user`, and the IPC bridge that serves MCP
agents refuses it whatever origin a request claims. In the app it is a
per-session switch in the top bar, off at start. Turning it off lists
what is left in Problems; it does not undo anything.

Saving is always allowed. A file saved mid-rework opens again, with its
rule errors listed.

On load, the stored validation reports (the project's and each commit's)
are recomputed rather than compared, as undo/redo reports already were
(ADR 0013).

## Consequences

- Rule changes no longer lock users out of their projects.
- Files with rule errors open. Only integrity failures, which the app
  cannot produce, still refuse a file.
- A refused edit names the rule it would break, and the app suggests
  batch edit.
- Agents cannot defer checks, but can rework freely inside one
  `apply_operations` call.
- Saddle and constraint-area bounds are still integrity checks, so
  shrinking the board under a saddle or a reserved area is still refused.
