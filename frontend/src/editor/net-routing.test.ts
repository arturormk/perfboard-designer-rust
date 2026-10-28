import { describe, expect, it } from "vitest";
import type { ProjectDocument, Wire } from "../api/project";
import { planWireReroutes } from "./net-routing";

const pin = (component_id: string) => ({ component_id, pin_id: "p" });
const terminal = (component_id: string) => ({
  kind: "pin" as const,
  component_id,
  pin_id: "p",
});

function fixture(locked = false): ProjectDocument {
  const makeWire = (id: string, start: string, end: string): Wire => ({
    id,
    net_id: "net",
    start: terminal(start),
    end: terminal(end),
    side: "solder",
    color: "green",
    vertices: [
      { x2: 0, y2: 0 },
      { x2: 2, y2: 0 },
    ],
    locked,
  });
  return {
    project_id: "project",
    name: "topology",
    revision: 0,
    design: {
      board: { name: "board", holes_x: 120, holes_y: 10, pitch_mm: 2.54 },
      component_definitions: {
        point: {
          schema_version: 1,
          id: "point",
          definition_version: "1",
          name: "Point",
          description: "",
          body: { x: 0, y: 0, width: 1, height: 1 },
          allowed_rotations: ["deg0"],
          pins: [{ id: "p", number: "1", name: "P", x: 0, y: 0 }],
        },
      },
      components: {
        A: {
          id: "A",
          definition_id: "point",
          name: "A",
          position: { x: 0, y: 0 },
          rotation: "deg0",
        },
        B: {
          id: "B",
          definition_id: "point",
          name: "B",
          position: { x: 100, y: 0 },
          rotation: "deg0",
        },
        C: {
          id: "C",
          definition_id: "point",
          name: "C",
          position: { x: 1, y: 0 },
          rotation: "deg0",
        },
      },
      nets: {
        net: {
          id: "net",
          name: "Net",
          state: "accepted",
          pins: [pin("A"), pin("B"), pin("C")],
        },
      },
      wires: {
        first: makeWire("first", "A", "B"),
        second: makeWire("second", "B", "C"),
      },
    },
  };
}

const topology = (project: ProjectDocument): string[] =>
  planWireReroutes(project)
    .map(({ start, end }) => {
      if (start.kind !== "pin" || end.kind !== "pin") return "junction";
      return [start.component_id, end.component_id].sort().join("-");
    })
    .sort();

describe("net reroute topology planning", () => {
  it("replaces a longer A-B, B-C tree with the shorter A-C, C-B tree", () => {
    expect(topology(fixture())).toEqual(["A-C", "B-C"]);
  });

  it("does not rewrite a topology containing locked wires", () => {
    expect(topology(fixture(true))).toEqual(["A-B", "B-C"]);
  });
});
