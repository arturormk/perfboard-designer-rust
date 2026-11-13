/**
 * Browser preview backend — development only.
 *
 * `npm run dev` outside Tauri has no Rust engine. This module installs a fake
 * `__TAURI_INTERNALS__` so the real UI code paths can run against an example
 * project. It applies a small subset of operations naively and validates
 * nothing; it exists for layout and interaction work, never as a second engine.
 * Load another fixture with `?fixture=<name>` (files in `examples/` or the
 * git-ignored `frontend/dev-fixtures/`). `window.__mock.agentEdit()` simulates
 * an MCP change.
 */
import type {
  ChangeSummary,
  CommitSummary,
  ComponentDefinition,
  ComponentDraftOperation,
  Diagnostic,
  NetConnectivity,
  OperationPayload,
  PinRef,
  ProjectDocument,
  ProjectView,
  RouteProposal,
  SemanticDiff,
  TerminalRef,
} from "../api/project";
import { pinPosition, type Design } from "../model/geometry";

type Doc = ProjectDocument;
interface Entry {
  summary: ChangeSummary;
  before: Design;
  after: Design;
}

let lastPdfPages: string[] = [];
const clone = <T>(value: T): T => structuredClone(value);
const fixtures = {
  ...import.meta.glob("../../../examples/*.json", { import: "default" }),
  ...import.meta.glob("../../dev-fixtures/*.json", { import: "default" }),
};

async function loadFixture(): Promise<Doc> {
  const wanted =
    new URLSearchParams(location.search).get("fixture") ?? "nano-rs485";
  const key =
    Object.keys(fixtures).find((path) => path.includes(`/${wanted}`)) ??
    Object.keys(fixtures).find((path) => path.includes("nano-rs485"))!;
  return clone((await fixtures[key]!()) as Doc);
}

function emptyDiff(): SemanticDiff {
  return {
    components: [],
    nets: [],
    wires: [],
    rails: [],
    saddles: [],
    constraints: [],
    routes: [],
  };
}

function diffDesigns(before: Design, after: Design): SemanticDiff {
  const diff = emptyDiff();
  const visit = (
    key: "components" | "wires" | "rails" | "saddles" | "nets",
    type: string,
  ) => {
    const left = (before[key] ?? {}) as Record<string, { name?: string }>;
    const right = (after[key] ?? {}) as Record<string, { name?: string }>;
    for (const id of new Set([...Object.keys(left), ...Object.keys(right)])) {
      const a = left[id];
      const b = right[id];
      if (JSON.stringify(a) === JSON.stringify(b)) continue;
      const kind = !a ? "added" : !b ? "removed" : "modified";
      diff[key].push({
        object_type: type,
        object_id: id,
        kind,
        summary: `${kind[0]!.toUpperCase()}${kind.slice(1)} ${(b ?? a)?.name ?? type}`,
      });
    }
  };
  visit("components", "component");
  visit("wires", "wire");
  visit("rails", "rail");
  visit("saddles", "saddle");
  visit("nets", "net");
  return diff;
}

