/**
 * Wire bending templates: every wire shape drawn at 1:1 so a cut and stripped
 * wire can be laid on the paper and bent to match.
 *
 * Pure presentation geometry derived from stored wire vertices. Wires with the
 * same shape share one template, because a bent wire can be turned over or
 * rotated before it goes into the board.
 */
import type { Rail, Wire, WireColor } from "../api/project";
import type { Design, RoutePoint } from "../model/geometry";
import { WIRE_COLORS, wireColorName } from "../model/naming";

/**
 * Bare lead left at each end: board thickness (about 1.6 mm) plus a short tail
 * to solder. An estimate, labelled as such on the sheet.
 */
export const LEAD_MM = 5;

/** Drops repeated points and points in the middle of a straight run. */
export function simplifyPolyline(points: RoutePoint[]): RoutePoint[] {
  const unique = points.filter(
    (point, index) =>
      index === 0 ||
      point.x2 !== points[index - 1]!.x2 ||
      point.y2 !== points[index - 1]!.y2,
  );
  const result: RoutePoint[] = [];
  for (const point of unique) {
    while (result.length >= 2) {
      const a = result[result.length - 2]!;
      const b = result[result.length - 1]!;
      const cross =
        (b.x2 - a.x2) * (point.y2 - b.y2) - (b.y2 - a.y2) * (point.x2 - b.x2);
      const forward =
        (b.x2 - a.x2) * (point.x2 - b.x2) + (b.y2 - a.y2) * (point.y2 - b.y2);
      // Collinear and continuing the same way: the middle point is no bend.
      if (cross === 0 && forward > 0) result.pop();
      else break;
    }
    result.push(point);
  }
  return result;
}

const SYMMETRIES: Array<(x: number, y: number) => [number, number]> = [
  (x, y) => [x, y],
  (x, y) => [-x, y],
  (x, y) => [x, -y],
  (x, y) => [-x, -y],
  (x, y) => [y, x],
  (x, y) => [-y, x],
  (x, y) => [y, -x],
  (x, y) => [-y, -x],
];

/**
 * A key equal for wires that are the same physical shape: independent of
 * position, direction of travel, rotation by 90° steps and turning over.
 */
export function shapeKey(vertices: RoutePoint[]): string {
  const points = simplifyPolyline(vertices);
  let best: string | undefined;
  for (const order of [points, [...points].reverse()]) {
    for (const transform of SYMMETRIES) {
      const moved = order.map((point) => transform(point.x2, point.y2));
      const [x0, y0] = moved[0] ?? [0, 0];
      const key = moved.map(([x, y]) => `${x - x0},${y - y0}`).join(";");
      if (best === undefined || key < best) best = key;
    }
  }
  return best ?? "";
}

export interface TemplateSegment {
  /** Signed run in hole pitches (may be a half pitch). */
  dx: number;
  dy: number;
  lengthMm: number;
}

export function templateSegments(
  vertices: RoutePoint[],
  pitchMm: number,
): TemplateSegment[] {
  const points = simplifyPolyline(vertices);
  return points.slice(1).map((point, index) => {
    const dx = (point.x2 - points[index]!.x2) / 2;
    const dy = (point.y2 - points[index]!.y2) / 2;
    return { dx, dy, lengthMm: Math.hypot(dx, dy) * pitchMm };
  });
}

/** "5 pitches", or "3 × 3 pitches diagonal" for a 45° run. */
export function describeSpacing(segment: TemplateSegment): string {
  const ax = Math.abs(segment.dx);
  const ay = Math.abs(segment.dy);
  const count = (value: number) =>
    `${value} ${value === 1 ? "pitch" : "pitches"}`;
  if (ax === 0 || ay === 0) return count(ax || ay);
  if (ax === ay) return `${ax} × ${ay} pitches diagonal`;
  return `${ax} × ${ay} pitches`;
}

export interface WireShape {
  key: string;
  /** Wires sharing this shape, in label order. */
  wires: Wire[];
  /** The first wire's simplified path, mirrored if it runs on the solder side. */
  points: RoutePoint[];
  pathMm: number;
  cutMm: number;
  bends: number;
}

export interface ColorGroup {
  color: WireColor;
  name: string;
  shapes: WireShape[];
  wireCount: number;
  totalCutMm: number;
}

function colorRank(name: string): number {
  const index = (WIRE_COLORS as readonly string[]).indexOf(name.toLowerCase());
  return index < 0 ? WIRE_COLORS.length : index;
}

