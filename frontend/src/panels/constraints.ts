/** Typed descriptions of constraint kinds for forms and readable summaries. */
import type { Constraint } from "../api/project";
import type { Design } from "../model/geometry";

export type FieldKind =
  | "component"
  | "net"
  | "wire"
  | "number"
  | "integer"
  | "rotation"
  | "side"
  | "position"
  | "area"
  | "text";

export interface FieldSpec {
  key: string;
  label: string;
  kind: FieldKind;
  min?: number;
}

export interface ConstraintSpec {
  type: string;
  label: string;
  help: string;
  fields: FieldSpec[];
}

export const CONSTRAINT_SPECS: ConstraintSpec[] = [
  {
    type: "fixed_placement",
    label: "Fixed placement",
    help: "Pin a part to an exact hole.",
    fields: [
      { key: "component_id", label: "Part", kind: "component" },
      { key: "position", label: "Anchor hole", kind: "position" },
    ],
  },
  {
    type: "orientation",
    label: "Orientation",
    help: "Require a part to keep one rotation.",
    fields: [
      { key: "component_id", label: "Part", kind: "component" },
      { key: "rotation", label: "Rotation", kind: "rotation" },
    ],
  },
  {
    type: "proximity",
    label: "Keep close",
    help: "Keep two parts within a distance, e.g. a decoupling capacitor.",
    fields: [
      { key: "first_component_id", label: "Part", kind: "component" },
      { key: "second_component_id", label: "Near", kind: "component" },
      {
        key: "max_distance_holes",
        label: "Max distance (holes)",
        kind: "number",
        min: 0,
      },
    ],
  },
  {
    type: "edge_placement",
    label: "Near an edge",
    help: "Keep a connector close to the board edge.",
    fields: [
      { key: "component_id", label: "Part", kind: "component" },
      {
        key: "max_edge_distance_holes",
        label: "Max distance (holes)",
        kind: "integer",
        min: 0,
      },
    ],
  },
  {
    type: "access",
    label: "Keep accessible",
    help: "Enforce a part's access region (USB, screw terminal…).",
    fields: [{ key: "component_id", label: "Part", kind: "component" }],
  },
  {
    type: "reserved_area",
    label: "Reserved area",
    help: "Keep a rectangle of holes free for later.",
    fields: [
      { key: "area", label: "Area", kind: "area" },
      { key: "label", label: "Label", kind: "text" },
    ],
  },
  {
    type: "preferred_maximum_length",
    label: "Short wires",
    help: "Prefer wires on a net below a length.",
    fields: [
      { key: "net_id", label: "Net", kind: "net" },
      {
        key: "max_length_mm",
        label: "Max length (mm)",
        kind: "number",
        min: 0,
      },
    ],
  },
  {
    type: "wire_side",
    label: "Wire side",
    help: "Route a net's wires on one face.",
    fields: [
      { key: "net_id", label: "Net", kind: "net" },
      { key: "side", label: "Side", kind: "side" },
    ],
  },
  {
    type: "bend_spacing",
    label: "Bend spacing",
    help: "Minimum distance between bends, in half-hole steps.",
    fields: [
      {
        key: "min_half_grid_steps",
        label: "Min half-steps",
        kind: "integer",
        min: 0,
      },
    ],
  },
  {
    type: "locked_route",
    label: "Locked route",
    help: "Never reroute this wire automatically.",
    fields: [{ key: "wire_id", label: "Wire", kind: "wire" }],
  },
  {
    type: "assembly_access",
    label: "Soldering access",
    help: "Keep an area around a part clear for the iron.",
    fields: [
      { key: "component_id", label: "Part", kind: "component" },
      { key: "area", label: "Area", kind: "area" },
    ],
  },
];

export function specFor(type: string): ConstraintSpec | undefined {
  return CONSTRAINT_SPECS.find((spec) => spec.type === type);
}

export function defaultValue(field: FieldSpec): unknown {
  switch (field.kind) {
    case "number":
    case "integer":
      return field.key === "max_length_mm" ? 50 : 2;
    case "rotation":
      return "deg0";
    case "side":
      return "component";
    case "position":
      return { x: 0, y: 0 };
    case "area":
      return { min: { x: 0, y: 0 }, max: { x: 2, y: 2 } };
    default:
      return "";
  }
}

export function describeConstraint(
  constraint: Constraint,
  design: Design,
): string {
  const part = (key: string) =>
    design.components?.[String(constraint[key])]?.name ?? "missing part";
  const net = (key: string) =>
    design.nets?.[String(constraint[key])]?.name ?? "missing net";
  const area = constraint.area as
    | { min: { x: number; y: number }; max: { x: number; y: number } }
    | undefined;
  const areaText = area
    ? `(${area.min.x},${area.min.y})–(${area.max.x},${area.max.y})`
    : "";
  switch (constraint.type) {
    case "fixed_placement": {
      const position = constraint.position as { x: number; y: number };
      return `${part("component_id")} fixed at ${position.x},${position.y}`;
    }
    case "orientation":
      return `${part("component_id")} rotated ${String(constraint.rotation).replace("deg", "")}°`;
    case "proximity":
      return `${part("first_component_id")} within ${constraint.max_distance_holes} holes of ${part("second_component_id")}`;
    case "edge_placement":
      return `${part("component_id")} within ${constraint.max_edge_distance_holes} holes of an edge`;
    case "access":
      return `${part("component_id")} stays accessible`;
    case "reserved_area":
      return `Reserve ${areaText}${constraint.label ? ` · ${constraint.label}` : ""}`;
    case "preferred_maximum_length":
      return `${net("net_id")} wires under ${constraint.max_length_mm} mm`;
    case "wire_side":
      return `${net("net_id")} on the ${constraint.side} side`;
    case "bend_spacing":
      return `Bends at least ${constraint.min_half_grid_steps} half-steps apart`;
    case "locked_route":
      return `Locked wire route`;
    case "assembly_access":
      return `Soldering access around ${part("component_id")} ${areaText}`;
    default:
      return constraint.type.replaceAll("_", " ");
  }
}