function islands(design: Design): NetConnectivity[] {
  const result: NetConnectivity[] = [];
  for (const net of Object.values(design.nets ?? {})) {
    if (net.state !== "accepted") continue;
    const pins = net.pins ?? [];
    const key = (ref: PinRef) => `${ref.component_id}:${ref.pin_id}`;
    const parent = new Map(pins.map((pin) => [key(pin), key(pin)]));
    const find = (value: string): string => {
      while (parent.get(value) !== value) value = parent.get(value)!;
      return value;
    };
    const union = (a: string, b: string) => {
      if (parent.has(a) && parent.has(b)) parent.set(find(a), find(b));
    };
    const terminalKey = (terminal: TerminalRef) =>
      terminal.kind === "pin" ? key(terminal) : undefined;
    for (const wire of Object.values(design.wires ?? {})) {
      if (wire.net_id !== net.id) continue;
      const a = terminalKey(wire.start);
      const b = terminalKey(wire.end);
      if (a && b) union(a, b);
    }
    // Pins sharing a hole, and pins joined by a bridge, are connected.
    const located = pins.map((pin) => ({
      pin,
      point: pinPosition(design, pin),
    }));
    for (const a of located)
      for (const b of located)
        if (
          a.point &&
          b.point &&
          a.point.x === b.point.x &&
          a.point.y === b.point.y
        )
          union(key(a.pin), key(b.pin));
    for (const instance of Object.values(design.components ?? {})) {
      const definition = design.component_definitions?.[instance.definition_id];
      if (definition?.kind !== "bridge") continue;
      const ends = definition.pins.map((pin) =>
        pinPosition(design, { component_id: instance.id, pin_id: pin.id }),
      );
      const at = (index: number) =>
        located.filter(
          (item) =>
            item.point &&
            ends[index] &&
            item.point.x === ends[index]!.x &&
            item.point.y === ends[index]!.y,
        );
      for (const a of at(0))
        for (const b of at(1)) union(key(a.pin), key(b.pin));
    }
    // A fitted shunt joins a jumper's two pins.
    for (const instance of Object.values(design.components ?? {})) {
      const definition = design.component_definitions?.[instance.definition_id];
      if (definition?.kind !== "jumper" || !instance.shunted) continue;
      const ends = definition.pins.map((pin) => ({
        component_id: instance.id,
        pin_id: pin.id,
      }));
      if (
        ends.length === 2 &&
        ends.every((end) => pins.some((pin) => key(pin) === key(end)))
      )
        union(key(ends[0]!), key(ends[1]!));
    }
    const groups = new Map<string, PinRef[]>();
    for (const pin of pins) {
      const root = find(key(pin));
      groups.set(root, [...(groups.get(root) ?? []), pin]);
    }
    result.push({
      net_id: net.id,
      islands: [...groups.values()],
      joined_nets: [],
      shunts: [],
    });
  }
  // A shunt across two different nets joins them (the computed view).
  const joins = shuntJoins(design);
  for (const item of result) {
    const joined = new Set<string>();
    const pending = [item.net_id];
    while (pending.length) {
      const current = pending.pop()!;
      for (const join of joins) {
        const other =
          join.a === current ? join.b : join.b === current ? join.a : undefined;
        if (other && other !== item.net_id && !joined.has(other)) {
          joined.add(other);
          pending.push(other);
        }
      }
    }
    item.joined_nets = [...joined].sort();
    item.shunts = [
      ...new Set(
        joins
          .filter(
            (join) =>
              join.a === item.net_id ||
              join.b === item.net_id ||
              (joined.has(join.a) && joined.has(join.b)),
          )
          .map((join) => join.via),
      ),
    ].sort();
  }
  return result;
}

/** Fitted shunts whose two pins sit on two different accepted nets. */
function shuntJoins(design: Design): { a: string; b: string; via: string }[] {
  const netOf = (ref: PinRef) =>
    Object.values(design.nets ?? {}).find(
      (net) =>
        net.state === "accepted" &&
        (net.pins ?? []).some(
          (pin) =>
            pin.component_id === ref.component_id && pin.pin_id === ref.pin_id,
        ),
    );
  const joins: { a: string; b: string; via: string }[] = [];
  for (const instance of Object.values(design.components ?? {})) {
    const definition = design.component_definitions?.[instance.definition_id];
    if (definition?.kind !== "jumper" || !instance.shunted) continue;
    const [first, second] = definition.pins.map((pin) =>
      netOf({ component_id: instance.id, pin_id: pin.id }),
    );
    if (first && second && first.id !== second.id)
      joins.push({ a: first.id, b: second.id, via: instance.id });
  }
  return joins;
}

function validation(design: Design): Diagnostic[] {
  const result: Diagnostic[] = [];
  for (const item of islands(design)) {
    const net = design.nets?.[item.net_id];
    if ((net?.pins?.length ?? 0) < 2)
      result.push({
        code: "circuit.net_too_small",
        severity: "warning",
        message: `Net \`${net?.name}\` has fewer than two pins`,
        objects: [{ kind: "net", id: item.net_id }],
      });
    else if (item.islands.length > 1)
      result.push({
        code: "circuit.net_incomplete",
        severity: "warning",
        message: `Net \`${net?.name}\` is not completely routed`,
        objects: [{ kind: "net", id: item.net_id }],
      });
  }
  for (const join of shuntJoins(design)) {
    const name = (id: string) => design.nets?.[id]?.name;
    result.push({
      code: "circuit.shunt_joins_nets",
      severity: "advisory",
      message: `Shunt on ${design.components?.[join.via]?.name} joins nets \`${name(join.a)}\` and \`${name(join.b)}\``,
      objects: [
        { kind: "component", id: join.via },
        { kind: "net", id: join.a },
        { kind: "net", id: join.b },
      ],
    });
  }
  for (const net of Object.values(design.nets ?? {}))
    if (net.state === "proposed")
      result.push({
        code: "circuit.proposed_net",
        severity: "advisory",
        message: `Net \`${net.name}\` is a proposal awaiting approval`,
        objects: [{ kind: "net", id: net.id }],
      });
  return result;
}

