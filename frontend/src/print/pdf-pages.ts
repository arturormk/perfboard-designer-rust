/**
 * True-scale pages for the wire cut guide and bending templates.
 *
 * Builds complete page-sized SVG documents in millimetres, which Rust turns
 * into a PDF (`export_pdf`). Never send these through the web view's print:
 * WebKitGTK rescales printed pages, so nothing printed from HTML is 1:1. Only
 * presentation attributes are used (no CSS), so any SVG renderer agrees.
 */
import type { Design } from "../model/geometry";
import {
  describeWire,
  railLabels,
  wireColorCss,
  wireColorName,
} from "../model/naming";
import {
  cutColumns,
  groupWireTemplates,
  layoutTemplate,
  LEAD_MM,
  RAIL_WINDOW_MM,
  railPieces,
  type ColorGroup,
  type CutColumn,
  type RailPiece,
  type TemplateLayout,
  type WireShape,
} from "./wire-templates";

export interface Paper {
  name: string;
  width: number;
  height: number;
}

export const A4: Paper = { name: "A4", width: 210, height: 297 };
export const LETTER: Paper = { name: "Letter", width: 215.9, height: 279.4 };

/** Regions that use US Letter paper; everywhere else uses A4. */
const LETTER_REGIONS = new Set([
  "US",
  "CA",
  "MX",
  "PH",
  "CL",
  "CO",
  "VE",
  "CR",
  "GT",
  "PA",
  "DO",
  "PR",
  "SV",
  "NI",
  "HN",
  "BZ",
]);

export function paperForLocale(locale: string | undefined): Paper {
  const region = locale?.split(/[-_]/)[1]?.toUpperCase();
  return region && LETTER_REGIONS.has(region) ? LETTER : A4;
}

const MARGIN = 10;
/** Title, calibration bar and the page-scaling warning above the content. */
const HEADER = 25;
const FOOTER = 5;
const CALIBRATION_MM = 50;
/** Average glyph advance as a share of font size, slightly generous. */
const GLYPH_EM = 0.58;

const CUT_COLUMN_W = 15;
const CUT_LABEL_H = 9;
const CUT_ROW_GAP = 4;

const CARD_PAD = 3;
const CARD_MIN_INNER = 52;
const CARD_GAP = 4;
const HEADING_H = 7;
const WIRE_WIDTH_MM = 1.4;
const LEAD_COLOR = "#c8a76a";

// ── SVG primitives ──────────────────────────────────────────────────────────

const num = (value: number) => String(Math.round(value * 1000) / 1000);

export function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface TextStyle {
  size: number;
  bold?: boolean;
  anchor?: "start" | "middle" | "end";
  fill?: string;
}

function textAttrs(style: TextStyle): string {
  return [
    `font-size="${num(style.size)}"`,
    style.bold ? `font-weight="bold"` : "",
    style.anchor && style.anchor !== "start"
      ? `text-anchor="${style.anchor}"`
      : "",
    `fill="${style.fill ?? "#000"}"`,
  ]
    .filter(Boolean)
    .join(" ");
}

/** Text with its baseline at `y`. */
function text(x: number, y: number, content: string, style: TextStyle): string {
  return `<text x="${num(x)}" y="${num(y)}" ${textAttrs(style)}>${escapeXml(content)}</text>`;
}

/** Text centred on (x, y), rotated by `angle` degrees. */
function centredText(
  x: number,
  y: number,
  content: string,
  angle: number,
  style: TextStyle,
): string {
  return `<text transform="translate(${num(x)} ${num(y)}) rotate(${num(angle)})" y="${num(style.size * 0.35)}" ${textAttrs({ ...style, anchor: "middle" })}>${escapeXml(content)}</text>`;
}

interface Stroke {
  color: string;
  width: number;
  dash?: string;
  cap?: "butt" | "round";
}

function strokeAttrs(stroke: Stroke): string {
  return [
    `stroke="${stroke.color}"`,
    `stroke-width="${num(stroke.width)}"`,
    stroke.dash ? `stroke-dasharray="${stroke.dash}"` : "",
    stroke.cap ? `stroke-linecap="${stroke.cap}"` : "",
  ]
    .filter(Boolean)
    .join(" ");
}

function line(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  stroke: Stroke,
): string {
  return `<line x1="${num(x1)}" y1="${num(y1)}" x2="${num(x2)}" y2="${num(y2)}" ${strokeAttrs(stroke)}/>`;
}

