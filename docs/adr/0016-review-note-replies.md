# ADR 0016: Replying to review notes

- Status: accepted
- Date: 2026-11-14

## Context

Review notes (ADR 0015) are one-way: the agent writes them and the user
deletes them. In practice the user agrees with some findings, disagrees with
others, and has questions about a few. Settling each one in the chat means
going through the notes one by one with the agent. It is often quicker to
mark up the whole list in the editor and then ask the agent to look at the
answers in one pass.

## Decision

A note gains three optional fields, all omitted when empty, so the project
format needs no version change:

- `status`: `open` (the default, not written), `acknowledged` or
  `review_requested`.
- `reply`: the user's answer, plain text, at most 2000 characters.
- `response`: the agent's answer to the reply, with the same limit.

The exchange runs like this:

1. The agent adds notes, which start `open`.
2. In the Notes tab the user **acknowledges** a note (accepts the finding),
   or writes a reply and **requests a review** (disagrees or asks a
   question). A reply can also be saved without sending the note back.
3. The user asks the agent to look again. The tab offers a ready-made
   request to paste into the agent's chat.
4. The agent reads `get_review_notes` with `status: review_requested`,
   checks the current design, and answers each note with
   `update_review_note`: a `response` and, if the finding changed, a new
   severity, title or detail. Writing a response sets the status back to
   `open`, so the next move is the user's: acknowledge, reply again, or
   delete.

There is one reply and one response per note, not a thread. Replying again
replaces the earlier reply, and the agent's next response replaces its
earlier one. Most disagreements settle in one round, and a single pair keeps
the cards short. A thread can be added later without breaking files that
have one pair.

### Who may change what

One request, `UpdateReviewNote { note_id, update }`, carries optional
severity, title, detail, status, reply and response. The editor sends only
status and reply. The MCP tool `update_review_note` exposes only response,
severity, title and detail, and sets the status to `open` itself when it
writes a response. So the agent cannot acknowledge on the user's behalf or
rewrite the user's reply, and the user cannot put words in the agent's mouth.
Agents are told to leave acknowledged notes alone unless asked, and to ask
before removing a note even when they concede it.

### Revisions

When the severity, title, detail or response changes, the note records the
current revision, because the agent has just reviewed it against the current
design. A status change or reply leaves the recorded revision alone, since
the user's answer does not change what the note describes.

Like the other note writes, updates need no expected revision, do not change
the document revision, are not undo steps and count as unsaved changes
(ADR 0015).

## Consequences

- Users can work through a long review in the editor and send back only the
  notes they dispute.
- The editor shows a toast when the agent answers, and badges notes as
  Acknowledged, Review requested or Answered.
- Older builds drop `status`, `reply` and `response` when they save a
  project, as they drop `review_notes` itself.
