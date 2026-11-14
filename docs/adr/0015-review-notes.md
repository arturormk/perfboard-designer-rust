# ADR 0015: Review notes outside the design history

- Status: accepted
- Date: 2026-11-14

## Context

Asked to review an open project against what the user is trying to build,
an agent produces findings of different severity: an RS-485 pair swapped at a
connector, a missing bulk capacitor, a header that would make probing easier.
Today those findings live only in the chat transcript. The user wants them in
the project, as a list they work through and delete item by item, in an order
they choose.

The MCP adapter writes to the desktop session through revision-checked
requests (ADR 0007). Design changes go through operations, become undo steps
and end up in commits (ADR 0005). Two existing project fields sit outside the
design: `name`, and `component_paths` (ADR 0014).

## Decision

### Storage

Projects gain an optional top-level `review_notes` array beside `name` and
`component_paths`. Each note has a UUID `id`, a `severity` (`critical`,
`warning`, `suggestion`, `info`), a `title`, optional plain-text `detail`,
optional `objects` (the `{ kind, id }` references diagnostics already use),
`created_ms` and `revision`, the document revision current when the note was
added. Array order is the display order. The field is omitted when it
is empty, so it needs no schema version change (see
[project format v2](../format/project-v2.md#review-notes)).

Notes are metadata, not design. They are not undo steps, commits do not
record them, and restoring or comparing commits leaves them alone. A note
about the design should not disappear because the user undid an unrelated
edit, and deleting a note is not a design change to commit.

### Order: severity by default, then whatever the user chooses

A new note goes in front of the first note that is less severe, or at the
end when there is none. Starting from an empty list this keeps the notes in
severity order, critical first, with notes of equal severity in the order
they arrived. The list is never re-sorted, though: once the user (or an
agent, with `move_review_note`) has moved a note, that order stands, and a
new critical note still lands before the first warning wherever that is.
Sorting on display instead would make dragging meaningless, and re-sorting on
every add would undo the user's choices.

### Writes need no expected revision, and leave it alone

Every other write names the revision it was based on, so that a stale writer
cannot overwrite a change it has not seen. Note writes name note IDs instead:
add appends, remove deletes by ID, move puts one note in front of another (or
at the end), and insert puts a deleted note back. None of these can lose
someone else's change. An agent that writes notes while the user drags them
around therefore never gets `revision.stale`, and neither does the user.

Note writes do not advance the revision either. A note describes the design
at the revision it records; if adding it moved the revision on, every note
would point at a revision that never had that design, and an agent's next
design write would fail as stale for no reason. The editor shows each note's
revision and marks notes written before the latest design change.

Because the revision no longer captures every change, save state is tracked
separately. The session counts note changes in memory (`notes_generation`)
and remembers the revision and count at the last save, open or create. The
project view reports `unsaved` from that comparison, the editor's unsaved
marker uses it, and the editor polls both numbers to notice missed changes.
(An earlier draft of this ADR advanced the revision instead. That made notes
lose against an unsaved-changes check based on revisions alone, and stamped
them with a revision they did not describe.)

### Closing over unsaved work

Agents write notes without the user touching the editor, so closing the
window used to lose them silently. The desktop app now intercepts the close
request when the project has unsaved changes or the Part designer has an
unsaved draft, and the editor asks to save, discard or cancel. If the editor
does not acknowledge the request (a hung page), a second close request three
seconds later goes through, so the window can always be closed. Opening
another project or starting a new board asks the same question.

### Who does what

- MCP: `add_review_notes`, `get_review_notes`, `move_review_note`,
  `remove_review_notes`. Agents
  are told to read existing notes before adding more, and to ask before
  removing notes they did not just write.
- Editor: a Notes tab in the right panel, beside Inspect and Problems; History
  moved to the left panel to make room. Clicking a note's object chip selects
  and frames the object without leaving the Notes tab. The user
  deletes notes, with Undo in the confirmation toast, and reorders them by
  dragging the grip or with the arrow keys on it. A toast announces notes
  that arrive from an agent.
- The editor does not author or edit notes. That can be added later without
  changing the format.

Limits keep a runaway agent from bloating the file: 500 notes, titles up to
300 characters, details up to 8000 and 32 object references per note.

## Consequences

- Review findings are saved with the project and travel with it.
- The undo history and commits do not show note changes. Deleting a note can
  be undone only from the toast, while it is showing.
- The revision alone no longer says whether a project changed; clients that
  watch for changes compare the revision and `notes_generation`.
- Object references can dangle after the design changes. The editor shows
  them struck through instead of failing.
- Builds from before this change drop `review_notes` when they save a
  project, as they do `component_paths`.
