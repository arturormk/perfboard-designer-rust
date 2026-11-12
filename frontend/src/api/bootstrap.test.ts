import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { formatVersion, getBootstrapInfo } from "./bootstrap";

describe("Rust bootstrap boundary", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("invokes the typed Tauri command", async () => {
    vi.mocked(invoke).mockResolvedValue({
      app_name: "Perfboard Designer",
      version: "0.1.26",
      schema_version: 1,
    });

    await expect(getBootstrapInfo()).resolves.toEqual({
      app_name: "Perfboard Designer",
      version: "0.1.26",
      schema_version: 1,
    });
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("get_bootstrap_info");
  });

  it("prefixes numeric versions with v", () => {
    expect(formatVersion("0.1.26")).toBe("v0.1.26");
    expect(formatVersion("browser preview")).toBe("browser preview");
  });
});
