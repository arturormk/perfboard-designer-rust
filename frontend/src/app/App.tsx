import { useEffect } from "preact/hooks";
import { AssemblyMode } from "../assembly/AssemblyMode";
import { BoardCanvas } from "../board/BoardCanvas";
import { ComponentDesigner } from "../component-designer/ComponentDesigner";
import { HistoryPanel } from "../panels/HistoryPanel";
import { Inspector } from "../panels/Inspector";
import { NetsPanel } from "../panels/NetsPanel";
import { PartsPanel } from "../panels/PartsPanel";
import { ProblemsPanel } from "../panels/ProblemsPanel";
import { PrintSheet } from "../print/PrintSheet";
import { Dialogs } from "../shell/Dialogs";
import { openDialog, openDialogKind } from "../shell/dialogs";
import { StatusBar } from "../shell/StatusBar";
import { Toasts } from "../shell/Toasts";
import { ToolRail, activateTool } from "../shell/ToolRail";
import { TopBar } from "../shell/TopBar";
import { Welcome } from "../shell/Welcome";
import {
  focusBoard,
  mutate,
  openDialog as openFile,
  redo,
  removeSelection,
  save,
  undo,
} from "../state/actions";
import {
  appMode,
  design,
  engine,
  leftOpen,
  leftTab,
  placing,
  problemCounts,
  proposedNets,
  rightOpen,
  rightTab,
  selection,
  side,
  view,
  zoomRequest,
} from "../state/store";
import { definitionOf, nextRotation } from "../model/geometry";
import { Icon } from "../ui/Icon";

function isTyping(target: EventTarget | null): boolean {
  return Boolean(
    (target as HTMLElement | null)?.closest?.(
      "input, textarea, select, [contenteditable]",
    ),
  );
}

