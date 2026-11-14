/** Floating controls layered over the board canvas. */
import { useSignal, type ReadonlySignal } from "@preact/signals";
import {
  acceptRoute,
  chooseRouteAlternative,
  rejectRoute,
} from "../state/actions";
import type { RouteStrategy } from "../api/project";
import {
  activeNetId,
  design,
  fadeComponents,
  pendingRoute,
  pinLabels,
  placing,
  showChanges,
  showKeepouts,
  showRatsnest,
  side,
  splitCompare,
  tool,
  view,
  wireColorChoice,
  changeCount,
} from "../state/store";
import { Icon } from "../ui/Icon";
import {
  WIRE_COLORS,
  formatMm,
  suggestedWireColor,
  wireColorCss,
} from "../model/naming";

export function CanvasHud(props: {
  onZoom: (factor: number) => void;
  onFit: () => void;
  zoomPercent: ReadonlySignal<number>;
  wireDrafting: boolean;
  railDrafting: boolean;
  onFinishRail: () => void;
}) {
  return (
    <>
      <div class="hud hud-top-left">
        <ToolOptions {...props} />
      </div>
      <div class="hud hud-top-right">
        <RouteCard />
      </div>
      <div class="hud hud-bottom">
        <ViewOptions />
        <SideSwitch />
        <div class="zoom-group">
          <button
            class="icon-btn"
            title="Zoom out (−)"
            aria-label="Zoom out"
            onClick={() => props.onZoom(1.25)}
          >
            <Icon name="minus" />
          </button>
          <button
            class="zoom-readout"
            title="Fit board (0)"
            onClick={props.onFit}
          >
            {props.zoomPercent}%
          </button>
          <button
            class="icon-btn"
            title="Zoom in (+)"
            aria-label="Zoom in"
            onClick={() => props.onZoom(0.8)}
          >
            <Icon name="plus" />
          </button>
        </div>
      </div>
    </>
  );
}

function SideSwitch() {
  const current = side.value;
  return (
    <div
      class={`side-switch side-${current}`}
      role="radiogroup"
      aria-label="Board face"
    >
      <button
        role="radio"
        aria-checked={current === "component"}
        class={current === "component" ? "on" : ""}
        onClick={() => (side.value = "component")}
        title="Component side, seen from above"
      >
        Component<span class="wide-only"> side</span>
      </button>
      <button
        role="radio"
        aria-checked={current === "solder"}
        class={current === "solder" ? "on" : ""}
        onClick={() => (side.value = "solder")}
        title="Solder side, seen from below (mirrored)"
      >
        Solder<span class="wide-only"> side</span>
      </button>
      {current === "solder" && <span class="mirror-tag">mirrored</span>}
    </div>
  );
}

function NetPicker() {
  const nets = Object.values(design.value?.nets ?? {}).filter(
    (net) => net.state === "accepted",
  );
  return (
    <label class="hud-field">
      <span>Net</span>
      <select
        value={activeNetId.value ?? ""}
        onChange={(event) =>
          (activeNetId.value =
            (event.target as HTMLSelectElement).value || undefined)
        }
      >
        <option value="">From first pin</option>
        {nets.map((net) => (
          <option key={net.id} value={net.id}>
            {net.name}
          </option>
        ))}
      </select>
    </label>
  );
}

function ColorPicker() {
  const choice = wireColorChoice.value;
  const net = activeNetId.value
    ? design.value?.nets?.[activeNetId.value]
    : undefined;
  const automatic = suggestedWireColor(net?.name);
  return (
    <div class="hud-field">
      <span>Colour</span>
      <div class="swatches" role="radiogroup" aria-label="Wire colour">
        <button
          role="radio"
          aria-checked={choice === "auto"}
          class={`swatch auto${choice === "auto" ? " on" : ""}`}
          style={{ "--swatch": wireColorCss(automatic) }}
          title={`Automatic from net name (${automatic as string})`}
          onClick={() => (wireColorChoice.value = "auto")}
        >
          A
        </button>
        {WIRE_COLORS.map((color) => (
          <button
            key={color}
            role="radio"
            aria-checked={choice === color}
            class={`swatch${choice === color ? " on" : ""}`}
            style={{ "--swatch": wireColorCss(color) }}
            title={color}
            aria-label={color}
            onClick={() => (wireColorChoice.value = color)}
          />
        ))}
      </div>
    </div>
  );
}

