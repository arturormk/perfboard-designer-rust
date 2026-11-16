/**
 * Pure helpers for the breadboard view (ADR 0018): the board's holes and
 * markings, the parts as a design the board renderer can draw, and jumper
 * shapes. The layout itself comes from Rust and is never changed here.
 */
import type {
  BreadboardGeometry,
  BreadboardJumper,
  BreadboardLayout,
  BreadboardPart,
  HolePos,
  RailPolarity,
} from "../api/project";
import type { Design } from "../model/geometry";

export interface BoardMarkings {
  /** Left edge of each board, in holes. */
  origins: number[];
  /** Every hole: strips and rails. */
  holes: HolePos[];
  /** Polarity stripes beside the rails. */
  stripes: Array<{ x: number; y1: number; y2: number; polarity: RailPolarity }>;
  /** Row numbers beside the strips. */
  rowNumbers: Array<{ x: number; y: number; text: string }>;
  /** Column letters above and below the strips. */
  columnLetters: Array<{ x: number; y: number; text: string }>;
}

const LETTERS = "abcdefghij";

/** Holes, stripes and labels of every board in the layout. */
export function boardMarkings(
  geometry: BreadboardGeometry,
  boardCount: number,
): BoardMarkings {
  const origins = Array.from(
    { length: boardCount },
    (_, board) => board * geometry.board_stride,
  );
  const holes: HolePos[] = [];
  const stripes: BoardMarkings["stripes"] = [];
  const rowNumbers: BoardMarkings["rowNumbers"] = [];
  const columnLetters: BoardMarkings["columnLetters"] = [];
  const strips = [...geometry.left_columns, ...geometry.right_columns];
  const lastRow = geometry.rows - 1;
  for (const x0 of origins) {
    for (let y = 0; y < geometry.rows; y += 1)
      for (const column of strips) holes.push({ x: x0 + column, y });
    for (const rail of geometry.rail_columns) {
      for (const y of geometry.rail_rows)
        holes.push({ x: x0 + rail.column, y });
      // Each pair reads red +, blue − from left to right on both sides: the
      // + stripe runs left of its rail, the − stripe right of it.
      const offset = rail.polarity === "plus" ? -1 : 1;
      stripes.push({
        x: x0 + rail.column + offset * 0.55,
        y1: geometry.rail_first_row - 0.6,
        y2: geometry.rail_last_row + 0.6,
        polarity: rail.polarity,
      });
    }
    const left = geometry.left_columns;
    const right = geometry.right_columns;
    for (let y = 0; y < geometry.rows; y += 1) {
      const row = y + 1;
      if (row !== 1 && row % 5 !== 0) continue;
      rowNumbers.push({ x: x0 + left[0]! - 1, y, text: String(row) });
      rowNumbers.push({
        x: x0 + right[right.length - 1]! + 1,
        y,
        text: String(row),
      });
    }
    strips.forEach((column, index) => {
      const text = LETTERS[index] ?? "";
      columnLetters.push({ x: x0 + column, y: -1.1, text });
      columnLetters.push({ x: x0 + column, y: lastRow + 1.1, text });
    });
  }
  return { origins, holes, stripes, rowNumbers, columnLetters };
}

/**
 * The parts at their breadboard positions, as a design the board renderer
 * draws: same definitions and nets, no perfboard wiring.
 */
export function breadboardDesign(
  design: Design,
  layout: BreadboardLayout,
): Design {
  const components: NonNullable<Design["components"]> = {};
  for (const part of layout.parts) {
    const instance = design.components?.[part.component_id];
    if (!instance) continue;
    components[part.component_id] = {
      ...instance,
      position: part.position,
      rotation: part.rotation,
    };
  }
  return {
    ...design,
    board: {
      ...design.board,
      holes_x: Math.max(1, layout.geometry.width),
      holes_y: Math.max(1, layout.geometry.height),
      margins_mm: undefined,
      mounting_holes: undefined,
      forbidden_areas: undefined,
    },
    components,
    wires: {},
    rails: {},
    saddles: {},
    crossing_choices: {},
    constraints: {},
  };
}

/**
 * A jumper as a gentle arc, the way a Dupont wire lies on a board, and the
 * arc's midpoint for its number. Arcs bow to alternate sides by number, so
 * jumpers between nearby holes separate.
 */
