/** Keeps the view in step with the Rust session, including MCP edits. */
import { listen } from "@tauri-apps/api/event";
import {
  acknowledgeCloseRequest,
  getChangeMarker,
  resolveCloseRequest,
  type ProjectView,
  type SessionChangeEvent,
  type UnsavedWork,
} from "../api/project";
import { errorMessage } from "../error-message";
import {
  focusBoard,
  loadRecentProjects,
  refresh,
  refreshLibrary,
  settleUnsavedChanges,
} from "./actions";
import { engine, view } from "./store";

const POLL_INTERVAL_MS = 1200;

/** Whether the view already shows the state a marker or event names. */
function isCurrent(
  current: ProjectView,
  marker: { revision: number; notes_generation?: number },
): boolean {
  return (
    current.revision === marker.revision &&
    (marker.notes_generation === undefined ||
      (current.notes_generation ?? 0) === marker.notes_generation)
  );
}

let closeDialogOpen = false;

/** The backend held the window open over unsaved work; ask what to do. */
async function confirmClose(unsaved: UnsavedWork): Promise<void> {
  if (closeDialogOpen) return;
  closeDialogOpen = true;
  try {
    await acknowledgeCloseRequest();
    // The backend's answer may be newer than the last refresh.
    await refresh();
    const close = await settleUnsavedChanges("close", {
      partDraft: unsaved.part_draft,
    });
    await resolveCloseRequest(close);
  } catch {
    // If the dialog cannot run, the backend lets a second close through.
  } finally {
    closeDialogOpen = false;
  }
}

export async function startSync(preview: boolean): Promise<void> {
  try {
    await listen<SessionChangeEvent>("project-changed", (event) => {
      const current = view.peek();
      // Review-note changes keep the revision, so their events always count.
      if (
        !current ||
        event.payload.kind === "review_notes" ||
        event.payload.revision !== current.revision
      )
        void refresh();
    });
    await listen<UnsavedWork>("close-requested", (event) => {
      void confirmClose(event.payload);
    });
    await Promise.all([refresh(), refreshLibrary()]);
    void loadRecentProjects();
    engine.value = { kind: "ready", preview };
    focusBoard();
  } catch (error) {
    engine.value = { kind: "offline", message: errorMessage(error) };
    return;
  }

  // Events are the primary signal; polling the change marker is a cheap
  // safety net for missed events while the window is visible.
  const poll = async () => {
    if (document.visibilityState === "hidden") return;
    const current = view.peek();
    if (!current) return;
    try {
      if (!isCurrent(current, await getChangeMarker())) await refresh();
    } catch {
      // Transient bridge errors surface on the next explicit action.
    }
  };
  window.setInterval(() => void poll(), POLL_INTERVAL_MS);
  window.addEventListener("focus", () => void poll());
}