/**
 * Wires grouped by insulation colour (conventional colours first), then by
 * shape. Shapes keep the order of their first wire's label.
 */
export function groupWireTemplates(
  design: Design,
  labels: Map<string, string>,
): ColorGroup[] {
  const pitch = design.board.pitch_mm;
  const label = (wire: Wire) => labels.get(wire.id) ?? wire.id;
  const wires = Object.values(design.wires ?? {})
    .filter((wire) => simplifyPolyline(wire.vertices).length >= 2)
    .sort((a, b) =>
      label(a).localeCompare(label(b), undefined, { numeric: true }),
    );
  const groups = new Map<string, ColorGroup>();
  for (const wire of wires) {
    const name = wireColorName(wire.color);
    const colorKey = name.toLowerCase();
    let group = groups.get(colorKey);
    if (!group) {
      group = {
        color: wire.color,
        name,
        shapes: [],
        wireCount: 0,
        totalCutMm: 0,
      };
      groups.set(colorKey, group);
    }
    const key = shapeKey(wire.vertices);
    let shape = group.shapes.find((candidate) => candidate.key === key);
    if (!shape) {
      const simplified = simplifyPolyline(wire.vertices);
      const points =
        wire.side === "solder"
          ? simplified.map((point) => ({ x2: -point.x2, y2: point.y2 }))
          : simplified;
      const pathMm = templateSegments(simplified, pitch).reduce(
        (total, segment) => total + segment.lengthMm,
        0,
      );
      shape = {
        key,
        wires: [],
        points,
        pathMm,
        cutMm: pathMm + 2 * LEAD_MM,
        bends: simplified.length - 2,
      };
      group.shapes.push(shape);
    }
    shape.wires.push(wire);
    group.wireCount += 1;
    group.totalCutMm += shape.cutMm;
  }
  return [...groups.values()].sort(
    (a, b) =>
      colorRank(a.name) - colorRank(b.name) || a.name.localeCompare(b.name),
  );
}

interface Vec {
  x: number;
  y: number;
}

export interface DimensionMark {
  /** Dimension line, offset from the wire segment. */
  from: Vec;
  to: Vec;
  /** Witness lines from the wire to the dimension line. */
  witness: [Vec, Vec][];
  /** Length on the line nearest the wire, hole spacing on the one beyond. */
  length: string;
  spacing: string;
  /** Centres of the two text lines and their rotation (never upside down). */
  lengthAt: Vec;
  spacingAt: Vec;
  angle: number;
}

export interface TemplateLayout {
  width: number;
  height: number;
  /** Wire path in millimetres. */
  path: Vec[];
  /** Bare leads extending each end straight out, before they are bent down. */
  leads: [Vec, Vec][];
  /** Where each lead is bent 90° down into its hole. */
  bendHoles: Vec[];
  /** Bare windows along the path, one per rail connection point. */
  windows: Vec[][];
  /** Reference hole grid around the wire, at board pitch. */
  holes: Vec[];
  dimensions: DimensionMark[];
}

/** Dimension text height in millimetres (about 6.5 pt). */
export const TEXT_MM = 2.3;
const MARGIN_MM = 4;
const DIMENSION_OFFSET_MM = 4.5;

/** Rough ends of a centred text line, so the drawing can make room for it. */
function textEnds(at: Vec, text: string, angle: number): Vec[] {
  const half = text.length * TEXT_MM * 0.3;
  const radians = (angle * Math.PI) / 180;
  const dx = Math.cos(radians) * half;
  const dy = Math.sin(radians) * half;
  const rise = TEXT_MM * 0.6;
  return [
    { x: at.x - dx, y: at.y - dy - rise },
    { x: at.x + dx, y: at.y + dy + rise },
  ];
}

/**
 * Lays a shape out in millimetres, origin at the top-left of the drawing.
 *
 * With `rail`, the shape is a rail: each end has its own lead (bent into a
 * hole, or a short bare tip where the end lies between holes), every run
 * between two of its nodes is dimensioned, and each listed connection node
 * gets a bare window of `rail.windowMm` centred on it.
 */
