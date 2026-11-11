import type {
  ComponentDefinition,
  ComponentInstance,
  ProjectDocument,
  Side,
} from "../api/project";
import { fullyEnclosedHoleDimensions } from "./board-size";
import { bridgeConductorPaths } from "./bridge-geometry";
const PAD = 2;
const RIGHT_STAGING_PAD = 4;
const COORDINATE_LABEL_OFFSET = 0.55;
function escape(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ] ?? c,
  );
}
function degrees(rotation: ComponentInstance["rotation"]): number {
  return { deg0: 0, deg90: 90, deg180: 180, deg270: 270 }[rotation];
}
function rotateLocal(
  rotation: ComponentInstance["rotation"],
  x: number,
  y: number,
): [number, number] {
  return {
    deg0: [x, y],
    deg90: [-y, x],
    deg180: [-x, -y],
    deg270: [y, -x],
  }[rotation] as [number, number];
}
function pinLabel(id: string, x: number, y: number): string {
  const fit =
    id.length > 3 ? ' textLength=".42" lengthAdjust="spacingAndGlyphs"' : "";
  return `<text class="pin-label readable-label" x="${x}" y="${y}"${fit}>${escape(id)}</text>`;
}
function pinTooltip(
  pin: ComponentDefinition["pins"][number],
  pinNumber: string,
): string {
  const details = [`Pin ${pinNumber}: ${pin.name} (${pin.id})`];
  if (pin.aliases?.length) details.push(`Aliases: ${pin.aliases.join(", ")}`);
  if (pin.functions?.length)
    details.push(`Functions: ${pin.functions.join(", ")}`);
  return details.join(" · ");
}
function componentTooltip(
  instance: ComponentInstance,
  definition: ComponentDefinition,
): string {
  const details = [
    instance.name,
    `Type: ${definition.name} (${definition.id} v${definition.definition_version})`,
  ];
  if (definition.description) details.push(definition.description);
  details.push(
    `Anchor: ${instance.position.x}, ${instance.position.y}`,
    `Rotation: ${degrees(instance.rotation)}°`,
    `Pins: ${definition.pins.length}`,
  );
  const properties = Object.entries(instance.properties ?? {});
  if (properties.length) {
    details.push(
      `Properties: ${properties.map(([key, value]) => `${key}=${value}`).join(", ")}`,
    );
  }
  if (instance.notes) details.push(`Notes: ${instance.notes}`);
  return details.join("\n");
}
function rectPoints(
  min: { x: number; y: number },
  max: { x: number; y: number },
): string {
  return `${min.x - 0.5} ${min.y - 0.5} ${max.x - min.x + 1} ${max.y - min.y + 1}`;
}
function renderComponent(
  instance: ComponentInstance,
  definition: ComponentDefinition,
  selected: boolean,
  pitch: number,
  oppositeSide: boolean,
  changeKind?: string,
): string {
  const body = definition.body;
  const pins = definition.pins
    .map(
      (pin) =>
        `<g class="pin" data-pin-id="${escape(pin.id)}" data-pin-number="${escape(pin.number)}"><circle cx="${pin.x}" cy="${pin.y}" r=".27"/><title>${escape(pinTooltip(pin, pin.number))}</title></g>`,
    )
    .join("");
  const keepouts = (definition.keepouts ?? [])
    .map(
      (area) =>
        `<rect class="keepout" x="${area.min.x - 0.5}" y="${area.min.y - 0.5}" width="${area.max.x - area.min.x + 1}" height="${area.max.y - area.min.y + 1}" rx=".15"><title>${escape(area.label)}</title></rect>`,
    )
    .join("");
  const wireKeepouts = (definition.wire_keepouts ?? [])
    .map(
      (area) =>
        `<rect class="wire-keepout" x="${area.min.x - 0.5}" y="${area.min.y - 0.5}" width="${area.max.x - area.min.x + 1}" height="${area.max.y - area.min.y + 1}" rx=".15"><title>${escape(area.label)}</title></rect>`,
    )
    .join("");
  const access = (definition.access ?? [])
    .map((area) => {
      const [x, y, width, height] = rectPoints(area.min, area.max).split(" ");
      const middleX = (area.min.x + area.max.x) / 2;
      const middleY = (area.min.y + area.max.y) / 2;
      const arrow = { north: "↑", east: "→", south: "↓", west: "←" }[
        area.direction
      ];
      return `<g class="access"><rect x="${x}" y="${y}" width="${width}" height="${height}"/><text x="${middleX}" y="${middleY + 0.25}">${arrow}</text><title>${escape(area.label)}</title></g>`;
    })
    .join("");
  if (definition.kind === "bridge") {
    const [start, end] = definition.pins;
    if (!start || !end) return "";
    const conductorPaths = bridgeConductorPaths(start, end);
    return `<g class="component bridge${oppositeSide ? " opposite-side" : ""}${selected ? " selected" : ""}${changeKind ? ` diff-${escape(changeKind)}` : ""}" data-component-id="${instance.id}" data-component-kind="bridge" data-rotation-degrees="${degrees(instance.rotation)}" transform="translate(${instance.position.x} ${instance.position.y}) rotate(${degrees(instance.rotation)})"><line class="bridge-hit-target" x1="${start.x}" y1="${start.y}" x2="${end.x}" y2="${end.y}"/><path class="bridge-conductor bridge-span" d="${conductorPaths.span}"><title>${escape(componentTooltip(instance, definition))}</title></path><path class="bridge-conductor bridge-terminals" d="${conductorPaths.terminals}"/>${pins}</g>`;
  }
  return `<g class="component${oppositeSide ? " opposite-side" : ""}${selected ? " selected" : ""}${changeKind ? ` diff-${escape(changeKind)}` : ""}" data-component-id="${instance.id}" data-rotation-degrees="${degrees(instance.rotation)}" transform="translate(${instance.position.x} ${instance.position.y}) rotate(${degrees(instance.rotation)})">${keepouts}${wireKeepouts}<rect class="body" x="${body.x / pitch}" y="${body.y / pitch}" width="${body.width / pitch}" height="${body.height / pitch}" rx=".35"/><path class="orientation" d="M .25 .25 L 1 .25 L .25 1 Z"><title>${escape(componentTooltip(instance, definition))}</title></path>${pins}${access}</g>`;
}
function wireColor(color: import("../api/project").WireColor): string {
  return typeof color === "string" ? color : color.other;
}
function renderedWireColor(color: import("../api/project").WireColor): string {
  const value = wireColor(color);
  return value.toLowerCase() === "white" ? "#e6e6e6" : value;
}
function wireMidpoint(vertices: Array<{ x2: number; y2: number }>): {
  x: number;
  y: number;
} {
  const points = vertices.map(({ x2, y2 }) => ({ x: x2 / 2, y: y2 / 2 }));
  const first = points[0];
  if (!first) return { x: 0, y: 0 };
  if (points.length === 1) return first;

  const segments = points.slice(0, -1).map((start, index) => {
    const end = points[index + 1]!;
    return { start, end, length: Math.hypot(end.x - start.x, end.y - start.y) };
  });
  const halfway =
    segments.reduce((sum, segment) => sum + segment.length, 0) / 2;
  let traversed = 0;
  for (const segment of segments) {
    if (segment.length > 0 && traversed + segment.length >= halfway) {
      const ratio = (halfway - traversed) / segment.length;
      return {
        x: segment.start.x + (segment.end.x - segment.start.x) * ratio,
        y: segment.start.y + (segment.end.y - segment.start.y) * ratio,
      };
    }
    traversed += segment.length;
  }
  return points.at(-1)!;
}
function componentPinPositions(document: ProjectDocument): Array<{
  x: number;
  y: number;
  number: string;
  side: Side;
  isBridge: boolean;
}> {
  const definitions = document.design.component_definitions ?? {};
  return Object.values(document.design.components ?? {}).flatMap((instance) => {
    const definition = definitions[instance.definition_id];
    if (!definition) return [];
    return definition.pins.map((pin) => {
      const [localX, localY] = rotateLocal(instance.rotation, pin.x, pin.y);
      return {
        x: instance.position.x + localX,
        y: instance.position.y + localY,
        number: pin.number,
        side: definition.kind === "bridge" ? "solder" : "component",
        isBridge: definition.kind === "bridge",
      };
    });
  });
}
export function renderBoardSvg(
  document: ProjectDocument,
  selectedId?: string,
  viewBox?: string,
  side: Side = "component",
  changes: Record<string, string> = {},
  svgId = "board",
  resizeMode = false,
  printMode = false,
  fadeComponents = false,
): string {
  const board = document.design.board;
  const definitions = document.design.component_definitions ?? {};
  const holes = Array.from({ length: board.holes_y }, (_, y) =>
    Array.from(
      { length: board.holes_x },
      (_, x) =>
        `<circle class="hole" cx="${x}" cy="${y}" r=".12" data-x="${x}" data-y="${y}"><title>Hole (${x}, ${y})</title></circle>`,
    ).join(""),
  ).join("");
  const forbidden = (board.forbidden_areas ?? [])
    .map(
      (area) =>
        `<rect class="invalid-region" x="${area.min.x - 0.45}" y="${area.min.y - 0.45}" width="${area.max.x - area.min.x + 0.9}" height="${area.max.y - area.min.y + 0.9}"><title>${escape(area.label)}</title></rect>`,
    )
    .join("");
  const mountingHoles = (board.mounting_holes ?? [])
    .map(
      (hole) =>
        `<circle class="mounting-hole" cx="${hole.position.x}" cy="${hole.position.y}" r="${hole.diameter_mm / board.pitch_mm / 2}"><title>${escape(hole.label)}</title></circle>`,
    )
    .join("");
  const componentValues = Object.values(document.design.components ?? {});
  const renderedComponents = componentValues
    .sort((left, right) => {
      if (left.id === selectedId) return 1;
      if (right.id === selectedId) return -1;
      return 0;
    })
    .map((instance) => {
      const definition = definitions[instance.definition_id];
      const ownSide = definition?.kind === "bridge" ? "solder" : "component";
      return {
        side: ownSide,
        markup: definition
          ? renderComponent(
              instance,
              definition,
              instance.id === selectedId && side === ownSide,
              board.pitch_mm,
              side !== ownSide,
              changes[instance.id],
            )
          : "",
      };
    });
  const components = renderedComponents.map(({ markup }) => markup).join("");
  const ownComponents = renderedComponents
    .filter((component) => component.side === side)
    .map(({ markup }) => markup)
    .join("");
  const oppositeComponents = renderedComponents
    .filter((component) => component.side !== side)
    .map(({ markup }) => markup)
    .join("");
  const renderedWires = Object.values(document.design.wires ?? {}).map(
    (wire, index) => {
      const points = wire.vertices
        .map((point) => `${point.x2 / 2},${point.y2 / 2}`)
        .join(" ");
      const hiddenSide = wire.side !== side ? " opposite-side" : "";
      const selected =
        wire.id === selectedId && wire.side === side ? " selected" : "";
      const changed = changes[wire.id]
        ? ` diff-${escape(changes[wire.id]!)}`
        : "";
      const selectionHalo = selected
        ? `<polyline class="wire-selection" points="${points}"/>`
        : "";
      const segmentTargets = wire.vertices
        .slice(0, -1)
        .map((point, segmentIndex) => {
          const next = wire.vertices[segmentIndex + 1]!;
          return `<line class="wire-segment-hit-target" data-wire-segment="${segmentIndex}" x1="${point.x2 / 2}" y1="${point.y2 / 2}" x2="${next.x2 / 2}" y2="${next.y2 / 2}"><title>Long-press to add a wire node</title></line>`;
        })
        .join("");
      const nodeHandles =
        selected && !wire.locked
          ? wire.vertices
              .slice(1, -1)
              .map(
                (point, index) =>
                  `<circle class="wire-node" data-wire-vertex-index="${index + 1}" cx="${point.x2 / 2}" cy="${point.y2 / 2}" r=".2"><title>Drag wire node; drop onto another node to remove</title></circle>`,
              )
              .join("")
          : "";
      const endpointHandles =
        selected && !wire.locked
          ? [wire.vertices[0], wire.vertices.at(-1)]
              .map((point, endpointIndex) => {
                if (!point) return "";
                const vertexIndex =
                  endpointIndex === 0 ? 0 : wire.vertices.length - 1;
                const endpoint = endpointIndex === 0 ? "start" : "end";
                return `<circle class="wire-node wire-endpoint" data-wire-vertex-index="${vertexIndex}" data-wire-endpoint="${endpoint}" data-wire-id="${wire.id}" cx="${point.x2 / 2}" cy="${point.y2 / 2}" r=".24"><title>Drag to reconnect this wire end to a pin</title></circle>`;
              })
              .join("")
          : "";
      const labelPosition = wireMidpoint(wire.vertices);
      return {
        side: wire.side,
        endpointHandles,
        label: `W${index + 1}`,
        labelX: labelPosition.x,
        labelY: labelPosition.y,
        markup: `<g class="wire${hiddenSide}${selected}${changed}" data-wire-id="${wire.id}">${selectionHalo}<polyline class="wire-casing" points="${points}"/><polyline class="wire-path" points="${points}" style="--wire-color:${escape(renderedWireColor(wire.color))}"/>${printMode ? "" : segmentTargets + nodeHandles}</g>`,
      };
    },
  );
  const renderedRails = Object.values(document.design.rails ?? {}).map(
    (rail, index) => {
      const points = rail.vertices
        .map((point) => `${point.x2 / 2},${point.y2 / 2}`)
        .join(" ");
      const hiddenSide = rail.side !== side ? " opposite-side" : "";
      const selected =
        rail.id === selectedId && rail.side === side ? " selected" : "";
      const nodes = rail.vertices
        .map(
          (point, nodeIndex) =>
            `<circle class="rail-node" data-rail-id="${rail.id}" data-rail-node-index="${nodeIndex}" data-x2="${point.x2}" data-y2="${point.y2}" cx="${point.x2 / 2}" cy="${point.y2 / 2}" r=".2"><title>Rail connection node</title></circle>`,
        )
        .join("");
      const labelPosition = wireMidpoint(rail.vertices);
      return {
        side: rail.side,
        label: `R${index + 1}`,
        labelX: labelPosition.x,
        labelY: labelPosition.y,
        markup: `<g class="wire rail${hiddenSide}${selected}" data-rail-id="${rail.id}" style="--wire-color:${escape(renderedWireColor(rail.color))}"><polyline class="wire-casing" points="${points}"/><polyline class="wire-path" points="${points}"/>${printMode ? "" : nodes}</g>`,
      };
    },
  );
  const wires = renderedWires.map(({ markup }) => markup).join("");
  const ownWires = renderedWires
    .filter((wire) => wire.side === side)
    .map(({ markup }) => markup)
    .join("");
  const oppositeWires = renderedWires
    .filter((wire) => wire.side !== side)
    .map(({ markup }) => markup)
    .join("");
  const ownWireEndpointHandles = renderedWires
    .filter((wire) => wire.side === side)
    .map((wire) => wire.endpointHandles)
    .join("");
  const rails = renderedRails.map(({ markup }) => markup).join("");
  const ownRails = renderedRails
    .filter((rail) => rail.side === side)
    .map(({ markup }) => markup)
    .join("");
  const oppositeRails = renderedRails
    .filter((rail) => rail.side !== side)
    .map(({ markup }) => markup)
    .join("");
  const defaultViewBox = `${-PAD} ${-PAD} ${board.holes_x - 1 + PAD + RIGHT_STAGING_PAD} ${board.holes_y - 1 + PAD * 2}`;
  const margins = board.margins_mm ?? { top: 0, right: 0, bottom: 0, left: 0 };
  const left = margins.left / board.pitch_mm;
  const top = margins.top / board.pitch_mm;
  const width =
    board.holes_x - 1 + (margins.left + margins.right) / board.pitch_mm;
  const height =
    board.holes_y - 1 + (margins.top + margins.bottom) / board.pitch_mm;
  const mirror =
    side === "solder" ? `translate(${board.holes_x - 1} 0) scale(-1 1)` : "";
  const columnLabels = Array.from(
    { length: board.holes_x },
    (_, canonicalX) => {
      const displayX =
        side === "solder" ? board.holes_x - 1 - canonicalX : canonicalX;
      return { canonicalX, displayX };
    },
  );
  const topColumnLabels = columnLabels
    .map(
      ({ canonicalX, displayX }) =>
        `<text x="${displayX}" y="-${COORDINATE_LABEL_OFFSET}">${canonicalX}</text>`,
    )
    .join("");
  const bottomColumnLabels = columnLabels
    .map(
      ({ canonicalX, displayX }) =>
        `<text x="${displayX}" y="${board.holes_y - 1 + COORDINATE_LABEL_OFFSET}">${canonicalX}</text>`,
    )
    .join("");
  const leftRowLabels = Array.from(
    { length: board.holes_y },
    (_, y) => `<text x="-${COORDINATE_LABEL_OFFSET}" y="${y}">${y}</text>`,
  ).join("");
  const rightRowLabels = Array.from(
    { length: board.holes_y },
    (_, y) =>
      `<text x="${board.holes_x - 1 + COORDINATE_LABEL_OFFSET}" y="${y}">${y}</text>`,
  ).join("");
  const coordinateLabels = `<g class="hole-coordinate-labels" aria-label="Internal hole coordinates"><g class="column-coordinates">${topColumnLabels}</g><g class="column-coordinates bottom-coordinates">${bottomColumnLabels}</g><g class="row-coordinates">${leftRowLabels}</g><g class="row-coordinates right-coordinates">${rightRowLabels}</g><text class="coordinate-axis-label x-axis" x="${board.holes_x}" y="-${COORDINATE_LABEL_OFFSET}">${side === "solder" ? "← x" : "x →"}</text><text class="coordinate-axis-label y-axis" x="-${COORDINATE_LABEL_OFFSET}" y="${board.holes_y}">y ↓</text></g>`;
  const componentPins = componentPinPositions(document);
  const pinLabels = componentPins
    .filter((pin) => pin.side === side && !pin.isBridge)
    .map(({ x: canonicalX, y, number }) => {
      const x = side === "solder" ? board.holes_x - 1 - canonicalX : canonicalX;
      return pinLabel(number, x, y);
    })
    .join("");
  const wireLabels = renderedWires
    .filter((wire) => wire.side === side)
    .map(({ label, labelX: canonicalX, labelY }) => {
      const x = side === "solder" ? board.holes_x - 1 - canonicalX : canonicalX;
      return `<text class="wire-label readable-label" x="${x}" y="${labelY}">${label}</text>`;
    })
    .join("");
  const railLabels = renderedRails
    .filter((rail) => rail.side === side)
    .map(({ label, labelX: canonicalX, labelY }) => {
      const x = side === "solder" ? board.holes_x - 1 - canonicalX : canonicalX;
      return `<text class="wire-label rail-label readable-label" x="${x}" y="${labelY}">${label}</text>`;
    })
    .join("");
  const solderPoints = printMode
    ? componentPins
        .map(
          ({ x, y }) =>
            `<g class="solder-point" transform="translate(${x} ${y})"><circle class="solder-ring" r=".31"/><circle class="solder-hole" r=".11"/></g>`,
        )
        .join("")
    : "";
  const resizeHandles = resizeMode
    ? (() => {
        const fullHoles = fullyEnclosedHoleDimensions({
          holesX: board.holes_x,
          holesY: board.holes_y,
        });
        return `<g class="board-resize-controls" aria-label="Board resize handles"><rect class="board-resize-boundary" x="0" y="0" width="${board.holes_x - 1}" height="${board.holes_y - 1}"/><text class="board-resize-feedback" x="${(board.holes_x - 1) / 2}" y="${(board.holes_y - 1) / 2}" text-anchor="middle" aria-live="polite">${fullHoles.holesX} full holes wide × ${fullHoles.holesY} tall</text><line data-resize-handle="top" x1="0" y1="0" x2="${board.holes_x - 1}" y2="0"/><line data-resize-handle="right" x1="${board.holes_x - 1}" y1="0" x2="${board.holes_x - 1}" y2="${board.holes_y - 1}"/><line data-resize-handle="bottom" x1="0" y1="${board.holes_y - 1}" x2="${board.holes_x - 1}" y2="${board.holes_y - 1}"/><line data-resize-handle="left" x1="0" y1="0" x2="0" y2="${board.holes_y - 1}"/>${[
          ["top-left", 0, 0],
          ["top-right", board.holes_x - 1, 0],
          ["bottom-right", board.holes_x - 1, board.holes_y - 1],
          ["bottom-left", 0, board.holes_y - 1],
        ]
          .map(
            ([handle, x, y]) =>
              `<rect class="board-resize-corner" data-resize-handle="${handle}" x="${Number(x) - 0.3}" y="${Number(y) - 0.3}" width=".6" height=".6" rx=".12"/>`,
          )
          .join("")}</g>`;
      })()
    : "";
  const drawingLayers = printMode
    ? `<g class="components">${components}</g><g class="wires">${rails}${wires}</g><g class="solder-points">${solderPoints}</g>`
    : `<g class="components opposite-side-objects">${oppositeComponents}</g><g class="wires opposite-side-objects">${oppositeRails}${oppositeWires}</g><g class="wires">${ownRails}${ownWires}</g><g class="components">${ownComponents}</g><g class="wire-endpoint-handles">${ownWireEndpointHandles}</g>`;
  const fadeClass =
    fadeComponents && side === "component" && !printMode
      ? " components-faded"
      : "";
  return `<svg id="${escape(svgId)}" class="${printMode ? "print-board-svg" : "interactive-board-svg"}${fadeClass}" aria-label="${escape(board.name)}, ${side} side" role="img" viewBox="${viewBox ?? defaultViewBox}" data-default-view-box="${defaultViewBox}" data-side="${side}"><rect class="board-outline" x="${-left}" y="${-top}" width="${width}" height="${height}" rx=".5"/><g class="board-geometry" transform="${mirror}"><g class="holes">${holes}</g>${mountingHoles}${forbidden}${drawingLayers}</g>${coordinateLabels}<g class="wire-labels">${wireLabels}${railLabels}</g><g class="pin-labels">${pinLabels}</g>${resizeHandles}</svg>`;
}

export function renderPrintLayout(document: ProjectDocument): string {
  const board = document.design.board;
  return `<header class="print-header"><div><p>BUILD DRAWING</p><h1>${escape(document.name)}</h1></div><dl><div><dt>Board</dt><dd>${escape(board.name)}</dd></div><div><dt>Grid</dt><dd>${board.holes_x} × ${board.holes_y} holes · ${board.pitch_mm} mm pitch</dd></div></dl></header><main class="print-layout"><figure class="print-board"><figcaption><strong>Component side</strong><span>Viewed from above</span></figcaption>${renderBoardSvg(document, undefined, undefined, "component", {}, "print-component-board", false, true)}</figure><figure class="print-board"><figcaption><strong>Solder side</strong><span>Viewed from underneath · mirrored</span></figcaption>${renderBoardSvg(document, undefined, undefined, "solder", {}, "print-solder-board", false, true)}</figure></main><footer class="print-legend" aria-label="Drawing legend"><span><i class="legend-hole"></i>Board hole</span><span><i class="legend-joint"></i>Solder joint / component pin</span><span><i class="legend-wire"></i>Wire on this side</span><span><i class="legend-component"></i>Component outline</span><span><i class="legend-bridge"></i>Bridge · install last</span></footer>`;
}
