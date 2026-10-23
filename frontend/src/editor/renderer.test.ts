import { describe, expect, it } from "vitest";
import type { ProjectDocument } from "../api/project";
import { renderBoardSvg } from "./renderer";
const fixture: ProjectDocument = {
  project_id: "project",
  name: "Fixture",
  revision: 2,
  design: {
    board: { name: "Board", holes_x: 4, holes_y: 3, pitch_mm: 2.54 },
    component_definitions: {
      part: {
        schema_version: 1,
        id: "part",
        definition_version: "1",
        name: "Part",
        description: "",
        body: { x: -1, y: -1, width: 5, height: 3 },
        allowed_rotations: ["deg0", "deg90"],
        pins: [
          {
            id: "1",
            number: "1",
            name: "Pin 1",
            x: 0,
            y: 0,
            aliases: ["P1"],
            functions: ["gpio"],
          },
        ],
      },
    },
    components: {
      instance: {
        id: "instance",
        definition_id: "part",
        name: "U1",
        position: { x: 2, y: 1 },
        rotation: "deg90",
      },
    },
  },
};
describe("board SVG", () => {
  it("renders holes, selection, pins, labels, and rotation deterministically", () => {
    const svg = renderBoardSvg(fixture, "instance");
    expect(svg.match(/class="hole"/g)).toHaveLength(12);
    expect(svg).toContain('class="component selected"');
    expect(svg).toContain("translate(2 1) rotate(90)");
    expect(svg).toContain('class="orientation"');
    expect(svg).toContain("Pin 1: Pin 1");
    expect(svg).toContain("Aliases: P1 · Functions: gpio");
    expect(svg).toContain('data-pin-id="1"');
    expect(svg).toContain('data-pin-number="1"');
    expect(svg).toContain(
      '<text class="pin-label readable-label" x="2" y="1">1</text>',
    );
  });
  it("keeps the unrotated component in canonical coordinates", () => {
    const unrotated = structuredClone(fixture);
    unrotated.design.components!.instance!.rotation = "deg0";
    expect(renderBoardSvg(unrotated)).toContain("translate(2 1) rotate(0)");
  });

  it("renders the selected component last so it appears on top", () => {
    const withOverlap = structuredClone(fixture);
    withOverlap.design.components!.other = {
      ...withOverlap.design.components!.instance!,
      id: "other",
      name: "U2",
    };
    const svg = renderBoardSvg(withOverlap, "instance");
    expect(svg.indexOf('data-component-id="other"')).toBeLessThan(
      svg.indexOf('data-component-id="instance"'),
    );
  });

  it("mirrors solder-side geometry while keeping labels readable", () => {
    const svg = renderBoardSvg(fixture, undefined, undefined, "solder");
    expect(svg).toContain('transform="translate(3 0) scale(-1 1)"');
    expect(svg).toContain('class="component-label readable-label"');
    expect(svg).toContain('x="1.5"');
    expect(svg).toContain('data-side="solder"');
    expect(svg).toContain(
      '<text class="pin-label readable-label" x="1" y="1">1</text>',
    );
  });

  it("renders wires with identifiers and non-color side patterns", () => {
    const routed = structuredClone(fixture);
    routed.design.wires = {
      wire: {
        id: "wire",
        net_id: "net",
        start: { kind: "pin", component_id: "instance", pin_id: "1" },
        end: { kind: "pin", component_id: "instance", pin_id: "2" },
        side: "solder",
        color: "green",
        vertices: [
          { x2: 0, y2: 0 },
          { x2: 2, y2: 2 },
        ],
      },
    };
    const svg = renderBoardSvg(routed, undefined, undefined, "component");
    expect(svg).toContain('class="wire opposite-side"');
    expect(svg).toContain("W1");
    expect(svg).toContain("--wire-color:green");
  });
});