function handleKey(event: KeyboardEvent): void {
  if (event.defaultPrevented) return;
  const modifier = event.ctrlKey || event.metaKey;
  const key = event.key.toLowerCase();
  if (modifier && key === "s") {
    event.preventDefault();
    void save();
    return;
  }
  if (modifier && key === "o") {
    event.preventDefault();
    void openFile();
    return;
  }
  if (
    appMode.value !== "board" ||
    openDialogKind.value ||
    isTyping(event.target)
  )
    return;
  if (modifier && key === "z") {
    event.preventDefault();
    void (event.shiftKey ? redo() : undo());
    return;
  }
  if (modifier && key === "y") {
    event.preventDefault();
    void redo();
    return;
  }
  if (modifier || event.altKey) return;
  const current = design.value;
  const selected = selection.value;
  switch (event.key) {
    case "v":
    case "V":
      activateTool("select");
      return;
    case "w":
    case "W":
      activateTool("wire");
      return;
    case "l":
    case "L":
      activateTool("rail");
      return;
    case "b":
    case "B":
      activateTool("bridge");
      return;
    case "s":
    case "S":
      activateTool("saddle");
      return;
    case "f":
    case "F":
      side.value = side.value === "component" ? "solder" : "component";
      return;
    case "0":
      focusBoard();
      return;
    case "+":
    case "=":
      zoomRequest.value = { factor: 0.8, nonce: Date.now() };
      return;
    case "-":
      zoomRequest.value = { factor: 1.25, nonce: Date.now() };
      return;
    case "?":
      openDialog("shortcuts");
      return;
    case "Delete":
    case "Backspace":
      if (selected) {
        event.preventDefault();
        removeSelection();
      }
      return;
  }
  if (selected?.kind === "component" && current && !placing.value) {
    const instance = current.components?.[selected.id];
    if (!instance) return;
    if (event.key === "r" || event.key === "R") {
      void mutate({
        type: "rotate_component",
        instance_id: instance.id,
        rotation: nextRotation(
          instance.rotation,
          definitionOf(current, instance)?.allowed_rotations,
        ),
      });
      return;
    }
    const step: Record<string, [number, number]> = {
      ArrowLeft: [side.value === "solder" ? 1 : -1, 0],
      ArrowRight: [side.value === "solder" ? -1 : 1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    const delta = step[event.key];
    if (delta) {
      event.preventDefault();
      const position = {
        x: Math.max(0, instance.position.x + delta[0]),
        y: Math.max(0, instance.position.y + delta[1]),
      };
      void mutate({
        type: "move_component",
        instance_id: instance.id,
        position,
      });
    }
  }
}

function LeftPanel() {
  const proposals = proposedNets.value.length;
  return (
    <aside class="side-panel left" aria-label="Parts and nets">
      <div class="panel-tabs" role="tablist">
        <button
          role="tab"
          aria-selected={leftTab.value === "parts"}
          class={leftTab.value === "parts" ? "on" : ""}
          onClick={() => (leftTab.value = "parts")}
        >
          <Icon name="chip" size={15} /> Parts
        </button>
        <button
          role="tab"
          aria-selected={leftTab.value === "nets"}
          class={leftTab.value === "nets" ? "on" : ""}
          onClick={() => (leftTab.value = "nets")}
        >
          <Icon name="net" size={15} /> Nets
          {proposals > 0 && <span class="tab-badge agent">{proposals}</span>}
        </button>
        <button
          class="icon-btn small collapse"
          title="Hide panel"
          aria-label="Hide panel"
          onClick={() => (leftOpen.value = false)}
        >
          <Icon name="panelLeft" size={15} />
        </button>
      </div>
      {leftTab.value === "parts" ? <PartsPanel /> : <NetsPanel />}
    </aside>
  );
}

function RightPanel() {
  const counts = problemCounts.value;
  const steps = view.value?.undo.length ?? 0;
  return (
    <aside class="side-panel right" aria-label="Inspector">
      <div class="panel-tabs" role="tablist">
        <button
          role="tab"
          aria-selected={rightTab.value === "inspect"}
          class={rightTab.value === "inspect" ? "on" : ""}
          onClick={() => (rightTab.value = "inspect")}
        >
          Inspect
        </button>
        <button
          role="tab"
          aria-selected={rightTab.value === "problems"}
          class={rightTab.value === "problems" ? "on" : ""}
          onClick={() => (rightTab.value = "problems")}
        >
          Problems
          {counts.error + counts.warning > 0 && (
            <span class={`tab-badge ${counts.error ? "error" : "warning"}`}>
              {counts.error + counts.warning}
            </span>
          )}
        </button>
        <button
          role="tab"
          aria-selected={rightTab.value === "history"}
          class={rightTab.value === "history" ? "on" : ""}
          onClick={() => (rightTab.value = "history")}
        >
          History
          {steps > 0 && <span class="tab-badge">{steps}</span>}
        </button>
        <button
          class="icon-btn small collapse"
          title="Hide panel"
          aria-label="Hide panel"
          onClick={() => (rightOpen.value = false)}
        >
          <Icon name="panelRight" size={15} />
        </button>
      </div>
      {rightTab.value === "inspect" ? (
        <Inspector />
      ) : rightTab.value === "problems" ? (
        <ProblemsPanel />
      ) : (
        <HistoryPanel />
      )}
    </aside>
  );
}

export function App() {
  useEffect(() => {
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, []);

  const state = engine.value;
  if (state.kind === "offline")
    return (
      <div class="fatal">
        <Icon name="error" size={28} />
        <h1>The design engine is not reachable</h1>
        <p>{state.message}</p>
        <p class="field-note">
          Start the desktop app with <code>cargo tauri dev</code>. For
          browser-only UI work run <code>npm --prefix frontend run dev</code>,
          which uses the preview backend.
        </p>
      </div>
    );

  const empty =
    view.value &&
    !Object.keys(view.value.design.components ?? {}).length &&
    !view.value.commits.length;

  return (
    <div class={`app mode-${appMode.value}`}>
      <TopBar />
      {appMode.value === "board" ? (
        <main
          class={`workspace${leftOpen.value ? "" : " no-left"}${rightOpen.value ? "" : " no-right"}`}
        >
          {leftOpen.value ? (
            <LeftPanel />
          ) : (
            <button
              class="panel-reveal left"
              aria-label="Show parts panel"
              onClick={() => (leftOpen.value = true)}
            >
              <Icon name="panelLeft" size={16} />
            </button>
          )}
          <ToolRail />
          <section class="canvas-area" aria-label="Board">
            <BoardCanvas />
            {empty && <Welcome />}
          </section>
          {rightOpen.value ? (
            <RightPanel />
          ) : (
            <button
              class="panel-reveal right"
              aria-label="Show inspector"
              onClick={() => (rightOpen.value = true)}
            >
              <Icon name="panelRight" size={16} />
            </button>
          )}
        </main>
      ) : appMode.value === "assembly" ? (
        <AssemblyMode />
      ) : (
        <ComponentDesigner />
      )}
      <StatusBar />
      <Toasts />
      <Dialogs />
      <PrintSheet />
    </div>
  );
}
