/** Pure helpers for assembly plans and checkmarks (ADR 0012). */
import type { AssemblyPlan, AssemblyStep, Checkmarks } from "../api/project";

export type StepStatus = "todo" | "done" | "changed";

/** What changed between the plan last shown and the one just derived. */
export interface PlanUpdate {
  added: number;
  removed: number;
  changed: number;
  /** Ticked steps whose content changed since they were ticked. */
  invalidated: number;
}

export function statusOf(step: AssemblyStep, marks: Checkmarks): StepStatus {
  const mark = marks[step.id];
  if (mark === undefined) return "todo";
  return mark === step.fingerprint ? "done" : "changed";
}

/** Compares two plans of the same project by step ID and fingerprint. */
export function comparePlans(
  before: AssemblyPlan,
  after: AssemblyPlan,
  marks: Checkmarks,
): PlanUpdate | null {
  if (before.fingerprint === after.fingerprint) return null;
  const old = new Map(before.steps.map((step) => [step.id, step]));
  const ids = new Set(after.steps.map((step) => step.id));
  let added = 0;
  let changed = 0;
  let invalidated = 0;
  for (const step of after.steps) {
    const previous = old.get(step.id);
    if (!previous) added += 1;
    else if (previous.fingerprint !== step.fingerprint) {
      changed += 1;
      if (marks[step.id] === previous.fingerprint) invalidated += 1;
    }
  }
  const removed = before.steps.filter((step) => !ids.has(step.id)).length;
  // Renumbering alone, or a new warning, still deserves a quiet mention.
  return { added, removed, changed, invalidated };
}

/** The first step not done yet, or the last step when all are done. */
export function firstOpenStep(
  next: AssemblyPlan,
  marks: Checkmarks,
): AssemblyStep | undefined {
  return (
    next.steps.find((step) => statusOf(step, marks) !== "done") ??
    next.steps.at(-1)
  );
}