function polyline(points: { x: number; y: number }[], stroke: Stroke): string {
  const coords = points.map((point) => `${num(point.x)},${num(point.y)}`);
  return `<polyline points="${coords.join(" ")}" fill="none" stroke-linejoin="round" ${strokeAttrs(stroke)}/>`;
}

export function textWidth(content: string, size: number): number {
  return content.length * size * GLYPH_EM;
}

/** Greedy word wrap to a width in millimetres; long words stay whole. */
export function wrapText(
  content: string,
  width: number,
  size: number,
): string[] {
  const lines: string[] = [];
  let current = "";
  for (const word of content.split(/\s+/).filter(Boolean)) {
    const candidate = current ? `${current} ${word}` : word;
    if (current && textWidth(candidate, size) > width) {
      lines.push(current);
      current = word;
    } else current = candidate;
  }
  if (current) lines.push(current);
  return lines;
}

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

// ── Page frame ──────────────────────────────────────────────────────────────

interface PageContent {
  title: string;
  body: string[];
}

interface Frame {
  paper: Paper;
  left: number;
  right: number;
  top: number;
  bottom: number;
}

function frameFor(paper: Paper): Frame {
  return {
    paper,
    left: MARGIN,
    right: paper.width - MARGIN,
    top: MARGIN + HEADER,
    bottom: paper.height - MARGIN - FOOTER,
  };
}

function calibrationBar(x: number, y: number): string {
  const parts = [
    line(x, y, x + CALIBRATION_MM, y, { color: "#000", width: 0.3 }),
    line(x, y - 1.5, x, y + 1.5, { color: "#000", width: 0.3 }),
    line(x + CALIBRATION_MM, y - 1.5, x + CALIBRATION_MM, y + 1.5, {
      color: "#000",
      width: 0.3,
    }),
  ];
  for (let mm = 10; mm < CALIBRATION_MM; mm += 10)
    parts.push(
      line(x + mm, y - 0.8, x + mm, y + 0.8, { color: "#000", width: 0.25 }),
    );
  return parts.join("");
}

const WARNING_RED = "#d00000";

/**
 * The print dialog's page scaling is the one setting that can ruin these
 * sheets, so every page says so in red, using the dialog's own wording.
 */
function scaleWarning(x: number, y: number, width: number): string {
  return [
    `<rect x="${num(x)}" y="${num(y)}" width="${num(width)}" height="9.5" rx="1" fill="none" stroke="${WARNING_RED}" stroke-width="0.5"/>`,
    text(
      x + 2.5,
      y + 4,
      "Print at actual size: in the print dialog, set Page Scaling to None.",
      { size: 3.2, bold: true, fill: WARNING_RED },
    ),
    text(
      x + 2.5,
      y + 7.8,
      "“Fit”, “Shrink to printable area” or any scale other than 100 % makes every template and length wrong.",
      { size: 2.5, fill: WARNING_RED },
    ),
  ].join("");
}

function renderPage(
  page: PageContent,
  index: number,
  total: number,
  paper: Paper,
  footer: string,
): string {
  const { width, height } = paper;
  const right = width - MARGIN;
  const header = [
    text(MARGIN, MARGIN + 4, page.title, { size: 4.2, bold: true }),
    text(right, MARGIN + 4, `Page ${index + 1} of ${total}`, {
      size: 2.6,
      anchor: "end",
    }),
    calibrationBar(MARGIN, MARGIN + 9.5),
    text(
      MARGIN + CALIBRATION_MM + 3,
      MARGIN + 10.4,
      "Check after printing: this bar must measure exactly 50 mm.",
      { size: 2.6, bold: true },
    ),
    scaleWarning(MARGIN, MARGIN + 12.5, right - MARGIN),
  ];
  const foot = [
    text(MARGIN, height - MARGIN, footer, { size: 2.3, fill: "#555" }),
    text(right, height - MARGIN, `${paper.name} · drawn at 1:1`, {
      size: 2.3,
      anchor: "end",
      fill: "#555",
    }),
  ];
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${num(width)}mm" height="${num(height)}mm" viewBox="0 0 ${num(width)} ${num(height)}" font-family="sans-serif">`,
    `<rect width="${num(width)}" height="${num(height)}" fill="#fff"/>`,
    ...header,
    ...page.body,
    ...foot,
    "</svg>",
  ].join("\n");
}

