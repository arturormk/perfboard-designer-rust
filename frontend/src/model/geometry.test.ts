import { describe, expect, it } from "vitest";
import {
  adjacentHoles,
  bendCount,
  bodyBounds,
  bridgeRotation,
  componentSide,
  fromRoute,
  nextRotation,
  pinPosition,
  polylineLengthMm,
  polylineMidpoint,
  rotateLocal,
  toRoute,
  viewX,
} from "./geometry";
import { bridge, resistor, testDesign } from "./test-design";

describe("rotation", () => {
  it("rotates clockwise in screen coordinates without negative zero", () => {
    expect(rotateLocal("deg90", 2, 0)).toEqual({ x: 0, y: 2 });
    expect(rotateLocal("deg180", 2, 0)).toEqual({ x: -2, y: 0 });
    expect(Object.is(rotateLocal("deg180", 0, 0).x, -0)).toBe(false);
  });

  it("cycles only through the rotations a part allows", () => {
    expect(nextRotation("deg0", ["deg0", "deg90"])).toBe("deg90");
    expect(nextRotation("deg90", ["deg0", "deg90"])).toBe("deg0");
    expect(nextRotation("deg270")).toBe("deg0");
    expect(nextRotation("deg0", [])).toBe("deg90");
  });
});

describe("pin and body placement", () => {
  it("places pins through the instance rotation", () => {
    const design = testDesign();
    expect(pinPosition(design, { component_id: "r1", pin_id: "b" })).toEqual({
      x: 3,
      y: 1,
    });
    expect(pinPosition(design, { component_id: "r2", pin_id: "b" })).toEqual({
      x: 1,
      y: 6,
    });
    expect(
      pinPosition(design, { component_id: "missing", pin_id: "a" }),
    ).toBeUndefined();
  });

  it("computes rotated body bounds in holes", () => {
    const design = testDesign();
    const upright = bodyBounds(design.components!.r1!, resistor, 2.54);
    expect(upright).toEqual({ x: 1.25, y: 0.5, width: 1.5, height: 1 });
    const turned = bodyBounds(design.components!.r2!, resistor, 2.54);
    expect(turned.width).toBeCloseTo(1);
    expect(turned.height).toBeCloseTo(1.5);
  });

  it("puts bridges on the solder side", () => {
    expect(componentSide(bridge)).toBe("solder");
    expect(componentSide(resistor)).toBe("component");
  });
});

describe("routes", () => {
  it("converts between holes and half-grid route units", () => {
    expect(toRoute({ x: 1.5, y: 2 })).toEqual({ x2: 3, y2: 4 });
    expect(fromRoute({ x2: 3, y2: 4 })).toEqual({ x: 1.5, y: 2 });
  });

  it("measures length and bends of an orthogonal path", () => {
    const path = [
      { x2: 0, y2: 0 },
      { x2: 4, y2: 0 },
      { x2: 4, y2: 2 },
      { x2: 8, y2: 2 },
    ];
    expect(polylineLengthMm(path, 2.54)).toBeCloseTo(5 * 2.54);
    expect(bendCount(path)).toBe(2);
    expect(bendCount([path[0]!, path[1]!, { x2: 8, y2: 0 }])).toBe(0);
  });

  it("finds the midpoint by travelled distance", () => {
    const { point, horizontal } = polylineMidpoint([
      { x2: 0, y2: 0 },
      { x2: 8, y2: 0 },
      { x2: 8, y2: 4 },
    ]);
    expect(point).toEqual({ x: 3, y: 0 });
    expect(horizontal).toBe(true);
  });
});

describe("solder-side mirroring", () => {
  it("mirrors x about the board centre only on the solder side", () => {
    expect(viewX(0, 10, "solder")).toBe(9);
    expect(viewX(9, 10, "solder")).toBe(0);
    expect(viewX(3, 10, "component")).toBe(3);
  });
});

describe("bridges", () => {
  it("lists in-board neighbours and orients toward them", () => {
    expect(adjacentHoles({ x: 0, y: 0 }, 10, 8)).toEqual([
      { x: 1, y: 0 },
      { x: 0, y: 1 },
    ]);
    expect(bridgeRotation({ x: 2, y: 2 }, { x: 2, y: 1 })).toBe("deg270");
    expect(bridgeRotation({ x: 2, y: 2 }, { x: 4, y: 2 })).toBeUndefined();
  });
});
