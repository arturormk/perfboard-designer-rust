import { describe, expect, it } from "vitest";
import type { Rail, Wire } from "../api/project";
import type { Design, RoutePoint } from "../model/geometry";
import {
  bendPositions,
  cutColumns,
  describeSpacing,
  groupWireTemplates,
  layoutTemplate,
  LEAD_MM,
  mergeWindows,
  RAIL_WINDOW_MM,
  railConnections,
  railEnds,
  railPieces,
  shapeKey,
  simplifyPolyline,
  templateSegments,
} from "./wire-templates";

const p = (x: number, y: number): RoutePoint => ({ x2: x * 2, y2: y * 2 });

function wire(
  id: string,
  color: Wire["color"],
  vertices: RoutePoint[],
  side: Wire["side"] = "component",
): Wire {
  return {
    id,
    net_id: "n",
    start: { kind: "junction" } as unknown as Wire["start"],
    end: { kind: "junction" } as unknown as Wire["end"],
    side,
    color,
    vertices,
  };
}

function designWith(wires: Wire[]): Design {
  return {
    board: { pitch_mm: 2.54, holes_x: 25, holes_y: 18 },
    wires: Object.fromEntries(wires.map((item) => [item.id, item])),
  } as unknown as Design;
}

describe("simplifyPolyline", () => {
  it("drops duplicates and points on a straight run", () => {
    expect(
      simplifyPolyline([p(0, 0), p(0, 0), p(2, 0), p(5, 0), p(5, 3)]),
    ).toEqual([p(0, 0), p(5, 0), p(5, 3)]);
  });

  it("keeps a point where the wire doubles back", () => {
    expect(simplifyPolyline([p(0, 0), p(4, 0), p(2, 0)])).toHaveLength(3);
  });
});

describe("shapeKey", () => {
  const u = [p(0, 0), p(0, -6), p(12, -6), p(12, 0)];

  it("ignores position, direction, rotation and turning over", () => {
    const key = shapeKey(u);
    const moved = u.map((point) => ({ x2: point.x2 + 7, y2: point.y2 + 3 }));
    const reversed = [...u].reverse();
    const rotated = u.map((point) => ({ x2: -point.y2, y2: point.x2 }));
    const mirrored = u.map((point) => ({ x2: -point.x2, y2: point.y2 }));
    for (const other of [moved, reversed, rotated, mirrored])
      expect(shapeKey(other)).toBe(key);
  });

  it("tells different shapes apart", () => {
    expect(shapeKey(u)).not.toBe(
      shapeKey([p(0, 0), p(0, -5), p(12, -5), p(12, 0)]),
    );
    expect(shapeKey([p(0, 0), p(3, 0), p(3, 2)])).not.toBe(
      shapeKey([p(0, 0), p(3, 0), p(3, 2), p(5, 2)]),
    );
  });
});

describe("templateSegments", () => {
  it("measures runs in pitches and millimetres", () => {
    const segments = templateSegments([p(0, 0), p(12, 0), p(15, 3)], 2.54);
    expect(segments[0]).toMatchObject({ dx: 12, dy: 0 });
    expect(segments[0]!.lengthMm).toBeCloseTo(30.48);
    expect(segments[1]!.lengthMm).toBeCloseTo(3 * Math.SQRT2 * 2.54);
    expect(describeSpacing(segments[0]!)).toBe("12 pitches");
    expect(describeSpacing(segments[1]!)).toBe("3 × 3 pitches diagonal");
    expect(describeSpacing({ dx: 0, dy: -1, lengthMm: 2.54 })).toBe("1 pitch");
  });
});

describe("groupWireTemplates", () => {
  it("groups by colour, conventional colours first, then by shape", () => {
    const labels = new Map([
      ["a", "W1"],
      ["b", "W2"],
      ["c", "W3"],
      ["d", "W4"],
    ]);
    const groups = groupWireTemplates(
      designWith([
        wire("a", "white", [p(0, 0), p(4, 0)]),
        wire("b", "red", [p(0, 0), p(0, 3), p(6, 3), p(6, 0)]),
        wire("c", "red", [p(9, 9), p(9, 6), p(15, 6), p(15, 9)], "solder"),
        wire("d", "red", [p(1, 1), p(5, 1)]),
      ]),
      labels,
    );
    expect(groups.map((group) => group.name)).toEqual(["red", "white"]);
    const red = groups[0]!;
    expect(red.wireCount).toBe(3);
    expect(
      red.shapes.map((shape) => shape.wires.map((item) => item.id)),
    ).toEqual([["b", "c"], ["d"]]);
    const u = red.shapes[0]!;
    expect(u.bends).toBe(2);
    expect(u.pathMm).toBeCloseTo(12 * 2.54);
    expect(u.cutMm).toBeCloseTo(12 * 2.54 + 2 * LEAD_MM);
    expect(red.totalCutMm).toBeCloseTo(2 * u.cutMm + 4 * 2.54 + 2 * LEAD_MM);
  });

  it("draws a solder-side wire mirrored, as seen from that side", () => {
    const groups = groupWireTemplates(
      designWith([wire("a", "red", [p(1, 0), p(1, 2), p(4, 2)], "solder")]),
      new Map(),
    );
    expect(groups[0]!.shapes[0]!.points).toEqual([
      { x2: -2, y2: 0 },
      { x2: -2, y2: 4 },
      { x2: -8, y2: 4 },
    ]);
  });

  it("skips wires without a usable path", () => {
    expect(
      groupWireTemplates(designWith([wire("a", "red", [p(1, 1)])]), new Map()),
    ).toEqual([]);
  });
});

