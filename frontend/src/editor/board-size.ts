export interface BoardDimensions {
  holesX: number;
  holesY: number;
}

export type ResizeHandle =
  | "top"
  | "right"
  | "bottom"
  | "left"
  | "top-left"
  | "top-right"
  | "bottom-right"
  | "bottom-left";

export interface BoardResizePlan extends BoardDimensions {
  offsetX: number;
  offsetY: number;
}

export function fullyEnclosedHoleDimensions(
  dimensions: BoardDimensions,
): BoardDimensions {
  return {
    holesX: Math.max(0, dimensions.holesX - 2),
    holesY: Math.max(0, dimensions.holesY - 2),
  };
}

export function planBoardResize(
  original: BoardDimensions,
  handle: ResizeHandle,
  deltaX: number,
  deltaY: number,
  mirrored = false,
): BoardResizePlan | undefined {
  const movesLeft = handle.includes("left");
  const movesRight = handle.includes("right");
  const movesTop = handle.includes("top");
  const movesBottom = handle.includes("bottom");
  const holesX =
    original.holesX + (movesRight ? deltaX : 0) - (movesLeft ? deltaX : 0);
  const holesY =
    original.holesY + (movesBottom ? deltaY : 0) - (movesTop ? deltaY : 0);
  if (holesX < 1 || holesY < 1) return undefined;
  return {
    holesX,
    holesY,
    offsetX: mirrored ? (movesRight ? deltaX : 0) : movesLeft ? -deltaX : 0,
    offsetY: movesTop ? -deltaY : 0,
  };
}

export function parseBoardDimensions(
  holesXValue: string,
  holesYValue: string,
): BoardDimensions | undefined {
  const holesX = Number(holesXValue);
  const holesY = Number(holesYValue);
  if (
    !Number.isInteger(holesX) ||
    !Number.isInteger(holesY) ||
    holesX < 1 ||
    holesY < 1 ||
    holesX > 0xffff_ffff ||
    holesY > 0xffff_ffff
  )
    return undefined;
  return { holesX, holesY };
}
