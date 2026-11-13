/**
 * Assembly mode state: the derived build plan, the builder's checkmarks and
 * the step on screen (ADR 0012).
 *
 * The plan comes from Rust and is never edited here. Checkmarks are stored by
 * the backend outside the project file, each with the fingerprint the step had
 * when it was ticked, so a step that changed since reads as "changed", not
 * "done".
 */
import { computed, signal } from "@preact/signals";
import {
  getAssemblyPlan,
  getAssemblyProgress,
  resetAssemblyProgress,
  setAssemblyStepDone,
  type AssemblyPlan,
  type AssemblyStep,
  type Checkmarks,
} from "../api/project";
import { errorMessage } from "../error-message";
import { view } from "./store";
import { toast } from "./toasts";
import {
  comparePlans,
  firstOpenStep,
  statusOf,
  type PlanUpdate,
} from "../assembly/plan";

export type { PlanUpdate, StepStatus } from "../assembly/plan";

export const plan = signal<AssemblyPlan | null>(null);
export const checkmarks = signal<Checkmarks>({});
export const currentStepId = signal<string | null>(null);
export const planUpdate = signal<PlanUpdate | null>(null);
export const planError = signal<string | null>(null);
/** The measurement picked in a check step, by step and index. */
export const selectedProbe = signal<{ stepId: string; index: number } | null>(
  null,
);
/** Project the plan and checkmarks belong to. */
let planProject: string | undefined;

export const statuses = computed(() => {
  const marks = checkmarks.value;
  return new Map(
    (plan.value?.steps ?? []).map((step) => [step.id, statusOf(step, marks)]),
  );
});

export const currentStep = computed<AssemblyStep | undefined>(() => {
  const steps = plan.value?.steps ?? [];
  return steps.find((step) => step.id === currentStepId.value) ?? steps[0];
});

export const currentProbe = computed(() => {
  const step = currentStep.value;
  const picked = selectedProbe.value;
  return step && picked?.stepId === step.id
    ? step.probes?.[picked.index]
    : undefined;
});

/** Picks a measurement of the step on screen, or clears it when picked again. */
export function toggleProbe(index: number): void {
  const step = currentStep.peek();
  if (!step) return;
  const picked = selectedProbe.peek();
  selectedProbe.value =
    picked?.stepId === step.id && picked.index === index
      ? null
      : { stepId: step.id, index };
}

export const progress = computed(() => {
  const all = [...statuses.value.values()];
  return {
    done: all.filter((status) => status === "done").length,
    changed: all.filter((status) => status === "changed").length,
    total: all.length,
  };
});

/** Derives the plan for the current revision and loads the checkmarks. */
export async function loadAssembly(): Promise<void> {
  const project = view.peek()?.project_id;
  try {
    const [next, marks] = await Promise.all([
      getAssemblyPlan(),
      getAssemblyProgress(),
    ]);
    const previous = plan.peek();
    const sameProject = previous !== null && planProject === project;
    if (sameProject) {
      const update = comparePlans(previous, next, checkmarks.peek());
      if (update) planUpdate.value = update;
    } else planUpdate.value = null;
    planProject = project;
    plan.value = next;
    checkmarks.value = marks;
    planError.value = null;
    const shown = currentStepId.peek();
    if (!sameProject || !next.steps.some((step) => step.id === shown))
      currentStepId.value = firstOpenStep(next, marks)?.id ?? null;
  } catch (error) {
    planError.value = errorMessage(error);
  }
}

export function goToStep(id: string): void {
  currentStepId.value = id;
}

/** Moves by `delta` steps through the plan, staying within it. */
export function stepBy(delta: number): void {
  const steps = plan.peek()?.steps ?? [];
  const index = steps.findIndex((step) => step.id === currentStep.peek()?.id);
  const next = steps[Math.min(steps.length - 1, Math.max(0, index + delta))];
  if (next) currentStepId.value = next.id;
}

export async function setDone(
  step: AssemblyStep,
  done: boolean,
): Promise<void> {
  try {
    checkmarks.value = await setAssemblyStepDone(
      step.id,
      done ? step.fingerprint : null,
    );
  } catch (error) {
    toast("error", "Could not save assembly progress", {
      detail: errorMessage(error),
    });
  }
}

/** Ticks the step on screen and moves to the next one not done yet. */
let advancing = false;

export async function completeAndAdvance(): Promise<void> {
  // A second press while the first is saving would tick the same step again.
  if (advancing) return;
  advancing = true;
  try {
    await advance();
  } finally {
    advancing = false;
  }
}

async function advance(): Promise<void> {
  const step = currentStep.peek();
  const current = plan.peek();
  if (!step || !current) return;
  if (statusOf(step, checkmarks.peek()) !== "done") await setDone(step, true);
  const marks = checkmarks.peek();
  const after = current.steps.find(
    (candidate) =>
      candidate.number > step.number && statusOf(candidate, marks) !== "done",
  );
  const next = after ?? firstOpenStep(current, marks);
  if (next) currentStepId.value = next.id;
}

export async function resetProgress(): Promise<void> {
  try {
    checkmarks.value = await resetAssemblyProgress();
    const current = plan.peek();
    currentStepId.value = current?.steps[0]?.id ?? null;
  } catch (error) {
    toast("error", "Could not save assembly progress", {
      detail: errorMessage(error),
    });
  }
}
