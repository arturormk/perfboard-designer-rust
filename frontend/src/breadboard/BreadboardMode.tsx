/**
 * Breadboard view: the design rebuilt on solderless breadboards, to try the
 * circuit on the bench before soldering (ADR 0018).
 *
 * Read-only. The layout is derived by Rust for the current revision and
 * re-derived whenever the design changes. Drag to pan, wheel to zoom.
 */
import {
  useComputed,
  useSignal,
  useSignalEffect,
  type Signal,
} from "@preact/signals";
import { useEffect, useMemo, useRef } from "preact/hooks";
import type {
  Band,
  BreadboardLayout,
  RailPolarity,
  RailSlot,
} from "../api/project";
import { BoardScene } from "../board/BoardScene";
import { HoverCard } from "../board/HoverCard";
import { parsePinKey } from "../board/hit";
import {
  ZOOM_SETTLE_MS,
  clientToBox,
  previewTransform,
  type ViewBox,
} from "../board/zoom-preview";
import { pinBoardPosition, pinPosition, type Design } from "../model/geometry";
import { isDarkWire, wireColorCss, wireColorName } from "../model/naming";
import {
  breadboard,
  breadboardError,
  focusNet,
  hoverJumper,
  loadBreadboard,
  toggleFocusNet,
} from "../state/breadboard";
import {
  design as designSignal,
  hover,
  pinLabels,
  showBreadboardColorCodes,
  view,
  type HoverTarget,
} from "../state/store";
import { Toggle } from "../board/CanvasHud";
import { ColorCodeRow, ColorCodeStrip } from "../ui/ColorCode";
import { Icon } from "../ui/Icon";
import {
  boardMarkings,
  breadboardDesign,
  holeLabel,
  jumperGeometry,
  jumpersByNet,
  layoutBox,
  partWhereabouts,
  railLinks,
} from "./scene";

const SLOT_NAME: Record<RailSlot, string> = {
  left_outer: "Left +",
  left_inner: "Left −",
  right_inner: "Right +",
  right_outer: "Right −",
};

export function BreadboardMode() {
  // Re-derive whenever the design changes, including MCP edits.
  useSignalEffect(() => {
    void view.value?.revision;
    void loadBreadboard();
  });

  const layout = breadboard.value;
  const target = designSignal.value;
  if (!layout || !target)
    return (
      <main class="breadboard breadboard-empty">
        {breadboardError.value ? (
          <p class="form-error">
            Could not lay out the breadboard: {breadboardError.value}
          </p>
        ) : (
          <p class="field-note">Laying out the breadboard…</p>
        )}
      </main>
    );
  if (!layout.parts.length)
    return (
      <main class="breadboard breadboard-empty">
        <Icon name="breadboard" size={28} />
        <h2>Nothing to try out yet</h2>
        <p class="field-note">
          Place parts and connect them on the Board, and they appear here on a
          solderless breadboard.
        </p>
      </main>
    );
  return (
    <main class="breadboard">
      <section class="canvas-area breadboard-canvas" aria-label="Breadboard">
        <BreadboardCanvas layout={layout} />
      </section>
      <BreadboardPanel layout={layout} />
    </main>
  );
}

// ── Canvas ────────────────────────────────────────────────────────────────