function applyPayload(design: Design, op: OperationPayload): void {
  const d = design;
  switch (op.type) {
    case "add_component":
      d.component_definitions = {
        ...d.component_definitions,
        [op.definition.id]: op.definition,
      };
      d.components = { ...d.components, [op.instance.id]: op.instance };
      return;
    case "remove_component":
      delete d.components?.[op.instance_id];
      for (const net of Object.values(d.nets ?? {}))
        net.pins = net.pins?.filter(
          (pin) => pin.component_id !== op.instance_id,
        );
      for (const [id, wire] of Object.entries(d.wires ?? {}))
        if (
          (wire.start.kind === "pin" &&
            wire.start.component_id === op.instance_id) ||
          (wire.end.kind === "pin" && wire.end.component_id === op.instance_id)
        )
          delete d.wires![id];
      return;
    case "move_component":
      d.components![op.instance_id]!.position = op.position;
      return;
    case "rotate_component":
      d.components![op.instance_id]!.rotation = op.rotation;
      return;
    case "change_instance_properties": {
      const instance = d.components![op.instance_id]!;
      Object.assign(instance, {
        name: op.name,
        notes: op.notes,
        properties: op.properties,
      });
      return;
    }
    case "set_jumper_shunt":
      d.components![op.instance_id]!.shunted = op.shunted;
      return;
    case "define_net":
      d.nets = { ...d.nets, [op.net.id]: op.net };
      return;
    case "remove_net":
    case "reject_net_proposal":
      delete d.nets?.[op.net_id];
      for (const [id, wire] of Object.entries(d.wires ?? {}))
        if (wire.net_id === op.net_id) delete d.wires![id];
      return;
    case "rename_net":
      d.nets![op.net_id]!.name = op.name.trim();
      return;
    case "merge_nets": {
      const source = d.nets![op.source_net_id]!;
      const target = d.nets![op.target_net_id]!;
      target.pins = [...(target.pins ?? []), ...(source.pins ?? [])];
      delete d.nets![op.source_net_id];
      for (const item of [
        ...Object.values(d.wires ?? {}),
        ...Object.values(d.rails ?? {}),
      ])
        if (item.net_id === op.source_net_id) item.net_id = op.target_net_id;
      return;
    }
    case "accept_net_proposal":
      d.nets![op.net_id]!.state = "accepted";
      return;
    case "assign_pin":
      d.nets![op.net_id]!.pins = [...(d.nets![op.net_id]!.pins ?? []), op.pin];
      return;
    case "unassign_pin":
      d.nets![op.net_id]!.pins = d.nets![op.net_id]!.pins?.filter(
        (pin) =>
          !(
            pin.component_id === op.pin.component_id &&
            pin.pin_id === op.pin.pin_id
          ),
      );
      return;
    case "add_wire":
      d.wires = { ...d.wires, [op.wire.id]: op.wire };
      return;
    case "remove_wire":
      delete d.wires?.[op.wire_id];
      return;
    case "modify_wire_path":
      d.wires![op.wire_id]!.vertices = op.vertices;
      return;
    case "set_wire_color":
      d.wires![op.wire_id]!.color = op.color;
      return;
    case "set_wire_side":
      d.wires![op.wire_id]!.side = op.side;
      return;
    case "set_wire_lock":
      d.wires![op.wire_id]!.locked = op.locked;
      return;
    case "reconnect_wire_endpoint":
      d.wires![op.wire_id]![op.endpoint] = op.terminal;
      return;
    case "add_rail":
      d.rails = { ...d.rails, [op.rail.id]: op.rail };
      return;
    case "remove_rail":
      delete d.rails?.[op.rail_id];
      return;
    case "modify_rail_path":
      d.rails![op.rail_id]!.vertices = op.vertices;
      return;
    case "set_rail_color":
      d.rails![op.rail_id]!.color = op.color;
      return;
    case "add_saddle":
    case "update_saddle":
      d.saddles = { ...d.saddles, [op.saddle.id]: op.saddle };
      return;
    case "remove_saddle":
      delete d.saddles?.[op.saddle_id];
      return;
    case "resize_board":
      d.board.holes_x = op.holes_x;
      d.board.holes_y = op.holes_y;
      return;
    case "add_constraint":
    case "update_constraint":
      d.constraints = { ...d.constraints, [op.constraint.id]: op.constraint };
      return;
    case "remove_constraint":
      delete d.constraints?.[op.constraint_id];
      return;
    case "upsert_jumper_inventory":
      d.jumper_inventory = { ...d.jumper_inventory, [op.entry.id]: op.entry };
      return;
    case "remove_jumper_inventory":
      delete d.jumper_inventory?.[op.inventory_id];
      return;
    case "accept_route_proposal": {
      const existing = op.replace_wire_id
        ? d.wires?.[op.replace_wire_id]
        : undefined;
      d.wires = {
        ...d.wires,
        [op.wire_id]: {
          id: op.wire_id,
          net_id: op.proposal.net_id,
          start: op.proposal.start,
          end: op.proposal.end,
          side: op.proposal.side,
          color: op.color,
          vertices: op.proposal.vertices,
          locked: existing?.locked ?? false,
          notes: op.notes,
        },
      };
      return;
    }
    case "update_component_metadata":
      d.component_definitions = {
        ...d.component_definitions,
        [op.definition.id]: op.definition,
      };
      return;
  }
}

