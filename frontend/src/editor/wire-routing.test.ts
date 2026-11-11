import { describe, expect, it } from "vitest";
import {
  appendOrthogonal,
  closestWireSegmentIndex,
  finishWireNodeDrag,
  insertWireNode,
  isStraightRoute,
  orthogonalRoute,
  snappedRoutePoint,
  straightRoute,
} from "./wire-routing";

describe("wire route shapes", () => {
  it("converts a straight wire to a route with a 90-degree bend", () => {
    expect(
      orthogonalRoute([
        { x2: 2, y2: 4 },
        { x2: 10, y2: 12 },
      ]),
    ).toEqual([
      { x2: 2, y2: 4 },
      { x2: 10, y2: 4 },
      { x2: 10, y2: 12 },
    ]);
  });

  it("converts a bent wire to a single straight segment", () => {
    expect(
      straightRoute([
        { x2: 2, y2: 4 },
        { x2: 10, y2: 4 },
        { x2: 10, y2: 12 },
      ]),
    ).toEqual([
      { x2: 2, y2: 4 },
      { x2: 10, y2: 12 },
    ]);
  });

  it("recognizes redundant collinear vertices as straight", () => {
    expect(
      isStraightRoute([
        { x2: 0, y2: 0 },
        { x2: 3, y2: 2 },
        { x2: 6, y2: 4 },
      ]),
    ).toBe(true);
  });

  it("adds only horizontal and vertical segments while drawing", () => {
    expect(appendOrthogonal([{ x2: 2, y2: 4 }], { x2: 10, y2: 12 })).toEqual([
      { x2: 2, y2: 4 },
      { x2: 10, y2: 4 },
      { x2: 10, y2: 12 },
    ]);
  });

  it("snaps dragged nodes to the bounded half-hole grid", () => {
    expect(
      snappedRoutePoint({ x: 10.31, y: -0.3 }, { maxX2: 22, maxY2: 16 }),
    ).toEqual({ x2: 21, y2: 0 });
  });

  it("inserts the closest on-segment half-grid node", () => {
    expect(
      insertWireNode(
        [
          { x2: 0, y2: 0 },
          { x2: 8, y2: 4 },
        ],
        0,
        { x: 2.1, y: 1.1 },
      ),
    ).toEqual([
      { x2: 0, y2: 0 },
      { x2: 4, y2: 2 },
      { x2: 8, y2: 4 },
    ]);
  });

  it("inserts a minimal bend in a diagonal with no on-segment grid point", () => {
    expect(
      insertWireNode(
        [
          { x2: 24, y2: 30 },
          { x2: 25, y2: 31 },
        ],
        0,
        { x: 12.25, y: 15.25 },
      ),
    ).toEqual([
      { x2: 24, y2: 30 },
      { x2: 24, y2: 31 },
      { x2: 25, y2: 31 },
    ]);
  });

  it("finds the nearest segment of a bent wire", () => {
    const vertices = [
      { x2: 0, y2: 0 },
      { x2: 8, y2: 0 },
      { x2: 8, y2: 8 },
    ];

    expect(closestWireSegmentIndex(vertices, { x: 2, y: 0.1 })).toBe(0);
    expect(closestWireSegmentIndex(vertices, { x: 3.9, y: 3 })).toBe(1);
  });

  it("does not insert a node when a segment has no interior grid point", () => {
    const vertices = [
      { x2: 0, y2: 0 },
      { x2: 1, y2: 0 },
    ];
    expect(insertWireNode(vertices, 0, { x: 0.25, y: 0 })).toBe(vertices);
  });

  it("moves a node or removes it when dropped over any other vertex", () => {
    const vertices = [
      { x2: 0, y2: 0 },
      { x2: 2, y2: 2 },
      { x2: 4, y2: 4 },
      { x2: 6, y2: 0 },
    ];
    expect(finishWireNodeDrag(vertices, 1, { x2: 3, y2: 2 })).toEqual({
      vertices: [
        { x2: 0, y2: 0 },
        { x2: 3, y2: 2 },
        { x2: 4, y2: 4 },
        { x2: 6, y2: 0 },
      ],
      deleted: false,
    });
    expect(finishWireNodeDrag(vertices, 1, { x2: 6, y2: 0 })).toEqual({
      vertices: [
        { x2: 0, y2: 0 },
        { x2: 4, y2: 4 },
        { x2: 6, y2: 0 },
      ],
      deleted: true,
    });
  });
});
