import type { ComponentDefinition, Rotation } from "../api/project";

export type PinLabelMode =
  "number" | "name" | "function" | "combined" | "hidden";

export interface ComponentPreviewOptions {
  rotation: Rotation;
  labelMode: PinLabelMode;
  selectedPinId?: string;
  showKeepouts: boolean;
  showAccess: boolean;
  showOccupiedHoles: boolean;
  viewBox?: string;
}

const PITCH_MM = 2.54;
const PAD = 2;
const LABEL_OFFSET = 0.5;

const escape = (value: string): string =>
  value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character] ?? character,
  );

export function rotationDegrees(rotation: Rotation): number {
  return { deg0: 0, deg90: 90, deg180: 180, deg270: 270 }[rotation];
}

export function rotatePoint(
  rotation: Rotation,
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

export function pinLabel(
  pin: ComponentDefinition["pins"][number],
  mode: PinLabelMode,
): string {
  if (mode === "hidden") return "";
  if (mode === "number") return pin.number;
  if (mode === "name") return pin.name;
  if (mode === "function") return pin.functions?.join(", ") || "—";
  const functions = pin.functions?.join(", ");
  return `${pin.number} · ${pin.name}${functions ? ` · ${functions}` : ""}`;
}

export function pinLabelPlacement(
  rotation: Rotation,
  x: number,
  y: number,
): { x: number; y: number; textAnchor: "start" | "middle" | "end" } {
  const [rotatedX, rotatedY] = rotatePoint(rotation, x, y);
  switch (rotation) {
    case "deg0":
      return {
        x: rotatedX + LABEL_OFFSET,
        y: rotatedY,
        textAnchor: "start",
      };
    case "deg90":
      return {
        x: rotatedX,
        y: rotatedY + LABEL_OFFSET,
        textAnchor: "middle",
      };
    case "deg180":
      return {
        x: rotatedX - LABEL_OFFSET,
        y: rotatedY,
        textAnchor: "end",
      };
    case "deg270":
      return {
        x: rotatedX,
        y: rotatedY - LABEL_OFFSET,
        textAnchor: "middle",
      };
  }
}

function defaultViewBox(
  definition: ComponentDefinition,
  rotation: Rotation,
): [number, number, number, number] {
  const body = definition.body;
  const points = (
    [
      [body.x / PITCH_MM, body.y / PITCH_MM],
      [(body.x + body.width) / PITCH_MM, body.y / PITCH_MM],
      [body.x / PITCH_MM, (body.y + body.height) / PITCH_MM],
      [(body.x + body.width) / PITCH_MM, (body.y + body.height) / PITCH_MM],
      ...definition.pins.map((pin) => [pin.x, pin.y] as [number, number]),
      ...(definition.occupied_holes ?? []).map(
        (hole) => [hole.x, hole.y] as [number, number],
      ),
      ...(definition.keepouts ?? []).flatMap((area) => [
        [area.min.x - 0.5, area.min.y - 0.5] as [number, number],
        [area.max.x + 0.5, area.max.y + 0.5] as [number, number],
      ]),
      ...(definition.access ?? []).flatMap((area) => [
        [area.min.x - 0.5, area.min.y - 0.5] as [number, number],
        [area.max.x + 0.5, area.max.y + 0.5] as [number, number],
      ]),
    ] as Array<[number, number]>
  ).map(([x, y]) => rotatePoint(rotation, x, y));
  points.push([0, 0]);
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  const minX = Math.floor(Math.min(...xs)) - PAD;
  const minY = Math.floor(Math.min(...ys)) - PAD;
  const maxX = Math.ceil(Math.max(...xs)) + PAD;
  const maxY = Math.ceil(Math.max(...ys)) + PAD;
  return [minX, minY, Math.max(5, maxX - minX), Math.max(5, maxY - minY)];
}

function rect(area: {
  min: { x: number; y: number };
  max: { x: number; y: number };
}): string {
  return `x="${area.min.x - 0.42}" y="${area.min.y - 0.42}" width="${area.max.x - area.min.x + 0.84}" height="${area.max.y - area.min.y + 0.84}"`;
}

export function renderComponentPreviewSvg(
  definition: ComponentDefinition,
  options: ComponentPreviewOptions,
): string {
  const defaultBox = defaultViewBox(definition, options.rotation);
  const gridMinX = Math.floor(defaultBox[0]);
  const gridMinY = Math.floor(defaultBox[1]);
  const gridMaxX = Math.ceil(defaultBox[0] + defaultBox[2]);
  const gridMaxY = Math.ceil(defaultBox[1] + defaultBox[3]);
  const holes = Array.from({ length: gridMaxY - gridMinY + 1 }, (_, row) =>
    Array.from(
      { length: gridMaxX - gridMinX + 1 },
      (_, column) =>
        `<circle class="preview-grid-hole" cx="${gridMinX + column}" cy="${gridMinY + row}" r=".09"/>`,
    ).join(""),
  ).join("");
  const keepouts = options.showKeepouts
    ? (definition.keepouts ?? [])
        .map(
          (area) =>
            `<rect class="preview-keepout" ${rect(area)}><title>${escape(area.label)}</title></rect>`,
        )
        .join("")
    : "";
  const access = options.showAccess
    ? (definition.access ?? [])
        .map((area) => {
          const arrow = { north: "↑", east: "→", south: "↓", west: "←" }[
            area.direction
          ];
          const x = (area.min.x + area.max.x) / 2;
          const y = (area.min.y + area.max.y) / 2;
          return `<g class="preview-access"><rect ${rect(area)}/><text x="${x}" y="${y + 0.2}">${arrow}</text><title>${escape(area.label)}</title></g>`;
        })
        .join("")
    : "";
  const occupied = options.showOccupiedHoles
    ? (definition.occupied_holes ?? [])
        .map(
          (hole) =>
            `<path class="preview-occupied" d="M ${hole.x - 0.25} ${hole.y - 0.25} L ${hole.x + 0.25} ${hole.y + 0.25} M ${hole.x + 0.25} ${hole.y - 0.25} L ${hole.x - 0.25} ${hole.y + 0.25}"/>`,
        )
        .join("")
    : "";
  const pins = definition.pins
    .map((pin) => {
      const selected = pin.id === options.selectedPinId ? " selected" : "";
      return `<g class="preview-pin${selected}" data-preview-pin-id="${escape(pin.id)}"><circle cx="${pin.x}" cy="${pin.y}" r=".28"/><title>${escape(`${pin.number}: ${pin.name}`)}</title></g>`;
    })
    .join("");
  const labels = definition.pins
    .map((pin) => {
      const label = pinLabel(pin, options.labelMode);
      if (!label) return "";
      const placement = pinLabelPlacement(options.rotation, pin.x, pin.y);
      return `<text class="preview-pin-label" data-preview-pin-label-id="${escape(pin.id)}" x="${placement.x}" y="${placement.y}" text-anchor="${placement.textAnchor}" dominant-baseline="central">${escape(label)}</text>`;
    })
    .join("");
  const rotation = rotationDegrees(options.rotation);
  const body = definition.body;
  const viewBox = options.viewBox ?? defaultBox.join(" ");
  return `<svg id="component-preview" role="img" aria-label="${escape(definition.name)} component preview" viewBox="${viewBox}" data-default-view-box="${defaultBox.join(" ")}"><g class="preview-grid">${holes}</g><g class="preview-axes"><path d="M ${gridMinX} 0 H ${gridMaxX} M 0 ${gridMinY} V ${gridMaxY}"/><circle cx="0" cy="0" r=".18"/><text x=".25" y="-.25">0,0</text></g><g class="preview-geometry" transform="rotate(${rotation})">${keepouts}<rect class="preview-body" x="${body.x / PITCH_MM}" y="${body.y / PITCH_MM}" width="${body.width / PITCH_MM}" height="${body.height / PITCH_MM}" rx=".35"/><path class="preview-orientation" d="M .2 .2 L .85 .2 L .2 .85 Z"/>${occupied}${pins}${access}</g><g class="preview-labels">${labels}</g></svg>`;
}
