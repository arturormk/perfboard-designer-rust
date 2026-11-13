/**
 * The board as it stands at an assembly step: everything installed up to and
 * including the step, with earlier work faded and the step itself in focus.
 */
import type { AssemblyPlan, AssemblyStep, Probe } from "../api/project";
import type { Design } from "../model/geometry";

export interface AssemblyScene {
  design: Design;
  /** Installed in earlier steps. */
  dimIds: ReadonlySet<string>;
  /** Installed in this step. */
  focusIds: ReadonlySet<string>;
}

function keep<T>(
  items: Record<string, T> | undefined,
  later: ReadonlySet<string>,
): Record<string, T> | undefined {
  if (!items) return items;
  return Object.fromEntries(
    Object.entries(items).filter(([id]) => !later.has(id)),
  );
}

export function assemblyScene(
  design: Design,
  plan: AssemblyPlan,
  step: AssemblyStep | undefined,
  probe?: Probe,
): AssemblyScene {
  const none = new Set<string>();
  // Check steps look at the finished board; nothing is new in them. A picked
  // measurement lights up the wiring of its nets and fades the rest.
  if (!step || step.kind === "check") {
    if (!probe) return { design, dimIds: none, focusIds: none };
    const nets = new Set(probe.nets);
    const focusIds = new Set<string>();
    const dimIds = new Set<string>();
    for (const item of [
      ...Object.values(design.wires ?? {}),
      ...Object.values(design.rails ?? {}),
    ])
      (nets.has(item.net_id) ? focusIds : dimIds).add(item.id);
    return { design, dimIds, focusIds };
  }
  const later = new Set<string>();
  const earlier = new Set<string>();
  for (const other of plan.steps) {
    const target = other.number > step.number ? later : earlier;
    if (other.number !== step.number)
      for (const object of other.objects) target.add(object.id);
  }
  const focusIds = new Set(step.objects.map((object) => object.id));
  return {
    design: {
      ...design,
      components: keep(design.components, later),
      wires: keep(design.wires, later),
      rails: keep(design.rails, later),
      saddles: keep(design.saddles, later),
    },
    dimIds: earlier,
    focusIds,
  };
}