// ── Cut guide ───────────────────────────────────────────────────────────────

function gridStroke(mm: number): Stroke {
  if (mm % 10 === 0) return { color: "#9a9a9a", width: 0.22 };
  if (mm % 5 === 0) return { color: "#c8c8c8", width: 0.12 };
  return { color: "#e2e2e2", width: 0.08 };
}

function cutGrid(x: number, y: number, width: number, span: number): string {
  const parts: string[] = [];
  for (let mm = 0; mm <= width; mm += 1)
    parts.push(line(x + mm, y, x + mm, y + span, gridStroke(mm)));
  for (let mm = 0; mm <= span; mm += 1)
    parts.push(line(x, y + mm, x + width, y + mm, gridStroke(mm)));
  return parts.join("");
}

function cutColumn(
  column: CutColumn,
  label: string,
  x: number,
  top: number,
): string {
  const { fromMm, toMm, cutMm, pathMm, ends } = column;
  const topLead = ends[0].leadMm;
  const bodyEnd = topLead + pathMm;
  const centre = x + CUT_COLUMN_W / 2;
  const gridTop = top + CUT_LABEL_H;
  const y = (along: number) => gridTop + along - fromMm;
  const visible = (along: number) =>
    along >= fromMm - 1e-9 && along <= toMm + 1e-9;
  const stretch = (start: number, end: number) => {
    const a = Math.max(start, fromMm);
    const b = Math.min(end, toMm);
    return b > a ? ([y(a), y(b)] as const) : undefined;
  };
  const parts = [
    text(
      centre,
      top + 3.2,
      column.parts > 1 ? `${label} · ${column.part}/${column.parts}` : label,
      { size: 3, bold: true, anchor: "middle" },
    ),
    text(centre, top + 7, `${cutMm.toFixed(1)} mm`, {
      size: 2.6,
      anchor: "middle",
    }),
  ];
  for (
    let cm = Math.max(1, Math.ceil(fromMm / 10));
    cm * 10 <= toMm + 1e-9;
    cm += 1
  )
    parts.push(
      text(x + 2.6, y(cm * 10) + 0.63, String(cm), {
        size: 1.8,
        anchor: "middle",
        fill: "#555",
      }),
    );
  for (const lead of [stretch(0, topLead), stretch(bodyEnd, cutMm)])
    if (lead)
      parts.push(
        line(centre, lead[0], centre, lead[1], {
          color: LEAD_COLOR,
          width: 0.8,
        }),
      );
  const body = stretch(topLead, bodyEnd);
  if (body) {
    parts.push(
      line(centre, body[0], centre, body[1], {
        color: "#000",
        width: WIRE_WIDTH_MM + 0.35,
      }),
      line(centre, body[0], centre, body[1], {
        color: wireColorCss(column.color),
        width: WIRE_WIDTH_MM,
      }),
    );
  }
  for (const window of column.windows) {
    const bare = stretch(window.fromMm, window.toMm);
    if (!bare) continue;
    parts.push(
      line(centre, bare[0], centre, bare[1], {
        color: LEAD_COLOR,
        width: WIRE_WIDTH_MM + 0.35,
      }),
    );
    // Slice lines across the insulation at each edge of the window.
    for (const along of [window.fromMm, window.toMm].filter(
      (edge) => visible(edge) && edge > 1e-9 && edge < cutMm - 1e-9,
    ))
      parts.push(
        line(centre - 2, y(along), centre + 2, y(along), {
          color: "#000",
          width: 0.25,
        }),
      );
  }
  // Dashed strip ticks where a lead bends down into its hole.
  for (const along of [topLead, bodyEnd].filter(
    (at, index) => ends[index]!.intoHole && visible(at),
  ))
    parts.push(
      line(centre - 3, y(along), centre + 3, y(along), {
        color: "#000",
        width: 0.2,
        dash: "0.6 0.4",
      }),
    );
  let lastLabel = -Infinity;
  for (const bend of column.bends.filter((item) => visible(item.alongMm))) {
    const at = y(bend.alongMm);
    // A small filled triangle pointing at the bend, from the right.
    parts.push(
      `<polygon points="${num(centre + 1.05)},${num(at)} ${num(centre + 2.7)},${num(at - 0.95)} ${num(centre + 2.7)},${num(at + 0.95)}" fill="#000"/>`,
    );
    // Bends closer than a label's height share the first label.
    if (at - lastLabel >= 2.4) {
      parts.push(
        text(centre + 3.1, at + 0.65, `${bend.angle}°`, { size: 1.8 }),
      );
      lastLabel = at;
    }
  }
  for (const along of [0, cutMm].filter(visible))
    parts.push(
      line(x + 1, y(along), x + CUT_COLUMN_W - 1, y(along), {
        color: "#000",
        width: 0.35,
      }),
      text(x + CUT_COLUMN_W - 1, y(along) + 2.2, "cut", {
        size: 1.8,
        anchor: "end",
      }),
    );
  return parts.join("");
}

