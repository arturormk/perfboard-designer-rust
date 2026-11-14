import { describe, expect, it } from "vitest";
import type { ComponentDefinition, ComponentLibrary } from "../api/project";
import { libraryProblems, partProvenance } from "./library";

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