describe("layoutTemplate", () => {
  it("lays the wire out in millimetres with leads and dimensions", () => {
    const layout = layoutTemplate([p(0, 0), p(12, 0)], 2.54);
    const [a, b] = layout.path;
    expect(b!.x - a!.x).toBeCloseTo(30.48);
    // Leads extend straight out from each end.
    expect(layout.leads[0]![1].x).toBeCloseTo(a!.x - LEAD_MM);
    expect(layout.leads[1]![1].x).toBeCloseTo(b!.x + LEAD_MM);
    expect(layout.dimensions).toHaveLength(1);
    expect(layout.dimensions[0]!.length).toBe("30.48 mm");
    expect(layout.dimensions[0]!.spacing).toBe("12 pitches");
    // A straight wire is dimensioned above itself.
    expect(layout.dimensions[0]!.from.y).toBeLessThan(a!.y);
    // Everything fits inside the drawing.
    for (const point of [...layout.path, ...layout.leads.flat()]) {
      expect(point.x).toBeGreaterThan(0);
      expect(point.x).toBeLessThan(layout.width);
      expect(point.y).toBeGreaterThan(0);
      expect(point.y).toBeLessThan(layout.height);
    }
  });

  it("puts reference holes on the board grid, including the end holes", () => {
    const layout = layoutTemplate([p(0, 0), p(0, 3), p(4, 3)], 2.54);
    const has = (target: { x: number; y: number }) =>
      layout.holes.some(
        (hole) =>
          Math.abs(hole.x - target.x) < 1e-6 &&
          Math.abs(hole.y - target.y) < 1e-6,
      );
    for (const point of layout.path) expect(has(point)).toBe(true);
    // One pitch of holes around the wire: 7 columns × 6 rows.
    expect(layout.holes).toHaveLength(7 * 6);
  });

  it("keeps the grid aligned for mirrored, negative coordinates", () => {
    const layout = layoutTemplate([p(-1, 0), p(-4, 0)], 2.54);
    for (const point of layout.path)
      expect(
        layout.holes.some(
          (hole) =>
            Math.abs(hole.x - point.x) < 1e-6 &&
            Math.abs(hole.y - point.y) < 1e-6,
        ),
      ).toBe(true);
  });

  it("dimensions an L-shape on its outside", () => {
    const layout = layoutTemplate([p(0, 0), p(0, 4), p(4, 4)], 2.54);
    const [down, across] = layout.dimensions;
    // The vertical run is dimensioned to its left, the horizontal one below.
    expect(down!.from.x).toBeLessThan(layout.path[0]!.x);
    expect(across!.from.y).toBeGreaterThan(layout.path[1]!.y);
  });
});

describe("layoutTemplate short runs", () => {
  it("leaves half-pitch jogs unlabelled", () => {
    const layout = layoutTemplate([p(0, 0), p(0.5, 0.5), p(6.5, 0.5)], 2.54);
    expect(layout.dimensions.map((mark) => mark.spacing)).toEqual([
      "6 pitches",
    ]);
  });
});

