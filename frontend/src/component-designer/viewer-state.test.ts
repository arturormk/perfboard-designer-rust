import { describe, expect, it } from "vitest";
import type {
  ComponentDefinition,
  ComponentViewerSnapshot,
  RuntimeComponentLoad,
} from "../api/project";
import {
  isActiveViewerSnapshot,
  transitionViewerState,
  type ComponentViewerState,
} from "./viewer-state";

const definition: ComponentDefinition = {
  schema_version: 1,
  id: "dev.example.watch",
  definition_version: "1.0.0",
  name: "Watched",
  description: "",
  body: { x: 0, y: 0, width: 2.54, height: 2.54 },
  allowed_rotations: ["deg0"],
  pins: [],
};

const load = (revision: string, valid: boolean): RuntimeComponentLoad => ({
  definition: valid ? { ...definition, name: revision } : undefined,
  component_path: "/library/example",
  revision,
  diagnostics: valid
    ? []
    : [
        {
          code: "component.invalid_toml",
          severity: "error",
          message: "invalid TOML",
          file: "component.toml",
        },
      ],
});

const snapshot = (
  generation: number,
  componentLoad?: RuntimeComponentLoad,
): ComponentViewerSnapshot => ({
  generation,
  watcher_status: "watching",
  attempted_at_ms: 1_000,
  load: componentLoad,
});

describe("component viewer live-reload state", () => {
  it("preserves the last valid preview through invalid edits and recovers", () => {
    const current: ComponentViewerState = {
      kind: "current",
      load: load("valid-1", true),
    };
    const stale = transitionViewerState(
      current,
      snapshot(4, load("invalid", false)),
    );
    expect(stale.kind).toBe("stale");
    if (stale.kind !== "stale") throw new Error("expected stale state");
    expect(stale.preview.revision).toBe("valid-1");
    expect(stale.attempted.revision).toBe("invalid");

    const recovered = transitionViewerState(
      stale,
      snapshot(4, load("valid-2", true)),
    );
    expect(recovered.kind).toBe("current");
    if (recovered.kind === "current") {
      expect(recovered.load.revision).toBe("valid-2");
    }
  });

  it("keeps state for status-only events and rejects late generations", () => {
    const current: ComponentViewerState = {
      kind: "current",
      load: load("valid", true),
    };
    expect(transitionViewerState(current, snapshot(8))).toBe(current);
    expect(isActiveViewerSnapshot(8, snapshot(8))).toBe(true);
    expect(isActiveViewerSnapshot(9, snapshot(8))).toBe(false);
    expect(isActiveViewerSnapshot(undefined, snapshot(8))).toBe(false);
  });

  it("shows a load error when no valid preview exists", () => {
    const failed = transitionViewerState(
      { kind: "no_component" },
      snapshot(2, load("missing", false)),
    );
    expect(failed.kind).toBe("load_error");
  });
});
