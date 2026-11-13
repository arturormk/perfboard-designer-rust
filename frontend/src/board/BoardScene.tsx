/**
 * Declarative SVG scene for one face of the board.
 *
 * Geometry is drawn in canonical board units inside a group that mirrors the
 * solder side; text is drawn outside that group at mirrored positions so it
 * stays readable. Interactive elements carry `data-*` attributes that the
 * canvas uses for hit-testing; the scene itself has no event handlers.
 */
import type { ComponentChildren } from "preact";
import { memo } from "preact/compat";
import type {
  ComponentDefinition,
  ComponentInstance,
  Rail,
  Saddle,
  Side,
  Wire,
} from "../api/project";
import {
  bodyBounds,
  componentSide,
  pinBoardPosition,
  polylineMidpoint,
  rotationDegrees,
  viewX,
  type Design,
  type Point,
} from "../model/geometry";
import { isDarkWire, wireColorCss } from "../model/naming";
import type { Airwire } from "../model/ratsnest";
import type { PinLabelMode, Selection } from "../state/store";

export interface SceneProps {
  design: Design;
  side: Side;
  mode: "edit" | "readonly" | "print";
  idPrefix: string;
  selection?: Selection | null;
  hoveredId?: string;
  emphasisNetId?: string;
  changeKinds?: Map<string, string>;
  showChanges?: boolean;
  removedFrom?: Design;
  flashIds?: ReadonlySet<string>;
  dimIds?: ReadonlySet<string>;
  fade?: boolean;
  pinLabels?: PinLabelMode;
  showKeepouts?: boolean;
  airwires?: Airwire[];
  wireLabels?: Map<string, string>;
  railLabels?: Map<string, string>;
  saddleLabels?: Map<string, string>;
  /**
   * Draw the substrate and hole grid in SVG (default). The interactive
   * canvas passes `false` and draws them on a `<BoardBase>` canvas instead.
   */
  baseLayer?: boolean;
  /** Extra geometry in canonical board space (tool previews, handles). */
  overlay?: ComponentChildren;
  /** Extra content in view space (readable text). */
  viewOverlay?: ComponentChildren;
}

const BOARD_MARGIN = 0.7;

function points(vertices: Array<{ x2: number; y2: number }>): string {
  return vertices.map((point) => `${point.x2 / 2},${point.y2 / 2}`).join(" ");
}

function classes(...names: Array<string | false | undefined | null>): string {
  return names.filter(Boolean).join(" ");
}

function netForPin(design: Design): Map<string, string> {
  const map = new Map<string, string>();
  for (const net of Object.values(design.nets ?? {}))
    for (const pin of net.pins ?? [])
      map.set(`${pin.component_id}\u0000${pin.pin_id}`, net.id);
  return map;
}

