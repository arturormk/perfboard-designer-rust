/**
 * Edge labels for Assembly mode: the row and column names printed on the
 * physical perfboard, drawn in the board margin.
 *
 * Perfboards label their holes in different ways (1-based numbers, letters,
 * counted from either end). These labels only change what is drawn on the
 * board in Assembly mode. Internal coordinates (0-based, shown by the rulers
 * outside the board) stay the same everywhere else.
 *
 * The settings belong to the physical board a project is built on, so they are
 * stored per project in local storage, not in the project file. A project with
 * no settings yet starts from the last ones saved.
 */
import { signal } from "@preact/signals";

export type LabelStyle = "numbers" | "letters";

export interface AxisLabels {
  style: LabelStyle;
  /** Counts from the far end: the last column or row gets the first label. */
  reversed: boolean;
}

export interface EdgeLabels {
  enabled: boolean;
  columns: AxisLabels;
  rows: AxisLabels;
}

export const DEFAULT_EDGE_LABELS: EdgeLabels = {
  enabled: false,
  columns: { style: "numbers", reversed: false },
  rows: { style: "letters", reversed: false },
};

/** 0 → "A", 25 → "Z", 26 → "AA", as spreadsheet columns are named. */
export function letterName(index: number): string {
  let name = "";
  for (let n = index + 1; n > 0; n = Math.floor((n - 1) / 26))
    name = String.fromCharCode(65 + ((n - 1) % 26)) + name;
  return name;
}

/** The printed label of internal index `index` on an axis of `count` holes. */
export function axisLabel(
  index: number,
  count: number,
  axis: AxisLabels,
): string {
  const position = axis.reversed ? count - 1 - index : index;
  return axis.style === "letters" ? letterName(position) : String(position + 1);
}

const KEY_PREFIX = "perfboard.edgeLabels.";
const LAST_KEY = `${KEY_PREFIX}last`;

function parseAxis(value: unknown, fallback: AxisLabels): AxisLabels {
  const axis = (value ?? {}) as Partial<AxisLabels>;
  return {
    style:
      axis.style === "numbers" || axis.style === "letters"
        ? axis.style
        : fallback.style,
    reversed:
      typeof axis.reversed === "boolean" ? axis.reversed : fallback.reversed,
  };
}

/** Reads stored settings, keeping defaults for anything missing or invalid. */
export function parseEdgeLabels(text: string | null): EdgeLabels | null {
  if (!text) return null;
  try {
    const value = JSON.parse(text) as Partial<EdgeLabels> | null;
    if (!value || typeof value !== "object") return null;
    return {
      enabled:
        typeof value.enabled === "boolean"
          ? value.enabled
          : DEFAULT_EDGE_LABELS.enabled,
      columns: parseAxis(value.columns, DEFAULT_EDGE_LABELS.columns),
      rows: parseAxis(value.rows, DEFAULT_EDGE_LABELS.rows),
    };
  } catch {
    return null;
  }
}

function read(key: string): EdgeLabels | null {
  try {
    return parseEdgeLabels(localStorage.getItem(key));
  } catch {
    return null;
  }
}

/** Settings of the project last loaded with `loadEdgeLabels`. */
export const edgeLabels = signal<EdgeLabels>(DEFAULT_EDGE_LABELS);
let edgeLabelsProject: string | undefined;

export function loadEdgeLabels(projectId: string | undefined): void {
  if (projectId === edgeLabelsProject) return;
  edgeLabelsProject = projectId;
  edgeLabels.value =
    (projectId ? read(KEY_PREFIX + projectId) : null) ??
    read(LAST_KEY) ??
    DEFAULT_EDGE_LABELS;
}

export function setEdgeLabels(next: EdgeLabels): void {
  edgeLabels.value = next;
  const text = JSON.stringify(next);
  try {
    if (edgeLabelsProject)
      localStorage.setItem(KEY_PREFIX + edgeLabelsProject, text);
    localStorage.setItem(LAST_KEY, text);
  } catch {
    // Storage may be unavailable; the setting still applies this session.
  }
}
