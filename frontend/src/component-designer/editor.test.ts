import { describe, expect, it } from "vitest";
import { movePinOperation, snapGridCoordinate, splitList } from "./editor";

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
});