export function jumperGeometry(jumper: BreadboardJumper): {
  d: string;
  mid: { x: number; y: number };
} {
  const { from, to } = jumper;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.hypot(dx, dy);
  if (length === 0) return { d: `M ${from.x} ${from.y}`, mid: { ...from } };
  const bow =
    Math.min(2.5, 0.35 + length * 0.12) * (jumper.number % 2 ? 1 : -1);
  const cx = round((from.x + to.x) / 2 + (-dy / length) * bow);
  const cy = round((from.y + to.y) / 2 + (dx / length) * bow);
  return {
    d: `M ${from.x} ${from.y} Q ${cx} ${cy} ${to.x} ${to.y}`,
    // A quadratic Bézier's midpoint: ¼ P0 + ½ C + ¼ P2.
    mid: {
      x: round((from.x + to.x) / 4 + cx / 2),
      y: round((from.y + to.y) / 4 + cy / 2),
    },
  };
}

function round(value: number): number {
  return Math.round(value * 1000) / 1000;
}

/** View box that shows every board and the off-board parts, with margin. */
export function layoutBox(
  layout: BreadboardLayout,
): [number, number, number, number] {
  // Extra room on top for the board names and the off-board caption.
  const pad = 2.5;
  const top = 4;
  return [
    -pad,
    -top,
    layout.geometry.width - 1 + pad * 2,
    layout.geometry.height - 1 + pad + top,
  ];
}

export interface NetGroup {
  netId: string;
  name: string;
  color: BreadboardJumper["color"];
  jumpers: BreadboardJumper[];
}

/** Jumpers grouped by net, in layout order (by net name). */
export function jumpersByNet(layout: BreadboardLayout): NetGroup[] {
  const groups = new Map<string, NetGroup>();
  for (const jumper of layout.jumpers) {
    let group = groups.get(jumper.net_id);
    if (!group) {
      group = {
        netId: jumper.net_id,
        name: jumper.net_name,
        color: jumper.color,
        jumpers: [],
      };
      groups.set(jumper.net_id, group);
    }
    group.jumpers.push(jumper);
  }
  return [...groups.values()];
}

/**
 * Jumpers that join two rails of a net. Each rail is copper of its own, so
 * these are what tie a net's rails together, and worth pointing out.
 */
export function railLinks(layout: BreadboardLayout): BreadboardJumper[] {
  const g = layout.geometry;
  const onRail = (hole: HolePos) =>
    g.rail_columns.some((rail) => rail.column === hole.x % g.board_stride);
  return layout.jumpers.filter(
    (jumper) =>
      jumper.kind === "jumper" && onRail(jumper.from) && onRail(jumper.to),
  );
}

/**
 * Breadboard notation for a hole, as the layout's jumper labels use it: `e12`,
 * `left − rail, row 7`; prefixed with the board (`B2 e12`) when there are
 * several.
 */
export function holeLabel(layout: BreadboardLayout, hole: HolePos): string {
  const g = layout.geometry;
  const board = Math.floor(hole.x / g.board_stride);
  const column = hole.x % g.board_stride;
  const prefix = layout.board_count > 1 ? `B${board + 1} ` : "";
  const row = hole.y + 1;
  const strip = [...g.left_columns, ...g.right_columns].indexOf(column);
  if (strip >= 0 && hole.x < layout.board_count * g.board_stride)
    return `${prefix}${LETTERS[strip]}${row}`;
  const rail = g.rail_columns.find((candidate) => candidate.column === column);
  if (rail && hole.x < layout.board_count * g.board_stride) {
    const side = column < g.board_columns / 2 ? "left" : "right";
    const sign = rail.polarity === "plus" ? "+" : "−";
    return `${prefix}${side} ${sign} rail, row ${row}`;
  }
  return "beside the boards";
}

/** Where a part sits, from its placement and its pins' holes. */
export function partWhereabouts(
  layout: BreadboardLayout,
  part: BreadboardPart,
  pinHoles: HolePos[],
): string {
  if (part.placement === "off_board")
    return "Beside the boards · wire it in with male–female Dupont leads";
  const rows = pinHoles.map((hole) => hole.y + 1);
  const first = Math.min(...rows);
  const last = Math.max(...rows);
  const span = first === last ? `row ${first}` : `rows ${first}–${last}`;
  const board =
    layout.board_count > 1 && part.board !== undefined
      ? `Board ${part.board + 1} · `
      : "";
  const where =
    part.placement === "straddle"
      ? "Straddles the channel"
      : part.placement === "left"
        ? "Columns a–e"
        : "Columns f–j";
  return `${board}${where} · ${span}`;
}
