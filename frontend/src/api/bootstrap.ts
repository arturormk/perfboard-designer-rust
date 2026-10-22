import { invoke } from "@tauri-apps/api/core";

export interface BootstrapInfo {
  app_name: string;
  schema_version: number;
}

export function getBootstrapInfo(): Promise<BootstrapInfo> {
  return invoke<BootstrapInfo>("get_bootstrap_info");
}
