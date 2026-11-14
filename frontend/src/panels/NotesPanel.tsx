/**
 * Review notes (ADR 0015): findings an agent added over MCP, which the user
 * deletes as they address them and drags into the order they want to work in.
 */
import { useEffect, useRef, useState } from "preact/hooks";
import type {
  ObjectReference,
  ReviewNote,
  ReviewNoteSeverity,
} from "../api/project";
import { deleteReviewNote, locateObject, moveNote } from "../state/actions";
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
  return (
    <li
      class={`note ${note.severity}${dragging ? " dragging" : ""}`}
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
