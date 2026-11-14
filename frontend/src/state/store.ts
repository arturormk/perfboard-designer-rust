/**
 * Application state as Preact signals.
 *
 * Rust owns the design. These signals hold the latest *view* of it plus purely
 * presentational UI state (selection, tool, camera, panel tabs). Nothing here
 * decides whether a design is valid; derived values are display aids only.
 */
import { computed, signal } from "@preact/signals";
import type {
  ComponentDefinition,
  ComponentLibrary,
  Diagnostic,
  NetConnectivity,
  RecentProject,
  PinRef,
  ProjectView,
  RouteProposal,
  SemanticDiff,
  Side,
  WireColor,
} from "../api/project";
import { computeAirwires } from "../model/ratsnest";
import { railLabels, saddleLabels, wireLabels } from "../model/naming";
import type { Design, Point } from "../model/geometry";

export const EMPTY_DIFF: SemanticDiff = {
  components: [],
  nets: [],
  wires: [],
  rails: [],
  saddles: [],
  constraints: [],
  routes: [],
};

export type EngineState =
  | { kind: "connecting" }
  | { kind: "ready"; preview: boolean }
  | { kind: "offline"; message: string };

export type Selection =
  | { kind: "component"; id: string }
  | { kind: "wire"; id: string }
  | { kind: "rail"; id: string }
  | { kind: "saddle"; id: string }
  | { kind: "net"; id: string };

export type HoverTarget =
  | { kind: "pin"; ref: PinRef }
  | { kind: "component"; id: string }
  | { kind: "wire"; id: string }
  | { kind: "rail"; id: string }
  | { kind: "saddle"; id: string }
  | { kind: "net"; id: string }
  | { kind: "airwire"; netId: string };

export type Tool = "select" | "wire" | "rail" | "saddle" | "bridge" | "resize";
export type AppMode = "board" | "assembly" | "components";
export type PinLabelMode = "off" | "numbers" | "names";

export interface PendingRoute {
  proposal: RouteProposal;
  color: WireColor;
  replaceWireId?: string;
  newRouteOver: boolean;
}

// ── Engine data ────────────────────────────────────────────────────────────
/** Application version from the backend; empty until it has answered. */
export const appVersion = signal("");
export const engine = signal<EngineState>({ kind: "connecting" });
export const view = signal<ProjectView | null>(null);
export const library = signal<ComponentDefinition[]>([]);
/** Layers, provenance and diagnostics behind `library`. */
export const libraryDetails = signal<ComponentLibrary | null>(null);
export const connectivity = signal<NetConnectivity[]>([]);
/** Design compared against the working copy (head commit unless chosen). */
export const compareCommitId = signal<string | undefined>(undefined);
export const compareDesign = signal<Design | undefined>(undefined);
export const comparison = signal<SemanticDiff>(EMPTY_DIFF);

// ── File state ─────────────────────────────────────────────────────────────
export const filePath = signal<string | null>(
  localStorage.getItem("perfboard.lastPath"),
);
export const savedRevision = signal<number | null>(null);
/** Most recent first; persisted by the backend in the app's config folder. */
export const recentProjects = signal<RecentProject[]>([]);

// ── UI state ───────────────────────────────────────────────────────────────
export const appMode = signal<AppMode>("board");
export const tool = signal<Tool>("select");
export const selection = signal<Selection | null>(null);
export const hover = signal<HoverTarget | null>(null);
export const cursorHole = signal<Point | null>(null);
export const activeNetId = signal<string | undefined>(undefined);
export const wireColorChoice = signal<WireColor | "auto">("auto");
export const pendingRoute = signal<PendingRoute | null>(null);
export const placing = signal<ComponentDefinition | null>(null);
export const busy = signal<string | null>(null);

export const side = signal<Side>("component");
export const fadeComponents = signal(false);
export const pinLabels = signal<PinLabelMode>("numbers");
export const showRatsnest = signal(true);
export const showChanges = signal(false);
export const showKeepouts = signal(false);
export const splitCompare = signal(false);

export const leftTab = signal<"parts" | "nets">("parts");
export const rightTab = signal<"inspect" | "problems" | "history">("inspect");
export const leftOpen = signal(true);
export const rightOpen = signal(true);

/** Objects briefly highlighted after an agent change or history hover. */
export const flashIds = signal<ReadonlySet<string>>(new Set());
/** Objects highlighted while hovering a history entry. */
export const historyHighlight = signal<ReadonlySet<string>>(new Set());
/** Requests the canvas to frame these board-space bounds. */
export const focusRequest = signal<{
  x: number;
  y: number;
  width: number;
  height: number;
  nonce: number;
} | null>(null);

// ── Derived ────────────────────────────────────────────────────────────────
export const design = computed(() => view.value?.design);
export const diagnostics = computed<Diagnostic[]>(
  () => view.value?.validation ?? [],
);
export const problemCounts = computed(() => {
  const counts = { error: 0, warning: 0, advisory: 0 };
  for (const item of diagnostics.value) counts[item.severity] += 1;
  return counts;
});
export const wireLabelMap = computed(() =>
  design.value ? wireLabels(design.value) : new Map<string, string>(),
);
export const railLabelMap = computed(() =>
  design.value ? railLabels(design.value) : new Map<string, string>(),
);
export const saddleLabelMap = computed(() =>
  design.value ? saddleLabels(design.value) : new Map<string, string>(),
);
export const airwires = computed(() =>
  design.value ? computeAirwires(design.value, connectivity.value) : [],
);
export const unroutedByNet = computed(() => {
  const counts = new Map<string, number>();
  for (const airwire of airwires.value)
    counts.set(airwire.netId, (counts.get(airwire.netId) ?? 0) + 1);
  return counts;
});
/** Object id → change kind relative to the compared commit. */
export const changeKinds = computed(() => {
  const diff = comparison.value;
  return new Map(
    [...diff.components, ...diff.wires, ...diff.rails, ...diff.nets].map(
      (entry) => [entry.object_id, entry.kind],
    ),
  );
});
export const changeCount = computed(() => {
  const diff = comparison.value;
  return (
    diff.components.length +
    diff.nets.length +
    diff.wires.length +
    diff.rails.length +
    diff.constraints.length
  );
});
export const dirty = computed(
  () =>
    view.value !== null &&
    savedRevision.value !== null &&
    view.value.revision !== savedRevision.value,
);
export const proposedNets = computed(() =>
  Object.values(design.value?.nets ?? {}).filter(
    (net) => net.state === "proposed",
  ),
);
/** The net whose wires and pins should be emphasised on the board. */
export const emphasisNetId = computed<string | undefined>(() => {
  const target = hover.value;
  if (target?.kind === "net") return target.id;
  if (target?.kind === "airwire") return target.netId;
  const current = selection.value;
  if (current?.kind === "net") return current.id;
  if (current?.kind === "wire")
    return design.value?.wires?.[current.id]?.net_id;
  if (current?.kind === "rail")
    return design.value?.rails?.[current.id]?.net_id;
  if (tool.value === "wire" || tool.value === "rail") return activeNetId.value;
  return undefined;
});
/** Asks the canvas to zoom about its centre (keyboard shortcuts). */
export const zoomRequest = signal<{ factor: number; nonce: number } | null>(
  null,
);
/**
 * Whether the working copy differs from the head commit. Undo steps do not
 * survive reopening a file, so the semantic diff is the better signal when it
 * is measured against the head.
 */
export const canCommit = computed(() => {
  const current = view.value;
  if (!current) return false;
  if (!current.head_commit || current.undo.length > 0) return true;
  return compareCommitId.value === undefined && changeCount.value > 0;
});