function summaryOf(op: OperationPayload): string {
  return op.type.replaceAll("_", " ").replace(/^./, (c) => c.toUpperCase());
}

function terminalPoint(design: Design, terminal: TerminalRef) {
  if (terminal.kind === "pin") {
    const point = pinPosition(design, terminal);
    return point ? { x2: point.x * 2, y2: point.y * 2 } : undefined;
  }
  return terminal.kind === "rail_node" ? terminal.position : undefined;
}

function routeProposal(
  design: Design,
  request: {
    net_id: string;
    start: TerminalRef;
    end: TerminalRef;
    preferred_side?: "component" | "solder";
  },
): RouteProposal {
  const a = terminalPoint(design, request.start);
  const b = terminalPoint(design, request.end);
  if (!a || !b) throw { code: "route.invalid", message: "Terminal not found" };
  const vertices =
    a.x2 === b.x2 || a.y2 === b.y2 ? [a, b] : [a, { x2: b.x2, y2: a.y2 }, b];
  const length = vertices
    .slice(1)
    .reduce(
      (sum, point, index) =>
        sum +
        Math.hypot(
          point.x2 - vertices[index]!.x2,
          point.y2 - vertices[index]!.y2,
        ) /
          2,
      0,
    );
  return {
    net_id: request.net_id,
    start: request.start,
    end: request.end,
    side: request.preferred_side ?? "component",
    vertices,
    length_mm: length * design.board.pitch_mm,
    bend_count: vertices.length - 2,
    crossings: [],
    cost: {
      length: Math.round(length * 10),
      bends: (vertices.length - 2) * 30,
      cramped_bends: 0,
      crossings: 0,
      access: 0,
      side_transitions: 0,
      parallel_runs: 0,
      board_edges: 0,
      jumper_adjustment: 0,
      total: Math.round(length * 10) + (vertices.length - 2) * 30,
    },
    warnings: ["Preview backend: straight-line L route, obstacles ignored"],
  };
}

/**
 * Part designer in the preview: edits a copy of a library part in memory so
 * the designer can be exercised without the Rust draft engine. No validation.
 */