export function layoutTemplate(
  points: RoutePoint[],
  pitchMm: number,
  leadMm = LEAD_MM,
  rail?: {
    connections: number[];
    windowMm: number;
    ends: [WireEnd, WireEnd];
  },
): TemplateLayout {
  const ends: [WireEnd, WireEnd] = rail?.ends ?? [
    { leadMm, intoHole: true },
    { leadMm, intoHole: true },
  ];
  const raw = points.map((point) => ({
    x: (point.x2 / 2) * pitchMm,
    y: (point.y2 / 2) * pitchMm,
  }));
  const unit = (a: Vec, b: Vec): Vec => {
    const length = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    return { x: (b.x - a.x) / length, y: (b.y - a.y) / length };
  };
  const first = raw[0]!;
  const last = raw[raw.length - 1]!;
  const startOut = unit(raw[1]!, first);
  const endOut = unit(raw[raw.length - 2]!, last);
  const startLead = ends[0].leadMm;
  const endLead = ends[1].leadMm;
  const rawLeads: [Vec, Vec][] = [
    [
      first,
      {
        x: first.x + startOut.x * startLead,
        y: first.y + startOut.y * startLead,
      },
    ],
    [last, { x: last.x + endOut.x * endLead, y: last.y + endOut.y * endLead }],
  ];

  const centroid = {
    x: raw.reduce((sum, point) => sum + point.x, 0) / raw.length,
    y: raw.reduce((sum, point) => sum + point.y, 0) / raw.length,
  };
  // A rail keeps its connection nodes, so each run between them gets its
  // own dimension; a wire is dimensioned bend to bend.
  const nodes = rail
    ? raw
    : simplifyPolyline(points).map((point) => ({
        x: (point.x2 / 2) * pitchMm,
        y: (point.y2 / 2) * pitchMm,
      }));
  const segments = rail
    ? points.slice(1).map((point, index) => {
        const dx = (point.x2 - points[index]!.x2) / 2;
        const dy = (point.y2 - points[index]!.y2) / 2;
        return { dx, dy, lengthMm: Math.hypot(dx, dy) * pitchMm };
      })
    : templateSegments(points, pitchMm);
  // Labels wider than their run would collide with the next run's labels:
  // consecutive short runs alternate between two dimension levels.
  let previousShortLevel = -1;
  const rawDimensions: DimensionMark[] = segments.flatMap((segment, index) => {
    // Half-pitch jogs are too short to label; the hole grid shows them.
    if (segment.lengthMm < pitchMm * 0.99) {
      previousShortLevel = -1;
      return [];
    }
    const labelMm = `${segment.lengthMm.toFixed(2)} mm`.length * TEXT_MM * 0.6;
    const short = labelMm > segment.lengthMm;
    const level = short && previousShortLevel === 0 ? 1 : 0;
    previousShortLevel = short ? level : -1;
    const offset = DIMENSION_OFFSET_MM + level * (TEXT_MM * 2.4 + 1);
    const a = nodes[index]!;
    const b = nodes[index + 1]!;
    const along = unit(a, b);
    let normal = { x: along.y, y: -along.x };
    const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const outward =
      (mid.x - centroid.x) * normal.x + (mid.y - centroid.y) * normal.y;
    // Point dimensions away from the shape; a straight wire gets them above.
    if (outward < -1e-9 || (Math.abs(outward) <= 1e-9 && normal.y > 0))
      normal = { x: -normal.x, y: -normal.y };
    const shift = (point: Vec, distance: number): Vec => ({
      x: point.x + normal.x * distance,
      y: point.y + normal.y * distance,
    });
    let angle = (Math.atan2(along.y, along.x) * 180) / Math.PI;
    if (angle > 90) angle -= 180;
    if (angle <= -90) angle += 180;
    return [
      {
        from: shift(a, offset),
        to: shift(b, offset),
        witness: [
          [shift(a, 1.2), shift(a, offset + 1)],
          [shift(b, 1.2), shift(b, offset + 1)],
        ],
        length: `${segment.lengthMm.toFixed(2)} mm`,
        spacing: describeSpacing(segment),
        lengthAt: shift(mid, offset + TEXT_MM * 0.75),
        spacingAt: shift(mid, offset + TEXT_MM * 1.85),
        angle,
      },
    ];
  });

  // Each window runs half its length either side of its node, following the
  // path round any corner and stopping at the path's ends (an end's bare tip
  // beyond the node is its lead).
  const along = raw.map((_, index) =>
    raw
      .slice(1, index + 1)
      .reduce(
        (total, point, at) =>
          total + Math.hypot(point.x - raw[at]!.x, point.y - raw[at]!.y),
        0,
      ),
  );
  const pathTotal = along[along.length - 1] ?? 0;
  const pointAt = (distance: number): Vec => {
    for (let index = 1; index < raw.length; index += 1) {
      if (distance <= along[index]! + 1e-9 || index === raw.length - 1) {
        const a = raw[index - 1]!;
        const b = raw[index]!;
        const run = along[index]! - along[index - 1]! || 1;
        const t = (distance - along[index - 1]!) / run;
        return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
      }
    }
    return raw[0]!;
  };
  const rawWindows: Vec[][] = mergeWindows(
    (rail?.connections ?? []).map((index) => ({
      fromMm: Math.max(0, along[index]! - rail!.windowMm / 2),
      toMm: Math.min(pathTotal, along[index]! + rail!.windowMm / 2),
    })),
  ).map(({ fromMm, toMm }) => [
    pointAt(fromMm),
    ...along
      .map((distance, index) => ({ distance, index }))
      .filter(({ distance }) => distance > fromMm && distance < toMm)
      .map(({ index }) => raw[index]!),
    pointAt(toMm),
  ]);

  // Fit the drawing: wire, leads and dimension lines, plus a margin.
  const extent = [
    ...raw,
    ...rawLeads.flat(),
    ...rawDimensions.flatMap((mark) => [
      mark.from,
      mark.to,
      ...textEnds(mark.lengthAt, mark.length, mark.angle),
      ...textEnds(mark.spacingAt, mark.spacing, mark.angle),
    ]),
  ];
  const minX = Math.min(...extent.map((point) => point.x)) - MARGIN_MM;
  const minY = Math.min(...extent.map((point) => point.y)) - MARGIN_MM;
  const maxX = Math.max(...extent.map((point) => point.x)) + MARGIN_MM;
  const maxY = Math.max(...extent.map((point) => point.y)) + MARGIN_MM;
  const move = (point: Vec): Vec => ({ x: point.x - minX, y: point.y - minY });

  // Holes sit on whole pitches of the board, aligned with the wire's endpoints'
  // grid even when the path uses half-pitch vertices.
  const holes: Vec[] = [];
  const gridX = (first.x / pitchMm) % 1;
  const gridY = (first.y / pitchMm) % 1;
  const span = (low: number, high: number, offset: number) => {
    const values: number[] = [];
    for (
      let step = Math.ceil((low + pitchMm * 0.3) / pitchMm - offset);
      (step + offset) * pitchMm <= high - pitchMm * 0.3;
      step += 1
    )
      values.push((step + offset) * pitchMm);
    return values;
  };
  const wireMinX = Math.min(...raw.map((point) => point.x)) - pitchMm * 1.3;
  const wireMaxX = Math.max(...raw.map((point) => point.x)) + pitchMm * 1.3;
  const wireMinY = Math.min(...raw.map((point) => point.y)) - pitchMm * 1.3;
  const wireMaxY = Math.max(...raw.map((point) => point.y)) + pitchMm * 1.3;
  for (const y of span(wireMinY, wireMaxY, gridY))
    for (const x of span(wireMinX, wireMaxX, gridX)) holes.push(move({ x, y }));

  return {
    width: maxX - minX,
    height: maxY - minY,
    path: raw.map(move),
    leads: rawLeads.map(([a, b]) => [move(a), move(b)] as [Vec, Vec]),
    bendHoles: [first, last]
      .filter((_, index) => ends[index]!.intoHole)
      .map(move),
    windows: rawWindows.map((window) => window.map(move)),
    holes,
    dimensions: rawDimensions.map((mark) => ({
      ...mark,
      from: move(mark.from),
      to: move(mark.to),
      witness: mark.witness.map(([a, b]) => [move(a), move(b)] as [Vec, Vec]),
      lengthAt: move(mark.lengthAt),
      spacingAt: move(mark.spacingAt),
    })),
  };
}