describe("cutColumns", () => {
  const labels = new Map([
    ["a", "W1"],
    ["b", "W2"],
    ["c", "W10"],
  ]);
  const design = designWith([
    wire("c", "red", [p(0, 0), p(4, 0)]),
    wire("a", "red", [p(0, 0), p(0, 3), p(6, 3), p(6, 0)]),
    wire("b", "red", [p(9, 9), p(13, 9)]),
  ]);

  it("gives every wire its own column, in label order within a colour", () => {
    const columns = cutColumns(
      groupWireTemplates(design, labels),
      labels,
      160,
      2.54,
    );
    expect(columns.map((column) => column.id)).toEqual(["a", "b", "c"]);
    expect(columns[0]).toMatchObject({ fromMm: 0, part: 1, parts: 1 });
    expect(columns[0]!.toMm).toBeCloseTo(12 * 2.54 + 2 * LEAD_MM);
  });

  it("continues long wires in the next column at a whole centimetre", () => {
    const long = designWith([wire("a", "red", [p(0, 0), p(40, 0)])]);
    const columns = cutColumns(
      groupWireTemplates(long, labels),
      labels,
      45,
      2.54,
    );
    // 101.6 mm path + 10 mm leads = 111.6 mm, in 40 mm columns.
    expect(columns.map((column) => [column.fromMm, column.part])).toEqual([
      [0, 1],
      [40, 2],
      [80, 3],
    ]);
    expect(columns[2]!.toMm).toBeCloseTo(111.6);
    expect(columns.every((column) => column.parts === 3)).toBe(true);
  });
});

describe("bendPositions", () => {
  it("measures each bend along the path from the start", () => {
    // 3 up, 6 across, 3 down: two 90° bends at 3 and 9 pitches.
    const bends = bendPositions([p(0, 0), p(0, -3), p(6, -3), p(6, 0)], 2.54);
    expect(bends.map((bend) => bend.angle)).toEqual([90, 90]);
    expect(bends[0]!.alongMm).toBeCloseTo(3 * 2.54);
    expect(bends[1]!.alongMm).toBeCloseTo(9 * 2.54);
  });

  it("reports 45° and 135° turns and ignores straight-through points", () => {
    const bends = bendPositions(
      [p(0, 0), p(4, 0), p(8, 0), p(10, 2), p(8, 4)],
      2.54,
    );
    expect(bends.map((bend) => bend.angle)).toEqual([45, 90]);
    expect(bends[0]!.alongMm).toBeCloseTo(8 * 2.54);
    expect(
      bendPositions([p(0, 0), p(4, 0), p(1, 3)], 2.54).map((b) => b.angle),
    ).toEqual([135]);
  });

  it("is placed in cut columns after the top lead", () => {
    const labels = new Map([["a", "W1"]]);
    const design = designWith([
      wire("a", "red", [p(0, 0), p(0, 3), p(6, 3), p(6, 0)]),
    ]);
    const [column] = cutColumns(
      groupWireTemplates(design, labels),
      labels,
      160,
      2.54,
    );
    expect(column!.bends[0]!.alongMm).toBeCloseTo(LEAD_MM + 3 * 2.54);
  });
});

function rail(
  id: string,
  vertices: RoutePoint[],
  side: Rail["side"] = "solder",
): Rail {
  return { id, net_id: "n", side, color: "black", vertices };
}

function designWithRails(rails: Rail[], wires: Wire[] = []): Design {
  return {
    ...designWith(wires),
    rails: Object.fromEntries(rails.map((item) => [item.id, item])),
  } as unknown as Design;
}

