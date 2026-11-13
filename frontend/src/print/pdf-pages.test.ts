import { describe, expect, it } from "vitest";
import type { Wire } from "../api/project";
import type { Design, RoutePoint } from "../model/geometry";
import {
  A4,
  buildWirePages,
  escapeXml,
  LETTER,
  paperForLocale,
  templateCard,
  textWidth,
  wrapText,
} from "./pdf-pages";
import { groupWireTemplates } from "./wire-templates";

const p = (x: number, y: number): RoutePoint => ({ x2: x * 2, y2: y * 2 });

function wire(id: string, vertices: RoutePoint[], color = "red"): Wire {
  return {
    id,
    net_id: "n1",
    start: { kind: "junction" } as unknown as Wire["start"],
    end: { kind: "junction" } as unknown as Wire["end"],
    side: "component",
    color: color as Wire["color"],
    vertices,
  };
}

function designWith(wires: Wire[], netName = "VCC"): Design {
  return {
    board: { name: "Test", pitch_mm: 2.54, holes_x: 25, holes_y: 18 },
    nets: { n1: { id: "n1", name: netName } },
    wires: Object.fromEntries(wires.map((item) => [item.id, item])),
  } as unknown as Design;
}

function labelsFor(design: Design): Map<string, string> {
  return new Map(
    Object.keys(design.wires ?? {}).map((id, index) => [id, `W${index + 1}`]),
  );
}

describe("paperForLocale", () => {
  it("uses Letter in the Americas that use it and A4 elsewhere", () => {
    expect(paperForLocale("en-US")).toBe(LETTER);
    expect(paperForLocale("es_MX")).toBe(LETTER);
    expect(paperForLocale("es-ES")).toBe(A4);
    expect(paperForLocale("de")).toBe(A4);
    expect(paperForLocale(undefined)).toBe(A4);
  });
});

describe("text helpers", () => {
  it("escapes XML special characters", () => {
    expect(escapeXml(`A<B & "C">`)).toBe("A&lt;B &amp; &quot;C&quot;&gt;");
  });

  it("wraps words to a width", () => {
    const lines = wrapText("one two three four five six", 20, 3);
    expect(lines.length).toBeGreaterThan(1);
    for (const content of lines)
      if (content.includes(" "))
        expect(textWidth(content, 3)).toBeLessThanOrEqual(20);
    expect(lines.join(" ")).toBe("one two three four five six");
  });
});

describe("buildWirePages", () => {
  const input = (design: Design) => ({
    design,
    labels: labelsFor(design),
    paper: A4,
    footer: "Test project · revision 3",
  });

  it("returns nothing without wires", () => {
    expect(buildWirePages(input(designWith([])))).toEqual([]);
  });

  it("builds complete A4 pages: the cut guide, then the templates", () => {
    const design = designWith([
      wire("a", [p(0, 0), p(0, 3), p(6, 3), p(6, 0)]),
      wire("b", [p(1, 1), p(5, 1)], "black"),
    ]);
    const pages = buildWirePages(input(design));
    expect(pages.length).toBe(2);
    for (const page of pages) {
      expect(page.startsWith('<svg xmlns="http://www.w3.org/2000/svg"')).toBe(
        true,
      );
      expect(page).toContain('width="210mm" height="297mm"');
      expect(page).toContain('viewBox="0 0 210 297"');
      expect(page).toContain("this bar must measure exactly 50 mm");
      expect(page).toContain("set Page Scaling to None");
      expect(page).toContain('fill="#d00000"');
      expect(page).toContain("Test project · revision 3");
      expect(page.trimEnd().endsWith("</svg>")).toBe(true);
    }
    expect(pages[0]).toContain("Wire cut guide");
    expect(pages[0]).toContain("Page 1 of 2");
    expect(pages[1]).toContain("Wire bending templates");
    for (const label of ["W1", "W2"]) {
      expect(pages[0]).toContain(`>${label}<`);
      expect(pages[1]).toContain(label);
    }
    // Every coloured wire is drawn in its colour.
    expect(pages[0]).toContain('stroke="#e5484d"');
    expect(pages[1]).toContain('stroke="#26282b"');
  });

  it("escapes names taken from the design", () => {
    const design = designWith([wire("a", [p(0, 0), p(4, 0)])], "A<B & C");
    const pages = buildWirePages(input(design));
    expect(pages.join("")).toContain("A&lt;B &amp; C");
    expect(pages.join("")).not.toContain("A<B");
  });

  it("continues a wire too long for one column", () => {
    const design = designWith([wire("a", [p(0, 0), p(0, 120)])]);
    const pages = buildWirePages(input(design));
    expect(pages[0]).toContain("W1 · 1/2");
    expect(pages[0]).toContain("W1 · 2/2");
  });
});

describe("templateCard", () => {
  it("turns a shape that is too wide for the paper a quarter", () => {
    const design = designWith([wire("a", [p(0, 0), p(80, 0), p(80, 2)])]);
    const labels = labelsFor(design);
    const shape = groupWireTemplates(design, labels)[0]!.shapes[0]!;
    const card = templateCard(shape, design, labels, 184);
    expect(card.width).toBeLessThan(80);
    expect(card.height).toBeGreaterThan(80 * 2.54);
  });

  it("keeps a shape that fits as it is on the board", () => {
    const design = designWith([wire("a", [p(0, 0), p(12, 0), p(12, 2)])]);
    const labels = labelsFor(design);
    const shape = groupWireTemplates(design, labels)[0]!.shapes[0]!;
    const card = templateCard(shape, design, labels, 184);
    expect(card.width).toBeGreaterThan(12 * 2.54);
    expect(card.width).toBeLessThanOrEqual(190);
  });
});
