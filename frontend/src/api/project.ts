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
  name: string;
  x: number;
  y: number;
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
    };

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
