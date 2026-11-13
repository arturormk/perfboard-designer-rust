import { useSignal } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";
import {
  forgetRecent,
  openDialog as openFile,
  exportWireTemplates,
  redo,
  save,
  saveAs,
  undo,
} from "../state/actions";
import {
  appMode,
  appVersion,
  changeCount,
  dirty,
  filePath,
  leftTab,
  proposedNets,
  rightTab,
  view,
  engine,
  canCommit,
} from "../state/store";
import { formatVersion } from "../api/bootstrap";
import { Icon } from "../ui/Icon";
import { printBuildDrawing } from "../print/checklist";
import { openDialog } from "./dialogs";
import { otherRecentProjects, RecentProjectList } from "./RecentProjects";

function fileName(path: string | null): string {
  if (!path) return "Not saved yet";
  return path.split(/[\\/]/).pop() ?? path;
}

function FileMenu() {
  const open = useSignal(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node))
        open.value = false;
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, []);
  const run = (action: () => void) => () => {
    open.value = false;
    action();
  };
  const recent = open.value ? otherRecentProjects() : [];
  return (
    <div class="menu" ref={ref}>
      <button
        class={`btn ghost menu-trigger${open.value ? " on" : ""}`}
        aria-haspopup="menu"
        aria-expanded={open.value}
        onClick={() => (open.value = !open.value)}
      >
        File <Icon name="chevronDown" size={14} />
      </button>
      {open.value && (
        <div class="menu-list" role="menu">
          <button role="menuitem" onClick={run(() => openDialog("new-board"))}>
            <Icon name="plus" size={15} /> New board…
          </button>
          <button role="menuitem" onClick={run(() => void openFile())}>
            <Icon name="folder" size={15} /> Open… <kbd>Ctrl O</kbd>
          </button>
          {recent.length > 0 && (
            <>
              <div class="menu-label">
                Recent
                <button
                  class="menu-label-action"
                  title="Remove every project from this list"
                  onClick={() => void forgetRecent()}
                >
                  Clear
                </button>
              </div>
              <RecentProjectList
                projects={recent}
                menu
                onOpen={() => (open.value = false)}
              />
            </>
          )}
          <hr />
          <button role="menuitem" onClick={run(() => void save())}>
            <Icon name="save" size={15} /> Save <kbd>Ctrl S</kbd>
          </button>
          <button role="menuitem" onClick={run(() => void saveAs())}>
            <Icon name="save" size={15} /> Save as…
          </button>
          <hr />
          <button role="menuitem" onClick={run(() => void printBuildDrawing())}>
            <Icon name="print" size={15} /> Print build drawing…
          </button>
          <button
            role="menuitem"
            title="Cut guide and bending templates for wires and rails, as a true-scale PDF"
            onClick={run(() => void exportWireTemplates())}
          >
            <Icon name="print" size={15} /> Export wire templates…
          </button>
          <button role="menuitem" onClick={run(() => openDialog("shortcuts"))}>
            <Icon name="keyboard" size={15} /> Keyboard shortcuts <kbd>?</kbd>
          </button>
          {appVersion.value && (
            <div class="menu-version">
              Perfboard Designer {formatVersion(appVersion.value)}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function TopBar() {
  const current = view.value;
  const proposals = proposedNets.value.length;
  const changes = changeCount.value;
  const uncommittedSteps = current?.undo.length ?? 0;
  const preview = engine.value.kind === "ready" && engine.value.preview;
  return (
    <header class="topbar">
      <div class="brand" aria-label="Perfboard Designer">
        <svg
          viewBox="0 0 24 24"
          width="22"
          height="22"
          aria-hidden="true"
          class="brand-mark"
        >
          <rect x="2" y="2" width="20" height="20" rx="4" />
          {[7, 12, 17].flatMap((y) =>
            [7, 12, 17].map((x) => (
              <circle key={`${x}${y}`} cx={x} cy={y} r="1.4" />
            )),
          )}
          <path d="M7 17 H12 V7 H17" />
        </svg>
      </div>
      <FileMenu />
      <div class="doc-title">
        <span
          class={dirty.value ? "dirty" : ""}
          title={filePath.value ?? "This project has not been saved to a file"}
        >
          {dirty.value && <i class="dirty-dot" aria-label="Unsaved changes" />}
          {fileName(filePath.value)}
        </span>
      </div>
      {preview && (
        <span
          class="badge preview"
          title="Running in a browser without the Rust engine. Edits are simulated and not validated."
        >
          Browser preview
        </span>
      )}

      <nav class="mode-tabs" role="tablist" aria-label="Workspace">
        <button
          role="tab"
          aria-selected={appMode.value === "board"}
          class={appMode.value === "board" ? "on" : ""}
          onClick={() => (appMode.value = "board")}
        >
          <Icon name="board" size={16} /> Board
        </button>
        <button
          role="tab"
          aria-selected={appMode.value === "assembly"}
          class={appMode.value === "assembly" ? "on" : ""}
          onClick={() => (appMode.value = "assembly")}
          title="Build the board step by step"
        >
          <Icon name="check" size={16} /> Assembly
        </button>
        <button
          role="tab"
          aria-selected={appMode.value === "components"}
          class={appMode.value === "components" ? "on" : ""}
          onClick={() => (appMode.value = "components")}
        >
          <Icon name="component" size={16} /> Part designer
        </button>
      </nav>

      <div class="topbar-actions">
        {proposals > 0 && appMode.value === "board" && (
          <button
            class="pill agent"
            onClick={() => (leftTab.value = "nets")}
            title="Connections proposed by the agent need your review"
          >
            <Icon name="agent" size={14} /> {proposals} to review
          </button>
        )}
        <div class="btn-group">
          <button
            class="icon-btn"
            title="Undo (Ctrl Z)"
            aria-label="Undo"
            disabled={!current?.undo.length}
            onClick={() => void undo()}
          >
            <Icon name="undo" />
          </button>
          <button
            class="icon-btn"
            title="Redo (Ctrl Shift Z)"
            aria-label="Redo"
            disabled={!current?.redo.length}
            onClick={() => void redo()}
          >
            <Icon name="redo" />
          </button>
        </div>
        <button
          class="pill"
          onClick={() => (rightTab.value = "history")}
          title={
            current?.head_commit
              ? "Changes since the last commit"
              : "No commit yet"
          }
        >
          <Icon name="history" size={14} />
          {current?.head_commit
            ? `${changes} change${changes === 1 ? "" : "s"}`
            : `${uncommittedSteps} steps`}
        </button>
        <button
          class="btn primary"
          disabled={!canCommit.value}
          onClick={() => openDialog("commit")}
        >
          <Icon name="commit" size={16} /> Commit
        </button>
      </div>
    </header>
  );
}