function spanOf(columns: CutColumn[]): number {
  const longest = Math.max(...columns.map((item) => item.toMm - item.fromMm));
  return Math.max(10, Math.ceil(longest / 10 - 1e-9) * 10);
}

function cutGuidePages(
  groups: ColorGroup[],
  rails: RailPiece[],
  labels: Map<string, string>,
  frame: Frame,
  pitchMm: number,
): PageContent[] {
  const title = "Wire cut guide";
  const width = frame.right - frame.left;
  const railNote =
    rails.length > 0
      ? ` Rails (R…) follow the wires. Their ends have leads like a wire’s (clip them if the rail is soldered only at its taps); the tan windows, ${RAIL_WINDOW_MM} mm long and centred on each connection point, are where the insulation is sliced off once the rail is bent.`
      : "";
  const intro = wrapText(
    `Lay each wire on its column straight from the spool and mark the cut lines, the bends (◀ with their angle) and the dashed strip ticks (≈ ${LEAD_MM} mm from each end, where the leads bend down into their holes). The top of each column is the wire’s start end. Grid squares are 10 mm; numbers are centimetres from the top cut. Wires are in colour order, then by number.${railNote}`,
    width,
    2.8,
  );
  const introH = intro.length * 3.8 + 2;
  const maxSpan =
    Math.floor((frame.bottom - frame.top - introH - CUT_LABEL_H) / 10) * 10;
  const columns = cutColumns(groups, labels, maxSpan, pitchMm, rails);
  if (columns.length === 0) return [];
  const perRow = Math.max(1, Math.floor(width / CUT_COLUMN_W));
  // Rails start a row of their own.
  const railIds = new Set(rails.map((piece) => piece.rail.id));
  const rows: CutColumn[][] = [];
  for (const list of [
    columns.filter((column) => !railIds.has(column.id)),
    columns.filter((column) => railIds.has(column.id)),
  ])
    for (let start = 0; start < list.length; start += perRow)
      rows.push(list.slice(start, start + perRow));

  const pages: PageContent[] = [{ title, body: [] }];
  let y = frame.top;
  intro.forEach((content, index) =>
    pages[0]!.body.push(
      text(frame.left, y + 2.8 + index * 3.8, content, { size: 2.8 }),
    ),
  );
  y += introH;
  for (const row of rows) {
    const span = spanOf(row);
    const height = CUT_LABEL_H + span;
    if (y + height > frame.bottom) {
      pages.push({ title, body: [] });
      y = frame.top;
    }
    const body = pages[pages.length - 1]!.body;
    body.push(
      cutGrid(frame.left, y + CUT_LABEL_H, row.length * CUT_COLUMN_W, span),
    );
    row.forEach((column, slot) =>
      body.push(
        cutColumn(
          column,
          labels.get(column.id) ?? column.id,
          frame.left + slot * CUT_COLUMN_W,
          y,
        ),
      ),
    );
    y += height + CUT_ROW_GAP;
  }
  return pages;
}

// ── Bending templates ───────────────────────────────────────────────────────

function arrowHead(
  tip: { x: number; y: number },
  from: { x: number; y: number },
): string {
  const length = Math.hypot(tip.x - from.x, tip.y - from.y) || 1;
  const ux = (tip.x - from.x) / length;
  const uy = (tip.y - from.y) / length;
  const back = { x: tip.x - ux * 1.2, y: tip.y - uy * 1.2 };
  return polyline(
    [
      { x: back.x - uy * 0.45, y: back.y + ux * 0.45 },
      tip,
      { x: back.x + uy * 0.45, y: back.y - ux * 0.45 },
    ],
    { color: "#444", width: 0.15 },
  );
}

