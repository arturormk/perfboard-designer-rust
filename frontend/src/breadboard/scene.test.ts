import { describe, expect, it } from "vitest";
import type { BreadboardJumper, BreadboardLayout } from "../api/project";
import { mockBreadboardLayout } from "../dev/mock-breadboard";
import { testDesign } from "../model/test-design";
import {
  boardMarkings,
  breadboardDesign,
  holeLabel,
  jumperGeometry,
  jumpersByNet,
  layoutBox,
  partWhereabouts,
  railLinks,
} from "./scene";

function layout(overrides: Partial<BreadboardLayout> = {}): BreadboardLayout {
  return { ...mockBreadboardLayout(testDesign(), 3), ...overrides };
}

function jumper(overrides: Partial<BreadboardJumper>): BreadboardJumper {
  return {
    id: "jumper:1",
    number: 1,
    kind: "jumper",
    net_id: "n1",
    net_name: "N1",
    color: "red",
    from: { x: 3, y: 4 },
    to: { x: 3, y: 10 },
    from_label: "a5",
    to_label: "a11",
    ...overrides,
  };
}

describe("boardMarkings", () => {
  it("draws every strip and rail hole of each board", () => {
    const { geometry } = layout();
    const one = boardMarkings(geometry, 1);
    // 63 rows × 10 strip holes, plus four rails of ten groups of five.
    expect(one.holes).toHaveLength(63 * 10 + 4 * 50);
    const rail = one.holes.filter((hole) => hole.x === 0).map((hole) => hole.y);
    expect(rail.slice(0, 6)).toEqual([2, 3, 4, 5, 6, 8]);
    expect(rail).not.toContain(7);
    expect(rail.at(-1)).toBe(60);
    const two = boardMarkings(geometry, 2);
    expect(two.holes).toHaveLength(2 * one.holes.length);
    expect(two.origins).toEqual([0, 19]);
    expect(two.holes.some((hole) => hole.x === 19 + 3)).toBe(true);
  });

  it("reads red +, blue − from left to right on both sides", () => {
    const { stripes } = boardMarkings(layout().geometry, 1);
    const byX = [...stripes].sort((a, b) => a.x - b.x);
    expect(byX.map((stripe) => stripe.polarity)).toEqual([
      "plus",
      "minus",
      "plus",
      "minus",
    ]);
    expect(byX[0]!.x).toBeLessThan(0);
    expect(byX[3]!.x).toBeGreaterThan(17);
  });

  it("numbers row 1 and every fifth row, and letters the columns", () => {
    const { rowNumbers, columnLetters } = boardMarkings(layout().geometry, 1);
    const rows = new Set(rowNumbers.map((label) => label.text));
    expect([...rows].slice(0, 3)).toEqual(["1", "5", "10"]);
    expect(rows.has("60")).toBe(true);
    // Each letter above and below its column.
    const top = columnLetters.filter((label) => label.y < 0);
    expect(top.map((label) => label.text).join("")).toBe("abcdefghij");
    expect(columnLetters).toHaveLength(20);
  });
});

describe("breadboardDesign", () => {
  it("moves parts to their breadboard places and drops perfboard wiring", () => {
    const design = testDesign();
    const current = layout();
    const scene = breadboardDesign(design, current);
    for (const part of current.parts) {
      expect(scene.components?.[part.component_id]?.position).toEqual(
        part.position,
      );
      expect(scene.components?.[part.component_id]?.rotation).toBe(
        part.rotation,
      );
    }
    expect(Object.keys(scene.wires ?? {})).toHaveLength(0);
    expect(scene.board.holes_x).toBe(current.geometry.width);
    expect(scene.board.holes_y).toBe(current.geometry.height);
    // The original design is untouched.
    expect(Object.keys(design.wires ?? {}).length).toBeGreaterThan(0);
  });
});

