/**
 * Every user intent that touches the engine goes through this module, so that
 * refreshes, stale-revision handling and feedback stay consistent.
 */
import { batch } from "@preact/signals";
import {
  applyOperation,
  applyTransaction,
  commitProject,
  compareProjectRevisions,
  createProject,
  discardProjectChanges,
  evaluateProjectRoute,
  getCommitDesign,
  getComponentLibrary,
  getNetConnectivity,
  getProjectView,
  openProject,
  pickProjectPath,
  redoProject,
  restoreProjectCommit,
  saveProject,
  suggestProjectRoute,
  trimProjectUndoHistory,
  undoProject,
  type ComponentDefinition,
  type ElectricalNet,
  type HolePos,
  type OperationPayload,
  type PinRef,
  type ProjectView,
  type Rotation,
  type RouteProposal,
  type Side,
  type TerminalRef,
  type Wire,
  type WireColor,
} from "../api/project";
import { errorMessage } from "../error-message";
import {
  bodyBounds,
  bridgeRotation,
  definitionOf,
  componentSide,
  fromRoute,
  pinPosition,
  type Design,
  type Point,
} from "../model/geometry";
import {
  nextInstanceName,
  suggestedWireColor,
  wireLabels,
} from "../model/naming";
import { planWireReroutes } from "../editor/net-routing";
import {
  EMPTY_DIFF,
  activeNetId,
  busy,
  compareCommitId,
  compareDesign,
  comparison,
  connectivity,
  design,
  engine,
  filePath,
  flashIds,
  focusRequest,
  library,
  pendingRoute,
  rightTab,
  savedRevision,
  selection,
  side,
  tool,
  view,
  wireColorChoice,
  type Selection,
} from "./store";
import { toast } from "./toasts";

// ── Refresh ────────────────────────────────────────────────────────────────

const commitDesignCache = new Map<string, Design>();
let refreshInFlight: Promise<void> | undefined;
let refreshQueued = false;
let knownOperationIds: Set<string> | undefined;

function rememberPath(project: ProjectView, path: string | null): void {
  if (path) {
    localStorage.setItem(`perfboard.path.${project.project_id}`, path);
    localStorage.setItem("perfboard.lastPath", path);
    const recent = recentPaths().filter((item) => item !== path);
    localStorage.setItem(
      "perfboard.recent",
      JSON.stringify([path, ...recent].slice(0, 6)),
    );
  }
}

export function recentPaths(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem("perfboard.recent") ?? "[]");
    return Array.isArray(value)
      ? value.filter((item) => typeof item === "string")
      : [];
  } catch {
    return [];
  }
}

async function loadComparison(next: ProjectView): Promise<void> {
  const target = compareCommitId.value ?? next.head_commit;
  if (!target) {
    comparison.value = EMPTY_DIFF;
    compareDesign.value = undefined;
    return;
  }
  const [diff, committed] = await Promise.all([
    compareProjectRevisions(target),
    target === next.head_commit && next.head_design
      ? Promise.resolve(next.head_design)
      : commitDesignCache.get(target)
        ? Promise.resolve(commitDesignCache.get(target)!)
        : getCommitDesign(target).then((loaded) => {
            commitDesignCache.set(target, loaded);
            return loaded;
          }),
  ]);
  comparison.value = diff;
  compareDesign.value = committed;
}

function announceExternalChanges(
  previous: ProjectView | null,
  next: ProjectView,
): void {
  const ids = new Set(next.undo.map((entry) => entry.operation_id));
  if (
    !knownOperationIds ||
    !previous ||
    previous.project_id !== next.project_id
  ) {
    knownOperationIds = ids;
    return;
  }
  const fresh = next.undo.filter(
    (entry) =>
      !knownOperationIds!.has(entry.operation_id) && entry.origin === "mcp",
  );
  knownOperationIds = ids;
  if (!fresh.length) return;
  const touched = new Set<string>();
  for (const entry of fresh)
    for (const item of [
      ...entry.diff.components,
      ...entry.diff.wires,
      ...entry.diff.rails,
      ...entry.diff.nets,
    ])
      touched.add(item.object_id);
  flash(touched);
  const latest = fresh.at(-1)!;
  toast(
    "agent",
    fresh.length === 1
      ? latest.summary
      : `${fresh.length} changes from the agent`,
    {
      detail:
        latest.rationale ?? (fresh.length > 1 ? latest.summary : undefined),
      actions: [
        { label: "Review", run: () => (rightTab.value = "history") },
        { label: "Undo", run: () => void undo() },
      ],
    },
  );
}