function templateDrawing(
  layout: TemplateLayout,
  color: string,
  x: number,
  y: number,
): string {
  const thin: Stroke = { color: "#444", width: 0.15 };
  const parts = [`<g transform="translate(${num(x)} ${num(y)})">`];
  for (const hole of layout.holes)
    parts.push(
      `<circle cx="${num(hole.x)}" cy="${num(hole.y)}" r="0.45" fill="#bbb"/>`,
    );
  for (const mark of layout.dimensions) {
    for (const [a, b] of mark.witness)
      parts.push(line(a.x, a.y, b.x, b.y, thin));
    parts.push(
      line(mark.from.x, mark.from.y, mark.to.x, mark.to.y, thin),
      arrowHead(mark.from, mark.to),
      arrowHead(mark.to, mark.from),
      centredText(mark.lengthAt.x, mark.lengthAt.y, mark.length, mark.angle, {
        size: 2.3,
      }),
      centredText(
        mark.spacingAt.x,
        mark.spacingAt.y,
        mark.spacing,
        mark.angle,
        {
          size: 2.3,
          fill: "#555",
        },
      ),
    );
  }
  for (const [a, b] of layout.leads)
    parts.push(
      line(a.x, a.y, b.x, b.y, { color: LEAD_COLOR, width: 0.8, cap: "round" }),
    );
  parts.push(
    polyline(layout.path, { color: "#000", width: WIRE_WIDTH_MM + 0.35 }),
    polyline(layout.path, { color, width: WIRE_WIDTH_MM }),
  );
  // Rail connection points: bare windows, with slice lines at their edges.
  for (const window of layout.windows) {
    parts.push(
      polyline(window, { color: LEAD_COLOR, width: WIRE_WIDTH_MM + 0.35 }),
    );
    for (const [edge, next] of [
      [window[0]!, window[1]!],
      [window[window.length - 1]!, window[window.length - 2]!],
    ] as const) {
      const length = Math.hypot(next.x - edge.x, next.y - edge.y) || 1;
      const nx = -(next.y - edge.y) / length;
      const ny = (next.x - edge.x) / length;
      parts.push(
        line(
          edge.x - nx * 1.6,
          edge.y - ny * 1.6,
          edge.x + nx * 1.6,
          edge.y + ny * 1.6,
          { color: "#000", width: 0.25 },
        ),
      );
    }
  }
  for (const hole of layout.bendHoles)
    parts.push(
      `<circle cx="${num(hole.x)}" cy="${num(hole.y)}" r="1.1" fill="none" ${strokeAttrs({ color: "#000", width: 0.25, dash: "0.6 0.4" })}/>`,
    );
  parts.push("</g>");
  return parts.join("");
}

interface Card {
  width: number;
  height: number;
  render: (x: number, y: number) => string;
}

const TITLE_SIZE = 3;
const INFO_SIZE = 2.6;
const LIST_SIZE = 2.3;

export function templateCard(
  shape: WireShape,
  design: Design,
  labels: Map<string, string>,
  maxInner: number,
): Card {
  const pitch = design.board.pitch_mm;
  let layout = layoutTemplate(shape.points, pitch);
  // A shape too wide for the paper fits better turned a quarter: it is the
  // same piece of wire either way.
  if (layout.width > maxInner) {
    const turned = layoutTemplate(
      shape.points.map((point) => ({ x2: -point.y2, y2: point.x2 })),
      pitch,
    );
    if (turned.width < layout.width) layout = turned;
  }
  const inner = Math.max(layout.width, CARD_MIN_INNER);
  const names = shape.wires.map((wire) => labels.get(wire.id) ?? wire.id);
  const title = wrapText(
    `${shape.wires.length > 1 ? `${shape.wires.length} × ` : ""}${names.join(", ")}`,
    inner,
    TITLE_SIZE,
  );
  const info = wrapText(
    `Cut ≈ ${shape.cutMm.toFixed(1)} mm · path ${shape.pathMm.toFixed(1)} mm · ${plural(shape.bends, "bend")} · strip ${LEAD_MM} mm each end`,
    inner,
    INFO_SIZE,
  );
  const nets = design.nets ?? {};
  const list = shape.wires.flatMap((wire) =>
    wrapText(
      `${labels.get(wire.id) ?? wire.id}: ${nets[wire.net_id]?.name ?? wire.net_id} · ${describeWire(design, wire)}${wire.side === "solder" ? " · solder side, drawn as seen from it" : ""}`,
      inner,
      LIST_SIZE,
    ),
  );
  return cardOf(
    inner,
    title,
    info,
    list,
    layout,
    wireColorCss(shape.wires[0]!.color),
  );
}

