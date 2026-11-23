import { describe, expect, it } from "vitest";
import type { Band, Rotation } from "../api/project";
import { bandRects } from "./bands";

const TEN_K: Band[] = ["brown", "black", "black", "red", "brown"];
// A lying 1/4 W resistor: three holes long, one high.
const LYING = { x: 0.5, y: -0.5, width: 3, height: 1 };

/** Screen-space centre of each band along the reading axis. */
function along(rotation: Rotation, body = LYING): number[] {
  const degrees = { deg0: 0, deg90: 90, deg180: 180, deg270: 270 }[rotation];
  const radians = (degrees * Math.PI) / 180;
  const cos = Math.round(Math.cos(radians));
  const sin = Math.round(Math.sin(radians));
  return bandRects(TEN_K, body, rotation).map((rect) => {
    const x = rect.x + rect.width / 2;
    const y = rect.y + rect.height / 2;
    // Rotate the part-local centre into screen space and read along the
    // axis the long side lies on.
    const sx = x * cos - y * sin;
    const sy = x * sin + y * cos;
    return cos !== 0 ? sx : sy;
  });
}

describe("resistor bands", () => {
  it("puts the tolerance band last in reading order at every rotation", () => {
    for (const rotation of ["deg0", "deg90", "deg180", "deg270"] as const) {
      const positions = along(rotation);
      expect(positions).toHaveLength(5);
      const sorted = [...positions].sort((a, b) => a - b);
      expect(positions, rotation).toEqual(sorted);
    }
  });

  it("leaves a wider gap before the tolerance band", () => {
    const positions = along("deg0");
    const gaps = positions
      .slice(1)
      .map((value, index) => value - positions[index]!);
    expect(gaps[3]).toBeGreaterThan(gaps[0]! * 1.5);
  });

  it("groups a four-band code the same way", () => {
    const fourBand: Band[] = ["brown", "black", "orange", "gold"];
    const centres = bandRects(fourBand, LYING, "deg0").map(
      (rect) => rect.x + rect.width / 2,
    );
    expect(centres).toHaveLength(4);
    const gaps = centres
      .slice(1)
      .map((value, index) => value - centres[index]!);
    expect(gaps[2]).toBeGreaterThan(gaps[0]! * 1.5);
  });

  it("draws a zero-ohm link as one centred band", () => {
    const [rect] = bandRects(["black"], LYING, "deg0");
    expect(rect!.x + rect!.width / 2).toBeCloseTo(LYING.x + LYING.width / 2);
  });

  it("skips bodies too short to be a lying resistor", () => {
    expect(
      bandRects(TEN_K, { x: -0.5, y: -0.5, width: 2, height: 1 }, "deg0"),
    ).toEqual([]);
  });

  it("follows a body drawn tall in its definition", () => {
    const tall = { x: -0.5, y: 0.5, width: 1, height: 3 };
    const rects = bandRects(TEN_K, tall, "deg0");
    expect(rects.every((rect) => rect.width === 1)).toBe(true);
    const ys = rects.map((rect) => rect.y);
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
  });
});
