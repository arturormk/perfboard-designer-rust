/**
 * Review notes (ADR 0015): findings an agent added over MCP, which the user
 * deletes as they address them and drags into the order they want to work in.
 * The user can also acknowledge a note, or reply and send it back to the
 * agent for another look; the agent's answer shows under the reply (ADR 0016).
 */
import { useEffect, useRef, useState } from "preact/hooks";
import {
  MAX_NOTE_REPLY_CHARS,
  type ObjectReference,
  type ReviewNote,
  type ReviewNoteSeverity,
} from "../api/project";
import {
  answerNote,
  deleteReviewNote,
  locateObject,
  moveNote,
} from "../state/actions";
import { toast } from "../state/toasts";
import {
  design,
  railLabelMap,
  reviewNotes,
  view,
  saddleLabelMap,
  wireLabelMap,
} from "../state/store";
import { Icon, type IconName } from "../ui/Icon";
import { dropSlot, moveTarget } from "./note-order";

const SEVERITY: Record<
  ReviewNoteSeverity,
  { label: string; icon: IconName; count: (n: number) => string }
> = {
  critical: { label: "Critical", icon: "error", count: (n) => `${n} critical` },
  warning: {
    label: "Warning",
    icon: "warning",
    count: (n) => `${n} warning${n === 1 ? "" : "s"}`,
  },
  suggestion: {
    label: "Suggestion",
    icon: "spark",
    count: (n) => `${n} suggestion${n === 1 ? "" : "s"}`,
  },
  info: { label: "Info", icon: "info", count: (n) => `${n} info` },
};
const SEVERITIES = Object.keys(SEVERITY) as ReviewNoteSeverity[];

/** Edge band, in pixels, where dragging scrolls the panel. */
const SCROLL_EDGE = 36;

interface Drag {
  id: string;
  from: number;
  pointerId: number;
  /** Item boxes relative to the list's top when the drag began. */
  rects: { top: number; height: number }[];
  /** Pointer's offset from the list's top when the drag began. */
  startY: number;
  /** Current pointer offset from the list's top. */
  y: number;
  /** Distance the other notes shift to make room. */
  shift: number;
}

/** A readable name for a referenced object, or undefined when it is gone. */
function objectLabel(target: ObjectReference): string | undefined {
  const current = design.value;
  if (!current) return undefined;
  switch (target.kind) {
    case "component":
      return current.components?.[target.id]?.name;
    case "net":
      return current.nets?.[target.id]?.name;
    case "wire":
      return current.wires?.[target.id]
        ? (wireLabelMap.value.get(target.id) ?? "Wire")
        : undefined;
    case "rail":
      return current.rails?.[target.id]
        ? (railLabelMap.value.get(target.id) ?? "Rail")
        : undefined;
    case "saddle":
      return current.saddles?.[target.id]
        ? (saddleLabelMap.value.get(target.id) ?? "Saddle")
        : undefined;
    default:
      return undefined;
  }
}

function ObjectChips({ objects }: { objects: ObjectReference[] }) {
  return (
    <div class="note-objects">
      {objects.map((target) => {
        const label = objectLabel(target);
        return (
          <button
            key={`${target.kind}:${target.id}`}
            class={`note-object${label ? "" : " missing"}`}
            disabled={!label}
            title={
              label
                ? `Show ${label} on the board`
                : `This ${target.kind} is no longer in the design`
            }
            onClick={() => locateObject(target, { keepTab: true })}
          >
            <Icon name="target" size={12} />
            {label ?? `missing ${target.kind}`}
          </button>
        );
      })}
    </div>
  );
}

/** The revision the note describes, flagged once the design has moved on. */
function NoteRevision({ revision }: { revision?: number }) {
  const current = view.value?.revision;
  if (revision === undefined || current === undefined) return null;
  const changed = revision !== current;
  return (
    <span
      class={`note-revision${changed ? " changed" : ""}`}
      title={
        changed
          ? `Written about revision ${revision}. The design has changed since (now revision ${current}), so the note may be out of date.`
          : `Written about revision ${revision}, the current design.`
      }
    >
      rev {revision}
      {changed && " · edited since"}
    </span>
  );
}

/** What to paste into the agent chat to get the sent-back notes reviewed. */
const REVIEW_PROMPT =
  "Please look again at the review notes I sent back (status review_requested): read my replies, check the current design, and answer each with update_review_note.";

