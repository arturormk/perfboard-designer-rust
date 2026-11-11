import { describe, expect, it } from "vitest";
import {
  describeWire,
  groupPlacedParts,
  isDarkWire,
  nextInstanceName,
  relativeTime,
  suggestedWireColor,
  wireColorCss,
  wireLabels,
} from "./naming";
import { resistor, testDesign } from "./test-design";

describe("wire colours", () => {
  it("suggests conventional colours from net names", () => {
    expect(suggestedWireColor("GND")).toBe("black");
    expect(suggestedWireColor("AGND_RETURN")).toBe("black");
    expect(suggestedWireColor("VCC_5V")).toBe("red");
    expect(suggestedWireColor("I2C_SCL")).toBe("yellow");
    expect(suggestedWireColor("RS485_A")).toBe("white");
    expect(suggestedWireColor(undefined)).toBe("white");
  });

  it("maps custom colours and flags dark insulation", () => {
    expect(wireColorCss({ other: "Purple" })).toBe("purple");
    expect(isDarkWire("black")).toBe(true);
    expect(isDarkWire("yellow")).toBe(false);
  });
});

describe("labels", () => {
  it("numbers wires in document order", () => {
    expect(wireLabels(testDesign()).get("w1")).toBe("W1");
  });

  it("describes a wire by its terminals", () => {
    const design = testDesign();
    expect(describeWire(design, design.wires!.w1!)).toBe("R1 · 1 → R2 · 1");
  });

  it("picks the next free instance name", () => {
    expect(nextInstanceName(testDesign(), resistor)).toBe("Resistor 1");
  });
});

describe("part groups", () => {
  it("groups by type, sorts names numerically and puts bridges last", () => {
    const groups = groupPlacedParts(testDesign());
    expect(groups.map((group) => group.definitionId)).toEqual([
      "test.resistor",
      "test.bridge",
    ]);
    expect(groups[0]!.instances.map((item) => item.name)).toEqual([
      "R1",
      "R2",
      "R10",
    ]);
    expect(groups[1]!.bridge).toBe(true);
  });
});

describe("relative time", () => {
  it("speaks in human units", () => {
    const now = 1_000_000_000;
    expect(relativeTime(now - 10_000, now)).toBe("just now");
    expect(relativeTime(now - 5 * 60_000, now)).toMatch(/5 min/);
  });
});
