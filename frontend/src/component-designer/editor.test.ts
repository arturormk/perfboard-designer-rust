import { describe, expect, it } from "vitest";
import {
  movePinOperation,
  reorderPinOperation,
  snapGridCoordinate,
  splitList,
} from "./editor";

describe("component authoring helpers", () => {
  it("snaps graphical pin movement to integer grid coordinates", () => {
    expect(snapGridCoordinate(2.49)).toBe(2);
    expect(snapGridCoordinate(-2.51)).toBe(-3);
  });

  it("produces the same typed pin operation used by keyboard forms", () => {
    expect(
      movePinOperation(
        { id: "1", number: "1", name: "VCC", x: 0, y: 0 },
        3.2,
        -1.7,
      ),
    ).toEqual({
      type: "upsert_pin",
      previous_id: "1",
      pin: { id: "1", number: "1", name: "VCC", x: 3, y: -2 },
    });
  });

  it("normalizes comma-separated metadata", () => {
    expect(splitList(" power, VCC, , supply ")).toEqual([
      "power",
      "VCC",
      "supply",
    ]);
  });

  it("calculates final pin indices when rows are reordered", () => {
    const pins = [
      { id: "a", number: "1", name: "A", x: 0, y: 0 },
      { id: "b", number: "2", name: "B", x: 0, y: 1 },
      { id: "c", number: "3", name: "C", x: 0, y: 2 },
    ];
    expect(reorderPinOperation(pins, "a", "c", true)).toEqual({
      type: "reorder_pin",
      id: "a",
      index: 2,
    });
    expect(reorderPinOperation(pins, "c", "a", false)).toEqual({
      type: "reorder_pin",
      id: "c",
      index: 0,
    });
    expect(reorderPinOperation(pins, "b", "c", false)).toBeUndefined();
  });
});
