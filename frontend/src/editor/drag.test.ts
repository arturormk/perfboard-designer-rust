import { describe, expect, it } from "vitest";
import { snappedDragPosition } from "./drag";

describe("snapped component dragging", () => {
  it("preserves the point grabbed within the component", () => {
    expect(
      snappedDragPosition(
        { x: 5, y: 4 },
        { x: 6.75, y: 5.25 },
        { x: 9.2, y: 8.1 },
      ),
    ).toEqual({ x: 7, y: 7 });
  });

  it("snaps to holes and does not move before crossing half a pitch", () => {
    expect(
      snappedDragPosition(
        { x: 5, y: 4 },
        { x: 5.2, y: 4.2 },
        { x: 5.6, y: 4.6 },
      ),
    ).toEqual({ x: 5, y: 4 });
  });

  it("keeps the component origin at non-negative board coordinates", () => {
    expect(
      snappedDragPosition({ x: 1, y: 1 }, { x: 2, y: 2 }, { x: -5, y: -4 }),
    ).toEqual({ x: 0, y: 0 });
  });
});
