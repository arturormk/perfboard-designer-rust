/**
 * Printed assembly plan: the wire list in build order and the build-order
 * checklist, both derived from the Rust plan (ADR 0012).
 */
import { signal } from "@preact/signals";
import {
  getAssemblyPlan,
  getAssemblyProgress,
  type AssemblyPlan,
  type AssemblyStep,
  type Checkmarks,
  type WireListEntry,
} from "../api/project";
import { statusOf } from "../assembly/plan";

/** The plan and checkmarks fetched for the next print. */
export const printPlan = signal<{
  plan: AssemblyPlan;
  marks: Checkmarks;
} | null>(null);

/** Wires and rails in the order they are installed. */
export function wireListInBuildOrder(plan: AssemblyPlan): WireListEntry[] {
  return [...plan.wire_list].sort(
    (a, b) => a.step - b.step || a.label.localeCompare(b.label),
  );
}

export interface ChecklistRow {
  step: AssemblyStep;
  /** Ticked in the app and unchanged since. */
  done: boolean;
  /** Numbers of the steps that must come first. */
  after: number[];
}

export function checklistRows(
  plan: AssemblyPlan,
  marks: Checkmarks,
): ChecklistRow[] {
  const numbers = new Map(plan.steps.map((step) => [step.id, step.number]));
  return plan.steps.map((step) => ({
    step,
    done: statusOf(step, marks) === "done",
    after: (step.after ?? [])
      .flatMap((dependency) => numbers.get(dependency.step_id) ?? [])
      .sort((a, b) => a - b),
  }));
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}

/**
 * Prints the build drawing with a freshly derived plan. Without one (the
 * engine failed, or a native print shortcut skipped this), the sheet falls
 * back to the plain wire table and leaves the checklist out.
 */
export async function printBuildDrawing(): Promise<void> {
  try {
    const [plan, marks] = await Promise.all([
      getAssemblyPlan(),
      getAssemblyProgress().catch(() => ({})),
    ]);
    printPlan.value = { plan, marks };
  } catch {
    printPlan.value = null;
  }
  await nextFrame();
  window.print();
}