async function copyReviewPrompt(): Promise<void> {
  try {
    await navigator.clipboard.writeText(REVIEW_PROMPT);
    toast("info", "Copied a request for your agent", {
      detail: "Paste it into the agent's chat.",
    });
  } catch {
    toast("info", "Ask your agent:", {
      detail: REVIEW_PROMPT,
      durationMs: 20000,
    });
  }
}

/** Badge for where the note stands, or nothing for a fresh open note. */
function NoteStatus({ note }: { note: ReviewNote }) {
  if (note.status === "acknowledged")
    return (
      <span class="note-status acknowledged" title="You accepted this note">
        <Icon name="check" size={12} />
        Acknowledged
      </span>
    );
  if (note.status === "review_requested")
    return (
      <span
        class="note-status requested"
        title="Waiting for the agent to look at your reply"
      >
        <Icon name="refresh" size={12} />
        Review requested
      </span>
    );
  if (note.response)
    return (
      <span class="note-status answered" title="The agent answered your reply">
        <Icon name="agent" size={12} />
        Answered
      </span>
    );
  return null;
}

/** The user's reply and the agent's response, oldest first. */
function NoteExchange({
  note,
  onEdit,
}: {
  note: ReviewNote;
  onEdit: () => void;
}) {
  if (!note.reply && !note.response) return null;
  return (
    <div class="note-exchange">
      {note.reply && (
        <div class="note-message user">
          <span class="note-message-head">
            <Icon name="user" size={12} />
            You
            <button class="note-message-edit" onClick={onEdit}>
              Edit
            </button>
          </span>
          <p>{note.reply}</p>
        </div>
      )}
      {note.response && (
        <div class="note-message agent">
          <span class="note-message-head">
            <Icon name="agent" size={12} />
            Agent
          </span>
          <p>{note.response}</p>
        </div>
      )}
    </div>
  );
}

/** Edits the reply; submitting with "Request review" also sends it back. */
function ReplyEditor({
  note,
  onDone,
}: {
  note: ReviewNote;
  onDone: () => void;
}) {
  const [text, setText] = useState(note.reply ?? "");
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => ref.current?.focus(), []);
  const length = [...text.trim()].length;
  const tooLong = length > MAX_NOTE_REPLY_CHARS;
  const unchanged = text.trim() === (note.reply ?? "");

  const submit = async (requestReview: boolean) => {
    if (tooLong || busy) return;
    if (!requestReview && unchanged) return onDone();
    setBusy(true);
    const saved = await answerNote(note.id, {
      reply: text.trim(),
      ...(requestReview ? { status: "review_requested" as const } : {}),
    });
    setBusy(false);
    if (saved) onDone();
  };

  return (
    <div class="note-reply-editor">
      <textarea
        ref={ref}
        rows={3}
        value={text}
        placeholder="Why you disagree, what you checked, or a question for the agent…"
        aria-label={`Reply to “${note.title}”`}
        onInput={(event) =>
          setText((event.target as HTMLTextAreaElement).value)
        }
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onDone();
          } else if (
            event.key === "Enter" &&
            (event.ctrlKey || event.metaKey)
          ) {
            event.preventDefault();
            void submit(true);
          }
        }}
      />
      <div class="note-reply-actions">
        <span class={`note-reply-count${tooLong ? " over" : ""}`}>
          {length > MAX_NOTE_REPLY_CHARS * 0.8 &&
            `${length} / ${MAX_NOTE_REPLY_CHARS}`}
        </span>
        <button class="btn small ghost" onClick={onDone} disabled={busy}>
          Cancel
        </button>
        <button
          class="btn small"
          onClick={() => void submit(false)}
          disabled={busy || tooLong}
        >
          Save
        </button>
        <button
          class="btn small primary"
          title="Save the reply and ask the agent to look again (Ctrl+Enter)"
          onClick={() => void submit(true)}
          disabled={busy || tooLong || !text.trim()}
        >
          Request review
        </button>
      </div>
    </div>
  );
}

