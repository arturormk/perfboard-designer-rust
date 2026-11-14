import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { applyOperation, applyTransaction, getChangeMarker } from "./project";

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

  it("sends grouped changes through the atomic transaction command", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const operations = [
      {
        type: "move_component" as const,
        instance_id: "component-id",
        position: { x: 2, y: 3 },
      },
      {
        type: "rotate_component" as const,
        instance_id: "component-id",
        rotation: "deg90" as const,
      },
    ];

    await applyTransaction(7, operations, "Align module");

    expect(invoke).toHaveBeenCalledWith("apply_project_transaction", {
      request: expect.objectContaining({
        expected_revision: 7,
        origin: "user",
        rationale: "Align module",
        operations,
      }),
    });
  });

  it("reads the lightweight change marker for external-change polling", async () => {
    const marker = { revision: 12, notes_generation: 3 };
    vi.mocked(invoke).mockResolvedValue(marker);

    await expect(getChangeMarker()).resolves.toEqual(marker);
    expect(invoke).toHaveBeenCalledWith("get_change_marker");
  });
});
