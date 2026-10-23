import type { HolePos } from "../api/project";

export function snappedDragPosition(
  initialPosition: HolePos,
  pointerStart: HolePos,
  pointerNow: HolePos,
): HolePos {
  return {
    x: Math.max(
      0,
      Math.round(initialPosition.x + pointerNow.x - pointerStart.x),
    ),
    y: Math.max(
      0,
      Math.round(initialPosition.y + pointerNow.y - pointerStart.y),
    ),
  };
}