function draftCommands(
  library: ComponentDefinition[],
): Record<string, (args: Record<string, unknown>) => unknown> {
  interface Draft {
    definition: ComponentDefinition;
    documentation: string;
  }
  let current: Draft | null = null;
  let saved: Draft | null = null;
  let undo: Draft[] = [];
  let redo: Draft[] = [];
  let generation = 0;
  const snapshot = () =>
    current && {
      definition: clone(current.definition),
      documentation: current.documentation,
      valid:
        current.definition.pins.length > 0 &&
        Boolean(current.definition.id.trim()),
      dirty: JSON.stringify(current) !== JSON.stringify(saved),
      can_undo: undo.length > 0,
      can_redo: redo.length > 0,
      diagnostics: current.definition.id.trim()
        ? []
        : ["Choose a stable ID before saving."],
    };
  const load = () =>
    current && {
      definition: clone(current.definition),
      documentation: current.documentation,
      component_path: `preview://components/${current.definition.id || "new-part"}`,
      revision: String(generation),
      diagnostics: [],
    };
  const viewer = () => ({
    generation,
    watcher_status: "watching",
    attempted_at_ms: Date.now(),
    load: load(),
  });
  const open = (definition: ComponentDefinition) => {
    generation += 1;
    current = {
      definition: clone(definition),
      documentation: definition.documentation ?? "",
    };
    saved = clone(current);
    undo = [];
    redo = [];
    return viewer();
  };
  const apply = (operation: ComponentDraftOperation) => {
    if (!current) throw new Error("No part is open");
    const next = clone(current);
    const d = next.definition;
    switch (operation.type) {
      case "set_metadata":
        Object.assign(d, {
          id: operation.id,
          definition_version: operation.definition_version,
          name: operation.name,
          description: operation.description,
        });
        break;
      case "set_body":
        d.body = operation.body;
        break;
      case "set_origin":
        d.origin_x = operation.x;
        d.origin_y = operation.y;
        break;
      case "set_rotations":
        d.allowed_rotations = operation.rotations;
        break;
      case "upsert_pin": {
        const at = d.pins.findIndex(
          (pin) => pin.id === (operation.previous_id ?? operation.pin.id),
        );
        if (at >= 0) d.pins[at] = operation.pin;
        else d.pins.push(operation.pin);
        break;
      }
      case "remove_pin":
        d.pins = d.pins.filter((pin) => pin.id !== operation.id);
        break;
      case "reorder_pin": {
        const pin = d.pins.find((item) => item.id === operation.id);
        if (!pin) break;
        d.pins = d.pins.filter((item) => item.id !== operation.id);
        d.pins.splice(operation.index, 0, pin);
        break;
      }
      case "set_occupied_holes":
        d.occupied_holes = operation.holes;
        break;
      case "set_keepouts":
        d.keepouts = operation.keepouts;
        break;
      case "set_wire_keepouts":
        d.wire_keepouts = operation.keepouts;
        break;
      case "set_access_regions":
        d.access = operation.access;
        break;
      case "set_documentation":
        next.documentation = operation.documentation;
        break;
    }
    undo.push(current);
    redo = [];
    current = next;
    return snapshot();
  };
  const largest = () =>
    [...library]
      .filter((item) => item.kind !== "bridge")
      .sort((a, b) => b.pins.length - a.pins.length)[0]!;
  return {
    select_runtime_component: () => open(largest()),
    new_component_draft: () =>
      open({
        schema_version: 1,
        id: "",
        definition_version: "1.0.0",
        name: "New part",
        description: "",
        body: { x: -1.27, y: -1.27, width: 7.62, height: 2.54 },
        allowed_rotations: ["deg0", "deg90", "deg180", "deg270"],
        pins: [
          { id: "p1", number: "1", name: "1", x: 0, y: 0 },
          { id: "p2", number: "2", name: "2", x: 2, y: 0 },
        ],
      }),
    reload_runtime_component: () => (current ? viewer() : null),
    close_runtime_component: () => {
      current = null;
      return undefined;
    },
    get_component_draft: () => snapshot(),
    apply_component_draft_operation: ({ operation }) =>
      apply(operation as ComponentDraftOperation),
    undo_component_draft: () => {
      const previous = undo.pop();
      if (!previous || !current) throw new Error("Nothing to undo");
      redo.push(current);
      current = previous;
      return snapshot();
    },
    redo_component_draft: () => {
      const next = redo.pop();
      if (!next || !current) throw new Error("Nothing to redo");
      undo.push(current);
      current = next;
      return snapshot();
    },
    save_component_draft: () => {
      saved = clone(current);
      return { draft: snapshot(), load: load() };
    },
  };
}

