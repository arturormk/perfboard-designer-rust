import { invoke } from "@tauri-apps/api/core";

export type Rotation = "deg0" | "deg90" | "deg180" | "deg270";

export interface HolePos {
  x: number;
  y: number;
}
export interface ObjectReference {
  kind: string;
  id: string;
}
export interface Diagnostic {
  code: string;
  severity: "error" | "warning" | "advisory";
  message: string;
  objects?: ObjectReference[];
}
export interface NetConnectivity {
  net_id: string;
  islands: PinRef[][];
  /** Other nets currently joined to this one through fitted shunts. */
  joined_nets: string[];
  /** The shunted jumpers doing that joining. */
  shunts: string[];
}
export interface SessionChangeEvent {
  revision: number;
  kind: string;
  summary: string;
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
  kind?: "standard" | "bridge" | "jumper";
  id: string;
  definition_version: string;
  name: string;
  description: string;
  origin_x?: number;
  origin_y?: number;
  body: { x: number; y: number; width: number; height: number };
  allowed_rotations: Rotation[];
  pins: PinDefinition[];
  occupied_holes?: HolePos[];
  keepouts?: LocalRect[];
  wire_keepouts?: LocalRect[];
  access?: AccessRegion[];
  documentation?: string;
}
export interface ComponentLoadDiagnostic {
  code: string;
  severity: "error";
  message: string;
  file: string;
  line?: number;
  column?: number;
  field?: string;
}
export interface RuntimeComponentLoad {
  definition?: ComponentDefinition;
  documentation?: string;
  component_path: string;
  revision: string;
  diagnostics: ComponentLoadDiagnostic[];
}
export type ComponentWatcherStatus = "watching" | "error" | "stopped";
export interface ComponentViewerSnapshot {
  generation: number;
  watcher_status: ComponentWatcherStatus;
  attempted_at_ms: number;
  load?: RuntimeComponentLoad;
  watcher_error?: string;
}
export interface ComponentDraftSnapshot {
  definition: ComponentDefinition;
  documentation: string;
  valid: boolean;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  diagnostics: string[];
}
export interface HistoryActionResult {
  revision: number;
  summary: string;
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
}
export type ComponentDraftOperation =
  | {
      type: "set_metadata";
      id: string;
      definition_version: string;
      name: string;
      description: string;
    }
  | { type: "set_body"; body: ComponentDefinition["body"] }
  | { type: "set_origin"; x: number; y: number }
  | { type: "set_rotations"; rotations: Rotation[] }
  | { type: "upsert_pin"; previous_id?: string; pin: PinDefinition }
  | { type: "remove_pin"; id: string }
  | { type: "reorder_pin"; id: string; index: number }
  | { type: "set_occupied_holes"; holes: HolePos[] }
  | { type: "set_keepouts"; keepouts: LocalRect[] }
  | { type: "set_wire_keepouts"; keepouts: LocalRect[] }
  | { type: "set_access_regions"; access: AccessRegion[] }
  | { type: "set_documentation"; documentation: string };
