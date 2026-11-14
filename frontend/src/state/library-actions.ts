/** Editing the open project's component libraries and grants (ADR 0014). */
import { signal } from "@preact/signals";
import {
  getLibrarySettings,
  grantUserLibrary,
  listLibraryGrants,
  pickComponentLayer,
  revokeLibraryGrant,
  revokeUserLibrary,
  setComponentPaths,
  type LibraryGrantEntry,
  type LibrarySettings,
} from "../api/project";
import { errorMessage } from "../error-message";
import { DEFAULT_PROJECT_LAYER, withLayer } from "../model/library";
import { refresh, refreshLibrary } from "./actions";
import { view } from "./store";
import { toast } from "./toasts";

export const librarySettings = signal<LibrarySettings | null>(null);
export const libraryGrants = signal<LibraryGrantEntry[]>([]);

/** The layers in effect: the project's list, or the default. */
export function effectivePaths(settings: LibrarySettings | null): string[] {
  return settings?.component_paths ?? [DEFAULT_PROJECT_LAYER];
}

export async function loadLibrarySettings(): Promise<void> {
  try {
    librarySettings.value = await getLibrarySettings();
  } catch (error) {
    toast("error", "Could not read the library settings", {
      detail: errorMessage(error),
    });
  }
}

async function afterChange(): Promise<void> {
  await Promise.all([refresh(), refreshLibrary(), loadLibrarySettings()]);
}

/** Replaces the project's layers; `null` restores the default. */
export async function updateComponentPaths(
  paths: string[] | null,
): Promise<void> {
  try {
    await setComponentPaths(view.peek()?.revision ?? 0, paths);
  } catch (error) {
    toast("error", "Could not change the component libraries", {
      detail: errorMessage(error),
    });
  }
  await afterChange();
}

export async function addLayerFromFolder(): Promise<void> {
  try {
    const entry = await pickComponentLayer();
    if (!entry) return;
    const current = effectivePaths(librarySettings.peek());
    if (current.includes(entry)) {
      toast("info", "That folder is already listed", { detail: entry });
      return;
    }
    await updateComponentPaths(withLayer(current, entry));
  } catch (error) {
    toast("error", "Could not add the folder", {
      detail: errorMessage(error),
    });
  }
}

export async function grantLibrary(): Promise<void> {
  try {
    const grant = await grantUserLibrary();
    if (!grant) return;
    toast("success", "User library granted to this project", {
      detail: grant.user_components,
    });
  } catch (error) {
    toast("error", "Could not grant the user library", {
      detail: errorMessage(error),
    });
  }
  await afterChange();
}

export async function revokeLibrary(): Promise<void> {
  try {
    await revokeUserLibrary();
  } catch (error) {
    toast("error", "Could not revoke the grant", {
      detail: errorMessage(error),
    });
  }
  await afterChange();
}

export async function loadLibraryGrants(): Promise<void> {
  try {
    libraryGrants.value = await listLibraryGrants();
  } catch (error) {
    toast("error", "Could not list the granted libraries", {
      detail: errorMessage(error),
    });
  }
}

export async function revokeGrantOf(projectId: string): Promise<void> {
  try {
    libraryGrants.value = await revokeLibraryGrant(projectId);
  } catch (error) {
    toast("error", "Could not revoke the grant", {
      detail: errorMessage(error),
    });
  }
  // The open project may be the one that lost its grant.
  await Promise.all([refreshLibrary(), loadLibrarySettings()]);
}