function ComponentBody({
  instance,
  definition,
  pitch,
  opposite,
  stateClass,
  showKeepouts,
}: {
  instance: ComponentInstance;
  definition: ComponentDefinition;
  pitch: number;
  opposite: boolean;
  stateClass: string;
  showKeepouts: boolean;
}) {
  const transform = `translate(${instance.position.x} ${instance.position.y}) rotate(${rotationDegrees(instance.rotation)})`;
  if (definition.kind === "bridge") {
    const [a, b] = definition.pins;
    if (!a || !b) return null;
    return (
      <g
        class={classes("bridge", opposite && "opposite", stateClass)}
        transform={transform}
        data-component={opposite ? undefined : instance.id}
      >
        <line class="bridge-hit" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
        <line
          class="bridge-shadow"
          x1={a.x}
          y1={a.y + 0.04}
          x2={b.x}
          y2={b.y + 0.04}
        />
        <line class="bridge-wire" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
        <circle class="bridge-joint" cx={a.x} cy={a.y} r={0.26} />
        <circle class="bridge-joint" cx={b.x} cy={b.y} r={0.26} />
      </g>
    );
  }
  const body = definition.body;
  const rect = (area: { min: Point; max: Point }) => ({
    x: area.min.x - 0.5,
    y: area.min.y - 0.5,
    width: area.max.x - area.min.x + 1,
    height: area.max.y - area.min.y + 1,
  });
  return (
    <g
      class={classes("part", opposite && "opposite", stateClass)}
      transform={transform}
      data-component={opposite ? undefined : instance.id}
    >
      {showKeepouts &&
        (definition.keepouts ?? []).map((area, index) => (
          <rect key={`k${index}`} class="keepout" {...rect(area)} />
        ))}
      {showKeepouts &&
        (definition.wire_keepouts ?? []).map((area, index) => (
          <rect key={`w${index}`} class="wire-keepout" {...rect(area)} />
        ))}
      <rect
        class="part-body"
        x={body.x / pitch}
        y={body.y / pitch}
        width={body.width / pitch}
        height={body.height / pitch}
        rx={0.3}
      />
      {!opposite && (
        <path
          class="part-notch"
          d={`M ${body.x / pitch} ${body.y / pitch + 0.9} L ${body.x / pitch} ${body.y / pitch + 0.3} Q ${body.x / pitch} ${body.y / pitch} ${body.x / pitch + 0.3} ${body.y / pitch} L ${body.x / pitch + 0.9} ${body.y / pitch} Z`}
        />
      )}
      {showKeepouts &&
        !opposite &&
        (definition.access ?? []).map((area, index) => {
          const r = rect(area);
          const cx = r.x + r.width / 2;
          const cy = r.y + r.height / 2;
          const arrow = { north: -90, east: 0, south: 90, west: 180 }[
            area.direction
          ];
          return (
            <g
              key={`a${index}`}
              class={classes("access", area.required && "required")}
            >
              <rect {...r} rx={0.2} />
              <g transform={`translate(${cx} ${cy}) rotate(${arrow})`}>
                <path
                  class="access-arrow"
                  d="M -0.55 0 H 0.45 M 0.15 -0.3 L 0.5 0 L 0.15 0.3"
                />
              </g>
            </g>
          );
        })}
    </g>
  );
}

function WireShape({
  wire,
  stateClass,
  opposite,
  interactive,
}: {
  wire: Wire | Rail;
  stateClass: string;
  opposite: boolean;
  interactive: boolean;
}) {
  const isRail = !("start" in wire);
  const pts = points(wire.vertices);
  const first = wire.vertices[0];
  const last = wire.vertices.at(-1);
  const dataAttrs =
    interactive && !opposite
      ? isRail
        ? { "data-rail": wire.id }
        : { "data-wire": wire.id }
      : {};
  return (
    <g
      class={classes(
        isRail ? "rail" : "wire",
        opposite && "opposite",
        isDarkWire(wire.color) && "dark",
        wire.locked && "locked",
        stateClass,
      )}
      style={{ "--wire": wireColorCss(wire.color) }}
      {...dataAttrs}
    >
      {interactive && !opposite && <polyline class="wire-hit" points={pts} />}
      <polyline class="wire-halo" points={pts} />
      <polyline class="wire-casing" points={pts} />
      <polyline class="wire-core" points={pts} />
      {!isRail &&
        !opposite &&
        first &&
        last &&
        [first, last].map((point, index) => (
          <circle
            key={index}
            class="wire-end"
            cx={point.x2 / 2}
            cy={point.y2 / 2}
            r={0.17}
          />
        ))}
      {isRail &&
        !opposite &&
        wire.vertices.map((point, index) => (
          <circle
            key={index}
            class="rail-node"
            cx={point.x2 / 2}
            cy={point.y2 / 2}
            r={0.2}
            data-rail-node={interactive ? `${wire.id}:${index}` : undefined}
          />
        ))}
    </g>
  );
}

/**
 * The nodes of a rail on the other face that sit on whole holes. They are
 * soldered through the board, so wires on this face can connect to them.
 */