export function flash(ids: Set<string>): void {
  if (!ids.size) return;
  flashIds.value = ids;
  window.setTimeout(() => {
    if (flashIds.value === ids) flashIds.value = new Set();
  }, 2600);
}

async function doRefresh(): Promise<void> {
  const [next, nets] = await Promise.all([
    getProjectView(),
    getNetConnectivity(),
  ]);
  const previous = view.peek();
  if (
    previous &&
    previous.project_id === next.project_id &&
    next.revision < previous.revision
  )
    return;
  await loadComparison(next);
  batch(() => {
    announceExternalChanges(previous, next);
    view.value = next;
    connectivity.value = nets;
    if (previous?.project_id !== next.project_id) {
      const stored = localStorage.getItem(`perfboard.path.${next.project_id}`);
      filePath.value = stored;
      savedRevision.value = stored ? next.revision : null;
    }
    const current = selection.peek();
    if (current && !objectExists(next.design, current)) selection.value = null;
    if (activeNetId.peek() && !next.design.nets?.[activeNetId.peek()!])
      activeNetId.value = undefined;
  });
}

/** Coalesces refresh requests; at most one is in flight and one queued. */
export function refresh(): Promise<void> {
  if (refreshInFlight) {
    refreshQueued = true;
    return refreshInFlight;
  }
  refreshInFlight = doRefresh()
    .catch((error: unknown) => {
      toast("error", "Could not read the design", {
        detail: errorMessage(error),
      });
    })
    .finally(() => {
      refreshInFlight = undefined;
      if (refreshQueued) {
        refreshQueued = false;
        void refresh();
      }
    });
  return refreshInFlight;
}

export async function refreshLibrary(announce = false): Promise<void> {
  try {
    library.value = await getComponentLibrary();
    if (announce)
      toast(
        "success",
        `Library refreshed · ${library.value.length} part types`,
      );
  } catch (error) {
    toast("error", "Could not load the component library", {
      detail: errorMessage(error),
    });
  }
}

function objectExists(target: Design, item: Selection): boolean {
  if (item.kind === "component") return Boolean(target.components?.[item.id]);
  if (item.kind === "wire") return Boolean(target.wires?.[item.id]);
  if (item.kind === "rail") return Boolean(target.rails?.[item.id]);
  return Boolean(target.nets?.[item.id]);
}

function isStale(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    Reflect.get(error, "code") === "revision.stale"
  );
}

async function guarded<T>(
  label: string,
  action: () => Promise<T>,
): Promise<T | undefined> {
  try {
    const result = await action();
    await refresh();
    return result;
  } catch (error) {
    if (isStale(error)) {
      await refresh();
      toast("info", "The design changed while you were editing", {
        detail:
          "Another client (probably the agent) updated it first. Your edit was not applied; please try again.",
      });
    } else {
      toast("error", label, { detail: errorMessage(error) });
      await refresh();
    }
    return undefined;
  }
}

function revision(): number {
  return view.peek()?.revision ?? 0;
}

/** Applies one or more operations atomically as a single undo step. */
export function mutate(
  operations: OperationPayload | OperationPayload[],
  options: { rationale?: string; failure?: string } = {},
): Promise<boolean> {
  const list = Array.isArray(operations) ? operations : [operations];
  return guarded(options.failure ?? "That change was rejected", async () => {
    if (list.length === 1 && !options.rationale)
      await applyOperation(revision(), list[0]!);
    else await applyTransaction(revision(), list, options.rationale);
    return true;
  }).then((result) => result ?? false);
}

// ── History ────────────────────────────────────────────────────────────────

export const undo = () =>
  guarded("Nothing to undo", () => undoProject(revision()));
export const redo = () =>
  guarded("Nothing to redo", () => redoProject(revision()));
export const discardChanges = () =>
  guarded("Could not discard changes", () => discardProjectChanges(revision()));
export const restoreCommit = (commitId: string) =>
  guarded("Could not restore that commit", () =>
    restoreProjectCommit(revision(), commitId),
  );
export const trimHistory = (keep: number) =>
  guarded("Could not trim history", () =>
    trimProjectUndoHistory(revision(), keep),
  );

export async function commit(
  name: string,
  description: string,
): Promise<boolean> {
  const result = await guarded("Could not commit", () =>
    commitProject(revision(), name, description),
  );
  if (result) {
    compareCommitId.value = undefined;
    await refresh();
    toast("success", `Committed “${name}”`);
  }
  return Boolean(result);
}