export async function installMockBackend(): Promise<void> {
  let doc = await loadFixture();
  doc.revision ||= 1;
  let undoStack: Entry[] = [];
  let redoStack: Entry[] = [];
  const commits: Array<CommitSummary & { design: Design }> = Object.values(
    doc.commits ?? {},
  ).map((commit) => ({
    id: commit.id,
    parent: commit.parent,
    name: commit.name,
    description: commit.description,
    timestamp_ms: commit.timestamp_ms,
    source_revision: commit.source_revision,
    change_count: 0,
    origins: [],
    rationales: [],
    design: commit.design,
  }));
  let head = doc.head_commit;
  const library = Object.values(
    doc.design.component_definitions ?? {},
  ) as ComponentDefinition[];
  const listeners = new Map<number, (event: unknown) => void>();
  let callbackId = 1;

  const emit = () => {
    for (const callback of listeners.values())
      callback({
        event: "project-changed",
        id: 0,
        payload: { revision: doc.revision, kind: "operation", summary: "" },
      });
  };
  const view = (): ProjectView => ({
    schema_version: 1,
    project_id: doc.project_id,
    name: doc.name,
    revision: doc.revision,
    design: clone(doc.design),
    validation: validation(doc.design),
    commits: commits.map(({ design: _design, ...summary }) => summary),
    head_commit: head,
    head_design: commits.find((commit) => commit.id === head)?.design,
    undo: undoStack.map((entry) => entry.summary),
    redo: redoStack.map((entry) => entry.summary),
  });
  const stale = (expected: number) => {
    if (expected !== doc.revision)
      throw {
        code: "revision.stale",
        message: `Expected revision ${expected}, found ${doc.revision}`,
      };
  };
  const apply = (
    operations: OperationPayload[],
    origin: "user" | "mcp",
    rationale?: string,
  ) => {
    const before = clone(doc.design);
    const after = clone(doc.design);
    for (const op of operations) applyPayload(after, op);
    doc.design = after;
    doc.revision += 1;
    undoStack.push({
      before,
      after,
      summary: {
        operation_id: crypto.randomUUID(),
        origin,
        rationale,
        summary: operations.map(summaryOf).join(", "),
        diff: diffDesigns(before, after),
      },
    });
    redoStack = [];
    emit();
  };

  const commands: Record<string, (args: Record<string, unknown>) => unknown> = {
    get_bootstrap_info: () => ({
      app_name: "Perfboard Designer",
      version: "browser preview",
      schema_version: 1,
    }),
    get_project_view: () => view(),
    get_project: () => clone(doc),
    get_project_revision: () => doc.revision,
    get_net_connectivity: () => islands(doc.design),
    get_component_library: () => clone(library),
    get_commit_design: ({ commitId }) =>
      clone(commits.find((commit) => commit.id === commitId)!.design),
    get_undo_history_bytes: () => JSON.stringify(undoStack).length,
    compare_project_revisions: ({ from }) => {
      const commit = commits.find((item) => item.id === from);
      return commit ? diffDesigns(commit.design, doc.design) : emptyDiff();
    },
    apply_project_operation: ({ request }) => {
      const {
        operation_id: _id,
        expected_revision,
        origin: _origin,
        allow_invalid_placement: _allow,
        payload: _payload,
        rationale,
        ...payload
      } = request as Record<string, unknown>;
      stale(expected_revision as number);
      apply(
        [payload as OperationPayload],
        "user",
        rationale as string | undefined,
      );
      return {};
    },
    apply_project_transaction: ({ request }) => {
      const value = request as {
        expected_revision: number;
        operations: OperationPayload[];
        rationale?: string;
      };
      stale(value.expected_revision);
      apply(value.operations, "user", value.rationale);
      return {};
    },
    undo_project: ({ expectedRevision }) => {
      stale(expectedRevision as number);
      const entry = undoStack.pop();
      if (!entry) throw { code: "history.empty", message: "Nothing to undo" };
      doc.design = clone(entry.before);
      redoStack.push(entry);
      doc.revision += 1;
      emit();
    },
    redo_project: ({ expectedRevision }) => {
      stale(expectedRevision as number);
      const entry = redoStack.pop();
      if (!entry) throw { code: "history.empty", message: "Nothing to redo" };
      doc.design = clone(entry.after);
      undoStack.push(entry);
      doc.revision += 1;
      emit();
    },
    discard_project_changes: () => {
      const commit = commits.find((item) => item.id === head);
      if (commit) doc.design = clone(commit.design);
      undoStack = [];
      redoStack = [];
      doc.revision += 1;
      emit();
    },
    trim_project_undo_history: ({ keepEntries }) => {
      undoStack = undoStack.slice(
        -(keepEntries as number) || undoStack.length + 1,
      );
      if (keepEntries === 0) undoStack = [];
      redoStack = [];
      doc.revision += 1;
      emit();
      return {
        revision: doc.revision,
        summary: "Trimmed history",
        can_undo: undoStack.length > 0,
        can_redo: false,
        dirty: true,
      };
    },
    commit_project: ({ name, description }) => {
      const id = crypto.randomUUID();
      commits.push({
        id,
        parent: head,
        name: name as string,
        description: description as string,
        timestamp_ms: Date.now(),
        source_revision: doc.revision,
        change_count: undoStack.length,
        origins: [...new Set(undoStack.map((entry) => entry.summary.origin))],
        rationales: undoStack.flatMap((entry) =>
          entry.summary.rationale ? [entry.summary.rationale] : [],
        ),
        design: clone(doc.design),
      });
      head = id;
      undoStack = [];
      redoStack = [];
      doc.revision += 1;
      emit();
      return { id };
    },
    restore_project_commit: ({ commitId }) => {
      const commit = commits.find((item) => item.id === commitId)!;
      apply([], "user");
      doc.design = clone(commit.design);
      undoStack.at(-1)!.after = clone(commit.design);
      emit();
    },
    suggest_project_route: ({ request }) =>
      routeProposal(doc.design, request as Parameters<typeof routeProposal>[1]),
    evaluate_project_route: ({ request, side, vertices }) => ({
      ...routeProposal(
        doc.design,
        request as Parameters<typeof routeProposal>[1],
      ),
      side,
      vertices,
    }),
    create_project: ({ name, holesX, holesY, pitchMm }) => {
      doc = {
        schema_version: 1,
        project_id: crypto.randomUUID(),
        name: name as string,
        revision: 1,
        design: {
          board: {
            name: `${holesX}x${holesY} perfboard`,
            holes_x: holesX as number,
            holes_y: holesY as number,
            pitch_mm: pitchMm as number,
          },
        },
      };
      undoStack = [];
      redoStack = [];
      commits.length = 0;
      head = undefined;
      emit();
      return clone(doc);
    },
    save_project: () => undefined,
    export_pdf: ({ pages }) => {
      // Preview only: keep the pages for inspection; nothing is written.
      lastPdfPages = pages as string[];
      return null;
    },
    open_project: async () => {
      doc = await loadFixture();
      undoStack = [];
      redoStack = [];
      emit();
      return clone(doc);
    },
    pick_project_path: ({ save }) =>
      window.prompt(
        save
          ? "Save project as (preview: not written)"
          : "Open project path (preview: reloads fixture)",
        "nano-rs485.perfboard.json",
      ),
    ...draftCommands(library),
    "plugin:event|listen": ({ handler }) => handler,
    "plugin:event|unlisten": () => undefined,
  };

  const callbacks = new Map<number, (event: unknown) => void>();
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
    transformCallback(callback: (event: unknown) => void) {
      const id = callbackId++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
      listeners.delete(id);
    },
    async invoke(command: string, args: Record<string, unknown> = {}) {
      const handler = commands[command];
      if (!handler) {
        console.info(
          `[preview backend] ${command} is not available in the browser preview`,
        );
        throw {
          code: "preview.unsupported",
          message: `${command} needs the desktop app`,
        };
      }
      if (command === "plugin:event|listen") {
        const id = args.handler as number;
        listeners.set(id, callbacks.get(id)!);
        return id;
      }
      // Async like the real bridge, so ordering bugs still show up.
      await new Promise((resolve) => setTimeout(resolve, 12));
      return clone(await handler(args));
    },
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
    },
  };
  (
    window as unknown as Record<string, unknown>
  ).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: () => undefined,
  };

  (window as unknown as Record<string, unknown>).__mock = {
    doc: () => doc,
    /** SVG pages from the last `export_pdf` call. */
    pdfPages: () => lastPdfPages,
    /** Simulates an MCP client moving the first component one hole right. */
    agentEdit(rationale = "Moved the module to leave room for the USB cable") {
      const instance = Object.values(doc.design.components ?? {}).find(
        (item) =>
          doc.design.component_definitions?.[item.definition_id]?.kind !==
          "bridge",
      );
      if (!instance) return;
      apply(
        [
          {
            type: "move_component",
            instance_id: instance.id,
            position: { x: instance.position.x + 1, y: instance.position.y },
          },
        ],
        "mcp",
        rationale,
      );
    },
    agentProposeNet(name = "I2C_SDA") {
      apply(
        [
          {
            type: "define_net",
            net: {
              id: crypto.randomUUID(),
              name,
              state: "proposed",
              pins: [],
              notes: "Proposed by agent",
            },
          },
        ],
        "mcp",
        "The sensor needs a data line",
      );
    },
  };
}