describe("jumperGeometry", () => {
  it("bows alternate jumpers to opposite sides", () => {
    const odd = jumperGeometry(jumper({ number: 1 }));
    const even = jumperGeometry(jumper({ number: 2 }));
    expect(odd.d.startsWith("M 3 4 Q")).toBe(true);
    expect(odd.d.endsWith("3 10")).toBe(true);
    expect(odd.mid.y).toBe(7);
    expect(Math.sign(odd.mid.x - 3)).toBe(-Math.sign(even.mid.x - 3));
  });

  it("copes with a jumper whose ends coincide", () => {
    const flat = jumperGeometry(jumper({ to: { x: 3, y: 4 } }));
    expect(flat.mid).toEqual({ x: 3, y: 4 });
  });
});

describe("jumpersByNet", () => {
  it("groups jumpers by net in layout order", () => {
    const groups = jumpersByNet(
      layout({
        jumpers: [
          jumper({ id: "jumper:1", net_id: "a", net_name: "A" }),
          jumper({ id: "jumper:2", net_id: "b", net_name: "B" }),
          jumper({ id: "jumper:3", net_id: "a", net_name: "A" }),
        ],
      }),
    );
    expect(groups.map((group) => group.name)).toEqual(["A", "B"]);
    expect(groups[0]!.jumpers.map((j) => j.id)).toEqual([
      "jumper:1",
      "jumper:3",
    ]);
  });
});

describe("layoutBox", () => {
  it("frames the whole layout with a margin", () => {
    const current = layout();
    const [x, y, width, height] = layoutBox(current);
    expect(x).toBeLessThan(0);
    expect(y).toBeLessThan(0);
    expect(width).toBeGreaterThan(current.geometry.width);
    expect(height).toBeGreaterThan(current.geometry.height);
  });
});

describe("holeLabel", () => {
  it("names strips by column letter and row, and rails by side and sign", () => {
    const one = layout();
    expect(holeLabel(one, { x: 3, y: 0 })).toBe("a1");
    expect(holeLabel(one, { x: 14, y: 11 })).toBe("j12");
    expect(holeLabel(one, { x: 0, y: 6 })).toBe("left + rail, row 7");
    expect(holeLabel(one, { x: 17, y: 6 })).toBe("right − rail, row 7");
    expect(holeLabel(one, { x: 40, y: 3 })).toBe("beside the boards");
  });

  it("prefixes the board when there are several", () => {
    const two = layout({ board_count: 2 });
    expect(holeLabel(two, { x: 19 + 7, y: 4 })).toBe("B2 e5");
  });
});

describe("partWhereabouts", () => {
  const part = layout().parts[0]!;
  const holes = [
    { x: 7, y: 2 },
    { x: 10, y: 16 },
  ];

  it("says how a part sits and which rows it takes", () => {
    const one = layout();
    expect(
      partWhereabouts(one, { ...part, placement: "straddle" }, holes),
    ).toBe("Straddles the channel · rows 3–17");
    expect(
      partWhereabouts(one, { ...part, placement: "left" }, [{ x: 7, y: 4 }]),
    ).toBe("Columns a–e · row 5");
    expect(
      partWhereabouts(
        layout({ board_count: 2 }),
        {
          ...part,
          placement: "right",
          board: 1,
        },
        holes,
      ),
    ).toBe("Board 2 · Columns f–j · rows 3–17");
  });

  it("points off-board parts to their leads", () => {
    expect(
      partWhereabouts(layout(), { ...part, placement: "off_board" }, holes),
    ).toContain("Dupont leads");
  });
});

describe("railLinks", () => {
  it("finds jumpers from one rail to another, on any board", () => {
    const links = railLinks(
      layout({
        board_count: 2,
        jumpers: [
          jumper({ id: "strip", from: { x: 3, y: 20 }, to: { x: 1, y: 20 } }),
          jumper({ id: "link", from: { x: 1, y: 41 }, to: { x: 17, y: 41 } }),
          jumper({
            id: "across",
            from: { x: 17, y: 42 },
            to: { x: 19 + 1, y: 42 },
          }),
          jumper({
            id: "lead",
            kind: "lead",
            from: { x: 0, y: 5 },
            to: { x: 16, y: 5 },
          }),
        ],
      }),
    );
    expect(links.map((link) => link.id)).toEqual(["link", "across"]);
  });
});
