/** Keeps the view in step with the Rust session, including MCP edits. */
import { listen } from "@tauri-apps/api/event";
import { getProjectRevision, type SessionChangeEvent } from "../api/project";
import { errorMessage } from "../error-message";
import { focusBoard, refresh, refreshLibrary } from "./actions";
import { engine, view } from "./store";

const POLL_INTERVAL_MS = 1200;

export async function startSync(preview: boolean): Promise<void> {
  try {
    await listen<SessionChangeEvent>("project-changed", (event) => {
      const current = view.peek();
      if (!current || event.payload.revision !== current.revision)
        void refresh();
    });
    await Promise.all([refresh(), refreshLibrary()]);
    engine.value = { kind: "ready", preview };
    focusBoard();
  } catch (error) {
    engine.value = { kind: "offline", message: errorMessage(error) };
    return;
  }

  // Events are the primary signal; polling the revision is a cheap safety net
  // for missed events while the window is visible.
  const poll = async () => {
    if (document.visibilityState === "hidden") return;
    const current = view.peek();
    if (!current) return;
    try {
      const revision = await getProjectRevision();
      if (revision !== current.revision) await refresh();
    } catch {
      // Transient bridge errors surface on the next explicit action.
    }
  };
  window.setInterval(() => void poll(), POLL_INTERVAL_MS);
  window.addEventListener("focus", () => void poll());
}
