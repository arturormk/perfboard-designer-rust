/** How the palette describes where a part comes from (ADR 0014). */
import type {
  ComponentLibrary,
  Diagnostic,
  LibraryDiagnostic,
  LibraryLayer,
} from "../api/project";

export interface PartProvenance {
  /** Short label for parts outside the standard library. */
  badge?: "Project" | "User";
  /** Tooltip: the layer, and what the part overrides. */
  detail: string;
  overrides: boolean;
}

function layerName(layer: LibraryLayer): string {
  switch (layer.kind) {
    case "project":
      return "project library";
    case "user":
      return "user library";
    case "standard":
      return "standard library";
  }
}

function layerLocation(layer: LibraryLayer): string {
  if (layer.kind === "standard")
    return layer.version
      ? `Standard library (built-in, v${layer.version})`
      : `Standard library (${layer.path ?? "override"})`;
  return `${layer.template ?? ""}${layer.path ? ` → ${layer.path}` : ""}`;
}

export function partProvenance(
  details: ComponentLibrary | null,
  definitionId: string,
): PartProvenance | undefined {
  const entry = details?.components.find(
    (component) => component.definition.id === definitionId,
  );
  const layer = entry && details?.layers[entry.layer];
  if (!entry || !layer) return undefined;
  const lines = [layerLocation(layer)];
  for (const shadow of entry.shadows ?? []) {
    const hidden = details.layers[shadow.layer];
    if (hidden)
      lines.push(
        `Overrides v${shadow.definition_version} from the ${layerName(hidden)}`,
      );
  }
  return {
    badge:
      layer.kind === "project"
        ? "Project"
        : layer.kind === "user"
          ? "User"
          : undefined,
    detail: lines.join("\n"),
    overrides: (entry.shadows?.length ?? 0) > 0,
  };
}

/** Library problems worth showing: errors and warnings. */
export function libraryProblems(
  details: ComponentLibrary | null,
): LibraryDiagnostic[] {
  return (details?.diagnostics ?? []).filter(
    (diagnostic) => diagnostic.severity !== "advisory",
  );
}

/** The layer used when a project lists none (mirrors the Rust default). */
export const DEFAULT_PROJECT_LAYER = "${PROJECT_DIR}/${PROJECT_STEM}.perfboard";
export const USER_LIBRARY_LAYER = "${USER_COMPONENTS}";

/** `paths` with `entry` appended, unless it is already there. */
export function withLayer(paths: string[], entry: string): string[] {
  return paths.includes(entry) ? paths : [...paths, entry];
}

export function withoutLayer(paths: string[], index: number): string[] {
  return paths.filter((_, at) => at !== index);
}

/** `paths` with the entry at `index` moved by `step` (−1 up, +1 down). */
export function movedLayer(
  paths: string[],
  index: number,
  step: -1 | 1,
): string[] {
  const target = index + step;
  if (index < 0 || index >= paths.length || target < 0) return paths;
  if (target >= paths.length) return paths;
  const next = [...paths];
  [next[index], next[target]] = [next[target]!, next[index]!];
  return next;
}

/** A short status for a layer row, and whether it is a problem. */
export function layerStatusLabel(layer: LibraryLayer | undefined): {
  text: string;
  tone: "ok" | "warn" | "error" | "muted";
} {
  if (!layer) return { text: "Not loaded yet", tone: "muted" };
  switch (layer.status) {
    case "loaded":
      return {
        text:
          layer.component_count === 1
            ? "1 part"
            : `${layer.component_count} parts`,
        tone: "ok",
      };
    case "missing":
      return { text: "Folder not found", tone: "warn" };
    case "not_granted":
      return { text: "No user library granted", tone: "error" };
    case "inactive":
      return { text: "Save the project first", tone: "muted" };
    case "duplicate":
      return { text: "Same folder as above", tone: "warn" };
    case "invalid":
      return { text: "Invalid", tone: "error" };
  }
}

/** Library problems as validation diagnostics, pointing at their layer. */
export function libraryValidation(
  details: ComponentLibrary | null,
): Diagnostic[] {
  return libraryProblems(details).map((problem) => ({
    code: problem.code,
    severity: problem.severity,
    message: problem.message,
    objects: [
      {
        kind: "component_layer",
        id: details?.layers[problem.layer]?.template ?? "standard",
      },
    ],
  }));
}
