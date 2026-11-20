import { describe, expect, it } from "vitest";
import { plusPath, polarityMark } from "./polarity";

// DO-41 diode and 5 mm electrolytic bodies, in holes at 2.54 mm pitch.
const diodeBody = { x: 0.976, y: -0.531, width: 2.047, height: 1.063 };
const capBody = { x: -0.484, y: -0.984, width: 1.969, height: 1.969 };

describe("polarityMark", () => {
  it("puts the cathode band at the cathode end of a diode", () => {
    const mark = polarityMark(
      [
        { id: "cathode", x: 0, y: 0 },
        { id: "anode", x: 4, y: 0 },
      ],
      diodeBody,
    );
    expect(mark?.kind).toBe("band");
    if (mark?.kind !== "band") return;
    expect(mark.x).toBeGreaterThan(diodeBody.x);
    expect(mark.x + mark.width).toBeLessThan(diodeBody.x + diodeBody.width / 2);
    expect(mark.height).toBe(diodeBody.height);
  });

  it("follows the cathode when it is the far lead", () => {
    const mark = polarityMark(
      [
        { id: "anode", x: 0, y: 0 },
        { id: "cathode", x: 4, y: 0 },
      ],
      diodeBody,
    );
    if (mark?.kind !== "band") throw new Error("no band");
    expect(mark.x).toBeGreaterThan(diodeBody.x + diodeBody.width / 2);
    expect(mark.x + mark.width).toBeLessThan(diodeBody.x + diodeBody.width);
  });

  it("bands a diode whose leads run down the y axis", () => {
    const mark = polarityMark(
      [
        { id: "cathode", x: 0, y: 4 },
        { id: "anode", x: 0, y: 0 },
      ],
      { x: -0.531, y: 0.976, width: 1.063, height: 2.047 },
    );
    if (mark?.kind !== "band") throw new Error("no band");
    expect(mark.width).toBe(1.063);
    expect(mark.y).toBeGreaterThan(0.976 + 2.047 / 2);
  });

  it("puts a plus sign by an electrolytic's positive lead, clear of its pad", () => {
    const mark = polarityMark(
      [
        { id: "positive", x: 0, y: 0 },
        { id: "negative", x: 1, y: 0 },
      ],
      capBody,
    );
    if (mark?.kind !== "plus") throw new Error("no plus");
    expect(mark.x).toBe(0);
    expect(mark.y + mark.arm).toBeLessThan(-0.36);
    expect(mark.y - mark.arm).toBeGreaterThan(capBody.y);
    expect(plusPath(mark)).toMatch(/^M -0\.\d+ -0\.\d+ H /);
  });

  it("marks nothing on unpolarized parts", () => {
    expect(
      polarityMark(
        [
          { id: "1", x: 0, y: 0 },
          { id: "2", x: 1, y: 0 },
        ],
        capBody,
      ),
    ).toBeUndefined();
  });
});
