import { describe, expect, it } from "vitest";
import type { ProjectDocument } from "../api/project";
import { netPinLabel } from "./net-pins";

const design: ProjectDocument["design"] = {
  board: {
    name: "test",
    holes_x: 10,
    holes_y: 10,
    pitch_mm: 2.54,
  },
  components: {
    component_1: {
      id: "component_1",
      definition_id: "resistor",
      name: "R1",
      position: { x: 1, y: 1 },
      rotation: "deg0",
    },
  },
  component_definitions: {
    resistor: {
      schema_version: 1,
      id: "resistor",
      definition_version: "1",
      name: "Resistor",
      description: "",
      allowed_rotations: ["deg0"],
      body: { x: 0, y: 0, width: 2.54, height: 2.54 },
      pins: [{ id: "a", number: "1", name: "A", x: 0, y: 0 }],
    },
  },
};

describe("net pin labels", () => {
  it("shows the placed component name, pin number, and pin name", () => {
    expect(
      netPinLabel(design, { component_id: "component_1", pin_id: "a" }),
    ).toBe("R1 · 1 (A)");
  });

  it("falls back to stable IDs when component metadata is unavailable", () => {
    expect(
      netPinLabel(design, { component_id: "missing", pin_id: "unknown" }),
    ).toBe("missing · pin unknown");
  });
});
