import { describe, expect, it } from "vitest";
import {
  netForWireStart,
  parsePinKey,
  pinsAt,
  pinUsableOnSide,
  terminalForWireEnd,
} from "./hit";
import { testDesign } from "../model/test-design";

describe("pin hit testing", () => {
  it("round-trips pin keys", () => {
    expect(parsePinKey("r1\u0000a")).toEqual({
      component_id: "r1",
      pin_id: "a",
    });
  });

  it("finds every pin in a hole, bridges included", () => {
    const design = testDesign();
    expect(pinsAt(design, { x: 3, y: 1 })).toEqual([
      { component_id: "r1", pin_id: "b" },
      { component_id: "b1", pin_id: "a" },
    ]);
    expect(pinsAt(design, { x: 9, y: 7 })).toEqual([]);
  });

  it("only lets bridges take wires on the solder side", () => {
    const design = testDesign();
    expect(
      pinUsableOnSide(design, { component_id: "b1", pin_id: "a" }, "component"),
    ).toBe(false);
    expect(
      pinUsableOnSide(design, { component_id: "b1", pin_id: "a" }, "solder"),
    ).toBe(true);
    expect(
      pinUsableOnSide(design, { component_id: "r1", pin_id: "a" }, "component"),
    ).toBe(true);
  });
});

describe("wire ends", () => {
  it("rejects half-grid positions and prefers a pin on the wire's net", () => {
    const design = testDesign();
    expect(
      terminalForWireEnd(design, "w1", "end", { x2: 3, y2: 2 }),
    ).toBeUndefined();
    expect(terminalForWireEnd(design, "w1", "end", { x2: 2, y2: 8 })).toEqual({
      kind: "pin",
      component_id: "r2",
      pin_id: "a",
    });
  });
});

describe("wire start net", () => {
  it("uses the pin's accepted net, refuses proposals, else the active net", () => {
    const design = testDesign();
    expect(
      netForWireStart(design, { component_id: "r1", pin_id: "a" }, undefined),
    ).toEqual({ netId: "gnd" });
    expect(
      netForWireStart(design, { component_id: "r1", pin_id: "b" }, "gnd")
        .reason,
    ).toMatch(/proposal/);
    expect(
      netForWireStart(design, { component_id: "r10", pin_id: "a" }, "gnd"),
    ).toEqual({ netId: "gnd" });
    expect(
      netForWireStart(design, { component_id: "r10", pin_id: "a" }, undefined),
    ).toEqual({});
  });
});
