import { describe, expect, it } from "vitest";
import { newComponentStagingPosition } from "./component-placement";

describe("new component staging position", () => {
  it("uses the central row two intervals beyond the rightmost hole", () => {
    expect(newComponentStagingPosition({ holes_x: 25, holes_y: 18 })).toEqual({
      x: 26,
      y: 9,
    });
  });

  it("uses the exact middle row for an odd row count", () => {
    expect(newComponentStagingPosition({ holes_x: 9, holes_y: 9 })).toEqual({
      x: 10,
      y: 4,
    });
  });
});
