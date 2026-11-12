import { invoke } from "@tauri-apps/api/core";

export interface BootstrapInfo {
  app_name: string;
  /** Effective version, e.g. `0.1.26` (see ADR 0010). */
  version: string;
  schema_version: number;
}

export function getBootstrapInfo(): Promise<BootstrapInfo> {
  return invoke<BootstrapInfo>("get_bootstrap_info");
}

/** `v0.1.26` for a numeric version; anything else (the preview) verbatim. */
export function formatVersion(version: string): string {
  return /^\d/.test(version) ? `v${version}` : version;
}