export interface ComponentDraftSaveResult {
  draft: ComponentDraftSnapshot;
  load: RuntimeComponentLoad;
}
export interface ComponentInstance {
  id: string;
  definition_id: string;
  name: string;
  position: HolePos;
  rotation: Rotation;
  properties?: Record<string, string>;
  notes?: string;
  /** A fitted shunt on a `jumper` part joins its two pins. */
  shunted?: boolean;
}
export type Side = "component" | "solder";
export type ConnectionState = "proposed" | "accepted";
export interface PinRef {
  component_id: string;
  pin_id: string;
}
export type TerminalRef =
  | { kind: "pin"; component_id: string; pin_id: string }
  | { kind: "junction"; junction_id: string }
  | {
      kind: "rail_node";
      rail_id: string;
      position: { x2: number; y2: number };
    };
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
export interface Rail {
  id: string;
  net_id: string;
  side: Side;
  color: WireColor;
  vertices: Array<{ x2: number; y2: number }>;
  locked?: boolean;
  notes?: string;
}
/** A net-free wire tie: a strap between two free holes holding wires down. */
export interface Saddle {
  id: string;
  side: Side;
  start: HolePos;
  end: HolePos;
  color: WireColor;
  notes?: string;
}
export type ConstraintSeverity = "required" | "preferred" | "advisory";
export type Constraint = {
  id: string;
  severity: ConstraintSeverity;
  origin: "user" | "mcp" | "import" | "system_derived";
  rationale?: string;
  annotation?: string;
  type: string;
  [key: string]: unknown;
};
export interface JumperInventoryEntry {
  id: string;
  span_holes: number;
  quantity?: number;
  color: string;
  wire_diameter_mm?: number;
  body_height_mm?: number;
  tolerance_holes: number;
}
export interface RouteRequest {
  net_id: string;
  start: TerminalRef;
  end: TerminalRef;
  preferred_side?: Side;
  replace_wire_id?: string;
}
export interface RouteProposal {
  net_id: string;
  start: TerminalRef;
  end: TerminalRef;
  side: Side;
  vertices: Array<{ x2: number; y2: number }>;
  length_mm: number;
  bend_count: number;
  crossings: Array<{
    other_wire_id: string;
    position: { x2: number; y2: number };
  }>;
  jumper_match?: {
    inventory_id: string;
    requested_span_holes: number;
    inventory_span_holes: number;
    kind: "exact" | "approximate" | "reshaped";
  };
  cost: {
    length: number;
    bends: number;
    cramped_bends: number;
    crossings: number;
    access: number;
    side_transitions: number;
    parallel_runs: number;
    board_edges: number;
    jumper_adjustment: number;
    total: number;
  };
  warnings?: string[];
}
export interface ProjectDocument {
  schema_version?: number;
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
    rails?: Record<string, Rail>;
    saddles?: Record<string, Saddle>;
    constraints?: Record<string, Constraint>;
    jumper_inventory?: Record<string, JumperInventoryEntry>;
    crossing_choices?: Record<
      string,
      {
        first_wire_id: string;
        second_wire_id: string;
        over_wire_id: string;
        position: { x2: number; y2: number };
      }
    >;
  };
  validation?: Diagnostic[];
  commits?: Record<string, Commit>;
  commit_order?: string[];
  head_commit?: string;
  working_base?: ProjectDocument["design"];
  undo_stack?: WorkingChange[];
  redo_stack?: WorkingChange[];
}

export interface Commit {
  id: string;
  parent?: string;
  name: string;
  description: string;
  timestamp_ms: number;
  source_revision: number;
  design: ProjectDocument["design"];
  validation?: Diagnostic[];
  changes?: WorkingChange[];
}
export interface WorkingChange {
  operation_id: string;
  origin: "user" | "mcp" | "import" | "system_derived";
  rationale?: string;
  summary: string;
  payloads: OperationPayload[];
  before: ProjectDocument["design"];
  after: ProjectDocument["design"];
}
export type ChangeKind =
  "added" | "removed" | "modified" | "moved" | "rotated" | "route_changed";
export interface DiffEntry {
  object_type: string;
  object_id: string;
  kind: ChangeKind;
  summary: string;
}
export interface SemanticDiff {
  components: DiffEntry[];
  nets: DiffEntry[];
  wires: DiffEntry[];
  rails: DiffEntry[];
  saddles: DiffEntry[];
  constraints: DiffEntry[];
  routes: DiffEntry[];
}

