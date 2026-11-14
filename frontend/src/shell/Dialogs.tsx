import { useSignal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import { getUndoHistoryBytes } from "../api/project";
import { parseBoardDimensions } from "../editor/board-size";
import { commit, newBoard, trimHistory } from "../state/actions";
import { comparison, view } from "../state/store";
import { Icon } from "../ui/Icon";
import { closeDialog, confirmRequest, openDialogKind } from "./dialogs";
import { LibrariesDialog, PreferencesDialog } from "./LibraryDialogs";
import { Modal } from "./Modal";

// Common prototype board sizes and their usual hole counts.
const PRESETS: Array<[string, number, number]> = [
  ["3 × 7 cm", 10, 24],
  ["4 × 6 cm", 14, 20],
  ["5 × 7 cm", 18, 24],
  ["7 × 9 cm", 24, 30],
];

function NewBoardDialog() {
  const name = useSignal("Untitled project");
  const holesX = useSignal("25");
  const holesY = useSignal("18");
  const error = useSignal("");
  const submit = async (event: Event) => {
    event.preventDefault();
    const dimensions = parseBoardDimensions(holesX.value, holesY.value);
    if (!dimensions || dimensions.holesX > 200 || dimensions.holesY > 200) {
      error.value = "Use whole numbers of holes between 1 and 200.";
      return;
    }
    closeDialog();
    await newBoard(
      name.value.trim() || "Untitled project",
      dimensions.holesX,
      dimensions.holesY,
    );
  };
  const x = Number(holesX.value) || 0;
  const y = Number(holesY.value) || 0;
  return (
    <Modal title="New board" onClose={closeDialog}>
      <form class="modal-body" onSubmit={submit}>
        <label class="field">
          <span>Project name</span>
          <input
            autoFocus
            value={name.value}
            onInput={(event) =>
              (name.value = (event.target as HTMLInputElement).value)
            }
          />
        </label>
        <div class="field">
          <span>Common sizes</span>
          <div class="preset-row">
            {PRESETS.map(([label, px, py]) => (
              <button
                type="button"
                key={label}
                class={`preset${x === px && y === py ? " on" : ""}`}
                onClick={() => {
                  holesX.value = String(px);
                  holesY.value = String(py);
                }}
              >
                <strong>
                  {px} × {py}
                </strong>
                <small>{label}</small>
              </button>
            ))}
          </div>
        </div>
        <div class="field-grid">
          <label class="field">
            <span>Holes across</span>
            <input
              type="number"
              min={1}
              value={holesX.value}
              onInput={(event) =>
                (holesX.value = (event.target as HTMLInputElement).value)
              }
            />
          </label>
          <label class="field">
            <span>Holes down</span>
            <input
              type="number"
              min={1}
              value={holesY.value}
              onInput={(event) =>
                (holesY.value = (event.target as HTMLInputElement).value)
              }
            />
          </label>
        </div>
        <p class="field-note">
          Count holes, not gaps. At 2.54 mm pitch this board spans{" "}
          {(Math.max(0, x - 1) * 2.54).toFixed(1)} ×{" "}
          {(Math.max(0, y - 1) * 2.54).toFixed(1)} mm between outer hole
          centres.
        </p>
        {error.value && <p class="form-error">{error.value}</p>}
        <p class="field-note warn">
          The current working copy is replaced. Save it first if you need it.
        </p>
        <footer class="modal-foot">
          <button type="button" class="btn ghost" onClick={closeDialog}>
            Cancel
          </button>
          <button type="submit" class="btn primary">
            Create board
          </button>
        </footer>
      </form>
    </Modal>
  );
}

function CommitDialog() {
  const name = useSignal("");
  const description = useSignal("");
  const current = view.value;
  const diff = comparison.value;
  const agentSteps =
    current?.undo.filter((entry) => entry.origin === "mcp") ?? [];
  const rationales = agentSteps.flatMap((entry) =>
    entry.rationale ? [entry.rationale] : [],
  );
  const lines = [
    ...diff.components,
    ...diff.nets,
    ...diff.wires,
    ...diff.rails,
    ...(diff.saddles ?? []),
    ...diff.constraints,
  ];
  const submit = async (event: Event) => {
    event.preventDefault();
    if (!name.value.trim()) return;
    closeDialog();
    await commit(name.value.trim(), description.value);
  };
  return (
    <Modal title="Commit working copy" onClose={closeDialog} wide>
      <form class="modal-body" onSubmit={submit}>
        <p class="field-note">
          A commit is a named checkpoint you can compare with and return to. It
          is stored inside the project file.
        </p>
        <label class="field">
          <span>Name</span>
          <input
            autoFocus
            placeholder="e.g. Power routed, RS-485 placed"
            value={name.value}
            onInput={(event) =>
              (name.value = (event.target as HTMLInputElement).value)
            }
          />
        </label>
        <label class="field">
          <span>Description</span>
          <textarea
            rows={3}
            placeholder="Optional"
            value={description.value}
            onInput={(event) =>
              (description.value = (event.target as HTMLTextAreaElement).value)
            }
          />
        </label>
        <div class="commit-review">
          <h3>
            {lines.length} change{lines.length === 1 ? "" : "s"} ·{" "}
            {current?.undo.length ?? 0} steps
            {agentSteps.length ? ` (${agentSteps.length} by the agent)` : ""}
          </h3>
          <ul>
            {lines.slice(0, 12).map((line) => (
              <li
                key={`${line.object_id}:${line.kind}`}
                class={`diff-line ${line.kind}`}
              >
                {line.summary}
              </li>
            ))}
            {lines.length > 12 && (
              <li class="more">…and {lines.length - 12} more</li>
            )}
          </ul>
          {rationales.length > 0 && (
            <>
              <h3>
                <Icon name="agent" size={14} /> Agent explanations
              </h3>
              <ul class="rationales">
                {rationales.slice(-6).map((text, index) => (
                  <li key={index}>{text}</li>
                ))}
              </ul>
            </>
          )}
        </div>
        <footer class="modal-foot">
          <button type="button" class="btn ghost" onClick={closeDialog}>
            Cancel
          </button>
          <button
            type="submit"
            class="btn primary"
            disabled={!name.value.trim()}
          >
            Commit
          </button>
        </footer>
      </form>
    </Modal>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function HistoryDialog() {
  const bytes = useSignal<number | null>(null);
  const keep = useSignal("25");
  const current = view.value;
  useEffect(() => {
    void getUndoHistoryBytes()
      .then((value) => (bytes.value = value))
      .catch(() => (bytes.value = null));
  }, []);
  const total = (current?.undo.length ?? 0) + (current?.redo.length ?? 0);
  return (
    <Modal title="Stored undo history" onClose={closeDialog}>
      <div class="modal-body">
        <p>
          <strong>{total}</strong> undo/redo steps
          {bytes.value !== null && (
            <>
              {" "}
              use about <strong>{formatBytes(bytes.value)}</strong>
            </>
          )}
          in the project file. Each step is saved as the changes it made, not as
          a copy of the whole design.
        </p>
        <p class="field-note">
          Trimming keeps the newest steps. Commits and the current design are
          not affected.
        </p>
        <label class="field">
          <span>Steps to keep</span>
          <input
            type="number"
            min={1}
            value={keep.value}
            onInput={(event) =>
              (keep.value = (event.target as HTMLInputElement).value)
            }
          />
        </label>
        <footer class="modal-foot">
          <button
            class="btn ghost danger-text"
            onClick={async () => {
              closeDialog();
              await trimHistory(0);
            }}
          >
            Delete all
          </button>
          <button class="btn ghost" onClick={closeDialog}>
            Cancel
          </button>
          <button
            class="btn primary"
            onClick={async () => {
              const value = Number(keep.value);
              if (!Number.isSafeInteger(value) || value < 1) return;
              closeDialog();
              await trimHistory(value);
            }}
          >
            Trim
          </button>
        </footer>
      </div>
    </Modal>
  );
}

const SHORTCUTS: Array<[string, string]> = [
  ["V", "Select and move"],
  ["W", "Wire tool"],
  ["L", "Rail tool"],
  ["B", "Bridge tool"],
  ["S", "Saddle (wire tie) tool"],
  ["R", "Rotate selected part (or part being placed)"],
  ["F", "Flip to the other board side"],
  ["Arrows", "Nudge selected part by one hole"],
  ["Delete", "Remove selection"],
  ["Esc", "Cancel / deselect"],
  ["Enter", "Accept suggested route · finish rail"],
  ["Backspace", "Remove last bend while drawing"],
  ["Double-click", "Add a point on a wire · remove a point"],
  ["Alt + click pin", "Toggle pin on the active net"],
  ["Space + drag", "Pan (also middle-drag or drag empty space)"],
  ["Wheel", "Zoom at pointer"],
  ["0", "Fit board"],
  ["Ctrl Z / Ctrl ⇧ Z", "Undo / redo"],
  ["Ctrl S", "Save"],
  ["Ctrl O", "Open"],
  ["?", "This list"],
];

function ShortcutsDialog() {
  return (
    <Modal title="Keyboard and mouse" onClose={closeDialog}>
      <div class="modal-body">
        <dl class="shortcut-list">
          {SHORTCUTS.map(([keys, action]) => (
            <div key={keys}>
              <dt>
                {keys.split(" / ").map((part, index) => (
                  <span key={part}>
                    {index > 0 && " / "}
                    <kbd>{part}</kbd>
                  </span>
                ))}
              </dt>
              <dd>{action}</dd>
            </div>
          ))}
        </dl>
      </div>
    </Modal>
  );
}

function ConfirmDialog() {
  const request = confirmRequest.value;
  if (!request) return null;
  return (
    <Modal title={request.title} onClose={() => request.resolve(false)}>
      <div class="modal-body">
        <p class="confirm-message">{request.message}</p>
        <footer class="modal-foot">
          <button class="btn ghost" onClick={() => request.resolve(false)}>
            Cancel
          </button>
          <button
            autoFocus
            class={`btn ${request.danger ? "danger" : "primary"}`}
            onClick={() => request.resolve(true)}
          >
            {request.confirmLabel}
          </button>
        </footer>
      </div>
    </Modal>
  );
}

export function Dialogs() {
  const kind = openDialogKind.value;
  return (
    <>
      {kind === "new-board" && <NewBoardDialog />}
      {kind === "commit" && <CommitDialog />}
      {kind === "history" && <HistoryDialog />}
      {kind === "shortcuts" && <ShortcutsDialog />}
      {kind === "libraries" && <LibrariesDialog />}
      {kind === "preferences" && <PreferencesDialog />}
      <ConfirmDialog />
    </>
  );
}