function RailThroughNodes({
  rail,
  stateClass,
}: {
  rail: Rail;
  stateClass: string;
}) {
  const nodes = rail.vertices
    .map((point, index) => ({ point, index }))
    .filter(({ point }) => point.x2 % 2 === 0 && point.y2 % 2 === 0);
  if (!nodes.length) return null;
  return (
    <g
      class={classes("rail-through", stateClass)}
      style={{ "--wire": wireColorCss(rail.color) }}
      data-rail={rail.id}
    >
      {nodes.map(({ point, index }) => (
        <circle
          key={index}
          class="rail-node"
          cx={point.x2 / 2}
          cy={point.y2 / 2}
          r={0.22}
          data-rail-node={`${rail.id}:${index}`}
        />
      ))}
    </g>
  );
}

/**
 * A wire tie: a strap lying across wires on its own face, with its two legs
 * soldered on the far face. Seen from the far face only the joints matter.
 */
function SaddleShape({
  saddle,
  stateClass,
  opposite,
  interactive,
}: {
  saddle: Saddle;
  stateClass: string;
  opposite: boolean;
  interactive: boolean;
}) {
  const { start, end } = saddle;
  const line = { x1: start.x, y1: start.y, x2: end.x, y2: end.y };
  return (
    <g
      class={classes(
        "saddle",
        opposite && "opposite",
        isDarkWire(saddle.color) && "dark",
        stateClass,
      )}
      style={{ "--wire": wireColorCss(saddle.color) }}
      {...(interactive && !opposite ? { "data-saddle": saddle.id } : {})}
    >
      {interactive && !opposite && <line class="saddle-hit" {...line} />}
      <line class="saddle-halo" {...line} />
      <line class="saddle-casing" {...line} />
      <line class="saddle-core" {...line} />
      {[start, end].map((point, index) => (
        <circle
          key={index}
          class={opposite ? "saddle-joint" : "saddle-leg"}
          cx={point.x}
          cy={point.y}
          r={opposite ? 0.26 : 0.16}
        />
      ))}
    </g>
  );
}

/**
 * The scene wrapper. The expensive static layers are memoized so that
 * camera changes, cursor moves and tool previews (the overlays) do not
 * rebuild every part, wire and pad.
 */
export function BoardScene(props: SceneProps) {
  const { overlay, viewOverlay, ...layerProps } = props;
  const { design, side, mode, fade, emphasisNetId } = props;
  const mirror =
    side === "solder"
      ? `translate(${design.board.holes_x - 1} 0) scale(-1 1)`
      : undefined;
  return (
    <g
      class={classes(
        "scene",
        `side-${side}`,
        `mode-${mode}`,
        fade && "faded",
        emphasisNetId && "has-emphasis",
      )}
    >
      <SceneLayers {...layerProps} />
      {overlay && (
        <g class="geometry" transform={mirror}>
          {overlay}
        </g>
      )}
      {viewOverlay && <g class="labels">{viewOverlay}</g>}
    </g>
  );
}

type LayerProps = Omit<SceneProps, "overlay" | "viewOverlay">;