export async function compareWith(commitId: string | undefined): Promise<void> {
  compareCommitId.value = commitId;
  await refresh();
}

// ── Files ──────────────────────────────────────────────────────────────────

function suggestedFileName(): string {
  const name = view.peek()?.name ?? "untitled";
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return `${slug || "untitled"}.perfboard.json`;
}

export async function saveAs(): Promise<void> {
  try {
    const path = await pickProjectPath(true, suggestedFileName());
    if (!path) return;
    await saveProject(path);
    filePath.value = path;
    savedRevision.value = revision();
    rememberPath(view.peek()!, path);
    toast("success", "Project saved", { detail: path });
  } catch (error) {
    toast("error", "Could not save the project", {
      detail: errorMessage(error),
    });
  }
}

export async function save(): Promise<void> {
  const path = filePath.peek();
  if (!path) return saveAs();
  try {
    await saveProject(path);
    savedRevision.value = revision();
    rememberPath(view.peek()!, path);
    toast("success", "Saved", { detail: path, durationMs: 2000 });
  } catch (error) {
    toast("error", "Could not save the project", {
      detail: errorMessage(error),
    });
  }
}

function resetWorkspace(): void {
  batch(() => {
    selection.value = null;
    activeNetId.value = undefined;
    pendingRoute.value = null;
    compareCommitId.value = undefined;
    tool.value = "select";
    side.value = "component";
    knownOperationIds = undefined;
  });
  commitDesignCache.clear();
}

export async function openPath(path: string): Promise<void> {
  try {
    busy.value = "Opening project…";
    await openProject(path);
    resetWorkspace();
    await refresh();
    filePath.value = path;
    savedRevision.value = revision();
    rememberPath(view.peek()!, path);
    focusBoard();
    toast("success", `Opened ${view.peek()?.name ?? "project"}`, {
      detail: path,
    });
  } catch (error) {
    toast("error", "Could not open the project", {
      detail: errorMessage(error),
    });
  } finally {
    busy.value = null;
  }
}

export async function openDialog(): Promise<void> {
  try {
    const path = await pickProjectPath(false);
    if (path) await openPath(path);
  } catch (error) {
    toast("error", "Could not open the project", {
      detail: errorMessage(error),
    });
  }
}

export async function newBoard(
  name: string,
  holesX: number,
  holesY: number,
): Promise<void> {
  try {
    await createProject(name, holesX, holesY);
    resetWorkspace();
    filePath.value = null;
    await refresh();
    savedRevision.value = null;
    focusBoard();
    toast("success", `Created a ${holesX} × ${holesY} board`);
  } catch (error) {
    toast("error", "Could not create the board", {
      detail: errorMessage(error),
    });
  }
}

// ── Selection and camera ───────────────────────────────────────────────────

export function select(
  next: Selection | null,
  options: { focus?: boolean } = {},
): void {
  const current = design.peek();
  if (
    next &&
    current &&
    (next.kind === "component" || next.kind === "wire" || next.kind === "rail")
  ) {
    const objectSide = sideOf(current, next);
    if (objectSide && objectSide !== side.peek()) side.value = objectSide;
  }
  selection.value = next;
  if (next) rightTab.value = "inspect";
  if (next && options.focus) focusOn(next);
}

export function sideOf(target: Design, item: Selection): Side | undefined {
  if (item.kind === "wire") return target.wires?.[item.id]?.side;
  if (item.kind === "rail") return target.rails?.[item.id]?.side;
  if (item.kind === "component")
    return componentSide(definitionOf(target, target.components?.[item.id]));
  return undefined;
}

export function boundsOf(
  target: Design,
  item: Selection,
): { x: number; y: number; width: number; height: number } | undefined {
  const points: Point[] = [];
  if (item.kind === "component") {
    const instance = target.components?.[item.id];
    const definition = definitionOf(target, instance);
    if (instance && definition)
      return bodyBounds(instance, definition, target.board.pitch_mm);
  } else if (item.kind === "wire") {
    points.push(...(target.wires?.[item.id]?.vertices ?? []).map(fromRoute));
  } else if (item.kind === "rail") {
    points.push(...(target.rails?.[item.id]?.vertices ?? []).map(fromRoute));
  } else {
    for (const pin of target.nets?.[item.id]?.pins ?? []) {
      const point = pinPosition(target, pin);
      if (point) points.push(point);
    }
    for (const wire of Object.values(target.wires ?? {}))
      if (wire.net_id === item.id) points.push(...wire.vertices.map(fromRoute));
  }
  if (!points.length) return undefined;
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  return {
    x: Math.min(...xs),
    y: Math.min(...ys),
    width: Math.max(...xs) - Math.min(...xs),
    height: Math.max(...ys) - Math.min(...ys),
  };
}

