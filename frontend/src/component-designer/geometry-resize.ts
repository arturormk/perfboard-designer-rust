import type { ComponentDefinition, LocalRect } from "../api/project";
import type { ResizeHandle } from "../editor/board-size";

export const COMPONENT_PITCH_MM = 2.54;

export interface GridRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

const rounded = (value: number): number => Number(value.toFixed(6));
const halfHole = (value: number): number => Math.round(value * 2) / 2;
const betweenHoles = (value: number): number => Math.round(value - 0.5) + 0.5;

function resizedGridRect(
  original: GridRect,
  handle: ResizeHandle,
  pointerX: number,
  pointerY: number,
  snap: (value: number) => number,
  minimumSize: number,
): GridRect | undefined {
  let left = original.x;
  let top = original.y;
  let right = original.x + original.width;
  let bottom = original.y + original.height;
  if (handle.includes("left")) left = snap(pointerX);
  if (handle.includes("right")) right = snap(pointerX);
  if (handle.includes("top")) top = snap(pointerY);
  if (handle.includes("bottom")) bottom = snap(pointerY);
  if (right - left < minimumSize || bottom - top < minimumSize)
    return undefined;
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export function bodyGridRect(body: ComponentDefinition["body"]): GridRect {
  return {
    x: body.x / COMPONENT_PITCH_MM,
    y: body.y / COMPONENT_PITCH_MM,
    width: body.width / COMPONENT_PITCH_MM,
    height: body.height / COMPONENT_PITCH_MM,
  };
}

export function planBodyResize(
  body: ComponentDefinition["body"],
  handle: ResizeHandle,
  pointerX: number,
  pointerY: number,
): ComponentDefinition["body"] | undefined {
  const resized = resizedGridRect(
    bodyGridRect(body),
    handle,
    pointerX,
    pointerY,
    halfHole,
    0.5,
  );
  if (!resized) return undefined;
  return {
    x: rounded(resized.x * COMPONENT_PITCH_MM),
    y: rounded(resized.y * COMPONENT_PITCH_MM),
    width: rounded(resized.width * COMPONENT_PITCH_MM),
    height: rounded(resized.height * COMPONENT_PITCH_MM),
  };
}

export function keepoutGridRect(keepout: LocalRect): GridRect {
  return {
    x: keepout.min.x - 0.5,
    y: keepout.min.y - 0.5,
    width: keepout.max.x - keepout.min.x + 1,
    height: keepout.max.y - keepout.min.y + 1,
  };
}

export function planKeepoutResize(
  keepout: LocalRect,
  handle: ResizeHandle,
  pointerX: number,
  pointerY: number,
): LocalRect | undefined {
  return planHoleRegionResize(keepout, handle, pointerX, pointerY);
}

export function planHoleRegionResize<T extends LocalRect>(
  region: T,
  handle: ResizeHandle,
  pointerX: number,
  pointerY: number,
): T | undefined {
  const resized = resizedGridRect(
    keepoutGridRect(region),
    handle,
    pointerX,
    pointerY,
    betweenHoles,
    1,
  );
  if (!resized) return undefined;
  return {
    ...region,
    min: {
      x: Math.round(resized.x + 0.5),
      y: Math.round(resized.y + 0.5),
    },
    max: {
      x: Math.round(resized.x + resized.width - 0.5),
      y: Math.round(resized.y + resized.height - 0.5),
    },
  } as T;
}