/** Acknowledge, request a review, and reply. */
function NoteActions({
  note,
  onReply,
}: {
  note: ReviewNote;
  onReply: () => void;
}) {
  const acknowledged = note.status === "acknowledged";
  const requested = note.status === "review_requested";
  return (
    <div class="note-actions">
      <button
        class={`note-action${acknowledged ? " on" : ""}`}
        aria-pressed={acknowledged}
        title={
          acknowledged
            ? "Mark as not acknowledged"
            : "Accept the finding; the agent leaves acknowledged notes alone"
        }
        onClick={() =>
          void answerNote(note.id, {
            status: acknowledged ? "open" : "acknowledged",
          })
        }
      >
        <Icon name="check" size={12} />
        Acknowledge
      </button>
      <button
        class={`note-action${requested ? " on" : ""}`}
        aria-pressed={requested}
        title={
          requested
            ? "Withdraw the review request"
            : note.reply
              ? "Ask the agent to look again, with your reply"
              : "Write a reply and ask the agent to look again"
        }
        onClick={() => {
          if (requested) void answerNote(note.id, { status: "open" });
          else if (note.reply)
            void answerNote(note.id, { status: "review_requested" });
          else onReply();
        }}
      >
        <Icon name="refresh" size={12} />
        Request review
      </button>
      {!note.reply && (
        <button
          class="note-action note-reply"
          title="Reply without sending the note back"
          aria-label={`Reply to “${note.title}”`}
          onClick={onReply}
        >
          <Icon name="reply" size={13} />
        </button>
      )}
    </div>
  );
}

function NoteCard({
  note,
  style,
  dragging,
  onGripDown,
  onGripKey,
}: {
  note: ReviewNote;
  style?: string;
  dragging: boolean;
  onGripDown: (event: PointerEvent) => void;
  onGripKey: (event: KeyboardEvent) => void;
}) {
  const severity = SEVERITY[note.severity];
  const [replying, setReplying] = useState(false);
  const status = note.status ?? "open";
  return (
    <li
      class={`note ${note.severity} status-${status}${dragging ? " dragging" : ""}`}
      style={style}
      data-note-id={note.id}
    >
      <button
        class="note-grip"
        data-note-grip={note.id}
        title="Drag to reorder (or focus and use ↑ ↓)"
        aria-label={`Move “${note.title}”. Use the up and down arrow keys.`}
        onPointerDown={onGripDown}
        onKeyDown={onGripKey}
      >
        <Icon name="grip" size={14} />
      </button>
      <div class="note-body">
        <div class="note-head">
          <span class="note-severity">
            <Icon name={severity.icon} size={13} />
            {severity.label}
          </span>
          <NoteRevision revision={note.revision} />
          <NoteStatus note={note} />
          <button
            class="icon-btn small note-delete"
            title="Delete note"
            aria-label={`Delete “${note.title}”`}
            onClick={() => void deleteReviewNote(note.id)}
          >
            <Icon name="trash" size={14} />
          </button>
        </div>
        <strong class="note-title">{note.title}</strong>
        {note.detail && <p class="note-detail">{note.detail}</p>}
        {note.objects?.length ? <ObjectChips objects={note.objects} /> : null}
        {!replying && (
          <NoteExchange note={note} onEdit={() => setReplying(true)} />
        )}
        {replying ? (
          <ReplyEditor note={note} onDone={() => setReplying(false)} />
        ) : (
          <NoteActions note={note} onReply={() => setReplying(true)} />
        )}
      </div>
    </li>
  );
}