/** Title, information line, drawing and list, framed. */
function cardOf(
  inner: number,
  title: string[],
  info: string[],
  list: string[],
  layout: TemplateLayout,
  color: string,
): Card {
  const titleH = title.length * 3.8;
  const infoH = info.length * 3.3;
  const listH = list.length * 3;
  const width = inner + 2 * CARD_PAD;
  const height =
    CARD_PAD + titleH + infoH + 1 + layout.height + 1 + listH + CARD_PAD;
  return {
    width,
    height,
    render: (x, y) => {
      const parts = [
        `<rect x="${num(x)}" y="${num(y)}" width="${num(width)}" height="${num(height)}" rx="1.5" fill="none" stroke="#999" stroke-width="0.25"/>`,
      ];
      let cursor = y + CARD_PAD;
      for (const content of title) {
        parts.push(
          text(x + CARD_PAD, cursor + 2.8, content, {
            size: TITLE_SIZE,
            bold: true,
          }),
        );
        cursor += 3.8;
      }
      for (const content of info) {
        parts.push(
          text(x + CARD_PAD, cursor + 2.5, content, { size: INFO_SIZE }),
        );
        cursor += 3.3;
      }
      cursor += 1;
      parts.push(templateDrawing(layout, color, x + CARD_PAD, cursor));
      cursor += layout.height + 1;
      for (const content of list) {
        parts.push(
          text(x + CARD_PAD, cursor + 2.2, content, { size: LIST_SIZE }),
        );
        cursor += 3;
      }
      return parts.join("");
    },
  };
}

function describeRailEnds(piece: RailPiece): string {
  const [start, end] = piece.ends;
  const one = (item: (typeof piece.ends)[number]) =>
    item.intoHole
      ? `strip ${item.leadMm} mm`
      : `bare ${item.leadMm} mm tip (between holes)`;
  if (start.intoHole === end.intoHole && start.leadMm === end.leadMm)
    return `${one(start)} each end`;
  return `${one(start)} at the start, ${one(end)} at the end`;
}

/** A template card for one rail: cut, bend, then slice the windows. */
export function railCard(
  piece: RailPiece,
  design: Design,
  label: string,
  maxInner: number,
): Card {
  const pitch = design.board.pitch_mm;
  const rail = {
    connections: piece.connections,
    windowMm: RAIL_WINDOW_MM,
    ends: piece.ends,
  };
  const lead = LEAD_MM;
  let layout = layoutTemplate(piece.points, pitch, lead, rail);
  if (layout.width > maxInner) {
    const turned = layoutTemplate(
      piece.points.map((point) => ({ x2: -point.y2, y2: point.x2 })),
      pitch,
      lead,
      rail,
    );
    if (turned.width < layout.width) layout = turned;
  }
  const inner = Math.max(layout.width, CARD_MIN_INNER);
  const title = wrapText(`${label} · rail`, inner, TITLE_SIZE);
  const info = wrapText(
    `Cut ≈ ${piece.cutMm.toFixed(1)} mm · path ${piece.pathMm.toFixed(1)} mm · ${plural(piece.bends.length, "bend")} · ${plural(piece.connections.length, "window")} of ${RAIL_WINDOW_MM} mm · ${describeRailEnds(piece)}`,
    inner,
    INFO_SIZE,
  );
  const net = design.nets?.[piece.rail.net_id]?.name ?? piece.rail.net_id;
  const list = wrapText(
    `${label}: ${net} · ${wireColorName(piece.rail.color)}${piece.rail.side === "solder" ? " · solder side, drawn as seen from it" : ""}`,
    inner,
    LIST_SIZE,
  );
  return cardOf(
    inner,
    title,
    info,
    list,
    layout,
    wireColorCss(piece.rail.color),
  );
}

function groupHeading(group: ColorGroup, x: number, y: number): string {
  const name = group.name.charAt(0).toUpperCase() + group.name.slice(1);
  return [
    `<rect x="${num(x)}" y="${num(y + 1)}" width="3.5" height="3.5" fill="${wireColorCss(group.color)}" stroke="#000" stroke-width="0.2"/>`,
    text(
      x + 5,
      y + 4.2,
      `${name} · ${plural(group.wireCount, "wire")} · ${plural(group.shapes.length, "shape")} · cut ≈ ${Math.round(group.totalCutMm)} mm in total`,
      { size: 3.2, bold: true },
    ),
  ].join("");
}

