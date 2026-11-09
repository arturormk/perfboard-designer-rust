import { describe, expect, it } from "vitest";
import {
  fullyEnclosedHoleDimensions,
  parseBoardDimensions,
  planBoardResize,
} from "./board-size";

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

describe("board resize planning", () => {
  it("does not count the half-holes on the resize boundary", () => {
    expect(fullyEnclosedHoleDimensions({ holesX: 25, holesY: 18 })).toEqual({
      holesX: 23,
      holesY: 16,
    });
    expect(fullyEnclosedHoleDimensions({ holesX: 1, holesY: 2 })).toEqual({
      holesX: 0,
      holesY: 0,
    });
  });

  it("grows from the right without moving existing contents", () => {
    expect(planBoardResize({ holesX: 25, holesY: 18 }, "right", 3, 0)).toEqual({
      holesX: 28,
      holesY: 18,
      offsetX: 0,
      offsetY: 0,
    });
  });

  it("grows from the top-left and translates existing contents", () => {
    expect(
      planBoardResize({ holesX: 25, holesY: 18 }, "top-left", -2, -1),
    ).toEqual({ holesX: 27, holesY: 19, offsetX: 2, offsetY: 1 });
  });

  it("maps visual edges correctly in the mirrored solder view", () => {
    expect(
      planBoardResize({ holesX: 25, holesY: 18 }, "bottom-right", 2, 3, true),
    ).toEqual({ holesX: 27, holesY: 21, offsetX: 2, offsetY: 0 });
    expect(
      planBoardResize({ holesX: 25, holesY: 18 }, "left", -2, 0, true),
    ).toEqual({ holesX: 27, holesY: 18, offsetX: 0, offsetY: 0 });
  });

  it("rejects a drag that would remove every hole on an axis", () => {
    expect(
      planBoardResize({ holesX: 3, holesY: 2 }, "top-right", -3, 2),
    ).toBeUndefined();
  });
});