function BreadboardCanvas({ layout }: { layout: BreadboardLayout }) {
  const target = designSignal.value!;
  const svgRef = useRef<SVGSVGElement>(null);
  // `viewBox` is the live camera; `shownBox` is what the SVG is drawn with.
  // While the wheel turns, the SVG keeps its old viewBox and is scaled with a
  // CSS transform; it redraws once zooming settles. Both are bound straight
  // to the DOM, so moving the camera never re-renders the scene.
  const viewBox = useSignal<ViewBox>(layoutBox(layout));
  const shownBox = useSignal<ViewBox | null>(null);
  const zoomPreview = useRef<number | null>(null);
  const viewBoxAttr = useComputed(() =>
    (shownBox.value ?? viewBox.value).join(" "),
  );
  const pan = useRef<{ x: number; y: number; box: ViewBox } | null>(null);
  const grabbing = useSignal(false);
  const svgClass = useComputed(
    () =>
      `board-svg readonly breadboard-svg${grabbing.value ? " cursor-grabbing" : ""}`,
  );
  const { width, height } = layout.geometry;

  // Show everything again when the layout grows or shrinks.
  useEffect(() => {
    settleZoom();
    viewBox.value = layoutBox(layout);
  }, [width, height]);

  // Keep the drawn SVG in step with the camera, or preview while zooming.
  useSignalEffect(() => {
    const live = viewBox.value;
    const svg = svgRef.current;
    if (zoomPreview.current !== null) {
      if (svg)
        svg.style.transform = previewTransform(
          svg,
          shownBox.peek() ?? live,
          live,
        );
      return;
    }
    shownBox.value = live;
    if (svg) svg.style.transform = "";
  });

  useEffect(
    () => () => {
      if (zoomPreview.current !== null)
        window.clearTimeout(zoomPreview.current);
    },
    [],
  );

  function settleZoom(): void {
    if (zoomPreview.current === null) return;
    window.clearTimeout(zoomPreview.current);
    zoomPreview.current = null;
    viewBox.value = [...viewBox.peek()] as ViewBox; // re-run the sync effect
  }

  // Wheel zoom around the cursor; non-passive so the page does not scroll.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = viewBox.peek();
      // The pane's layout box, not the SVG's CTM: during a preview the SVG
      // carries a transform and an outdated viewBox.
      const point = clientToBox(svg, box, event.clientX, event.clientY);
      if (!point) return;
      if (zoomPreview.current !== null)
        window.clearTimeout(zoomPreview.current);
      zoomPreview.current = window.setTimeout(settleZoom, ZOOM_SETTLE_MS);
      const factor = Math.exp(event.deltaY * (event.ctrlKey ? 0.01 : 0.0015));
      const widest = Math.max(60, (width + 10) * 2);
      const next = Math.min(widest, Math.max(4, box[2] * factor));
      const applied = next / box[2];
      viewBox.value = [
        point.x - (point.x - box[0]) * applied,
        point.y - (point.y - box[1]) * applied,
        box[2] * applied,
        box[3] * applied,
      ];
    };
    svg.addEventListener("wheel", onWheel, { passive: false });
    return () => svg.removeEventListener("wheel", onWheel);
  }, [width]);

  const markings = useMemo(
    () => boardMarkings(layout.geometry, layout.board_count),
    [layout.geometry, layout.board_count],
  );
  const scene = useMemo(
    () => breadboardDesign(target, layout),
    [target, layout],
  );

  // Hover card for the part or pin under the pointer, as in the editor. The
  // pointer position lives in its own signal, read only by the card, so
  // moving the mouse never re-renders the board.
  const hoverAt = useSignal<{ x: number; y: number } | null>(null);
  function clearHover(): void {
    if (hover.peek()) hover.value = null;
    hoverAt.value = null;
  }
  useEffect(() => clearHover, []);
  function updateHover(event: PointerEvent): void {
    if (pan.current) return clearHover();
    const element = event.target as Element;
    const pinEl = element.closest<SVGElement>("[data-pin]");
    const partEl = element.closest<SVGElement>("[data-component]");
    let next: HoverTarget | null = null;
    if (pinEl) next = { kind: "pin", ref: parsePinKey(pinEl.dataset.pin!) };
    else if (partEl)
      next = { kind: "component", id: partEl.dataset.component! };
    if (JSON.stringify(hover.peek()) !== JSON.stringify(next))
      hover.value = next;
    const host = svgRef.current?.parentElement?.getBoundingClientRect();
    hoverAt.value =
      next && host
        ? { x: event.clientX - host.left, y: event.clientY - host.top }
        : null;
  }

  return (
    <div class="canvas-host">
      <div class="work-pane">
        <svg
          ref={svgRef}
          class={svgClass}
          viewBox={viewBoxAttr}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={`Breadboard layout: ${layout.board_count} board${layout.board_count === 1 ? "" : "s"}, ${layout.jumpers.length} jumpers`}
          onPointerDown={(event) => {
            if (event.button !== 0 && event.button !== 1) return;
            // Panning needs the real geometry, not the zoom preview.
            settleZoom();
            (event.currentTarget as SVGSVGElement).setPointerCapture(
              event.pointerId,
            );
            pan.current = {
              x: event.clientX,
              y: event.clientY,
              box: viewBox.peek(),
            };
            grabbing.value = true;
            clearHover();
          }}
          onPointerMove={(event) => {
            const start = pan.current;
            const matrix = svgRef.current?.getScreenCTM();
            if (!start || !matrix) return updateHover(event);
            viewBox.value = [
              start.box[0] - (event.clientX - start.x) / matrix.a,
              start.box[1] - (event.clientY - start.y) / matrix.d,
              start.box[2],
              start.box[3],
            ];
          }}
          onPointerUp={() => {
            pan.current = null;
            grabbing.value = false;
          }}
          onPointerCancel={() => {
            pan.current = null;
            grabbing.value = false;
          }}
          onPointerLeave={clearHover}
        >
          <Boards layout={layout} markings={markings} />
          <BoardScene
            design={scene}
            side="component"
            mode="readonly"
            idPrefix="bb"
            baseLayer={false}
            rulers={false}
            showAccess
            hoverable
            pinLabels={pinLabels.value}
            showValues
            partBands={partBands(layout)}
            beigeResistors
            overlay={<Jumpers layout={layout} />}
          />
        </svg>
        <BreadboardHover at={hoverAt} layout={layout} scene={scene} />
        <div class="hud hud-top-right">
          <div class="zoom-group">
            <button
              class="icon-btn"
              title="Show everything"
              aria-label="Show everything"
              onClick={() => {
                settleZoom();
                viewBox.value = layoutBox(layout);
              }}
            >
              <Icon name="fit" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Component ID → its colour bands, as the layout reads its value. */
function partBands(layout: BreadboardLayout): Map<string, Band[]> {
  return new Map(
    layout.parts.flatMap((part) =>
      part.value?.bands?.length
        ? [[part.component_id, part.value.bands] as const]
        : [],
    ),
  );
}

/**
 * The editor's hover card for the part or pin under the pointer, with where
 * it sits on the breadboard.
 */
function BreadboardHover({
  at,
  layout,
  scene,
}: {
  at: Signal<{ x: number; y: number } | null>;
  layout: BreadboardLayout;
  scene: Design;
}) {
  const point = at.value;
  const target = hover.value;
  if (!point || !target) return null;
  let detail: string | undefined;
  let extra: preact.ComponentChildren;
  if (target.kind === "pin") {
    const hole = pinPosition(scene, target.ref);
    const part = layout.parts.find(
      (candidate) => candidate.component_id === target.ref.component_id,
    );
    if (hole && part?.placement === "off_board") {
      const lead = layout.jumpers.find(
        (jumper) => jumper.from.x === hole.x && jumper.from.y === hole.y,
      );
      detail = lead
        ? `Beside the boards · lead ${lead.number} to ${lead.to_label}`
        : "Beside the boards · not wired";
    } else if (hole) detail = `On the breadboard at ${holeLabel(layout, hole)}`;
  } else if (target.kind === "component") {
    const part = layout.parts.find(
      (candidate) => candidate.component_id === target.id,
    );
    const instance = scene.components?.[target.id];
    const definition = instance
      ? scene.component_definitions?.[instance.definition_id]
      : undefined;
    if (part && instance && definition)
      detail = partWhereabouts(
        layout,
        part,
        definition.pins.map((pin) => pinBoardPosition(instance, pin)),
      );
    const bands = part?.value?.bands;
    if (bands?.length) extra = <ColorCodeStrip bands={bands} />;
  }
  return <HoverCard at={point} detail={detail} extra={extra} />;
}

const STRIPE: Record<RailPolarity, string> = {
  minus: "bb-stripe minus",
  plus: "bb-stripe plus",
};

/** The boards themselves: plastic, channel, rail stripes, holes, labels. */
function Boards({
  layout,
  markings,
}: {
  layout: BreadboardLayout;
  markings: ReturnType<typeof boardMarkings>;
}) {
  const g = layout.geometry;
  const holes = useMemo(
    () =>
      markings.holes
        .map(({ x, y }) => `M${x - 0.21} ${y - 0.21}h0.42v0.42h-0.42z`)
        .join(""),
    [markings],
  );
  const channelX = (g.left_columns.at(-1)! + g.right_columns[0]!) / 2;
  const offBoard = layout.parts.some((part) => part.placement === "off_board");
  return (
    <g class="bb-boards" aria-hidden="true">
      {markings.origins.map((x0, board) => (
        <g key={board}>
          <rect
            class="bb-plastic"
            x={x0 - 1}
            y={-2}
            width={g.board_columns + 1}
            height={g.rows + 3}
            rx={0.6}
          />
          <rect
            class="bb-channel"
            x={x0 + channelX - 0.7}
            y={-1.6}
            width={1.4}
            height={g.rows + 2.2}
            rx={0.3}
          />
          {layout.board_count > 1 && (
            <text class="bb-board-name" x={x0 + channelX} y={-2.6}>
              Board {board + 1}
            </text>
          )}
        </g>
      ))}
      {markings.stripes.map((stripe, index) => (
        <line
          key={index}
          class={STRIPE[stripe.polarity]}
          x1={stripe.x}
          x2={stripe.x}
          y1={stripe.y1}
          y2={stripe.y2}
        />
      ))}
      <path class="bb-holes" d={holes} />
      {markings.rowNumbers.map((label, index) => (
        <text key={`r${index}`} class="bb-label" x={label.x} y={label.y}>
          {label.text}
        </text>
      ))}
      {markings.columnLetters.map((label, index) => (
        <text key={`c${index}`} class="bb-label" x={label.x} y={label.y}>
          {label.text}
        </text>
      ))}
      <RailTags layout={layout} />
      {offBoard && (
        <g class="bb-off-board">
          <rect
            x={g.off_board_x - 1.5}
            y={-2}
            width={g.width - g.off_board_x + 2.5}
            height={g.height + 2.5}
            rx={0.6}
          />
          <text x={g.off_board_x - 1} y={-2.6}>
            Off the board · Dupont leads
          </text>
        </g>
      )}
    </g>
  );
}

/** Net names on the rails that carry one, above and below the rail. */
function RailTags({ layout }: { layout: BreadboardLayout }) {
  const g = layout.geometry;
  const column = new Map(
    g.rail_columns.map((rail) => [rail.slot, rail.column]),
  );
  return (
    <g class="bb-rail-tags">
      {layout.rails.map((rail) => {
        const x = rail.board * g.board_stride + (column.get(rail.slot) ?? 0);
        const color = wireColorCss(rail.color);
        return [g.rail_first_row - 0.8, g.rail_last_row + 0.8].map((y, end) => (
          <text
            key={`${rail.board}-${rail.slot}-${end}`}
            class={`bb-rail-tag${isDarkWire(rail.color) ? " dark" : ""}`}
            x={x}
            y={y}
            transform={`rotate(-90 ${x} ${y})`}
            text-anchor={end ? "end" : "start"}
            style={{ fill: color }}
          >
            {rail.net_name}
          </text>
        ));
      })}
    </g>
  );
}

function Jumpers({ layout }: { layout: BreadboardLayout }) {
  const net = focusNet.value;
  const hovered = hoverJumper.value;
  // Draw the highlighted ones last, on top.
  const ordered = [...layout.jumpers].sort(
    (a, b) =>
      Number(a.id === hovered || a.net_id === net) -
      Number(b.id === hovered || b.net_id === net),
  );
  return (
    <g class="bb-jumpers">
      {ordered.map((jumper) => {
        const { d, mid } = jumperGeometry(jumper);
        const color = wireColorCss(jumper.color);
        const focused = jumper.id === hovered || jumper.net_id === net;
        const dim = (hovered || net) && !focused;
        return (
          <g
            key={jumper.id}
            class={`bb-jumper ${jumper.kind}${focused ? " focus" : ""}${dim ? " dim" : ""}${isDarkWire(jumper.color) ? " dark" : ""}`}
            onPointerEnter={() => (hoverJumper.value = jumper.id)}
            onPointerLeave={() => {
              if (hoverJumper.peek() === jumper.id) hoverJumper.value = null;
            }}
          >
            <title>
              {`${jumper.number} · ${jumper.net_name} (${wireColorName(jumper.color)}): ${jumper.from_label} → ${jumper.to_label}`}
            </title>
            <path class="casing" d={d} />
            <path class="core" d={d} style={{ stroke: color }} />
            {[jumper.from, jumper.to].map((end, index) => (
              <rect
                key={index}
                class={
                  jumper.kind === "lead" && index === 0
                    ? "housing female"
                    : "housing"
                }
                x={end.x - 0.27}
                y={end.y - 0.27}
                width={0.54}
                height={0.54}
                rx={0.08}
              />
            ))}
            <circle class="number-bg" cx={mid.x} cy={mid.y} r={0.42} />
            <text class="number" x={mid.x} y={mid.y}>
              {jumper.number}
            </text>
          </g>
        );
      })}
    </g>
  );
}

// ── Panel ─────────────────────────────────────────────────────────────────

/** A view option: each resistor's colour code, to pick the right one. */
function ColorCodes({ layout }: { layout: BreadboardLayout }) {
  const resistors = layout.parts
    .filter((part) => part.value?.bands?.length)
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  if (!resistors.length) return null;
  const on = showBreadboardColorCodes.value;
  return (
    <section class="bb-section bb-color-codes">
      <Toggle
        label="Resistor colour codes"
        hint="The bands to look for, 1% tolerance"
        value={on}
        onChange={(value) => (showBreadboardColorCodes.value = value)}
      />
      {on &&
        resistors.map((part) => (
          <ColorCodeRow
            key={part.component_id}
            name={part.name}
            value={part.value!.display}
            bands={part.value!.bands!}
          />
        ))}
    </section>
  );
}

function BreadboardPanel({ layout }: { layout: BreadboardLayout }) {
  const groups = jumpersByNet(layout);
  const leads = layout.jumpers.filter((jumper) => jumper.kind === "lead");
  const links = railLinks(layout);
  const rails = new Map<RailSlot, { name: string; color: string }>();
  for (const rail of layout.rails)
    rails.set(rail.slot, {
      name: rail.net_name,
      color: wireColorCss(rail.color),
    });
  const net = focusNet.value;
  const hovered = hoverJumper.value;
  return (
    <aside class="side-panel right breadboard-panel" aria-label="Breadboard">
      <header class="breadboard-summary">
        <strong>
          {layout.board_count} breadboard{layout.board_count === 1 ? "" : "s"} ·{" "}
          {layout.jumpers.length - leads.length} jumper
          {layout.jumpers.length - leads.length === 1 ? "" : "s"}
          {leads.length > 0 &&
            ` · ${leads.length} lead${leads.length === 1 ? "" : "s"}`}
        </strong>
        <p class="field-note">
          Read-only: change the design on the Board and this layout follows.
          Jumpers take the colour of the net&apos;s wires.
        </p>
      </header>
      <div class="panel-body breadboard-body">
        {layout.warnings.length > 0 && (
          <section class="bb-section">
            <h3>
              <Icon name="warning" size={14} /> Notes
            </h3>
            <ul class="bb-warnings">
              {layout.warnings.map((warning, index) => (
                <li
                  key={index}
                  class={
                    warning.code === "breadboard.check_failed" ? "bad" : ""
                  }
                >
                  {warning.message}
                </li>
              ))}
            </ul>
          </section>
        )}
        <ColorCodes layout={layout} />
        {rails.size > 0 && (
          <section class="bb-section">
            <h3>Rails</h3>
            <ul class="bb-rails">
              {(Object.keys(SLOT_NAME) as RailSlot[]).map((slot) => {
                const rail = rails.get(slot);
                return (
                  <li key={slot}>
                    <span class="bb-slot">{SLOT_NAME[slot]}</span>
                    {rail ? (
                      <span>
                        <i
                          class="bb-swatch"
                          style={{ background: rail.color }}
                        />
                        {rail.name}
                      </span>
                    ) : (
                      <span class="field-note">unused</span>
                    )}
                  </li>
                );
              })}
            </ul>
            {links.length > 0 && (
              <ul class="bb-links">
                {links.map((jumper) => (
                  <li
                    key={jumper.id}
                    class={hovered === jumper.id ? "on" : undefined}
                    onPointerEnter={() => (hoverJumper.value = jumper.id)}
                    onPointerLeave={() => {
                      if (hoverJumper.peek() === jumper.id)
                        hoverJumper.value = null;
                    }}
                  >
                    <i
                      class="bb-swatch"
                      style={{ background: wireColorCss(jumper.color) }}
                    />
                    {jumper.net_name} rails joined by jumper {jumper.number} at
                    row {jumper.from.y + 1}
                  </li>
                ))}
              </ul>
            )}
          </section>
        )}
        <section class="bb-section">
          <h3>Jumpers by net</h3>
          {groups.length === 0 && (
            <p class="field-note">No connections to make.</p>
          )}
          {groups.map((group) => (
            <div
              key={group.netId}
              class={`bb-net${net === group.netId ? " on" : ""}`}
            >
              <button
                class="bb-net-head"
                aria-pressed={net === group.netId}
                onClick={() => toggleFocusNet(group.netId)}
                title="Highlight this net's jumpers"
              >
                <i
                  class="bb-swatch"
                  style={{ background: wireColorCss(group.color) }}
                />
                <span class="bb-net-name">{group.name}</span>
                <span class="field-note">
                  {wireColorName(group.color)} · {group.jumpers.length}
                </span>
              </button>
              <ol class="bb-jumper-list">
                {group.jumpers.map((jumper) => (
                  <li
                    key={jumper.id}
                    class={hovered === jumper.id ? "on" : ""}
                    onPointerEnter={() => (hoverJumper.value = jumper.id)}
                    onPointerLeave={() => {
                      if (hoverJumper.peek() === jumper.id)
                        hoverJumper.value = null;
                    }}
                  >
                    <span class="bb-number">{jumper.number}</span>
                    <span class="bb-ends">
                      {jumper.from_label} → {jumper.to_label}
                    </span>
                    {jumper.kind === "lead" && (
                      <span class="bb-kind" title="Male–female Dupont lead">
                        lead
                      </span>
                    )}
                  </li>
                ))}
              </ol>
            </div>
          ))}
        </section>
      </div>
    </aside>
  );
}
