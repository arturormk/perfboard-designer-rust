import { describe, expect, it } from "vitest";
import type { ProjectDocument } from "../api/project";
import { renderBoardSvg, renderPrintLayout } from "./renderer";
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
  it("renders holes, selection, pin labels, and rotation deterministically", () => {
    const svg = renderBoardSvg(fixture, "instance");
    expect(svg.match(/class="hole"/g)).toHaveLength(12);
    expect(svg).toContain(
      '<circle class="hole" cx="2" cy="1" r=".12" data-x="2" data-y="1"><title>Hole (2, 1)</title></circle>',
    );
    expect(svg).toContain(
      '<g class="hole-coordinate-labels" aria-label="Internal hole coordinates">',
    );
    expect(svg).toContain(
      '<g class="column-coordinates"><text x="0" y="-0.55">0</text><text x="1" y="-0.55">1</text>',
    );
    expect(svg).toContain(
      '<g class="row-coordinates"><text x="-0.55" y="0">0</text><text x="-0.55" y="1">1</text>',
    );
    expect(svg).toContain(
      '<g class="column-coordinates bottom-coordinates"><text x="0" y="2.55">0</text><text x="1" y="2.55">1</text>',
    );
    expect(svg).toContain(
      '<g class="row-coordinates right-coordinates"><text x="3.55" y="0">0</text><text x="3.55" y="1">1</text>',
    );
    expect(svg).toContain('class="coordinate-axis-label x-axis"');
    expect(svg).toContain("x →</text>");
    expect(svg).toContain("y ↓</text>");
    expect(svg).toContain('class="component selected"');
    expect(svg).toContain("translate(2 1) rotate(90)");
    expect(svg).toContain('class="orientation"');
    expect(svg).toContain("U1\nType: Part (part v1)");
    expect(svg).toContain("Anchor: 2, 1\nRotation: 90°\nPins: 1");
    expect(svg).not.toContain('class="component-label"');
    expect(svg).not.toContain("Component orientation");
    expect(svg).toContain("Pin 1: Pin 1");
    expect(svg).toContain("Aliases: P1 · Functions: gpio");
    expect(svg).toContain('data-pin-id="1"');
    expect(svg).toContain('data-pin-number="1"');
    expect(svg).toContain('viewBox="-2 -2 9 6"');
    expect(svg).toContain(
      '<text class="pin-label readable-label" x="2" y="1">1</text>',
    );
  });
  it("keeps the unrotated component in canonical coordinates", () => {
    const unrotated = structuredClone(fixture);
    unrotated.design.components!.instance!.rotation = "deg0";
    expect(renderBoardSvg(unrotated)).toContain("translate(2 1) rotate(0)");
  });

  it("marks only an interactive component-side board for faded components", () => {
    const faded = renderBoardSvg(
      fixture,
      undefined,
      undefined,
      "component",
      {},
      "board",
      false,
      false,
      true,
    );
    const solder = renderBoardSvg(
      fixture,
      undefined,
      undefined,
      "solder",
      {},
      "board",
      false,
      false,
      true,
    );
    expect(faded).toContain('class="interactive-board-svg components-faded"');
    expect(solder).not.toContain("components-faded");
  });

  it("renders a bridge as a solder-side copper segment", () => {
    const bridged = structuredClone(fixture);
    const definition = bridged.design.component_definitions!.part!;
    definition.kind = "bridge";
    definition.pins = [
      { id: "a", number: "A", name: "End A", x: 0, y: 0 },
      { id: "b", number: "B", name: "End B", x: 1, y: 0 },
    ];
    const svg = renderBoardSvg(bridged, "instance", undefined, "solder");
    expect(svg).toContain('class="component bridge selected"');
    expect(svg).toContain('data-component-kind="bridge"');
    expect(svg).toContain(
      '<line class="bridge-hit-target" x1="0" y1="0" x2="1" y2="0"/>',
    );
    expect(svg).toContain(
      '<path class="bridge-conductor bridge-span" d="M 0.4 0 L 0.6 0">',
    );
    expect(svg).toContain(
      '<path class="bridge-conductor bridge-terminals" d="M 0 -0.36 Q 0.72 0 0 0.36 M 1 -0.36 Q 0.28 0 1 0.36"/>',
    );
    expect(svg).not.toContain('class="pin-label readable-label"');
    expect(svg).not.toContain('<rect class="body"');
    expect(svg).not.toContain('class="orientation"');
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

  it("mirrors solder-side geometry without labeling opposite-side components", () => {
    const svg = renderBoardSvg(fixture, undefined, undefined, "solder");
    expect(svg).toContain('transform="translate(3 0) scale(-1 1)"');
    expect(svg).not.toContain('class="component-label"');
    expect(svg).toContain('data-side="solder"');
    expect(svg).not.toContain(
      '<text class="pin-label readable-label" x="1" y="1">1</text>',
    );
    expect(svg).toContain(
      '<g class="column-coordinates"><text x="3" y="-0.55">0</text><text x="2" y="-0.55">1</text><text x="1" y="-0.55">2</text><text x="0" y="-0.55">3</text>',
    );
    expect(svg).toContain(
      '<g class="column-coordinates bottom-coordinates"><text x="3" y="2.55">0</text><text x="2" y="2.55">1</text><text x="1" y="2.55">2</text><text x="0" y="2.55">3</text>',
    );
    expect(svg).toContain("← x</text>");
  });

  it("includes optional component details in the orientation hint", () => {
    const detailed = structuredClone(fixture);
    detailed.design.component_definitions!.part!.description = "Timer module";
    detailed.design.components!.instance!.properties = {
      package: "DIP-8",
      value: "555 & friends",
    };
    detailed.design.components!.instance!.notes = "Keep near power < rail";
    const svg = renderBoardSvg(detailed);
    expect(svg).toContain("Timer module\nAnchor: 2, 1");
    expect(svg).toContain("Properties: package=DIP-8, value=555 &amp; friends");
    expect(svg).toContain("Notes: Keep near power &lt; rail");
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
    expect(svg).not.toContain("W1");
    expect(svg).toContain("--wire-color:green");
    expect(svg).toContain(
      '<polyline class="wire-casing" points="0,0 1,1"/><polyline class="wire-path" points="0,0 1,1"',
    );
    const solder = renderBoardSvg(routed, undefined, undefined, "solder");
    expect(solder).toContain(
      '<text class="wire-label readable-label" x="2.5" y="0.5">W1</text>',
    );
  });

  it("renders every explicit rail vertex as a wire connection node", () => {
    const routed = structuredClone(fixture);
    routed.design.rails = {
      rail: {
        id: "rail",
        net_id: "net",
        side: "component",
        color: "red",
        vertices: [
          { x2: 1, y2: 2 },
          { x2: 3, y2: 2 },
          { x2: 5, y2: 2 },
        ],
      },
    };
    const svg = renderBoardSvg(routed, "rail");
    expect(svg).toContain('class="wire rail selected" data-rail-id="rail"');
    expect(svg.match(/data-rail-node-index=/g)).toHaveLength(3);
    expect(svg).toContain('data-x2="3" data-y2="2" cx="1.5" cy="1"');
  });

  it("centers a wire label halfway along its total routed length", () => {
    const routed = structuredClone(fixture);
    routed.design.wires = {
      wire: {
        id: "wire",
        net_id: "net",
        start: { kind: "pin", component_id: "instance", pin_id: "1" },
        end: { kind: "pin", component_id: "instance", pin_id: "2" },
        side: "component",
        color: "green",
        vertices: [
          { x2: 0, y2: 0 },
          { x2: 8, y2: 0 },
          { x2: 8, y2: 2 },
        ],
      },
    };

    expect(renderBoardSvg(routed)).toContain(
      '<text class="wire-label readable-label" x="2.5" y="0">W1</text>',
    );
  });

  it("renders white wires as a subtle gray", () => {
    const routed = structuredClone(fixture);
    routed.design.wires = {
      wire: {
        id: "wire",
        net_id: "net",
        start: { kind: "pin", component_id: "instance", pin_id: "1" },
        end: { kind: "pin", component_id: "instance", pin_id: "2" },
        side: "component",
        color: "white",
        vertices: [
          { x2: 0, y2: 0 },
          { x2: 2, y2: 2 },
        ],
      },
    };

    expect(renderBoardSvg(routed)).toContain("--wire-color:#e6e6e6");
  });

  it("renders a selection halo for only the selected wire", () => {
    const routed = structuredClone(fixture);
    const wire = {
      id: "wire-one",
      net_id: "net",
      start: { kind: "pin" as const, component_id: "instance", pin_id: "1" },
      end: { kind: "pin" as const, component_id: "instance", pin_id: "2" },
      side: "component" as const,
      color: "green" as const,
      vertices: [
        { x2: 0, y2: 0 },
        { x2: 2, y2: 2 },
      ],
    };
    routed.design.wires = {
      "wire-one": wire,
      "wire-two": { ...wire, id: "wire-two", color: "red" },
    };

    const svg = renderBoardSvg(routed, "wire-two");
    expect(svg.match(/class="wire-selection"/g)).toHaveLength(1);
    expect(svg).toContain('class="wire selected" data-wire-id="wire-two"');
    expect(svg).toContain('class="wire" data-wire-id="wire-one"');
    expect(svg).toContain('data-wire-segment="0"');
    expect(svg).toContain("Long-press to add a wire node");
  });

  it("renders draggable handles for selected wire bend nodes", () => {
    const routed = structuredClone(fixture);
    routed.design.wires = {
      wire: {
        id: "wire",
        net_id: "net",
        start: { kind: "pin", component_id: "instance", pin_id: "1" },
        end: { kind: "pin", component_id: "instance", pin_id: "2" },
        side: "component",
        color: "green",
        vertices: [
          { x2: 0, y2: 0 },
          { x2: 3, y2: 1 },
          { x2: 6, y2: 4 },
        ],
      },
    };
    const svg = renderBoardSvg(routed, "wire");
    expect(svg).toContain('data-wire-endpoint="start"');
    expect(svg).toContain('data-wire-endpoint="end"');
    expect(svg).toContain('data-wire-vertex-index="1"');
    expect(svg).toContain('cx="1.5" cy="0.5"');
    expect(svg.match(/data-wire-segment=/g)).toHaveLength(2);
  });

  it("only marks objects selected when viewing their own board side", () => {
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

    expect(
      renderBoardSvg(routed, "wire", undefined, "component"),
    ).not.toContain("wire-selection");
    expect(renderBoardSvg(routed, "wire", undefined, "solder")).toContain(
      "wire-selection",
    );
    expect(
      renderBoardSvg(routed, "instance", undefined, "solder"),
    ).not.toContain('class="component selected"');
    expect(
      renderBoardSvg(routed, "instance", undefined, "component"),
    ).toContain('class="component selected"');
  });

  it("paints selectable objects above opposite-side objects", () => {
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

    const solder = renderBoardSvg(routed, undefined, undefined, "solder");
    expect(solder.indexOf('class="component opposite-side"')).toBeLessThan(
      solder.indexOf('<g class="wire"'),
    );

    const component = renderBoardSvg(routed, undefined, undefined, "component");
    expect(component.indexOf('<g class="wires">')).toBeLessThan(
      component.indexOf('<g class="components">'),
    );
  });

  it("keeps solder-side bridges above and independently selectable from component bodies", () => {
    const overlapped = structuredClone(fixture);
    overlapped.design.component_definitions!.bridge = {
      ...structuredClone(overlapped.design.component_definitions!.part!),
      id: "bridge",
      kind: "bridge",
      pins: [
        { id: "a", number: "A", name: "End A", x: 0, y: 0 },
        { id: "b", number: "B", name: "End B", x: 1, y: 0 },
      ],
    };
    overlapped.design.components!.bridge = {
      ...overlapped.design.components!.instance!,
      id: "bridge",
      definition_id: "bridge",
    };

    const svg = renderBoardSvg(overlapped, undefined, undefined, "solder");
    expect(svg).toContain('class="component opposite-side"');
    expect(svg).toContain('class="component bridge"');
    expect(svg.indexOf('class="component opposite-side"')).toBeLessThan(
      svg.indexOf('class="component bridge"'),
    );
  });

  it("uses the same renderer for named comparison panes and change highlights", () => {
    const svg = renderBoardSvg(
      fixture,
      undefined,
      undefined,
      "component",
      { instance: "moved" },
      "committed-board",
    );
    expect(svg).toContain('id="committed-board"');
    expect(svg).toContain('class="component diff-moved"');
  });

  it("renders edge and corner handles only when resize mode is active", () => {
    expect(renderBoardSvg(fixture)).not.toContain("data-resize-handle");
    const svg = renderBoardSvg(
      fixture,
      undefined,
      undefined,
      "component",
      {},
      "board",
      true,
    );
    expect(svg.match(/data-resize-handle=/g)).toHaveLength(8);
    expect(svg).toContain('data-resize-handle="bottom-right"');
    expect(svg).toContain('class="board-resize-boundary"');
    expect(svg).toContain(
      'class="board-resize-feedback" x="1.5" y="1" text-anchor="middle" aria-live="polite">2 full holes wide × 1 tall</text>',
    );
  });

  it("renders a builder-focused print layout with both board faces", () => {
    const routed = structuredClone(fixture);
    routed.name = "Fixture & build";
    routed.design.wires = {
      top: {
        id: "top",
        net_id: "net",
        start: { kind: "pin", component_id: "instance", pin_id: "1" },
        end: { kind: "pin", component_id: "instance", pin_id: "1" },
        side: "component",
        color: "red",
        vertices: [
          { x2: 0, y2: 0 },
          { x2: 2, y2: 0 },
        ],
      },
      white: {
        id: "white",
        net_id: "net",
        start: { kind: "pin", component_id: "instance", pin_id: "1" },
        end: { kind: "pin", component_id: "instance", pin_id: "1" },
        side: "solder",
        color: "white",
        vertices: [
          { x2: 0, y2: 2 },
          { x2: 2, y2: 2 },
        ],
      },
    };
    const html = renderPrintLayout(routed);
    expect(html).toContain("Fixture &amp; build");
    expect(html).toContain("Component side");
    expect(html).toContain("Solder side");
    expect(html).toContain('id="print-component-board"');
    expect(html).toContain('id="print-solder-board"');
    expect(html.match(/class="solder-point"/g)).toHaveLength(2);
    expect(html).toContain('transform="translate(3 0) scale(-1 1)"');
    expect(html).toContain("Solder joint / component pin");
    expect(html).toContain("Bridge · install last");
    expect(html).toContain("--wire-color:red");
    expect(html).toContain("--wire-color:#e6e6e6");
    expect(html).not.toContain("--wire-color:white");
  });
});