function ToolOptions(props: {
  wireDrafting: boolean;
  railDrafting: boolean;
  onFinishRail: () => void;
}) {
  const current = tool.value;
  if (placing.value)
    return (
      <div class="tool-options">
        <strong>Placing {placing.value.name}</strong>
        <span class="hint">
          Click a hole to place · <kbd>R</kbd> rotate · <kbd>Esc</kbd> cancel
        </span>
      </div>
    );
  if (current === "wire")
    return (
      <div class="tool-options">
        <strong>Wire</strong>
        <NetPicker />
        <ColorPicker />
        <span class="hint">
          {props.wireDrafting ? (
            <>
              Click holes to bend · click a pin to finish · <kbd>⌫</kbd> undo
              bend · <kbd>Esc</kbd> cancel
            </>
          ) : (
            <>Click a pin or a dashed connection to start</>
          )}
        </span>
      </div>
    );
  if (current === "rail")
    return (
      <div class="tool-options">
        <strong>Rail</strong>
        <NetPicker />
        <span class="hint">
          {props.railDrafting ? (
            <>
              Click to add nodes ·{" "}
              <button class="link" onClick={props.onFinishRail}>
                finish
              </button>{" "}
              with <kbd>Enter</kbd> or double-click
            </>
          ) : (
            <>Click half-grid points to lay a bus conductor</>
          )}
        </span>
      </div>
    );
  if (current === "saddle")
    return (
      <div class="tool-options">
        <strong>Saddle</strong>
        <span class="hint">
          Click a free hole, then another across the wires to hold them down ·
          the strap lies on the side you are viewing · <kbd>Esc</kbd> cancel
        </span>
      </div>
    );
  if (current === "bridge")
    return (
      <div class="tool-options">
        <strong>Bridge</strong>
        <span class="hint">
          Click a hole, then a neighbouring hole · bridges are solder-side links
          installed last
        </span>
      </div>
    );
  if (current === "resize")
    return (
      <div class="tool-options">
        <strong>Board size</strong>
        <span class="hint">Drag an edge or corner · snaps to whole holes</span>
      </div>
    );
  return null;
}

const STRATEGY_LABELS: Record<RouteStrategy, [string, string]> = {
  balanced: ["Balanced", "The router's usual balance of length and bends"],
  different_path: [
    "Different path",
    "Keeps off the balanced route where it can: another way through",
  ],
  fewest_bends: ["Fewest bends", "Fewer bends to form in hard wire"],
  shortest: ["Shortest", "The shortest wire, even with more bends"],
  other_side: ["Other side", "The balanced route on the other board side"],
};

