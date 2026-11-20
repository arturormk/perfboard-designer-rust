/**
 * Leads drawn from an axial part's body out to its pins, for parts such as
 * diodes whose body is shorter than the lead span: without them the pins look
 * unattached. Mirrors `lead_segments` in `crates/project/src/rendering.rs`.
 */

import type { BodyRect } from "./polarity";

interface LeadPin {
  x: number;
  y: number;
}

export interface LeadSegment {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/**
 * For a two-pin part with its pins on one axis, a segment from each body end
 * to the rim of the pad (`padRadius` around the pin) beyond it, in part-local
 * holes. A pin whose pad reaches the body gets no segment; a part with any
 * other pin layout gets none.
 */
export function leadSegments(
  pins: readonly LeadPin[],
  body: BodyRect,
  padRadius: number,
): LeadSegment[] {
  if (pins.length !== 2) return [];
  const [a, b] = pins as [LeadPin, LeadPin];
  const alongX = a.y === b.y && a.x !== b.x;
  const alongY = a.x === b.x && a.y !== b.y;
  if (!alongX && !alongY) return [];
  const start = alongX ? body.x : body.y;
  const end = start + (alongX ? body.width : body.height);
  const segments: LeadSegment[] = [];
  for (const pin of pins) {
    const at = alongX ? pin.x : pin.y;
    const across = alongX ? pin.y : pin.x;
    let from: number;
    let to: number;
    if (at + padRadius < start) {
      from = start;
      to = at + padRadius;
    } else if (at - padRadius > end) {
      from = end;
      to = at - padRadius;
    } else {
      continue;
    }
    segments.push(
      alongX
        ? { x1: from, y1: across, x2: to, y2: across }
        : { x1: across, y1: from, x2: across, y2: to },
    );
  }
  return segments;
}