const SceneLayers = memo(function SceneLayers(props: LayerProps) {
  const {
    design,
    side,
    mode,
    idPrefix,
    selection,
    emphasisNetId,
    changeKinds,
    showChanges,
    removedFrom,
    flashIds,
    dimIds,
    pinLabels = "numbers",
    showKeepouts,
    airwires,
    wireLabels,
    railLabels,
    saddleLabels,
    baseLayer = true,
  } = props;
  const board = design.board;
  const pitch = board.pitch_mm;
  const interactive = mode === "edit";
  const definitions = design.component_definitions ?? {};
  const pinNets = netForPin(design);
  const mirror =
    side === "solder"
      ? `translate(${board.holes_x - 1} 0) scale(-1 1)`
      : undefined;
  const vx = (x: number) => viewX(x, board.holes_x, side);

  const stateOf = (id: string, netId?: string): string => {
    const change = showChanges ? changeKinds?.get(id) : undefined;
    return classes(
      selection && selection.id === id && "selected",
      props.hoveredId === id && "hovered",
      flashIds?.has(id) && "flash",
      dimIds?.has(id) && "dim",
      emphasisNetId &&
        netId &&
        (netId === emphasisNetId ? "emphasis" : "deemphasis"),
      change && `diff-${change}`,
    );
  };

  const instances = Object.values(design.components ?? {});
  const placed = instances
    .map((instance) => {
      const definition = definitions[instance.definition_id];
      return definition
        ? { instance, definition, ownSide: componentSide(definition) }
        : undefined;
    })
    .filter((item): item is NonNullable<typeof item> => Boolean(item));
  const own = placed.filter((item) => item.ownSide === side);
  const other = placed.filter((item) => item.ownSide !== side);
  const ownParts = own.filter((item) => item.definition.kind !== "bridge");
  const ownBridges = own.filter((item) => item.definition.kind === "bridge");
  const wires = Object.values(design.wires ?? {});
  const rails = Object.values(design.rails ?? {});
  const saddles = Object.values(design.saddles ?? {});

  // Pads: every pin of every non-bridge part is a solder point on the board.
  const pads = placed
    .filter((item) => item.definition.kind !== "bridge")
    .flatMap(({ instance, definition, ownSide }) =>
      definition.pins.map((pin, index) => {
        const point = pinBoardPosition(instance, pin);
        const netId = pinNets.get(`${instance.id}\u0000${pin.id}`);
        return { instance, pin, point, netId, first: index === 0, ownSide };
      }),
    );

  const removed =
    showChanges && removedFrom
      ? {
          parts: Object.values(removedFrom.components ?? {}).filter(
            (instance) =>
              !design.components?.[instance.id] &&
              componentSide(
                removedFrom.component_definitions?.[instance.definition_id],
              ) === side,
          ),
          wires: Object.values(removedFrom.wires ?? {}).filter(
            (wire) => !design.wires?.[wire.id] && wire.side === side,
          ),
        }
      : undefined;

  const holeId = `${idPrefix}-holes`;
  const padId = `${idPrefix}-pads`;

  return (
    <>
      <defs>
        {baseLayer && (
          <pattern
            id={holeId}
            width="1"
            height="1"
            patternUnits="userSpaceOnUse"
            x="-0.5"
            y="-0.5"
          >
            <circle
              class="pad-ring"
              cx="0.5"
              cy="0.5"
              r={side === "solder" ? 0.36 : 0.27}
            />
            <circle class="hole" cx="0.5" cy="0.5" r="0.14" />
          </pattern>
        )}
        <pattern
          id={padId}
          width="0.5"
          height="0.5"
          patternUnits="userSpaceOnUse"
        >
          <path class="hatch" d="M -0.1 0.6 L 0.6 -0.1" />
        </pattern>
      </defs>
      <g class="geometry" transform={mirror}>
        {/* Offset copy instead of a CSS drop-shadow: filters re-rasterize
            at screen resolution and get very slow when zoomed in. */}
        {baseLayer && (
          <>
            <rect
              class="substrate-shadow"
              x={-BOARD_MARGIN}
              y={-BOARD_MARGIN + 0.22}
              width={board.holes_x - 1 + BOARD_MARGIN * 2}
              height={board.holes_y - 1 + BOARD_MARGIN * 2}
              rx={0.45}
            />
            <rect
              class="substrate"
              x={-BOARD_MARGIN}
              y={-BOARD_MARGIN}
              width={board.holes_x - 1 + BOARD_MARGIN * 2}
              height={board.holes_y - 1 + BOARD_MARGIN * 2}
              rx={0.45}
            />
            <rect
              class="hole-grid"
              x={-0.5}
              y={-0.5}
              width={board.holes_x}
              height={board.holes_y}
              fill={`url(#${holeId})`}
            />
          </>
        )}
        {(board.forbidden_areas ?? []).map((area, index) => (
          <rect
            key={index}
            class="forbidden"
            x={area.min.x - 0.5}
            y={area.min.y - 0.5}
            width={area.max.x - area.min.x + 1}
            height={area.max.y - area.min.y + 1}
            fill={`url(#${padId})`}
          />
        ))}
        {(board.mounting_holes ?? []).map((hole, index) => (
          <circle
            key={index}
            class="mounting-hole"
            cx={hole.position.x}
            cy={hole.position.y}
            r={hole.diameter_mm / pitch / 2}
          />
        ))}

        {/* The other face, seen through the board. */}
        <g class="layer-opposite">
          {other.map(({ instance, definition }) => (
            <ComponentBody
              key={instance.id}
              instance={instance}
              definition={definition}
              pitch={pitch}
              opposite
              stateClass={stateOf(instance.id)}
              showKeepouts={false}
            />
          ))}
          {[...rails, ...wires]
            .filter((wire) => wire.side !== side)
            .map((wire) => (
              <WireShape
                key={wire.id}
                wire={wire}
                opposite
                interactive={false}
                stateClass={stateOf(wire.id, wire.net_id)}
              />
            ))}
          {saddles
            .filter((saddle) => saddle.side !== side)
            .map((saddle) => (
              <SaddleShape
                key={saddle.id}
                saddle={saddle}
                opposite
                interactive={false}
                stateClass={stateOf(saddle.id)}
              />
            ))}
        </g>

        {removed && (
          <g class="layer-removed">
            {removed.parts.map((instance) => {
              const definition =
                removedFrom!.component_definitions?.[instance.definition_id];
              return definition ? (
                <ComponentBody
                  key={instance.id}
                  instance={instance}
                  definition={definition}
                  pitch={pitch}
                  opposite={false}
                  stateClass="ghost"
                  showKeepouts={false}
                />
              ) : null;
            })}
            {removed.wires.map((wire) => (
              <polyline
                key={wire.id}
                class="ghost-wire"
                points={points(wire.vertices)}
              />
            ))}
          </g>
        )}

        <g class="layer-parts">
          {ownParts.map(({ instance, definition }) => (
            <ComponentBody
              key={instance.id}
              instance={instance}
              definition={definition}
              pitch={pitch}
              opposite={false}
              stateClass={stateOf(instance.id)}
              showKeepouts={Boolean(
                showKeepouts || selection?.id === instance.id,
              )}
            />
          ))}
        </g>

        <g class="layer-pads">
          {pads.map(({ instance, pin, point, netId, first, ownSide }) => {
            const joint = ownSide !== side;
            const state = classes(
              joint ? "joint" : "pad",
              first && "first",
              netId && "assigned",
              emphasisNetId &&
                (netId === emphasisNetId ? "emphasis" : "deemphasis"),
              (selection?.id === instance.id || flashIds?.has(instance.id)) &&
                "of-selected",
            );
            return first && !joint ? (
              <rect
                key={`${instance.id}:${pin.id}`}
                class={state}
                x={point.x - 0.3}
                y={point.y - 0.3}
                width={0.6}
                height={0.6}
                rx={0.08}
                data-pin={
                  interactive ? `${instance.id}\u0000${pin.id}` : undefined
                }
              />
            ) : (
              <circle
                key={`${instance.id}:${pin.id}`}
                class={state}
                cx={point.x}
                cy={point.y}
                r={joint ? 0.3 : 0.32}
                data-pin={
                  interactive ? `${instance.id}\u0000${pin.id}` : undefined
                }
              />
            );
          })}
        </g>

        <g class="layer-wires">
          {rails
            .filter((rail) => rail.side === side)
            .map((rail) => (
              <WireShape
                key={rail.id}
                wire={rail}
                interactive={interactive}
                opposite={false}
                stateClass={stateOf(rail.id, rail.net_id)}
              />
            ))}
          {wires
            .filter((wire) => wire.side === side)
            .map((wire) => (
              <WireShape
                key={wire.id}
                wire={wire}
                interactive={interactive}
                opposite={false}
                stateClass={stateOf(wire.id, wire.net_id)}
              />
            ))}
          {interactive &&
            rails
              .filter((rail) => rail.side !== side)
              .map((rail) => (
                <RailThroughNodes
                  key={rail.id}
                  rail={rail}
                  stateClass={stateOf(rail.id, rail.net_id)}
                />
              ))}
        </g>

        <g class="layer-saddles">
          {saddles
            .filter((saddle) => saddle.side === side)
            .map((saddle) => (
              <SaddleShape
                key={saddle.id}
                saddle={saddle}
                interactive={interactive}
                opposite={false}
                stateClass={stateOf(saddle.id)}
              />
            ))}
        </g>

        <g class="layer-bridges">
          {ownBridges.map(({ instance, definition }) => (
            <ComponentBody
              key={instance.id}
              instance={instance}
              definition={definition}
              pitch={pitch}
              opposite={false}
              stateClass={stateOf(instance.id)}
              showKeepouts={false}
            />
          ))}
        </g>

        {airwires && airwires.length > 0 && (
          <g class="layer-airwires">
            {airwires.map((airwire, index) => (
              <g
                key={index}
                class={classes(
                  "airwire",
                  emphasisNetId &&
                    (airwire.netId === emphasisNetId
                      ? "emphasis"
                      : "deemphasis"),
                )}
                data-airwire={interactive ? String(index) : undefined}
              >
                <line
                  class="airwire-hit"
                  x1={airwire.a.x}
                  y1={airwire.a.y}
                  x2={airwire.b.x}
                  y2={airwire.b.y}
                />
                <line
                  class="airwire-casing"
                  x1={airwire.a.x}
                  y1={airwire.a.y}
                  x2={airwire.b.x}
                  y2={airwire.b.y}
                />
                <line
                  class="airwire-line"
                  x1={airwire.a.x}
                  y1={airwire.a.y}
                  x2={airwire.b.x}
                  y2={airwire.b.y}
                />
              </g>
            ))}
          </g>
        )}
      </g>

      {/* Readable text, positioned in view space. */}
      <g class="labels">
        <Rulers holesX={board.holes_x} holesY={board.holes_y} side={side} />
        {mode !== "print" &&
          ownParts.map(({ instance, definition }) => {
            const label = partLabel(
              instance.name,
              bodyBounds(instance, definition, pitch),
              vx,
            );
            if (!label) return null;
            return (
              <text
                key={instance.id}
                class={classes(
                  "part-label",
                  label.vertical && "vertical",
                  stateOf(instance.id),
                )}
                transform={label.transform}
                dy={TEXT_CENTER_DY}
              >
                {label.text}
              </text>
            );
          })}
        {mode === "print" &&
          ownParts.map(({ instance, definition }) => {
            const label = partLabel(
              instance.name,
              bodyBounds(instance, definition, pitch),
              vx,
              true,
            );
            return label ? (
              <text
                key={instance.id}
                class={classes("part-label", label.vertical && "vertical")}
                transform={label.transform}
                dy={TEXT_CENTER_DY}
              >
                {label.text}
              </text>
            ) : null;
          })}
        {pinLabels !== "off" &&
          pads
            .filter((pad) => pad.ownSide === side)
            .map(({ instance, pin, point }) => {
              const text = pinLabels === "names" ? pin.name : pin.number;
              if (!text) return null;
              return (
                <text
                  key={`${instance.id}:${pin.id}`}
                  class={classes(
                    "pin-label",
                    pinLabels === "names" && "names",
                    text.length > 2 && "long",
                  )}
                  x={vx(point.x)}
                  y={point.y}
                  dy={TEXT_CENTER_DY}
                >
                  {pinLabels === "names" && text.length > 5
                    ? `${text.slice(0, 5)}`
                    : text}
                </text>
              );
            })}
        {wireLabels &&
          wires
            .filter((wire) => wire.side === side)
            .filter(
              (wire) =>
                mode === "print" ||
                selection?.id === wire.id ||
                props.hoveredId === wire.id ||
                (emphasisNetId !== undefined && wire.net_id === emphasisNetId),
            )
            .map((wire) => {
              const middle = polylineMidpoint(wire.vertices);
              const net = design.nets?.[wire.net_id];
              const text =
                mode === "print"
                  ? (wireLabels.get(wire.id) ?? "")
                  : `${wireLabels.get(wire.id) ?? ""} · ${net?.name ?? ""}`;
              return (
                <WireLabel
                  key={wire.id}
                  x={vx(middle.point.x)}
                  y={middle.point.y}
                  text={text}
                />
              );
            })}
        {railLabels &&
          rails
            .filter(
              (rail) =>
                rail.side === side &&
                (mode === "print" || selection?.id === rail.id),
            )
            .map((rail) => {
              const middle = polylineMidpoint(rail.vertices);
              return (
                <WireLabel
                  key={rail.id}
                  x={vx(middle.point.x)}
                  y={middle.point.y}
                  text={railLabels.get(rail.id) ?? "Rail"}
                />
              );
            })}
        {saddleLabels &&
          saddles
            .filter(
              (saddle) =>
                saddle.side === side &&
                (mode === "print" || selection?.id === saddle.id),
            )
            .map((saddle) => (
              <WireLabel
                key={saddle.id}
                x={vx((saddle.start.x + saddle.end.x) / 2)}
                y={(saddle.start.y + saddle.end.y) / 2}
                text={saddleLabels.get(saddle.id) ?? "Saddle"}
              />
            ))}
      </g>
    </>
  );
});