function RouteCard() {
  const pending = pendingRoute.value;
  if (!pending) return null;
  const { proposal, alternatives, selected } = pending;
  const net = design.value?.nets?.[proposal.net_id];
  return (
    <div class="route-card" role="dialog" aria-label="Route proposal">
      <header>
        <Icon name="route" />
        <div>
          <strong>
            {alternatives.length > 1 ? "Suggested routes" : "Suggested route"}
          </strong>
          <span>
            {net?.name ?? "Net"}
            {pending.replaceWireId ? " · replaces existing wire" : ""}
          </span>
        </div>
      </header>
      {alternatives.length > 1 && (
        <div class="route-choices" role="tablist" aria-label="Alternatives">
          {alternatives.map((alternative, index) => {
            const [label, hint] = STRATEGY_LABELS[alternative.strategy];
            return (
              <button
                key={alternative.strategy}
                role="tab"
                aria-selected={index === selected}
                class={index === selected ? "on" : undefined}
                title={`${hint} (${index + 1})`}
                onClick={() => chooseRouteAlternative(index)}
              >
                <kbd>{index + 1}</kbd> {label}
              </button>
            );
          })}
        </div>
      )}
      <dl class="route-stats">
        <div>
          <dt>Length</dt>
          <dd>{formatMm(proposal.length_mm)}</dd>
        </div>
        <div>
          <dt>Bends</dt>
          <dd>{proposal.bend_count}</dd>
        </div>
        <div>
          <dt>Crossings</dt>
          <dd>{proposal.crossings.length}</dd>
        </div>
        <div>
          <dt>Side</dt>
          <dd>{proposal.side === "component" ? "Top" : "Bottom"}</dd>
        </div>
      </dl>
      {proposal.jumper_match && (
        <p class="route-jumper">
          <Icon name="check" size={14} /> Fits jumper{" "}
          <strong>{proposal.jumper_match.inventory_id}</strong> (
          {proposal.jumper_match.kind})
        </p>
      )}
      {(proposal.warnings ?? []).map((warning) => (
        <p key={warning} class="route-warning">
          <Icon name="warning" size={14} /> {warning}
        </p>
      ))}
      {proposal.crossings.length > 0 && (
        <label class="route-crossing">
          At crossings, the new wire goes
          <select
            value={pending.newRouteOver ? "over" : "under"}
            onChange={(event) =>
              (pendingRoute.value = {
                ...pending,
                newRouteOver:
                  (event.target as HTMLSelectElement).value === "over",
              })
            }
          >
            <option value="over">over (installed later)</option>
            <option value="under">under (installed first)</option>
          </select>
        </label>
      )}
      <div class="route-actions">
        <button class="btn ghost" onClick={rejectRoute}>
          Discard <kbd>Esc</kbd>
        </button>
        <button class="btn primary" onClick={() => void acceptRoute()}>
          Accept <kbd>⏎</kbd>
        </button>
      </div>
    </div>
  );
}

function ViewOptions() {
  const open = useSignal(false);
  const hasCommit = Boolean(view.value?.head_commit);
  return (
    <div class="view-options">
      <button
        class={`btn hud-btn${open.value ? " on" : ""}`}
        aria-expanded={open.value}
        onClick={() => (open.value = !open.value)}
      >
        <Icon name="layers" /> View
      </button>
      {open.value && (
        <div class="popover" role="dialog" aria-label="View options">
          <div class="popover-row">
            <span>Pin labels</span>
            <div class="segmented small">
              {(["off", "numbers", "names"] as const).map((mode) => (
                <button
                  key={mode}
                  class={pinLabels.value === mode ? "on" : ""}
                  onClick={() => (pinLabels.value = mode)}
                >
                  {mode === "off" ? "Off" : mode === "numbers" ? "No." : "Name"}
                </button>
              ))}
            </div>
          </div>
          <Toggle
            label="Unrouted connections"
            hint="Dashed hints between unjoined pins"
            value={showRatsnest.value}
            onChange={(value) => (showRatsnest.value = value)}
          />
          <Toggle
            label="See-through parts"
            hint="Fade bodies to inspect wires below"
            value={fadeComponents.value}
            onChange={(value) => (fadeComponents.value = value)}
          />
          <Toggle
            label="Keepouts"
            hint="Always show part keepout areas"
            value={showKeepouts.value}
            onChange={(value) => (showKeepouts.value = value)}
          />
          <Toggle
            label="Highlight changes"
            hint={
              hasCommit
                ? `${changeCount.value} since the last commit`
                : "No commit to compare with yet"
            }
            value={showChanges.value}
            disabled={!hasCommit}
            onChange={(value) => (showChanges.value = value)}
          />
          <Toggle
            label="Side-by-side compare"
            hint="Committed design next to the working copy"
            value={splitCompare.value}
            disabled={!hasCommit}
            onChange={(value) => (splitCompare.value = value)}
          />
        </div>
      )}
    </div>
  );
}

export function Toggle({
  label,
  hint,
  value,
  disabled,
  onChange,
}: {
  label: string;
  hint?: string;
  value: boolean;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label class={`toggle${disabled ? " disabled" : ""}`}>
      <span class="toggle-text">
        <span>{label}</span>
        {hint && <small>{hint}</small>}
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={value}
        disabled={disabled}
        onChange={(event) =>
          onChange((event.target as HTMLInputElement).checked)
        }
      />
      <span class="switch" aria-hidden="true" />
    </label>
  );
}