export function focusOn(item: Selection): void {
  const current = design.peek();
  const bounds = current && boundsOf(current, item);
  if (bounds) focusRequest.value = { ...bounds, nonce: Date.now() };
}

export function focusBoard(): void {
  focusRequest.value = { x: 0, y: 0, width: 0, height: 0, nonce: -Date.now() };
}

// ── Components ─────────────────────────────────────────────────────────────

export async function placeComponent(
  definition: ComponentDefinition,
  position: HolePos,
  rotation: Rotation = "deg0",
): Promise<void> {
  const current = design.peek();
  if (!current) return;
  const id = crypto.randomUUID();
  const ok = await mutate(
    {
      type: "add_component",
      definition,
      instance: {
        id,
        definition_id: definition.id,
        name: nextInstanceName(current, definition),
        position,
        rotation,
        properties: {},
        notes: "",
      },
    },
    { failure: `Could not place ${definition.name}` },
  );
  if (ok) select({ kind: "component", id });
}

export function bridgeDefinition(): ComponentDefinition | undefined {
  return (
    library.peek().find((definition) => definition.kind === "bridge") ??
    Object.values(design.peek()?.component_definitions ?? {}).find(
      (definition) => definition.kind === "bridge",
    )
  );
}

export async function addBridge(from: Point, to: Point): Promise<void> {
  const definition = bridgeDefinition();
  const rotation = bridgeRotation(from, to);
  if (!definition || !rotation) {
    toast("error", "No bridge part is available in the library");
    return;
  }
  const current = design.peek();
  if (!current) return;
  await mutate(
    {
      type: "add_component",
      definition,
      instance: {
        id: crypto.randomUUID(),
        definition_id: definition.id,
        name: nextInstanceName(current, definition),
        position: from,
        rotation,
        properties: {},
        notes: "",
      },
    },
    { failure: "Could not place the bridge" },
  );
}

export function removeSelection(): void {
  const current = selection.peek();
  if (!current) return;
  if (current.kind === "component")
    void mutate({ type: "remove_component", instance_id: current.id });
  else if (current.kind === "wire")
    void mutate({ type: "remove_wire", wire_id: current.id });
  else if (current.kind === "rail")
    void mutate({ type: "remove_rail", rail_id: current.id });
  else void mutate({ type: "remove_net", net_id: current.id });
  selection.value = null;
}

// ── Nets ───────────────────────────────────────────────────────────────────

export async function createNet(
  name: string,
  state: ElectricalNet["state"] = "accepted",
  pins: PinRef[] = [],
): Promise<string | undefined> {
  const id = crypto.randomUUID();
  const ok = await mutate(
    { type: "define_net", net: { id, name, state, pins, notes: "" } },
    { failure: `Could not create net ${name}` },
  );
  if (!ok) return undefined;
  activeNetId.value = id;
  return id;
}

export function assignPin(netId: string | null, pin: PinRef): Promise<boolean> {
  const current = design.peek();
  const owner = Object.values(current?.nets ?? {}).find((net) =>
    net.pins?.some(
      (item) =>
        item.component_id === pin.component_id && item.pin_id === pin.pin_id,
    ),
  );
  const operations: OperationPayload[] = [];
  if (owner && owner.id !== netId)
    operations.push({ type: "unassign_pin", net_id: owner.id, pin });
  if (netId && owner?.id !== netId)
    operations.push({ type: "assign_pin", net_id: netId, pin });
  if (!operations.length) return Promise.resolve(true);
  return mutate(operations, { failure: "Could not change the pin's net" });
}

export function colorForNet(netId: string | undefined): WireColor {
  const choice = wireColorChoice.peek();
  if (choice !== "auto") return choice;
  return suggestedWireColor(
    netId ? design.peek()?.nets?.[netId]?.name : undefined,
  );
}

// ── Wires and routing ──────────────────────────────────────────────────────

