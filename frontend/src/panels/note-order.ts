/** Pure helpers behind the order of review notes. */

const RANK: Record<string, number> = {
  critical: 0,
  warning: 1,
  suggestion: 2,
  info: 3,
};

/**
 * Adds `added` the way the backend does: each in front of the first item
 * that is less severe, or at the end. A severity-ordered list stays ordered
 * and a reordered one is not re-sorted.
 */
export function insertBySeverity<T extends { severity: string }>(
  items: readonly T[],
  added: readonly T[],
): T[] {
  const result = [...items];
  for (const item of added) {
    const rank = RANK[item.severity] ?? 3;
    const at = result.findIndex((other) => (RANK[other.severity] ?? 3) > rank);
    result.splice(at < 0 ? result.length : at, 0, item);
  }
  return result;
}

/**
 * The insertion slot, from 0 (before the first item) to `rects.length`
 * (after the last), for a pointer at `y`: the number of items whose middle
 * lies above it. `rects` are the items' current boxes, in list order.
 */
export function dropSlot(
  rects: ReadonlyArray<{ top: number; height: number }>,
  y: number,
): number {
  return rects.filter((rect) => rect.top + rect.height / 2 < y).length;
}

/**
 * What to send to `move_review_note` for dropping `id` into `slot`: the note
 * to move in front of, `null` for the end, or `undefined` when the drop
 * leaves the order unchanged.
 */
export function moveTarget(
  ids: readonly string[],
  id: string,
  slot: number,
): string | null | undefined {
  const from = ids.indexOf(id);
  if (from < 0 || slot === from || slot === from + 1) return undefined;
  return ids[slot] ?? null;
}

/** The order `moveTarget` produces, for showing a move before it lands. */
export function reordered<T>(
  items: readonly T[],
  key: (item: T) => string,
  id: string,
  before: string | null,
): T[] {
  const moving = items.find((item) => key(item) === id);
  if (!moving) return [...items];
  const rest = items.filter((item) => key(item) !== id);
  const at =
    before === null
      ? rest.length
      : rest.findIndex((item) => key(item) === before);
  if (at < 0) return [...items];
  rest.splice(at, 0, moving);
  return rest;
}
