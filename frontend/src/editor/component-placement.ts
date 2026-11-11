import type { HolePos } from "../api/project";

export interface HoleGridSize {
  holes_x: number;
  holes_y: number;
}

/** Stages a new part just outside the board, ready to drag into place. */
export function newComponentStagingPosition(board: HoleGridSize): HolePos {
  return {
    x: board.holes_x + 1,
    y: Math.floor(board.holes_y / 2),
  };
}