function templatePages(
  groups: ColorGroup[],
  rails: RailPiece[],
  design: Design,
  labels: Map<string, string>,
  frame: Frame,
): PageContent[] {
  const title = "Wire bending templates";
  const width = frame.right - frame.left;
  const railNote =
    rails.length > 0
      ? ` Rails come last: bend the rail on its template, then slice the insulation off at the tan windows, between the two lines across each, and solder there. Rail leads go into the ringed holes like a wire’s, or can be clipped if the rail is soldered only at its windows.`
      : "";
  const intro = wrapText(
    `Lay a cut wire on its template and bend it to match. Dots are board holes, ${design.board.pitch_mm} mm apart. Tan ends are bare leads: bend them down 90° at the ringed holes. Each run is dimensioned in millimetres and hole pitches. Lengths assume sharp bends, so cut lengths are estimates.${railNote}`,
    width,
    2.8,
  );
  const pages: PageContent[] = [{ title, body: [] }];
  let y = frame.top;
  intro.forEach((content, index) =>
    pages[0]!.body.push(
      text(frame.left, y + 2.8 + index * 3.8, content, { size: 2.8 }),
    ),
  );
  y += intro.length * 3.8 + 3;
  const newPage = () => {
    pages.push({ title, body: [] });
    y = frame.top;
  };
  const sections = groups.map((group) => ({
    heading: (x: number, at: number) => groupHeading(group, x, at),
    cards: group.shapes.map((shape) =>
      templateCard(shape, design, labels, width - 2 * CARD_PAD),
    ),
  }));
  if (rails.length > 0)
    sections.push({
      heading: (x: number, at: number) =>
        text(
          x,
          at + 4.2,
          `Rails · ${plural(rails.length, "rail")} · cut ≈ ${Math.round(rails.reduce((total, piece) => total + piece.cutMm, 0))} mm in total`,
          { size: 3.2, bold: true },
        ),
      cards: rails.map((piece) =>
        railCard(
          piece,
          design,
          labels.get(piece.rail.id) ?? piece.rail.id,
          width - 2 * CARD_PAD,
        ),
      ),
    });
  for (const { heading, cards } of sections) {
    if (cards.length === 0) continue;
    // Keep the heading with its first card.
    if (y + HEADING_H + cards[0]!.height > frame.bottom && y > frame.top)
      newPage();
    pages[pages.length - 1]!.body.push(heading(frame.left, y));
    y += HEADING_H;
    let x = frame.left;
    let rowHeight = 0;
    for (const card of cards) {
      if (x > frame.left && x + card.width > frame.right) {
        y += rowHeight + CARD_GAP;
        x = frame.left;
        rowHeight = 0;
      }
      if (y + card.height > frame.bottom && y > frame.top) {
        newPage();
        x = frame.left;
        rowHeight = 0;
      }
      pages[pages.length - 1]!.body.push(card.render(x, y));
      x += card.width + CARD_GAP;
      rowHeight = Math.max(rowHeight, card.height);
    }
    y += rowHeight + CARD_GAP + 2;
  }
  return pages;
}

// ── Document ────────────────────────────────────────────────────────────────

export interface WirePagesInput {
  design: Design;
  labels: Map<string, string>;
  paper: Paper;
  /** Shown in every page footer, e.g. project name, revision and date. */
  footer: string;
}

/**
 * The cut guide, then the bending templates, as complete SVG pages. Rails
 * follow the wires in both, labelled as on the board (R1, R2…).
 */
export function buildWirePages(input: WirePagesInput): string[] {
  const { design, paper, footer } = input;
  const labels = new Map([...input.labels, ...railLabels(design)]);
  const groups = groupWireTemplates(design, labels);
  const rails = railPieces(design, labels);
  if (groups.length === 0 && rails.length === 0) return [];
  const frame = frameFor(paper);
  const pages = [
    ...cutGuidePages(groups, rails, labels, frame, design.board.pitch_mm),
    ...templatePages(groups, rails, design, labels, frame),
  ];
  return pages.map((page, index) =>
    renderPage(page, index, pages.length, paper, footer),
  );
}