/** One column of the cut guide: a wire laid out straight, or part of one. */
export interface Bend {
  /** Distance along the wire's path from its start terminal. */
  alongMm: number;
  /** How far the wire turns: 45, 90 or 135 degrees. */
  angle: number;
}

/**
 * Where a wire bends, measured along its path from its start terminal. The
 * 90° bends of the leads into their holes are not included.
 */
export function bendPositions(vertices: RoutePoint[], pitchMm: number): Bend[] {
  const points = simplifyPolyline(vertices);
  const bends: Bend[] = [];
  let along = 0;
  for (let index = 1; index < points.length - 1; index += 1) {
    const a = points[index - 1]!;
    const b = points[index]!;
    const c = points[index + 1]!;
    along += (Math.hypot(b.x2 - a.x2, b.y2 - a.y2) / 2) * pitchMm;
    const turn = Math.abs(
      Math.atan2(c.y2 - b.y2, c.x2 - b.x2) -
        Math.atan2(b.y2 - a.y2, b.x2 - a.x2),
    );
    const degrees = Math.round(
      ((turn > Math.PI ? 2 * Math.PI - turn : turn) * 180) / Math.PI,
    );
    bends.push({ alongMm: along, angle: degrees });
  }
  return bends;
}

/** How one end of a wire or rail finishes. */
export interface WireEnd {
  /** Bare conductor beyond the end node. */
  leadMm: number;
  /** The lead is bent 90° down into the end node's hole. */
  intoHole: boolean;
}

