import { describe, expect, it } from "vitest";
import {
  bridgeEndAt,
  describedPin,
  netForWireStart,
  netsAtPin,
  parsePinKey,
  pickUnderPointer,
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

  it("describes the part pin under a bridge end, not the bridge", () => {
    const design = testDesign();
    expect(describedPin(design, { component_id: "b1", pin_id: "a" })).toEqual(
      { component_id: "r1", pin_id: "b" },
    );
    // A part's own pin is described as itself.
    expect(describedPin(design, { component_id: "r1", pin_id: "b" })).toEqual(
      { component_id: "r1", pin_id: "b" },
    );
  });

  it("finds the bridge end under the pointer", () => {
    const design = testDesign();
    expect(bridgeEndAt(design, "b1", { x: 3.1, y: 0.9 })).toEqual({
      component_id: "b1",
      pin_id: "a",
    });
    // Mid-span, and on a part that is not a bridge, there is no end.
    const [a, b] = ["a", "b"].map((pin_id) => {
      const instance = design.components!.b1!;
      const pin = design.component_definitions![instance.definition_id]!.pins.find(
        (item) => item.id === pin_id,
      )!;
      return { x: instance.position.x + pin.x, y: instance.position.y + pin.y };
    });
    expect(
      bridgeEndAt(design, "b1", { x: (a!.x + b!.x) / 2, y: (a!.y + b!.y) / 2 }),
    ).toBeUndefined();
    expect(bridgeEndAt(design, "r1", { x: 3, y: 1 })).toBeUndefined();
  });

  it("keeps a bridge end that sits alone in its hole", () => {
    const design = testDesign();
    const bridge = design.components!.b1!;
    bridge.position = { x: 9, y: 7 };
    expect(describedPin(design, { component_id: "b1", pin_id: "a" })).toEqual(
      { component_id: "b1", pin_id: "a" },
    );
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
    ).toEqual({ netId: "gnd", fromActive: true });
    expect(
      netForWireStart(design, { component_id: "r10", pin_id: "a" }, undefined),
    ).toEqual({});
  });
});

describe("bridge ends take the net they are soldered to", () => {
  const withBridge = (id: string, x: number, y: number) => {
    const design = testDesign();
    design.components![id] = {
      ...design.components!.b1!,
      id,
      name: id,
      position: { x, y },
    };
    return design;
  };

  it("reaches across a bridge, and along a chain of bridges", () => {
    // b1 runs from r1.b (I2C_SDA) at 3,1 to 4,1; b2 continues to 5,1.
    const design = withBridge("b2", 4, 1);
    const names = (pin: { component_id: string; pin_id: string }) =>
      netsAtPin(design, pin).map((net) => net.id);
    expect(names({ component_id: "b1", pin_id: "b" })).toEqual(["sda"]);
    expect(names({ component_id: "b2", pin_id: "b" })).toEqual(["sda"]);
  });

  it("starts a wire on the bridge's net rather than the active one", () => {
    const design = testDesign();
    const bridgeEnd = { component_id: "b1", pin_id: "b" };
    expect(netForWireStart(design, bridgeEnd, "gnd").reason).toMatch(
      /proposal/,
    );
    design.nets!.sda!.state = "accepted";
    expect(netForWireStart(design, bridgeEnd, "gnd")).toEqual({
      netId: "sda",
    });
  });

  it("borrows the active net only for an unconnected bridge", () => {
    const design = withBridge("b2", 6, 5);
    expect(
      netForWireStart(design, { component_id: "b2", pin_id: "a" }, "gnd"),
    ).toEqual({ netId: "gnd", fromActive: true });
  });

  it("refuses a hole where a bridge shorts two nets", () => {
    const design = testDesign();
    design.nets!.sda!.state = "accepted";
    design.nets!.gnd!.pins!.push({ component_id: "b1", pin_id: "b" });
    expect(
      netForWireStart(design, { component_id: "b1", pin_id: "b" }, undefined)
        .reason,
    ).toMatch(/joins/);
  });
});

describe("picking under the pointer", () => {
  const nano = { kind: "component", id: "nano" } as const;
  const resistor = { kind: "component", id: "r1" } as const;
  const wire = { kind: "wire", id: "w1" } as const;
  const stack = [nano, resistor, wire];

  it("takes the topmost object first", () => {
    expect(pickUnderPointer(stack, null, false)).toEqual(nano);
    expect(pickUnderPointer(stack, { kind: "net", id: "n" }, true)).toEqual(
      nano,
    );
  });

  it("steps down through the stack on repeated clicks, wrapping round", () => {
    expect(pickUnderPointer(stack, nano, true)).toEqual(resistor);
    expect(pickUnderPointer(stack, resistor, true)).toEqual(wire);
    expect(pickUnderPointer(stack, wire, true)).toEqual(nano);
  });

  it("keeps a covered selection when it is clicked at another spot", () => {
    expect(pickUnderPointer(stack, resistor, false)).toEqual(resistor);
  });

  it("returns nothing over empty board", () => {
    expect(pickUnderPointer([], nano, true)).toBeUndefined();
  });
});
