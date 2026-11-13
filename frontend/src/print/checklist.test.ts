import { describe, expect, it } from "vitest";
import type { AssemblyPlan, AssemblyStep, WireListEntry } from "../api/project";
import { checklistRows, wireListInBuildOrder } from "./checklist";

function step(number: number, id: string, after: string[] = []): AssemblyStep {
  return {
    id,
    number,
    kind: "wire",
    label: id,
    title: id,
    side: "component",
    objects: [],
    holes: [],
    instructions: [],
    after: after.map((step_id) => ({ step_id, reason: "" })),
    fingerprint: `f-${id}`,
  };
}

function entry(label: string, stepNumber: number): WireListEntry {
  return {
    kind: label.startsWith("R") ? "rail" : "wire",
    id: label,
    label,
    from: "",
    to: "",
    net: "",
    color: "red",
    side: "component",
    path_mm: 10,
    cut_mm: 20,
    bend_count: 0,
    step: stepNumber,
  };
}

const plan: AssemblyPlan = {
  source_revision: 1,
  fingerprint: "p",
  steps: [step(1, "a"), step(2, "b"), step(3, "c", ["b", "a"])],
  wire_list: [entry("W1", 3), entry("W2", 1), entry("R1", 2)],
  warnings: [],
};

describe("printed plan", () => {
  it("lists wires and rails in the order they go in", () => {
    expect(wireListInBuildOrder(plan).map((item) => item.label)).toEqual([
      "W2",
      "R1",
      "W1",
    ]);
  });

  it("numbers dependencies and keeps only unchanged ticks", () => {
    const rows = checklistRows(plan, { a: "f-a", b: "stale" });
    expect(rows.map((row) => row.done)).toEqual([true, false, false]);
    expect(rows[2]!.after).toEqual([1, 2]);
  });
});
