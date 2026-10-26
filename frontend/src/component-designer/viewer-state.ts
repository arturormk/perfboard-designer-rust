import type {
  ComponentViewerSnapshot,
  RuntimeComponentLoad,
} from "../api/project";

export type ComponentViewerState =
  | { kind: "no_component" }
  | { kind: "loading" }
  | { kind: "current"; load: RuntimeComponentLoad }
  | {
      kind: "stale";
      preview: RuntimeComponentLoad;
      attempted: RuntimeComponentLoad;
    }
  | { kind: "load_error"; load: RuntimeComponentLoad };

export function displayedLoad(
  state: ComponentViewerState,
): RuntimeComponentLoad | undefined {
  if (state.kind === "current") return state.load;
  if (state.kind === "stale") return state.preview;
  return undefined;
}

export function transitionViewerState(
  current: ComponentViewerState,
  snapshot: ComponentViewerSnapshot,
): ComponentViewerState {
  const load = snapshot.load;
  if (!load) return current;
  if (load.definition) return { kind: "current", load };
  const preview = displayedLoad(current);
  return preview?.definition
    ? { kind: "stale", preview, attempted: load }
    : { kind: "load_error", load };
}

export function isActiveViewerSnapshot(
  activeGeneration: number | undefined,
  snapshot: ComponentViewerSnapshot,
): boolean {
  return (
    activeGeneration !== undefined && snapshot.generation === activeGeneration
  );
}
