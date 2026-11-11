import {
  busy,
  cursorHole,
  design,
  engine,
  hover,
  placing,
  problemCounts,
  rightTab,
  side,
  tool,
  view,
} from "../state/store";
import { netOfPin, pinDisplay } from "../model/naming";
import { Icon } from "../ui/Icon";

const HINTS: Record<string, string> = {
  select:
    "Drag parts to move · click wires to edit · drag empty space to pan · ? for shortcuts",
  wire: "Click a pin to start a wire",
  rail: "Click points to lay a rail",
  bridge: "Click two neighbouring holes",
  resize: "Drag the board edges",
};

export function StatusBar() {
  const hole = cursorHole.value;
  const current = design.value;
  const target = hover.value;
  let hoverText = "";
  if (target?.kind === "pin" && current) {
    const label = pinDisplay(current, target.ref);
    const net = netOfPin(current, target.ref);
    hoverText = `${label.component} · pin ${label.pin} ${label.pinName}${net ? ` · ${net.name}` : ""}`;
  }
  const counts = problemCounts.value;
  const state = engine.value;
  return (
    <footer class="statusbar">
      <span class="status-hint">
        {busy.value ??
          (placing.value
            ? `Placing ${placing.value.name}`
            : hoverText || HINTS[tool.value])}
      </span>
      <span class="status-coords" aria-live="off">
        {hole ? (
          <>
            x <b>{hole.x}</b> y <b>{hole.y}</b>
          </>
        ) : (
          "—"
        )}
      </span>
      <span class="status-side">
        {side.value === "component"
          ? "Component side"
          : "Solder side · mirrored"}
      </span>
      <button
        class="status-problems"
        onClick={() => (rightTab.value = "problems")}
        title="Show problems"
      >
        <span class={counts.error ? "sev error" : "sev"}>
          <Icon name="error" size={13} /> {counts.error}
        </span>
        <span class={counts.warning ? "sev warning" : "sev"}>
          <Icon name="warning" size={13} /> {counts.warning}
        </span>
      </button>
      <span class={`status-engine ${state.kind}`}>
        <i />
        {state.kind === "ready"
          ? state.preview
            ? "Preview backend"
            : `Engine · r${view.value?.revision ?? 0}`
          : state.kind === "connecting"
            ? "Connecting…"
            : "Engine offline"}
      </span>
    </footer>
  );
}
