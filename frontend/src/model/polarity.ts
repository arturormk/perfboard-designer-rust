/**
 * Polarity marks on a part's body, read from its pin IDs: a diode (pins
 * `cathode` and `anode`) gets a cathode band, an electrolytic capacitor (pins
 * `positive` and `negative`) a plus sign by its positive lead. Mirrors
 * `polarity_mark` in `crates/project/src/rendering.rs`.
 */

interface MarkPin {
  id: string;
  x: number;
  y: number;
}

export interface BodyRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type PolarityMark =
  | { kind: "band"; x: number; y: number; width: number; height: number }
  /** A plus sign centred on (`x`, `y`), each arm `arm` long. */
  | { kind: "plus"; x: number; y: number; arm: number };

/**
 * The polarity mark for a part, in part-local holes, or `undefined` when the
 * part has none. `body` is the body rectangle in holes.
 */
export function polarityMark(
  pins: readonly MarkPin[],
  body: BodyRect,
): PolarityMark | undefined {
  const pin = (id: string) => pins.find((candidate) => candidate.id === id);
  const cathode = pin("cathode");
  const anode = pin("anode");
  if (cathode && anode) return cathodeBand(cathode, anode, body);
  const positive = pin("positive");
  const negative = pin("negative");
  if (positive && negative) return plusSign(positive, negative, body);
  return undefined;
}

/** Leads side by side along x (true), along y (false), or neither. */
function axisOf(a: MarkPin, b: MarkPin): boolean | undefined {
  if (a.y === b.y && a.x !== b.x) return true;
  if (a.x === b.x && a.y !== b.y) return false;
  return undefined;
}

function cathodeBand(
  cathode: MarkPin,
  anode: MarkPin,
  body: BodyRect,
): PolarityMark | undefined {
  const alongX = axisOf(cathode, anode);
  if (alongX === undefined) return undefined;
  const start = alongX ? body.x : body.y;
  const length = alongX ? body.width : body.height;
  const towardStart = alongX ? cathode.x < anode.x : cathode.y < anode.y;
  const width = length * 0.16;
  const inset = length * 0.1;
  const from = towardStart ? start + inset : start + length - inset - width;
  return alongX
    ? { kind: "band", x: from, y: body.y, width, height: body.height }
    : { kind: "band", x: body.x, y: from, width: body.width, height: width };
}

function plusSign(
  positive: MarkPin,
  negative: MarkPin,
  body: BodyRect,
): PolarityMark | undefined {
  const alongX = axisOf(positive, negative);
  if (alongX === undefined) return undefined;
  // Beside the positive lead, toward the body's top (or left) edge, clear of
  // the pad.
  const halfThickness = (alongX ? body.height : body.width) / 2;
  const centre = alongX ? body.y + halfThickness : body.x + halfThickness;
  const across = centre - halfThickness * 0.6;
  const arm = Math.min(0.2, halfThickness * 0.22);
  return alongX
    ? { kind: "plus", x: positive.x, y: across, arm }
    : { kind: "plus", x: across, y: positive.y, arm };
}

/** SVG path data for a plus sign mark. */
export function plusPath(mark: { x: number; y: number; arm: number }): string {
  const { x, y, arm } = mark;
  return `M ${x - arm} ${y} H ${x + arm} M ${x} ${y - arm} V ${y + arm}`;
}
