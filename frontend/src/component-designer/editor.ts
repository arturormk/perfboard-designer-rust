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

export function reorderPinOperation(
  pins: PinDefinition[],
  pinId: string,
  targetId: string,
  afterTarget: boolean,
): ComponentDraftOperation | undefined {
  const currentIndex = pins.findIndex((pin) => pin.id === pinId);
  if (currentIndex < 0 || pinId === targetId) return undefined;
  const remaining = pins.filter((pin) => pin.id !== pinId);
  const targetIndex = remaining.findIndex((pin) => pin.id === targetId);
  if (targetIndex < 0) return undefined;
  const index = targetIndex + (afterTarget ? 1 : 0);
  if (index === currentIndex) return undefined;
  return { type: "reorder_pin", id: pinId, index };
}

export function splitList(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}