const LABEL_CHAR_WIDTH = 0.29;

/**
 * Centres a part name on its body, running along the longer side so tall
 * modules (an Arduino Nano on its side) still get a readable label.
 */
/**
 * Vertical offset that centers digits and capitals on the text's y coordinate.
 *
 * `dominant-baseline: central` would be the idiomatic way, but WebKitGTK (the
 * Tauri webview on Linux) ignores it for sub-pixel font sizes, and every label
 * in board units is under 1px. `em` lengths are resolved from the computed
 * font size without font metrics, so this works in every engine. 0.35em is
 * half the typical cap height.
 */
export const TEXT_CENTER_DY = "0.35em";

function partLabel(
  name: string,
  bounds: { x: number; y: number; width: number; height: number },
  vx: (x: number) => number,
  force = false,
): { text: string; transform: string; vertical: boolean } | undefined {
  const cx = (vx(bounds.x) + vx(bounds.x + bounds.width)) / 2;
  const cy = bounds.y + bounds.height / 2;
  const vertical = bounds.height > bounds.width * 1.6 && bounds.width < 3.5;
  const room = (vertical ? bounds.height : bounds.width) - 0.6;
  const maxChars = Math.floor(room / LABEL_CHAR_WIDTH);
  if (!force && (maxChars < 5 || Math.min(bounds.width, bounds.height) < 0.9))
    return undefined;
  const text =
    !force && name.length > maxChars
      ? `${name.slice(0, Math.max(1, maxChars - 1))}…`
      : name;
  return {
    text,
    vertical,
    transform: `translate(${cx} ${cy})${vertical ? " rotate(-90)" : ""}`,
  };
}