/** A stretch along a straightened wire or rail, from its top cut. */
export interface Stretch {
  fromMm: number;
  toMm: number;
}

/** Joins overlapping or touching stretches, in order. */
export function mergeWindows(stretches: Stretch[]): Stretch[] {
  const merged: Stretch[] = [];
  for (const stretch of [...stretches].sort((a, b) => a.fromMm - b.fromMm)) {
    const last = merged[merged.length - 1];
    if (last && stretch.fromMm <= last.toMm + 1e-9)
      last.toMm = Math.max(last.toMm, stretch.toMm);
    else merged.push({ ...stretch });
  }
  return merged;
}

export interface CutColumn {
  id: string;
  color: WireColor;
  cutMm: number;
  pathMm: number;
  /** Bare lead at the top and bottom ends. */
  ends: [WireEnd, WireEnd];
  /** Bends, measured from the top cut (the start terminal's end). */
  bends: Bend[];
  /** Insulation to remove at a rail's connection points. */
  windows: Stretch[];
  /** Stretch of the straightened wire this column shows, from its top end. */
  fromMm: number;
  toMm: number;
  part: number;
  parts: number;
}

/**
 * Every wire straightened, in sheet order (colour, then label). A wire longer
 * than a column continues in the next one, split at a whole centimetre so the
 * page grid stays aligned with every column.
 */
export function cutColumns(
  groups: ColorGroup[],
  labels: Map<string, string>,
  columnMm: number,
  pitchMm: number,
  rails: RailPiece[] = [],
): CutColumn[] {
  const span = Math.max(10, Math.floor(columnMm / 10) * 10);
  const label = (wire: Wire) => labels.get(wire.id) ?? wire.id;
  const split = (
    base: Omit<CutColumn, "fromMm" | "toMm" | "part" | "parts">,
  ): CutColumn[] => {
    const parts = Math.max(1, Math.ceil(base.cutMm / span - 1e-9));
    return Array.from({ length: parts }, (_, part) => ({
      ...base,
      fromMm: part * span,
      toMm: Math.min(base.cutMm, (part + 1) * span),
      part: part + 1,
      parts,
    }));
  };
  const wireColumns = groups.flatMap((group) =>
    group.shapes
      .flatMap((shape) => shape.wires.map((wire) => ({ wire, shape })))
      .sort((a, b) =>
        label(a.wire).localeCompare(label(b.wire), undefined, {
          numeric: true,
        }),
      )
      .flatMap(({ wire, shape }) =>
        split({
          id: wire.id,
          color: wire.color,
          cutMm: shape.cutMm,
          pathMm: shape.pathMm,
          ends: [
            { leadMm: LEAD_MM, intoHole: true },
            { leadMm: LEAD_MM, intoHole: true },
          ],
          bends: bendPositions(wire.vertices, pitchMm).map((bend) => ({
            ...bend,
            alongMm: bend.alongMm + LEAD_MM,
          })),
          windows: [],
        }),
      ),
  );
  const railColumns = rails.flatMap((piece) =>
    split({
      id: piece.rail.id,
      color: piece.rail.color,
      cutMm: piece.cutMm,
      pathMm: piece.pathMm,
      ends: piece.ends,
      bends: piece.bends,
      windows: piece.windows,
    }),
  );
  return [...wireColumns, ...railColumns];
}

// ── Rails ───────────────────────────────────────────────────────────────────

/**
 * Bare insulation window at each rail connection point, centred on it. About
 * a pad and a half: room to solder without the next window running into it.
 */
