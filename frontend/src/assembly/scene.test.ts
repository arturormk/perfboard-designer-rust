import { describe, expect, it } from "vitest";
import type { AssemblyPlan, AssemblyStep, StepKind } from "../api/project";
import type { Design } from "../model/geometry";
import { comparePlans, firstOpenStep, statusOf } from "./plan";
import { assemblyScene } from "./scene";

function step(
  number: number,
  kind: StepKind,
  objectId?: string,
  fingerprint = `f${number}`,
): AssemblyStep {
  return {
    id: objectId ? `${kind}:${objectId}` : `check:${number}`,
    number,
    kind,
    label: objectId ?? "Check",
    title: `Step ${number}`,
    side: "component",
    objects: objectId ? [{ kind, id: objectId }] : [],
    holes: [],
    instructions: [],
    fingerprint,
  };
}

function planOf(steps: AssemblyStep[], fingerprint = "plan"): AssemblyPlan {
  return {
    source_revision: 1,
    fingerprint,
    steps,
    wire_list: [],
    warnings: [],
  };
}

const design = {
  board: { name: "b", holes_x: 10, holes_y: 10, pitch_mm: 2.54 },
  components: { c1: { id: "c1" }, c2: { id: "c2" } },
  wires: { w1: { id: "w1" }, w2: { id: "w2" } },
} as unknown as Design;

const plan = planOf([
  step(1, "wire", "w1"),
  step(2, "component", "c1"),
  step(3, "wire", "w2"),
  step(4, "component", "c2"),
  step(5, "check"),
]);

describe("assemblyScene", () => {
  it("shows the board as built up to the step", () => {
    const scene = assemblyScene(design, plan, plan.steps[1]);
    expect(Object.keys(scene.design.wires ?? {})).toEqual(["w1"]);
    expect(Object.keys(scene.design.components ?? {})).toEqual(["c1"]);
    expect([...scene.focusIds]).toEqual(["c1"]);
    expect([...scene.dimIds]).toEqual(["w1"]);
  });

  it("shows the finished board without fading on a check step", () => {
    const scene = assemblyScene(design, plan, plan.steps[4]);
    expect(scene.design).toBe(design);
    expect(scene.dimIds.size).toBe(0);
    expect(scene.focusIds.size).toBe(0);
  });
});

describe("checkmarks", () => {
  it("tells done from changed by fingerprint", () => {
    const wire = plan.steps[0]!;
    expect(statusOf(wire, {})).toBe("todo");
    expect(statusOf(wire, { [wire.id]: "f1" })).toBe("done");
    expect(statusOf(wire, { [wire.id]: "old" })).toBe("changed");
  });

  it("resumes at the first step not done", () => {
    const marks = { "wire:w1": "f1", "component:c1": "f2" };
    expect(firstOpenStep(plan, marks)?.id).toBe("wire:w2");
    const all = Object.fromEntries(
      plan.steps.map((item) => [item.id, item.fingerprint]),
    );
    expect(firstOpenStep(plan, all)?.id).toBe("check:5");
  });
});

describe("comparePlans", () => {
  it("is quiet when the plan is the same", () => {
    expect(comparePlans(plan, planOf(plan.steps), {})).toBeNull();
  });

  it("counts added, removed and changed steps and invalidated marks", () => {
    const next = planOf(
      [
        step(1, "wire", "w1", "f1-moved"),
        step(2, "component", "c1"),
        step(3, "component", "c2", "f4-rotated"),
        step(4, "wire", "w3", "new"),
        step(5, "check"),
      ],
      "plan-2",
    );
    expect(comparePlans(plan, next, { "wire:w1": "f1" })).toEqual({
      added: 1,
      removed: 1,
      changed: 2,
      invalidated: 1,
    });
  });
});

describe("a picked measurement", () => {
  it("lights the wiring of its nets and fades the rest", () => {
    const wired = {
      ...design,
      wires: {
        w1: { id: "w1", net_id: "gnd" },
        w2: { id: "w2", net_id: "sig" },
      },
      rails: { r1: { id: "r1", net_id: "gnd" } },
    } as unknown as Design;
    const check = plan.steps[4]!;
    const scene = assemblyScene(wired, plan, check, {
      text: "GND",
      nets: ["gnd"],
      holes: [],
    });
    expect([...scene.focusIds].sort()).toEqual(["r1", "w1"]);
    expect([...scene.dimIds]).toEqual(["w2"]);
  });
});
