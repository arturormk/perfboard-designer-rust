import type { PinRef, ProjectDocument } from "../api/project";

type ProjectDesign = ProjectDocument["design"];

export function netPinLabel(design: ProjectDesign, pinRef: PinRef): string {
  const component = design.components?.[pinRef.component_id];
  const definition =
    component && design.component_definitions?.[component.definition_id];
  const pin = definition?.pins.find(
    (candidate) => candidate.id === pinRef.pin_id,
  );

  const componentLabel = component?.name ?? pinRef.component_id;
  if (!pin) return `${componentLabel} · pin ${pinRef.pin_id}`;

  return `${componentLabel} · ${pin.number} (${pin.name})`;
}
