import { describe, expect, it } from "vitest";
import { changedObjectIds, computeAirwires } from "./ratsnest";
import { testDesign } from "./test-design";

describe("airwires", () => {
  it("joins islands with the shortest pin pairs", () => {
    const design = testDesign();
    const airwires = computeAirwires(design, [
      {
        net_id: "gnd",
        islands: [
          [
            { component_id: "r1", pin_id: "a" },
            { component_id: "r2", pin_id: "a" },
          ],
          [{ component_id: "r10", pin_id: "b" }],
        ],
      },
    ]);
    expect(airwires).toHaveLength(1);
    expect(airwires[0]!.from).toEqual({ component_id: "r1", pin_id: "a" });
    expect(airwires[0]!.b).toEqual({ x: 8, y: 1 });
  });

  it("draws n-1 hints for n islands and nothing for a joined net", () => {
    const design = testDesign();
    const pin = (component_id: string, pin_id: string) => [
      { component_id, pin_id },
    ];
    expect(
      computeAirwires(design, [
        {
          net_id: "gnd",
          islands: [pin("r1", "a"), pin("r2", "a"), pin("r10", "b")],
        },
      ]),
    ).toHaveLength(2);
    expect(
      computeAirwires(design, [{ net_id: "gnd", islands: [pin("r1", "a")] }]),
    ).toHaveLength(0);
  });

  it("ignores pins that no longer exist", () => {
    const airwires = computeAirwires(testDesign(), [
      {
        net_id: "gnd",
        islands: [
          [{ component_id: "r1", pin_id: "a" }],
          [{ component_id: "gone", pin_id: "a" }],
        ],
      },
    ]);
    expect(airwires).toHaveLength(0);
  });
});

describe("changed objects", () => {
  it("reports added, removed and modified ids", () => {
    const before = testDesign();
    const after = testDesign();
    after.components!.r1!.position = { x: 2, y: 1 };
    delete after.components!.r10;
    const changed = changedObjectIds(before, after);
    expect(changed.has("r1")).toBe(true);
    expect(changed.has("r10")).toBe(true);
    expect(changed.has("r2")).toBe(false);
  });
});