export async function addWire(
  netId: string,
  start: TerminalRef,
  end: TerminalRef,
  vertices: Array<{ x2: number; y2: number }>,
): Promise<void> {
  const current = design.peek();
  if (!current) return;
  const operations: OperationPayload[] = [];
  for (const terminal of [start, end]) {
    if (terminal.kind !== "pin") continue;
    const owner = Object.values(current.nets ?? {}).find((net) =>
      net.pins?.some(
        (pin) =>
          pin.component_id === terminal.component_id &&
          pin.pin_id === terminal.pin_id,
      ),
    );
    if (!owner)
      operations.push({
        type: "assign_pin",
        net_id: netId,
        pin: { component_id: terminal.component_id, pin_id: terminal.pin_id },
      });
  }
  const wire: Wire = {
    id: crypto.randomUUID(),
    net_id: netId,
    start,
    end,
    side: side.peek(),
    color: colorForNet(netId),
    vertices,
    locked: false,
    notes: "",
  };
  operations.push({ type: "add_wire", wire });
  const ok = await mutate(operations, { failure: "Could not add the wire" });
  if (ok) {
    const label = wireLabels(design.peek()!).get(wire.id);
    toast("success", `${label ?? "Wire"} added`, { durationMs: 1800 });
  }
}

export async function proposeRoute(
  netId: string,
  start: TerminalRef,
  end: TerminalRef,
  replaceWire?: Wire,
): Promise<void> {
  try {
    busy.value = "Finding a route…";
    const proposal = await suggestProjectRoute({
      net_id: netId,
      start,
      end,
      preferred_side: replaceWire?.side ?? side.peek(),
      replace_wire_id: replaceWire?.id,
    });
    if (proposal.side !== side.peek()) side.value = proposal.side;
    pendingRoute.value = {
      proposal,
      color: replaceWire?.color ?? colorForNet(netId),
      replaceWireId: replaceWire?.id,
      newRouteOver: true,
    };
  } catch (error) {
    toast("error", "No route found", { detail: errorMessage(error) });
  } finally {
    busy.value = null;
  }
}

export async function acceptRoute(): Promise<void> {
  const pending = pendingRoute.peek();
  if (!pending) return;
  pendingRoute.value = null;
  const wireId = pending.replaceWireId ?? crypto.randomUUID();
  const ok = await mutate(
    {
      type: "accept_route_proposal",
      proposal: pending.proposal,
      wire_id: wireId,
      color: pending.color,
      notes: pending.replaceWireId
        ? (design.peek()?.wires?.[pending.replaceWireId]?.notes ?? "")
        : "Accepted automatic route",
      replace_wire_id: pending.replaceWireId,
      new_route_over: pending.proposal.crossings.length
        ? pending.newRouteOver
        : undefined,
    },
    { failure: "Could not accept the route" },
  );
  if (ok) select({ kind: "wire", id: wireId });
}

export function rejectRoute(): void {
  pendingRoute.value = null;
}

export async function evaluateWire(
  wire: Wire,
): Promise<RouteProposal | undefined> {
  try {
    return await evaluateProjectRoute(
      {
        net_id: wire.net_id,
        start: wire.start,
        end: wire.end,
        preferred_side: wire.side,
        replace_wire_id: wire.id,
      },
      wire.side,
      wire.vertices,
    );
  } catch (error) {
    toast("error", "Could not evaluate the wire", {
      detail: errorMessage(error),
    });
    return undefined;
  }
}

/** Reroutes every unlocked wire, one undo step per wire, like the engine allows. */
export async function rerouteAll(): Promise<void> {
  const snapshot = view.peek();
  if (!snapshot) return;
  const targets = planWireReroutes(snapshot);
  let completed = 0;
  let skipped = 0;
  busy.value = "Rerouting wires…";
  try {
    for (const target of targets) {
      const wire = design.peek()?.wires?.[target.wireId];
      if (!wire || wire.locked) {
        skipped += 1;
        continue;
      }
      const proposal = await suggestProjectRoute({
        net_id: wire.net_id,
        start: target.start,
        end: target.end,
        preferred_side: wire.side,
        replace_wire_id: wire.id,
      });
      await applyOperation(revision(), {
        type: "accept_route_proposal",
        proposal,
        wire_id: wire.id,
        color: wire.color,
        notes: wire.notes ?? "",
        replace_wire_id: wire.id,
        new_route_over: proposal.crossings.length ? true : undefined,
      });
      await refresh();
      completed += 1;
    }
    toast(
      "success",
      `Rerouted ${completed} wire${completed === 1 ? "" : "s"}`,
      {
        detail: skipped
          ? `${skipped} locked wire${skipped === 1 ? " was" : "s were"} left alone`
          : undefined,
      },
    );
  } catch (error) {
    toast("error", `Rerouted ${completed} wires, then stopped`, {
      detail: errorMessage(error),
    });
    await refresh();
  } finally {
    busy.value = null;
  }
}

export function engineReady(): boolean {
  return engine.peek().kind === "ready";
}