describe("rails", () => {
  // The sample project's GND rail: straight, with three taps.
  const straight = rail("r", [
    p(19, 25),
    p(19, 12),
    p(19, 3),
    p(19, 1),
    p(19, 0),
  ]);
  // An L with a tap on each run and a bare corner.
  const bent = rail("l", [p(0, 0), p(4, 0), p(10, 0), p(10, 3), p(10, 6)]);
  // Ends between holes: they cannot go into one.
  const offGrid = rail("h", [p(0.5, 0), p(3, 0), p(6.5, 0)]);
  const half = RAIL_WINDOW_MM / 2;

  it("windows taps, but not ends on holes or a corner with nothing on it", () => {
    const design = designWithRails([straight, bent]);
    expect(railConnections(design, straight)).toEqual([1, 2, 3]);
    expect(railConnections(design, bent)).toEqual([1, 3]);
  });

  it("windows a corner that a wire is attached to", () => {
    const tapped = {
      ...wire("w", "red", [p(10, 0), p(10, -4)]),
      start: { kind: "rail_node", rail_id: "l", position: p(10, 0) },
    } as unknown as Wire;
    expect(railConnections(designWithRails([bent], [tapped]), bent)).toEqual([
      1, 2, 3,
    ]);
  });

  it("puts rail ends on holes into them on wire leads", () => {
    const [piece] = railPieces(
      designWithRails([straight]),
      new Map([["r", "R1"]]),
    );
    expect(piece!.ends).toEqual([
      { leadMm: LEAD_MM, intoHole: true },
      { leadMm: LEAD_MM, intoHole: true },
    ]);
    expect(piece!.pathMm).toBeCloseTo(25 * 2.54);
    expect(piece!.cutMm).toBeCloseTo(25 * 2.54 + 2 * LEAD_MM);
    expect(piece!.bends).toEqual([]);
    // Taps at 13, 22 and 24 pitches from the start, after the top lead.
    expect(piece!.windows.length).toBe(3);
    const at = (pitches: number) => LEAD_MM + pitches * 2.54;
    piece!.windows.forEach((window, index) => {
      expect(window.fromMm).toBeCloseTo(at([13, 22, 24][index]!) - half);
      expect(window.toMm).toBeCloseTo(at([13, 22, 24][index]!) + half);
    });
    expect(piece!.points[0]).toEqual({ x2: -38, y2: 50 });
  });

  it("gives an end between holes a bare tip with a centred window", () => {
    const design = designWithRails([offGrid]);
    expect(railConnections(design, offGrid)).toEqual([0, 1, 2]);
    const [piece] = railPieces(design, new Map());
    expect(piece!.ends).toEqual([
      { leadMm: half, intoHole: false },
      { leadMm: half, intoHole: false },
    ]);
    expect(piece!.cutMm).toBeCloseTo(6 * 2.54 + RAIL_WINDOW_MM);
    expect(piece!.windows[0]).toEqual({ fromMm: 0, toMm: RAIL_WINDOW_MM });
  });

  it("places bends after the top lead, like the windows", () => {
    const [piece] = railPieces(designWithRails([bent]), new Map());
    expect(piece!.bends.map((bend) => bend.angle)).toEqual([90]);
    expect(piece!.bends[0]!.alongMm).toBeCloseTo(10 * 2.54 + LEAD_MM);
  });

  it("gives rails their own cut columns after the wires", () => {
    const design = designWithRails(
      [bent],
      [wire("a", "red", [p(0, 0), p(4, 0)])],
    );
    const labels = new Map([
      ["a", "W1"],
      ["l", "R1"],
    ]);
    const columns = cutColumns(
      groupWireTemplates(design, labels),
      labels,
      160,
      2.54,
      railPieces(design, labels),
    );
    expect(columns.map((column) => column.id)).toEqual(["a", "l"]);
    expect(columns[1]!.ends[0]).toEqual({ leadMm: LEAD_MM, intoHole: true });
    expect(columns[1]!.windows.length).toBe(2);
  });

  it("draws windows and rings the holes rail leads go into", () => {
    const layout = layoutTemplate(bent.vertices, 2.54, LEAD_MM, {
      connections: [1, 3],
      windowMm: 3,
      ends: [
        { leadMm: LEAD_MM, intoHole: true },
        { leadMm: half, intoHole: false },
      ],
    });
    expect(layout.bendHoles.length).toBe(1);
    expect(layout.windows.length).toBe(2);
    const [startLead, endLead] = layout.leads.map(([a, b]) =>
      Math.hypot(b.x - a.x, b.y - a.y),
    );
    expect(startLead).toBeCloseTo(LEAD_MM);
    expect(endLead).toBeCloseTo(half);
    // A tap's window is 3 mm along the path, centred on the tap.
    const window = layout.windows[0]!;
    const length = window
      .slice(1)
      .reduce(
        (total, point, index) =>
          total +
          Math.hypot(point.x - window[index]!.x, point.y - window[index]!.y),
        0,
      );
    expect(length).toBeCloseTo(3);
    // Each run between nodes is dimensioned: 4 + 6 across, 3 + 3 down.
    expect(layout.dimensions.map((mark) => mark.spacing)).toEqual([
      "4 pitches",
      "6 pitches",
      "3 pitches",
      "3 pitches",
    ]);
  });

  it("staggers consecutive short dimensions so labels do not collide", () => {
    const ends = railEnds(straight);
    const layout = layoutTemplate(straight.vertices, 2.54, LEAD_MM, {
      connections: [1, 2, 3],
      windowMm: 3,
      ends,
    });
    const offsets = layout.dimensions.map((mark) =>
      Math.hypot(
        mark.from.x - mark.witness[0]![0].x,
        mark.from.y - mark.witness[0]![0].y,
      ),
    );
    // 13 and 9 pitches fit their labels; 2 and 1 pitches do not.
    expect(offsets[0]).toBeCloseTo(offsets[1]!);
    expect(offsets[3]!).toBeGreaterThan(offsets[2]!);
  });

  it("merges overlapping windows", () => {
    expect(
      mergeWindows([
        { fromMm: 5, toMm: 8 },
        { fromMm: 0, toMm: 3 },
        { fromMm: 7, toMm: 10 },
      ]),
    ).toEqual([
      { fromMm: 0, toMm: 3 },
      { fromMm: 5, toMm: 10 },
    ]);
  });
});
