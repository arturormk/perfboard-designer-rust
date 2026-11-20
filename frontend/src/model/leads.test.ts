import { describe, expect, it } from "vitest";
import { leadSegments } from "./leads";

// DO-41 diode body, in holes at 2.54 mm pitch: leads at x = 0 and x = 4.
const diodeBody = { x: 0.976, y: -0.531, width: 2.047, height: 1.063 };

describe("leadSegments", () => {
  it("joins each end of a short body to the rim of its pad", () => {
    const segments = leadSegments(
      [
        { x: 0, y: 0 },
        { x: 4, y: 0 },
      ],
      diodeBody,
      0.3,
    );
    expect(segments).toEqual([
      { x1: diodeBody.x, y1: 0, x2: 0.3, y2: 0 },
      { x1: diodeBody.x + diodeBody.width, y1: 0, x2: 3.7, y2: 0 },
    ]);
  });

  it("runs down the y axis for an upright part", () => {
    const body = { x: -0.531, y: 0.976, width: 1.063, height: 2.047 };
    const segments = leadSegments(
      [
        { x: 0, y: 0 },
        { x: 0, y: 4 },
      ],
      body,
      0.3,
    );
    expect(segments).toEqual([
      { x1: 0, y1: body.y, x2: 0, y2: 0.3 },
      { x1: 0, y1: body.y + body.height, x2: 0, y2: 3.7 },
    ]);
  });

  it("draws nothing where the body already reaches the pads", () => {
    const body = { x: -0.5, y: -0.5, width: 5, height: 1 };
    expect(
      leadSegments(
        [
          { x: 0, y: 0 },
          { x: 4, y: 0 },
        ],
        body,
        0.3,
      ),
    ).toEqual([]);
  });

  it("leaves parts with other pin layouts alone", () => {
    expect(
      leadSegments(
        [
          { x: 0, y: 0 },
          { x: 1, y: 1 },
        ],
        diodeBody,
        0.3,
      ),
    ).toEqual([]);
    expect(
      leadSegments(
        [
          { x: 0, y: 0 },
          { x: 2, y: 0 },
          { x: 4, y: 0 },
        ],
        diodeBody,
        0.3,
      ),
    ).toEqual([]);
  });
});