export type OperationPayload =
  | {
      type: "resize_board";
      holes_x: number;
      holes_y: number;
      offset_x: number;
      offset_y: number;
    }
  | {
      type: "add_component";
      definition: ComponentDefinition;
      instance: ComponentInstance;
    }
  | {
      type: "update_component_metadata";
      instance_id: string;
      definition: ComponentDefinition;
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
  | { type: "set_jumper_shunt"; instance_id: string; shunted: boolean }
  | { type: "define_net"; net: ElectricalNet }
  | { type: "remove_net"; net_id: string }
  | { type: "rename_net"; net_id: string; name: string }
  | { type: "merge_nets"; source_net_id: string; target_net_id: string }
  | { type: "assign_pin"; net_id: string; pin: PinRef }
  | { type: "unassign_pin"; net_id: string; pin: PinRef }
  | { type: "accept_net_proposal"; net_id: string }
  | { type: "reject_net_proposal"; net_id: string }
  | { type: "add_wire"; wire: Wire }
  | { type: "add_rail"; rail: Rail }
  | { type: "remove_rail"; rail_id: string }
  | {
      type: "modify_rail_path";
      rail_id: string;
      vertices: Array<{ x2: number; y2: number }>;
    }
  | { type: "set_rail_color"; rail_id: string; color: WireColor }
  | { type: "add_saddle"; saddle: Saddle }
  | { type: "update_saddle"; saddle: Saddle }
  | { type: "remove_saddle"; saddle_id: string }
  | { type: "remove_wire"; wire_id: string }
  | {
      type: "modify_wire_path";
      wire_id: string;
      vertices: Array<{ x2: number; y2: number }>;
    }
  | {
      type: "reconnect_wire_endpoint";
      wire_id: string;
      endpoint: "start" | "end";
      terminal: TerminalRef;
    }
  | { type: "set_wire_color"; wire_id: string; color: WireColor }
  | { type: "set_wire_side"; wire_id: string; side: Side }
  | { type: "set_wire_lock"; wire_id: string; locked: boolean }
  | { type: "add_constraint"; constraint: Constraint }
  | { type: "update_constraint"; constraint: Constraint }
  | { type: "remove_constraint"; constraint_id: string }
  | { type: "upsert_jumper_inventory"; entry: JumperInventoryEntry }
  | { type: "remove_jumper_inventory"; inventory_id: string }
  | {
      type: "accept_route_proposal";
      proposal: RouteProposal;
      wire_id: string;
      color: WireColor;
      notes: string;
      replace_wire_id?: string;
      new_route_over?: boolean;
    };

export function getProject(): Promise<ProjectDocument> {
  return invoke("get_project");
}
export function getProjectRevision(): Promise<number> {
  return invoke("get_project_revision");
}
export function getComponentLibrary(): Promise<ComponentDefinition[]> {
  return invoke("get_component_library");
}
export function selectRuntimeComponent(): Promise<ComponentViewerSnapshot | null> {
  return invoke("select_runtime_component");
}
export function newComponentDraft(): Promise<ComponentViewerSnapshot | null> {
  return invoke("new_component_draft");
}
export function reloadRuntimeComponent(): Promise<ComponentViewerSnapshot | null> {
  return invoke("reload_runtime_component");
}
export function closeRuntimeComponent(): Promise<void> {
  return invoke("close_runtime_component");
}
export function getComponentDraft(): Promise<ComponentDraftSnapshot | null> {
  return invoke("get_component_draft");
}
export function applyComponentDraftOperation(
  operation: ComponentDraftOperation,
): Promise<ComponentDraftSnapshot> {
  return invoke("apply_component_draft_operation", { operation });
}
export function undoComponentDraft(): Promise<ComponentDraftSnapshot> {
  return invoke("undo_component_draft");
}
export function redoComponentDraft(): Promise<ComponentDraftSnapshot> {
  return invoke("redo_component_draft");
}
export function saveComponentDraft(): Promise<ComponentDraftSaveResult> {
  return invoke("save_component_draft");
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
export function applyTransaction(
  revision: number,
  operations: OperationPayload[],
  rationale?: string,
): Promise<void> {
  return invoke("apply_project_transaction", {
    request: {
      operation_id: crypto.randomUUID(),
      expected_revision: revision,
      origin: "user",
      rationale,
      allow_invalid_placement: true,
      operations,
    },
  });
}
export function saveProject(path: string): Promise<void> {
  return invoke("save_project", { path });
}
export function openProject(path: string): Promise<ProjectDocument> {
  return invoke("open_project", { path });
}
/** A recently opened or saved project, kept in the app's configuration. */
export interface RecentProject {
  path: string;
  name: string;
  /** False when the file has been moved or deleted since. */
  exists: boolean;
}
export function getRecentProjects(): Promise<RecentProject[]> {
  return invoke("get_recent_projects");
}
/** Removes one entry, or every entry when `path` is omitted. */
export function forgetRecentProject(path?: string): Promise<RecentProject[]> {
  return invoke("forget_recent_project", { path: path ?? null });
}
export function importRecentProjects(
  paths: string[],
): Promise<RecentProject[]> {
  return invoke("import_recent_projects", { paths });
}
// ── Assembly planning (ADR 0012) ───────────────────────────────────────────
export type StepKind =
  "component" | "wire" | "rail" | "bridge" | "saddle" | "check";
export interface StepDependency {
  step_id: string;
  reason: string;
}
/** One measurement in a check step. */
export interface Probe {
  text: string;
  /** IDs of the nets being measured. */
  nets: string[];
  /** Test points: pin holes to touch with the meter's probes. */
  holes: Array<{ x2: number; y2: number }>;
}
export interface AssemblyStep {
  /** Stable across plans: `component:<id>`, `wire:<id>`, `check:<name>`… */
  id: string;
  /** 1-based position in the plan. */
  number: number;
  kind: StepKind;
  label: string;
  title: string;
  /** The face to look at while doing the step. */
  side: Side;
  objects: ObjectReference[];
  /** Holes and points to mark: pins, wire ends, rail nodes, saddle legs. */
  holes: Array<{ x2: number; y2: number }>;
  instructions: string[];
  notes?: string[];
  after?: StepDependency[];
  /** Check steps: the individual measurements, each with where to probe. */
  probes?: Probe[];
  /** What the step asks for; a checkmark applies while it is unchanged. */
  fingerprint: string;
}
export interface WireListEntry {
  kind: "wire" | "rail";
  id: string;
  label: string;
  from: string;
  to: string;
  net: string;
  color: WireColor;
  side: Side;
  path_mm: number;
  /** Estimated: path plus bare leads. */
  cut_mm: number;
  bend_count: number;
  jumper?: string;
  step: number;
}
export interface AssemblyWarning {
  code: string;
  message: string;
  objects: ObjectReference[];
}
export interface AssemblyPlan {
  source_revision: number;
  fingerprint: string;
  steps: AssemblyStep[];
  wire_list: WireListEntry[];
  warnings: AssemblyWarning[];
}
/** Step ID → fingerprint of the step when it was marked done. */
export type Checkmarks = Record<string, string>;

export function getAssemblyPlan(): Promise<AssemblyPlan> {
  return invoke("get_assembly_plan");
}
export function getAssemblyProgress(): Promise<Checkmarks> {
  return invoke("get_assembly_progress");
}
/** Ticks a step with its current fingerprint, or unticks it with `null`. */
export function setAssemblyStepDone(
  stepId: string,
  fingerprint: string | null,
): Promise<Checkmarks> {
  return invoke("set_assembly_step_done", { stepId, fingerprint });
}
export function resetAssemblyProgress(): Promise<Checkmarks> {
  return invoke("reset_assembly_progress");
}
export function undoProject(expectedRevision: number): Promise<void> {
  return invoke("undo_project", { expectedRevision });
}
export function redoProject(expectedRevision: number): Promise<void> {
  return invoke("redo_project", { expectedRevision });
}
export function trimProjectUndoHistory(
  expectedRevision: number,
  keepEntries: number,
): Promise<HistoryActionResult> {
  return invoke("trim_project_undo_history", { expectedRevision, keepEntries });
}
export function discardProjectChanges(expectedRevision: number): Promise<void> {
  return invoke("discard_project_changes", { expectedRevision });
}
export function commitProject(
  expectedRevision: number,
  name: string,
  description: string,
): Promise<Commit> {
  return invoke("commit_project", { expectedRevision, name, description });
}
export function restoreProjectCommit(
  expectedRevision: number,
  commitId: string,
): Promise<void> {
  return invoke("restore_project_commit", { expectedRevision, commitId });
}
export function compareProjectRevisions(
  from?: string,
  to?: string,
): Promise<SemanticDiff> {
  return invoke("compare_project_revisions", { from, to });
}
export function suggestProjectRoute(
  request: RouteRequest,
): Promise<RouteProposal> {
  return invoke("suggest_project_route", { request });
}
export function evaluateProjectRoute(
  request: RouteRequest,
  side: Side,
  vertices: Array<{ x2: number; y2: number }>,
): Promise<RouteProposal> {
  return invoke("evaluate_project_route", { request, side, vertices });
}
export function getNetConnectivity(): Promise<NetConnectivity[]> {
  return invoke("get_net_connectivity");
}
/**
 * Saves page-sized SVGs as a true-scale PDF via a save dialog and opens it.
 * Resolves to the saved path, or null when the user cancels.
 */
export function exportPdf(
  pages: string[],
  suggestedName: string,
): Promise<string | null> {
  return invoke("export_pdf", { pages, suggestedName });
}
export function pickProjectPath(
  save: boolean,
  suggestedName?: string,
): Promise<string | null> {
  return invoke("pick_project_path", { save, suggestedName });
}

/** Undo/redo entry without design snapshots (see `get_project_view`). */
export interface ChangeSummary {
  operation_id: string;
  origin: WorkingChange["origin"];
  rationale?: string;
  summary: string;
  diff: SemanticDiff;
}
export interface CommitSummary {
  id: string;
  parent?: string;
  name: string;
  description: string;
  timestamp_ms: number;
  source_revision: number;
  change_count: number;
  origins: WorkingChange["origin"][];
  rationales: string[];
}
/** Lightweight read model used by the editor after every change. */
export interface ProjectView {
  schema_version: number;
  project_id: string;
  name: string;
  revision: number;
  design: ProjectDocument["design"];
  validation: Diagnostic[];
  commits: CommitSummary[];
  head_commit?: string;
  head_design?: ProjectDocument["design"];
  undo: ChangeSummary[];
  redo: ChangeSummary[];
}
export function getProjectView(): Promise<ProjectView> {
  return invoke("get_project_view");
}
export function getCommitDesign(
  commitId: string,
): Promise<ProjectDocument["design"]> {
  return invoke("get_commit_design", { commitId });
}
export function getUndoHistoryBytes(): Promise<number> {
  return invoke("get_undo_history_bytes");
}
