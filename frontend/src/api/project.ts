import { invoke } from "@tauri-apps/api/core";

export type Rotation = "deg0" | "deg90" | "deg180" | "deg270";

export interface HolePos {
  x: number;
  y: number;
}
export interface Diagnostic {
  code: string;
  severity: "error" | "warning" | "advisory";
  message: string;
}
export interface PinDefinition {
  id: string;
  number: string;
  name: string;
  x: number;
  y: number;
  aliases?: string[];
  functions?: string[];
}
export interface LocalRect {
  min: HolePos;
  max: HolePos;
  label: string;
}
export interface AccessRegion extends LocalRect {
  direction: "north" | "east" | "south" | "west";
  required: boolean;
  allow_board_overhang: boolean;
}
export interface ComponentDefinition {
  schema_version: number;
  id: string;
  definition_version: string;
  name: string;
  description: string;
  body: { x: number; y: number; width: number; height: number };
  allowed_rotations: Rotation[];
  pins: PinDefinition[];
  occupied_holes?: HolePos[];
  keepouts?: LocalRect[];
  access?: AccessRegion[];
}
export interface ComponentInstance {
  id: string;
  definition_id: string;
  name: string;
  position: HolePos;
  rotation: Rotation;
  properties?: Record<string, string>;
  notes?: string;
}
export type Side = "component" | "solder";
export type ConnectionState = "proposed" | "accepted";
export interface PinRef {
  component_id: string;
  pin_id: string;
}
export type TerminalRef =
  | { kind: "pin"; component_id: string; pin_id: string }
  | { kind: "junction"; junction_id: string };
export interface ElectricalNet {
  id: string;
  name: string;
  state: ConnectionState;
  pins?: PinRef[];
  notes?: string;
}
export type WireColor =
  | "red"
  | "black"
  | "yellow"
  | "white"
  | "green"
  | "blue"
  | "orange"
  | { other: string };
export interface Wire {
  id: string;
  net_id: string;
  start: TerminalRef;
  end: TerminalRef;
  side: Side;
  color: WireColor;
  vertices: Array<{ x2: number; y2: number }>;
  locked?: boolean;
  notes?: string;
}
export interface ProjectDocument {
  project_id: string;
  name: string;
  revision: number;
  design: {
    board: {
      name: string;
      holes_x: number;
      holes_y: number;
      pitch_mm: number;
      margins_mm?: {
        top: number;
        right: number;
        bottom: number;
        left: number;
      };
      mounting_holes?: Array<{
        position: HolePos;
        diameter_mm: number;
        label: string;
      }>;
      forbidden_areas?: Array<{ min: HolePos; max: HolePos; label: string }>;
    };
    component_definitions?: Record<string, ComponentDefinition>;
    components?: Record<string, ComponentInstance>;
    nets?: Record<string, ElectricalNet>;
    wires?: Record<string, Wire>;
  };
  validation?: Diagnostic[];
}

export type OperationPayload =
  | {
      type: "add_component";
      definition: ComponentDefinition;
      instance: ComponentInstance;
    }
  | { type: "remove_component"; instance_id: string }
  | { type: "move_component"; instance_id: string; position: HolePos }
  | { type: "rotate_component"; instance_id: string; rotation: Rotation }
  | {
      type: "change_instance_properties";
      instance_id: string;
      name: string;
      properties: Record<string, string>;
      notes: string;
    }
  | { type: "define_net"; net: ElectricalNet }
  | { type: "remove_net"; net_id: string }
  | { type: "assign_pin"; net_id: string; pin: PinRef }
  | { type: "unassign_pin"; net_id: string; pin: PinRef }
  | { type: "accept_net_proposal"; net_id: string }
  | { type: "reject_net_proposal"; net_id: string }
  | { type: "add_wire"; wire: Wire }
  | { type: "remove_wire"; wire_id: string }
  | {
      type: "modify_wire_path";
      wire_id: string;
      vertices: Array<{ x2: number; y2: number }>;
    }
  | { type: "set_wire_color"; wire_id: string; color: WireColor }
  | { type: "set_wire_side"; wire_id: string; side: Side }
  | { type: "set_wire_lock"; wire_id: string; locked: boolean };

export function getProject(): Promise<ProjectDocument> {
  return invoke("get_project");
}
export function getComponentLibrary(): Promise<ComponentDefinition[]> {
  return invoke("get_component_library");
}
export function createProject(
  name: string,
  holesX = 25,
  holesY = 18,
  pitchMm = 2.54,
): Promise<ProjectDocument> {
  return invoke("create_project", { name, holesX, holesY, pitchMm });
}
export function applyOperation(
  revision: number,
  payload: OperationPayload,
): Promise<void> {
  return invoke("apply_project_operation", {
    request: {
      operation_id: crypto.randomUUID(),
      expected_revision: revision,
      origin: "user",
      allow_invalid_placement: true,
      payload: undefined,
      ...payload,
    },
  });
}
export function saveProject(path: string): Promise<void> {
  return invoke("save_project", { path });
}
export function openProject(path: string): Promise<ProjectDocument> {
  return invoke("open_project", { path });
}
