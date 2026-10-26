import { describe, expect, it } from "vitest";
import { parseBoardDimensions } from "./board-size";

describe("board dimensions", () => {
  it("parses positive whole-number dimensions", () => {
    expect(parseBoardDimensions("30", "20")).toEqual({
      holesX: 30,
      holesY: 20,
    });
  });

  it.each([
    ["", "18"],
    ["0", "18"],
    ["25", "-1"],
    ["2.5", "18"],
    ["25", "not-a-number"],
    ["4294967296", "18"],
  ])("rejects invalid dimensions %s by %s", (holesX, holesY) => {
    expect(parseBoardDimensions(holesX, holesY)).toBeUndefined();
  });
});
