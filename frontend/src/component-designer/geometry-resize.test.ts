import { describe, expect, it } from "vitest";
import {
  bodyGridRect,
  keepoutGridRect,
  planBodyResize,
  planHoleRegionResize,
  planKeepoutResize,
} from "./geometry-resize";

describe("component geometry resize planning", () => {
  it("snaps body borders to whole and half hole coordinates", () => {
    const body = { x: -1.27, y: -1.27, width: 7.62, height: 5.08 };
    expect(planBodyResize(body, "right", 3.74, 0)).toEqual({
      x: -1.27,
      y: -1.27,
      width: 10.16,
      height: 5.08,
    });
    expect(bodyGridRect(planBodyResize(body, "top-left", -1.1, -1.2)!)).toEqual(
      {
        x: -1,
        y: -1,
        width: 3.5,
        height: 2.5,
      },
    );
  });

  it("keeps keepout borders halfway between integer hole coordinates", () => {
    const keepout = {
      min: { x: 0, y: 0 },
      max: { x: 2, y: 3 },
      label: "Body",
    };
    const resized = planKeepoutResize(keepout, "bottom-right", 3.2, 4.8)!;
    expect(resized).toEqual({
      min: { x: 0, y: 0 },
      max: { x: 3, y: 4 },
      label: "Body",
    });
    expect(keepoutGridRect(resized)).toEqual({
      x: -0.5,
      y: -0.5,
      width: 4,
      height: 5,
    });
  });

  it("does not allow either rectangle to collapse", () => {
    expect(
      planBodyResize({ x: 0, y: 0, width: 2.54, height: 2.54 }, "left", 1, 0),
    ).toBeUndefined();
    expect(
      planKeepoutResize(
        { min: { x: 0, y: 0 }, max: { x: 0, y: 0 }, label: "one" },
        "right",
        -0.6,
        0,
      ),
    ).toBeUndefined();
  });

  it("preserves access-region metadata while resizing its hole range", () => {
    const access = {
      min: { x: 1, y: 2 },
      max: { x: 2, y: 3 },
      label: "USB",
      direction: "east" as const,
      required: true,
      allow_board_overhang: false,
    };
    expect(planHoleRegionResize(access, "right", 4.4, 0)).toEqual({
      ...access,
      max: { x: 4, y: 3 },
    });
  });
});
