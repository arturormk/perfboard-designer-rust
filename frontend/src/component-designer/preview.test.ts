import { describe, expect, it } from "vitest";
import type { ComponentDefinition } from "../api/project";
import {
  pinLabel,
  pinLabelPlacement,
  renderComponentPreviewSvg,
  rotatePoint,
} from "./preview";

const fixture: ComponentDefinition = {
  schema_version: 1,
  id: "dev.example.asymmetric",
  definition_version: "1.0.0",
  name: "Asymmetric fixture",
  description: "Exercises preview geometry",
  body: { x: -1.27, y: -2.54, width: 7.62, height: 10.16 },
  allowed_rotations: ["deg0", "deg90"],
  pins: [
    {
      id: "signal",
      number: "1",
      name: "Signal",
      x: 0,
      y: 0,
      aliases: ["SIG"],
      functions: ["gpio", "pwm"],
    },
    { id: "ground", number: "2", name: "Ground", x: 2, y: 3 },
  ],
  occupied_holes: [{ x: 1, y: 2 }],
  keepouts: [{ min: { x: 0, y: 0 }, max: { x: 2, y: 3 }, label: "Body" }],
  access: [
    {
      min: { x: 3, y: 1 },
      max: { x: 4, y: 2 },
      label: "Connector",
      direction: "east",
      required: true,
      allow_board_overhang: false,
    },
  ],
};

const options = {
  rotation: "deg90" as const,
  labelMode: "combined" as const,
  selectedPinId: "signal",
  showKeepouts: true,
  showAccess: true,
  showOccupiedHoles: true,
};

describe("component preview", () => {
  it("rotates local grid coordinates in 90 degree steps", () => {
    expect(rotatePoint("deg0", 2, 3)).toEqual([2, 3]);
    expect(rotatePoint("deg90", 2, 3)).toEqual([-3, 2]);
    expect(rotatePoint("deg180", 2, 3)).toEqual([-2, -3]);
    expect(rotatePoint("deg270", 2, 3)).toEqual([3, -2]);
  });

  it("derives every label mode from structured pin metadata", () => {
    const pin = fixture.pins[0]!;
    expect(pinLabel(pin, "number")).toBe("1");
    expect(pinLabel(pin, "name")).toBe("Signal");
    expect(pinLabel(pin, "function")).toBe("gpio, pwm");
    expect(pinLabel(pin, "combined")).toBe("1 · Signal · gpio, pwm");
    expect(pinLabel(pin, "hidden")).toBe("");
  });

  it("centers labels on the pin axis appropriate to each rotation", () => {
    expect(pinLabelPlacement("deg0", 2, 3)).toEqual({
      x: 2.5,
      y: 3,
      textAnchor: "start",
    });
    expect(pinLabelPlacement("deg180", 2, 3)).toEqual({
      x: -2.5,
      y: -3,
      textAnchor: "end",
    });
    expect(pinLabelPlacement("deg90", 2, 3)).toEqual({
      x: -3,
      y: 2.5,
      textAnchor: "middle",
    });
    expect(pinLabelPlacement("deg270", 2, 3)).toEqual({
      x: 3,
      y: -2.5,
      textAnchor: "middle",
    });
  });

  it("renders the grid, origin, body, overlays, selection, and readable labels", () => {
    const svg = renderComponentPreviewSvg(fixture, options);
    expect(svg).toContain('id="component-preview"');
    expect(svg).toContain('transform="rotate(90)"');
    expect(svg).toContain('class="preview-axes"');
    expect(svg).toContain('class="preview-body"');
    expect(svg).toContain('class="preview-keepout"');
    expect(svg).toContain('class="preview-access"');
    expect(svg).toContain('class="preview-occupied"');
    expect(svg).toContain('class="preview-pin selected"');
    expect(svg).toContain('text-anchor="middle" dominant-baseline="central"');
    expect(svg).toContain("1 · Signal · gpio, pwm");
  });

  it("omits disabled overlays and hidden labels", () => {
    const svg = renderComponentPreviewSvg(fixture, {
      ...options,
      labelMode: "hidden",
      showKeepouts: false,
      showAccess: false,
      showOccupiedHoles: false,
    });
    expect(svg).not.toContain("preview-keepout");
    expect(svg).not.toContain("preview-access");
    expect(svg).not.toContain("preview-occupied");
    expect(svg).not.toContain("preview-pin-label");
  });
});
