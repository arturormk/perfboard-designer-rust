/**
 * Unrouted-connection hints ("airwires").
 *
 * Rust decides which pins are already electrically joined (see
 * `get_net_connectivity`); this module only chooses which pin pairs to draw so
 * the islands of each net appear connected by the shortest set of hints.
 */
import type { NetConnectivity, PinRef } from "../api/project";
import { pinPosition, type Design, type Point } from "./geometry";

export interface Airwire {
  netId: string;
  from: PinRef;
  to: PinRef;
  a: Point;
  b: Point;
}

interface Located {
  ref: PinRef;
  point: Point;
}

function distance(a: Point, b: Point): number {
  return Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
}

export function computeAirwires(
  design: Design,
  connectivity: NetConnectivity[],
): Airwire[] {
  const result: Airwire[] = [];
  for (const net of connectivity) {
    const islands = net.islands
      .map((island) =>
        island
          .map((ref) => ({ ref, point: pinPosition(design, ref) }))
          .filter((item): item is Located => Boolean(item.point)),
      )
      .filter((island) => island.length > 0);
    if (islands.length < 2) continue;
    // Prim's algorithm over islands; edge weight is the closest pin pair.
    const joined = new Set([0]);
    while (joined.size < islands.length) {
      let best:
        { island: number; a: Located; b: Located; d: number } | undefined;
      for (const from of joined) {
        for (let to = 0; to < islands.length; to += 1) {
          if (joined.has(to)) continue;
          for (const a of islands[from]!) {
            for (const b of islands[to]!) {
              const d = distance(a.point, b.point);
              if (!best || d < best.d) best = { island: to, a, b, d };
            }
          }
        }
      }
      if (!best) break;
      joined.add(best.island);
      result.push({
        netId: net.net_id,
        from: best.a.ref,
        to: best.b.ref,
        a: best.a.point,
        b: best.b.point,
      });
    }
  }
  return result;
}

/** Ids of top-level objects that differ between two design snapshots. */
export function changedObjectIds(
  before: Design | undefined,
  after: Design | undefined,
): Set<string> {
  const changed = new Set<string>();
  const collections = ["components", "wires", "rails", "nets"] as const;
  for (const key of collections) {
    const left = (before?.[key] ?? {}) as Record<string, unknown>;
    const right = (after?.[key] ?? {}) as Record<string, unknown>;
    for (const id of new Set([...Object.keys(left), ...Object.keys(right)])) {
      if (JSON.stringify(left[id]) !== JSON.stringify(right[id]))
        changed.add(id);
    }
  }
  return changed;
}
