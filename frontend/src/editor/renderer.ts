import type {
  ComponentDefinition,
  ComponentInstance,
  ProjectDocument,
  Side,
} from "../api/project";
import { fullyEnclosedHoleDimensions } from "./board-size";
const PAD = 2;
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
  return `${min.x - 0.42} ${min.y - 0.42} ${max.x - min.x + 0.84} ${max.y - min.y + 0.84}`;
}
function renderComponent(
  instance: ComponentInstance,
  definition: ComponentDefinition,
  selected: boolean,
  pitch: number,
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
    .map((area) => {
      const [x, y, width, height] = rectPoints(area.min, area.max).split(" ");
      return `<rect class="keepout" x="${x}" y="${y}" width="${width}" height="${height}" rx=".15"><title>${escape(area.label)}</title></rect>`;
    })
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
  return `<g class="component${selected ? " selected" : ""}${changeKind ? ` diff-${escape(changeKind)}` : ""}" data-component-id="${instance.id}" data-rotation-degrees="${degrees(instance.rotation)}" transform="translate(${instance.position.x} ${instance.position.y}) rotate(${degrees(instance.rotation)})">${keepouts}<rect class="body" x="${body.x / pitch}" y="${body.y / pitch}" width="${body.width / pitch}" height="${body.height / pitch}" rx=".35"/><path class="orientation" d="M .25 .25 L 1 .25 L .25 1 Z"><title>${escape(componentTooltip(instance, definition))}</title></path>${pins}${access}</g>`;
}
function wireColor(color: import("../api/project").WireColor): string {
  return typeof color === "string" ? color : color.other;
}
export function renderBoardSvg(
  document: ProjectDocument,
  selectedId?: string,
  viewBox?: string,
  side: Side = "component",
  changes: Record<string, string> = {},
  svgId = "board",
  resizeMode = false,
): string {
  const board = document.design.board;
  const definitions = document.design.component_definitions ?? {};
  const holes = Array.from({ length: board.holes_y }, (_, y) =>
    Array.from(
      { length: board.holes_x },
      (_, x) => `<circle class="hole" cx="${x}" cy="${y}" r=".12"/>`,
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
  const components = componentValues
    .sort((left, right) => {
      if (left.id === selectedId) return 1;
      if (right.id === selectedId) return -1;
      return 0;
    })
    .map((instance) => {
      const definition = definitions[instance.definition_id];
      return definition
        ? renderComponent(
            instance,
            definition,
            instance.id === selectedId,
            board.pitch_mm,
            changes[instance.id],
          )
        : "";
    })
    .join("");
  const wires = Object.values(document.design.wires ?? {})
    .map((wire, index) => {
      const points = wire.vertices
        .map((point) => `${point.x2 / 2},${point.y2 / 2}`)
        .join(" ");
      const hiddenSide = wire.side !== side ? " opposite-side" : "";
      const selected = wire.id === selectedId ? " selected" : "";
      const changed = changes[wire.id]
        ? ` diff-${escape(changes[wire.id]!)}`
        : "";
      return `<g class="wire${hiddenSide}${selected}${changed}" data-wire-id="${wire.id}"><polyline points="${points}" style="--wire-color:${escape(wireColor(wire.color))}"/><text x="${wire.vertices[0]!.x2 / 2 + 0.2}" y="${wire.vertices[0]!.y2 / 2 - 0.2}">W${index + 1}</text></g>`;
    })
    .join("");
  const defaultViewBox = `${-PAD} ${-PAD} ${board.holes_x - 1 + PAD * 2} ${board.holes_y - 1 + PAD * 2}`;
  const margins = board.margins_mm ?? { top: 0, right: 0, bottom: 0, left: 0 };
  const left = margins.left / board.pitch_mm;
  const top = margins.top / board.pitch_mm;
  const width =
    board.holes_x - 1 + (margins.left + margins.right) / board.pitch_mm;
  const height =
    board.holes_y - 1 + (margins.top + margins.bottom) / board.pitch_mm;
  const mirror =
    side === "solder" ? `translate(${board.holes_x - 1} 0) scale(-1 1)` : "";
  const pinLabels = componentValues
    .flatMap((instance) => {
      const definition = definitions[instance.definition_id];
      if (!definition) return [];
      return definition.pins.map((pin) => {
        const [localX, localY] = rotateLocal(instance.rotation, pin.x, pin.y);
        const canonicalX = instance.position.x + localX;
        const x =
          side === "solder" ? board.holes_x - 1 - canonicalX : canonicalX;
        return pinLabel(pin.number, x, instance.position.y + localY);
      });
    })
    .join("");
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
  return `<svg id="${escape(svgId)}" aria-label="${escape(board.name)}, ${side} side" role="img" viewBox="${viewBox ?? defaultViewBox}" data-default-view-box="${defaultViewBox}" data-side="${side}"><rect class="board-outline" x="${-left}" y="${-top}" width="${width}" height="${height}" rx=".5"/><g class="board-geometry" transform="${mirror}"><g class="holes">${holes}</g>${mountingHoles}${forbidden}<g class="wires">${wires}</g><g class="components">${components}</g></g><g class="pin-labels">${pinLabels}</g>${resizeHandles}</svg>`;
}
