import type { ComponentDefinition, Rotation } from "../api/project";
import type { ResizeHandle } from "../editor/board-size";
import { bodyGridRect, keepoutGridRect } from "./geometry-resize";
import { bridgeConductorPaths } from "../editor/bridge-geometry";

export type PinLabelMode =
  "number" | "name" | "function" | "combined" | "hidden";

export interface ComponentPreviewOptions {
  rotation: Rotation;
  labelMode: PinLabelMode;
  selectedPinId?: string;
  showKeepouts: boolean;
  showWireKeepouts?: boolean;
  showAccess: boolean;
  showOccupiedHoles: boolean;
  resizeGeometry?: "body" | "keepout" | "access";
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
      ...(definition.wire_keepouts ?? []).flatMap((area) => [
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

function rect(
  area: {
    min: { x: number; y: number };
    max: { x: number; y: number };
  },
  padding = 0.42,
): string {
  return `x="${area.min.x - padding}" y="${area.min.y - padding}" width="${area.max.x - area.min.x + padding * 2}" height="${area.max.y - area.min.y + padding * 2}"`;
}

function geometryResizeControls(
  kind: "body" | "keepout" | "access",
  bounds: { x: number; y: number; width: number; height: number },
  index?: number,
): string {
  const { x, y, width, height } = bounds;
  const right = x + width;
  const bottom = y + height;
  const indexAttribute =
    index === undefined ? "" : ` data-component-resize-index="${index}"`;
  const common = (handle: ResizeHandle) =>
    `data-component-resize-kind="${kind}"${indexAttribute} data-component-resize-handle="${handle}"`;
  const edges = [
    ["top", x, y, right, y],
    ["right", right, y, right, bottom],
    ["bottom", x, bottom, right, bottom],
    ["left", x, y, x, bottom],
  ] as const;
  const corners = [
    ["top-left", x, y],
    ["top-right", right, y],
    ["bottom-right", right, bottom],
    ["bottom-left", x, bottom],
  ] as const;
  const feedback =
    kind === "body"
      ? `${width} × ${height} holes`
      : kind === "keepout"
        ? `${width} × ${height} holes kept out`
        : `${width} × ${height} holes accessible`;
  return `<g class="component-resize-controls" data-component-resize-controls="${kind}"${indexAttribute}><rect class="component-resize-boundary" x="${x}" y="${y}" width="${width}" height="${height}"/><text class="component-resize-feedback" x="${x + width / 2}" y="${y + height / 2}" text-anchor="middle" dy="0.35em">${feedback}</text>${edges.map(([handle, x1, y1, x2, y2]) => `<line ${common(handle)} x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`).join("")}${corners.map(([handle, cx, cy]) => `<rect class="component-resize-corner" ${common(handle)} x="${cx - 0.18}" y="${cy - 0.18}" width=".36" height=".36" rx=".07"/>`).join("")}</g>`;
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
  const keepouts =
    options.showKeepouts || options.resizeGeometry === "keepout"
      ? (definition.keepouts ?? [])
          .map(
            (area) =>
              `<rect class="preview-keepout" ${rect(area, 0.5)}><title>${escape(area.label)}</title></rect>`,
          )
          .join("")
      : "";
  const wireKeepouts = options.showWireKeepouts
    ? (definition.wire_keepouts ?? [])
        .map(
          (area) =>
            `<rect class="preview-wire-keepout" ${rect(area, 0.5)}><title>${escape(area.label)}</title></rect>`,
        )
        .join("")
    : "";
  const access =
    options.showAccess || options.resizeGeometry === "access"
      ? (definition.access ?? [])
          .map((area) => {
            const arrow = { north: "↑", east: "→", south: "↓", west: "←" }[
              area.direction
            ];
            const x = (area.min.x + area.max.x) / 2;
            const y = (area.min.y + area.max.y) / 2;
            return `<g class="preview-access"><rect ${rect(area, 0.5)}/><text x="${x}" y="${y + 0.2}">${arrow}</text><title>${escape(area.label)}</title></g>`;
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
  const labels = (definition.kind === "bridge" ? [] : definition.pins)
    .map((pin) => {
      const label = pinLabel(pin, options.labelMode);
      if (!label) return "";
      const placement = pinLabelPlacement(options.rotation, pin.x, pin.y);
      return `<text class="preview-pin-label" data-preview-pin-label-id="${escape(pin.id)}" x="${placement.x}" y="${placement.y}" text-anchor="${placement.textAnchor}" dy="0.35em">${escape(label)}</text>`;
    })
    .join("");
  const rotation = rotationDegrees(options.rotation);
  const body = definition.body;
  const bridgeGeometry = (() => {
    if (definition.kind !== "bridge" || definition.pins.length !== 2)
      return undefined;
    const paths = bridgeConductorPaths(
      definition.pins[0]!,
      definition.pins[1]!,
    );
    return `<path class="preview-bridge preview-bridge-span" d="${paths.span}"/><path class="preview-bridge preview-bridge-terminals" d="${paths.terminals}"/>`;
  })();
  const resizeControls =
    options.resizeGeometry === "body"
      ? geometryResizeControls("body", bodyGridRect(body))
      : options.resizeGeometry === "keepout"
        ? (definition.keepouts ?? [])
            .map((area, index) =>
              geometryResizeControls("keepout", keepoutGridRect(area), index),
            )
            .join("")
        : options.resizeGeometry === "access"
          ? (definition.access ?? [])
              .map((area, index) =>
                geometryResizeControls("access", keepoutGridRect(area), index),
              )
              .join("")
          : "";
  const viewBox = options.viewBox ?? defaultBox.join(" ");
  const primaryGeometry =
    bridgeGeometry ??
    `<rect class="preview-body" x="${body.x / PITCH_MM}" y="${body.y / PITCH_MM}" width="${body.width / PITCH_MM}" height="${body.height / PITCH_MM}" rx=".35"/><path class="preview-orientation" d="M .2 .2 L .85 .2 L .2 .85 Z"/>`;
  return `<svg id="component-preview" role="img" aria-label="${escape(definition.name)} component preview" viewBox="${viewBox}" data-default-view-box="${defaultBox.join(" ")}"><g class="preview-grid">${holes}</g><g class="preview-axes"><path d="M ${gridMinX} 0 H ${gridMaxX} M 0 ${gridMinY} V ${gridMaxY}"/><circle cx="0" cy="0" r=".18"/><text x=".25" y="-.25">0,0</text></g><g class="preview-geometry" transform="rotate(${rotation})">${keepouts}${wireKeepouts}${primaryGeometry}${occupied}${pins}${access}${resizeControls}</g><g class="preview-labels">${labels}</g></svg>`;
}
