export interface BoardDimensions {
  holesX: number;
  holesY: number;
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