export function WireLabel({
  x,
  y,
  text,
}: {
  x: number;
  y: number;
  text: string;
}) {
  const width = Math.max(1, text.length * 0.27 + 0.4);
  return (
    <g class="wire-label" transform={`translate(${x} ${y})`}>
      <rect x={-width / 2} y={-0.3} width={width} height={0.6} rx={0.3} />
      <text x={0} y={0} dy={TEXT_CENTER_DY}>
        {text}
      </text>
    </g>
  );
}

function Rulers({
  holesX,
  holesY,
  side,
}: {
  holesX: number;
  holesY: number;
  side: Side;
}) {
  const columns = Array.from({ length: holesX }, (_, x) => x);
  const rows = Array.from({ length: holesY }, (_, y) => y);
  return (
    <g class="rulers">
      {columns.map((x) => (
        <text
          key={`c${x}`}
          class={x % 5 === 0 ? "major" : undefined}
          x={viewX(x, holesX, side)}
          y={-1.25}
          dy={TEXT_CENTER_DY}
        >
          {x}
        </text>
      ))}
      {rows.map((y) => (
        <text
          key={`r${y}`}
          class={y % 5 === 0 ? "major" : undefined}
          x={-1.3}
          y={y}
          dy={TEXT_CENTER_DY}
        >
          {y}
        </text>
      ))}
    </g>
  );
}

/** Default framing: the board plus room for rulers and a staging strip. */
export function defaultViewBox(
  design: Design,
): [number, number, number, number] {
  const { holes_x, holes_y } = design.board;
  return [-2.2, -2.2, holes_x - 1 + 4.4, holes_y - 1 + 4.4];
}
