import type { ComponentDraftOperation, PinDefinition } from "../api/project";

export function snapGridCoordinate(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.round(value);
}

export function movePinOperation(
  pin: PinDefinition,
  x: number,
  y: number,
): ComponentDraftOperation {
  return {
    type: "upsert_pin",
    previous_id: pin.id,
    pin: {
      ...pin,
      x: snapGridCoordinate(x),
      y: snapGridCoordinate(y),
    },
  };
}

export function splitList(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}