export function NotesPanel() {
  const notes = reviewNotes.value;
  const listRef = useRef<HTMLUListElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  dragRef.current = drag;

  const counts = new Map<ReviewNoteSeverity, number>();
  for (const note of notes)
    counts.set(note.severity, (counts.get(note.severity) ?? 0) + 1);
  const requested = notes.filter(
    (note) => note.status === "review_requested",
  ).length;
  const acknowledged = notes.filter(
    (note) => note.status === "acknowledged",
  ).length;

  const listTop = () => listRef.current?.getBoundingClientRect().top ?? 0;

  /** Where the dragged note's middle is, relative to the list's top. */
  const slotOf = (state: Drag) => {
    const rect = state.rects[state.from]!;
    return dropSlot(
      state.rects,
      rect.top + rect.height / 2 + state.y - state.startY,
    );
  };

  // Scroll the panel while a dragged note is held near its top or bottom.
  useEffect(() => {
    if (!drag) return;
    let frame = 0;
    let clientY = 0;
    const track = (event: PointerEvent) => (clientY = event.clientY);
    const tick = () => {
      const scroller = listRef.current?.closest(".panel-body");
      if (scroller && clientY) {
        const box = scroller.getBoundingClientRect();
        const step =
          clientY < box.top + SCROLL_EDGE
            ? -8
            : clientY > box.bottom - SCROLL_EDGE
              ? 8
              : 0;
        if (step) {
          scroller.scrollTop += step;
          const current = dragRef.current;
          if (current) setDrag({ ...current, y: clientY - listTop() });
        }
      }
      frame = requestAnimationFrame(tick);
    };
    window.addEventListener("pointermove", track);
    frame = requestAnimationFrame(tick);
    return () => {
      window.removeEventListener("pointermove", track);
      cancelAnimationFrame(frame);
    };
  }, [drag?.id]);

  const startDrag = (index: number) => (event: PointerEvent) => {
    if (event.button !== 0 || !listRef.current) return;
    event.preventDefault();
    const top = listTop();
    const rects = [...listRef.current.children].map((child) => {
      const box = child.getBoundingClientRect();
      return { top: box.top - top, height: box.height };
    });
    const gap =
      rects.length > 1 ? rects[1]!.top - rects[0]!.top - rects[0]!.height : 6;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    setDrag({
      id: notes[index]!.id,
      from: index,
      pointerId: event.pointerId,
      rects,
      startY: event.clientY - top,
      y: event.clientY - top,
      shift: rects[index]!.height + gap,
    });
  };

  useEffect(() => {
    if (!drag) return;
    const move = (event: PointerEvent) => {
      const current = dragRef.current;
      if (current && event.pointerId === current.pointerId)
        setDrag({ ...current, y: event.clientY - listTop() });
    };
    const end = (event: PointerEvent) => {
      const current = dragRef.current;
      if (!current || event.pointerId !== current.pointerId) return;
      setDrag(null);
      if (event.type === "pointercancel") return;
      const before = moveTarget(
        notes.map((note) => note.id),
        current.id,
        slotOf(current),
      );
      if (before !== undefined) void moveNote(current.id, before);
    };
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDrag(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    window.addEventListener("keydown", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("keydown", cancel);
    };
  }, [drag?.id, notes]);

  const keyMove = (index: number) => (event: KeyboardEvent) => {
    const slot =
      event.key === "ArrowUp"
        ? index - 1
        : event.key === "ArrowDown"
          ? index + 2
          : undefined;
    if (slot === undefined) return;
    event.preventDefault();
    if (slot < 0 || slot > notes.length) return;
    const id = notes[index]!.id;
    const before = moveTarget(
      notes.map((note) => note.id),
      id,
      slot,
    );
    if (before === undefined) return;
    void moveNote(id, before).then(() =>
      requestAnimationFrame(() =>
        listRef.current
          ?.querySelector<HTMLElement>(`[data-note-grip="${id}"]`)
          ?.focus(),
      ),
    );
  };

  /** Inline transform for each note while one is being dragged. */
  const styleFor = (index: number): string | undefined => {
    if (!drag) return undefined;
    if (index === drag.from)
      return `transform: translateY(${drag.y - drag.startY}px)`;
    const slot = slotOf(drag);
    if (slot > drag.from + 1 && index > drag.from && index < slot)
      return `transform: translateY(${-drag.shift}px)`;
    if (slot < drag.from && index >= slot && index < drag.from)
      return `transform: translateY(${drag.shift}px)`;
    return undefined;
  };

  return (
    <div class={`panel-body${drag ? " note-drag-active" : ""}`}>
      <section class="panel-section">
        {notes.length ? (
          <div class="note-summary">
            {SEVERITIES.filter((severity) => counts.get(severity)).map(
              (severity) => (
                <span key={severity} class={`sev ${severity}`}>
                  <Icon name={SEVERITY[severity].icon} size={14} />
                  {SEVERITY[severity].count(counts.get(severity)!)}
                </span>
              ),
            )}
            {(acknowledged > 0 || requested > 0) && (
              <span class="note-progress">
                {[
                  acknowledged && `${acknowledged} acknowledged`,
                  requested && `${requested} sent back`,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            )}
          </div>
        ) : (
          <div class="all-clear notes-empty">
            <Icon name="agent" size={22} />
            <strong>No review notes</strong>
            <p>
              Ask your agent to review the project. Its findings appear here,
              saved with the project, for you to delete as you address them.
            </p>
          </div>
        )}
        {requested > 0 && (
          <div class="note-review-hint">
            <span>
              {requested === 1
                ? "1 note is waiting for the agent."
                : `${requested} notes are waiting for the agent.`}{" "}
              Ask it to look at them again.
            </span>
            <button class="btn small" onClick={() => void copyReviewPrompt()}>
              Copy request
            </button>
          </div>
        )}
        <ul class="note-list" ref={listRef}>
          {notes.map((note, index) => (
            <NoteCard
              key={note.id}
              note={note}
              style={styleFor(index)}
              dragging={drag?.id === note.id}
              onGripDown={startDrag(index)}
              onGripKey={keyMove(index)}
            />
          ))}
        </ul>
      </section>
    </div>
  );
}
