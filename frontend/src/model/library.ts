/** How the palette describes where a part comes from (ADR 0014). */
import type {
  ComponentLibrary,
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