export const RAIL_WINDOW_MM = 3;

export interface RailPiece {
  rail: Rail;
  /** Every node in path order, mirrored if the rail is on the solder side. */
  points: RoutePoint[];
  /** Indices into `points` where insulation is removed. */
  connections: number[];
  ends: [WireEnd, WireEnd];
  pathMm: number;
  /** Path plus both ends' leads. */
  cutMm: number;
  /** Measured from the top cut, like the windows. */
  bends: Bend[];
  windows: Stretch[];
}

const onHole = (point: RoutePoint) => point.x2 % 2 === 0 && point.y2 % 2 === 0;

/**
 * A rail end on a hole goes into it on a lead like a wire's; where the
 * rail is used only at its taps, the builder can clip it while installing.
 * An end between holes cannot, so it gets a bare tip that centres a window
 * on the end node instead.
 */
export function railEnds(rail: Rail): [WireEnd, WireEnd] {
  const end = (point: RoutePoint): WireEnd =>
    onHole(point)
      ? { leadMm: LEAD_MM, intoHole: true }
      : { leadMm: RAIL_WINDOW_MM / 2, intoHole: false };
  return [
    end(rail.vertices[0]!),
    end(rail.vertices[rail.vertices.length - 1]!),
  ];
}

/**
 * Where a rail needs a window in its insulation: every node in a straight
 * run (a node there exists only to be connected to), any corner a wire is
 * attached to, and an end between holes. An end on a hole is bare already:
 * it is the lead. Other corners are bends only.
 */
export function railConnections(design: Design, rail: Rail): number[] {
  const points = rail.vertices;
  const wired = (point: RoutePoint) =>
    Object.values(design.wires ?? {}).some((wire) =>
      [wire.start, wire.end].some(
        (terminal) =>
          terminal.kind === "rail_node" &&
          terminal.rail_id === rail.id &&
          terminal.position.x2 === point.x2 &&
          terminal.position.y2 === point.y2,
      ),
    );
  return points.flatMap((point, index) => {
    const before = points[index - 1];
    const after = points[index + 1];
    if (!before || !after) return onHole(point) ? [] : [index];
    const cross =
      (point.x2 - before.x2) * (after.y2 - point.y2) -
      (point.y2 - before.y2) * (after.x2 - point.x2);
    const forward =
      (point.x2 - before.x2) * (after.x2 - point.x2) +
      (point.y2 - before.y2) * (after.y2 - point.y2);
    const straight = cross === 0 && forward > 0;
    return straight || wired(point) ? [index] : [];
  });
}

/** Every rail as a piece to cut, bend and strip, in label order. */
export function railPieces(
  design: Design,
  labels: Map<string, string>,
): RailPiece[] {
  const pitch = design.board.pitch_mm;
  const label = (rail: Rail) => labels.get(rail.id) ?? rail.id;
  return Object.values(design.rails ?? {})
    .filter((rail) => simplifyPolyline(rail.vertices).length >= 2)
    .sort((a, b) =>
      label(a).localeCompare(label(b), undefined, { numeric: true }),
    )
    .map((rail) => {
      const vertices = rail.vertices;
      const along = vertices.map((_, index) =>
        vertices
          .slice(1, index + 1)
          .reduce(
            (total, point, at) =>
              total +
              (Math.hypot(
                point.x2 - vertices[at]!.x2,
                point.y2 - vertices[at]!.y2,
              ) /
                2) *
                pitch,
            0,
          ),
      );
      const pathMm = along[along.length - 1]!;
      const ends = railEnds(rail);
      const overhang = ends[0].leadMm;
      const cutMm = pathMm + ends[0].leadMm + ends[1].leadMm;
      const connections = railConnections(design, rail);
      return {
        rail,
        points:
          rail.side === "solder"
            ? vertices.map((point) => ({ x2: -point.x2, y2: point.y2 }))
            : vertices.map((point) => ({ ...point })),
        connections,
        ends,
        pathMm,
        cutMm,
        bends: bendPositions(vertices, pitch).map((bend) => ({
          ...bend,
          alongMm: bend.alongMm + overhang,
        })),
        windows: mergeWindows(
          connections.map((index) => ({
            fromMm: Math.max(0, along[index]! + overhang - RAIL_WINDOW_MM / 2),
            toMm: Math.min(
              cutMm,
              along[index]! + overhang + RAIL_WINDOW_MM / 2,
            ),
          })),
        ),
      };
    });
}
