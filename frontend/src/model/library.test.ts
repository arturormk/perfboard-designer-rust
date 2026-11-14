import { describe, expect, it } from "vitest";
import type {
  ComponentDefinition,
  ComponentLibrary,
  LibraryLayer,
} from "../api/project";
import {
  browserSections,
  layerStatusLabel,
  libraryProblems,
  movedLayer,
  partProvenance,
  withLayer,
  withoutLayer,
} from "./library";

const definition = (id: string) => ({ id }) as ComponentDefinition;

const details: ComponentLibrary = {
  layers: [
    {
      kind: "project",
      template: "${PROJECT_DIR}/${PROJECT_STEM}.perfboard",
      path: "/work/board.perfboard",
      status: "loaded",
      component_count: 1,
    },
    {
      kind: "user",
      template: "${USER_COMPONENTS}",
      status: "not_granted",
      component_count: 0,
    },
    {
      kind: "standard",
      version: "0.1.40",
      status: "loaded",
      component_count: 2,
    },
  ],
  components: [
    {
      definition: definition("dev.perfboard.bridge"),
      layer: 0,
      shadows: [{ layer: 2, definition_version: "1.0.0" }],
    },
    { definition: definition("dev.perfboard.pin"), layer: 2 },
  ],
  diagnostics: [
    {
      code: "library.not_granted",
      severity: "error",
      message: "needs a user library",
      layer: 1,
    },
    {
      code: "library.note",
      severity: "advisory",
      message: "fyi",
      layer: 2,
    },
  ],
};

describe("part provenance", () => {
  it("labels project parts and names what they override", () => {
    const provenance = partProvenance(details, "dev.perfboard.bridge");
    expect(provenance?.badge).toBe("Project");
    expect(provenance?.overrides).toBe(true);
    expect(provenance?.detail).toBe(
      "${PROJECT_DIR}/${PROJECT_STEM}.perfboard → /work/board.perfboard\n" +
        "Overrides v1.0.0 from the standard library",
    );
  });

  it("leaves standard parts unlabelled", () => {
    const provenance = partProvenance(details, "dev.perfboard.pin");
    expect(provenance?.badge).toBeUndefined();
    expect(provenance?.overrides).toBe(false);
    expect(provenance?.detail).toBe("Standard library (built-in, v0.1.40)");
  });

  it("knows nothing of unknown parts or a missing library", () => {
    expect(partProvenance(details, "nope")).toBeUndefined();
    expect(partProvenance(null, "dev.perfboard.pin")).toBeUndefined();
  });

  it("reports errors and warnings but not advisories", () => {
    expect(libraryProblems(details).map((item) => item.code)).toEqual([
      "library.not_granted",
    ]);
    expect(libraryProblems(null)).toEqual([]);
  });
});

describe("editing the layer list", () => {
  const paths = ["a", "b", "c"];

  it("appends a layer once", () => {
    expect(withLayer(paths, "d")).toEqual(["a", "b", "c", "d"]);
    expect(withLayer(paths, "b")).toBe(paths);
  });

  it("removes a layer by position", () => {
    expect(withoutLayer(paths, 1)).toEqual(["a", "c"]);
  });

  it("moves a layer up or down and stops at the ends", () => {
    expect(movedLayer(paths, 1, -1)).toEqual(["b", "a", "c"]);
    expect(movedLayer(paths, 1, 1)).toEqual(["a", "c", "b"]);
    expect(movedLayer(paths, 0, -1)).toBe(paths);
    expect(movedLayer(paths, 2, 1)).toBe(paths);
  });
});

describe("layer status", () => {
  const layer = (
    status: LibraryLayer["status"],
    component_count = 0,
  ): LibraryLayer => ({ kind: "project", status, component_count });

  it("counts parts and names problems", () => {
    expect(layerStatusLabel(layer("loaded", 1))).toEqual({
      text: "1 part",
      tone: "ok",
    });
    expect(layerStatusLabel(layer("loaded", 3)).text).toBe("3 parts");
    expect(layerStatusLabel(layer("not_granted")).tone).toBe("error");
    expect(layerStatusLabel(layer("inactive")).text).toBe(
      "Save the project first",
    );
    expect(layerStatusLabel(undefined).tone).toBe("muted");
  });
});

describe("part browser", () => {
  it("lists each layer's parts, hidden ones under their own layer", () => {
    const sections = browserSections({
      ...details,
      components: [
        {
          definition: {
            id: "dev.perfboard.bridge",
            name: "Bridge",
            pins: [{}, {}],
          } as unknown as ComponentDefinition,
          layer: 0,
          directory: "/work/board.perfboard/bridge",
          shadows: [{ layer: 2, definition_version: "1.0.0" }],
        },
        {
          definition: {
            id: "a",
            name: "Amp",
            pins: [],
          } as unknown as ComponentDefinition,
          layer: 2,
        },
      ],
    });
    expect(sections.map((section) => section.layer.kind)).toEqual([
      "project",
      "user",
      "standard",
    ]);
    expect(sections[0]!.parts).toEqual([
      {
        id: "dev.perfboard.bridge",
        name: "Bridge",
        directory: "/work/board.perfboard/bridge",
        pinCount: 2,
      },
    ]);
    expect(sections[1]!.parts).toEqual([]);
    expect(
      sections[2]!.parts.map((part) => [part.name, part.hiddenBy]),
    ).toEqual([
      ["Amp", undefined],
      ["Bridge", 0],
    ]);
    expect(browserSections(null)).toEqual([]);
  });
});
