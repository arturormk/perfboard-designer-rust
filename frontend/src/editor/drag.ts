import type { HolePos } from "../api/project";

/**
 * Where a dragged component lands. Positions are not clamped: a part may be
 * parked off the board on any side, with negative coordinates to the left or
 * above it.
 */
export function snappedDragPosition(
  initialPosition: HolePos,
  pointerStart: HolePos,
  pointerNow: HolePos,
): HolePos {
  return {
    x: Math.round(initialPosition.x + pointerNow.x - pointerStart.x),
    y: Math.round(initialPosition.y + pointerNow.y - pointerStart.y),
  };
}
