import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { applyOperation } from "./project";

describe("project operation boundary", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("allows temporary invalid placement for graphical edits", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await applyOperation(4, {
      type: "rotate_component",
      instance_id: "component-id",
      rotation: "deg90",
    });

    expect(invoke).toHaveBeenCalledWith("apply_project_operation", {
      request: expect.objectContaining({
        expected_revision: 4,
        origin: "user",
        allow_invalid_placement: true,
        type: "rotate_component",
        instance_id: "component-id",
        rotation: "deg90",
      }),
    });
  });
});
