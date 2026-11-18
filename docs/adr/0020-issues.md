# ADR 0020: Issues, the notes the user keeps

- Status: accepted
- Date: 2026-11-18

## Context

Review notes (ADR 0015) and replies (ADR 0016) turned out to be the main way
the user learns from an agent's review. Many notes are not defects to fix
but information and suggestions worth holding on to: why a pull-up is
there, a part to order later, a decision already made. They should survive
the next review. Today they do not reliably: the agent may remove notes when
a fresh review replaces them, and the user, wanting a clean list for the
next round, deletes notes they would rather have kept.

## Decision

### A second list

Projects gain an optional top-level `issues` array beside `review_notes`
([format](../format/project-v2.md#issues)). Entries are `ReviewNote`s, with
the same fields and limits, 500 per list. A note's ID is in at most one of
the two lists.

The editor's Notes tab switches between **Notes** and **Issues**. On a note,
**Keep as issue** moves it to the end of Issues; on an issue, **Back to
Notes** moves it to the end of Notes. Both offer Undo, which puts the note
back where it was. The note is moved unchanged: severity, text, objects,
recorded revision, status, reply and response all go with it, so nothing is
lost by moving back and forth.

Issues are reordered by dragging and deleted like notes. They have no
Acknowledge, Request review or reply controls: the agent cannot answer an
issue, so a review request on one would never be answered. A pending
request is kept but not shown while the note is an issue, and shows again
when it moves back to Notes. A user who wants the agent's view on an issue
moves it back.

A separate array was chosen over a flag on each note so that agents cannot
reach issues by construction (below), and so that each list keeps its own
order.

### Agents read issues and cannot change them

`get_review_notes` returns `issues` beside `review_notes`. Its description
tells agents to take issues into account, not to add notes that repeat
them, and to say in chat when one looks resolved or wrong.

Every MCP note write goes through a session request (`AddReviewNotes`,
`RemoveReviewNotes`, `UpdateReviewNote`, `MoveReviewNote`) that the desktop
app applies to the Notes list only. Asked to change a note that is an
issue, they fail with an error saying only the user can change it. Moving a
note between the lists (`transfer_review_note`), and removing, reordering or
restoring issues, are editor commands only; no session request reaches them.

### Revisions and saving

As with notes, issue writes need no expected revision, leave the document
revision alone, are not undo steps and count as unsaved changes through
`notes_generation` (ADR 0015).

## Consequences

- What the user keeps survives later reviews, whatever the agent does with
  its notes.
- The agent has the user's issues as context in every review.
- Agents cannot edit an issue even when it is out of date; they can only
  say so. Correcting it means moving it back to Notes.
- Builds from before this change drop `issues` when they save a project.
  An MCP server built before it drops `issues` from `get_review_notes`
  until it is restarted.
