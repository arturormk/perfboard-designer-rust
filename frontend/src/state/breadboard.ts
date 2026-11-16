/**
 * Breadboard view state (ADR 0018): the layout derived by Rust for the
 * current revision, and what the user is pointing at. Nothing here edits the
 * design.
 */
import { signal } from "@preact/signals";
import { getBreadboardLayout, type BreadboardLayout } from "../api/project";
import { errorMessage } from "../error-message";

export const breadboard = signal<BreadboardLayout | null>(null);
export const breadboardError = signal<string | null>(null);
/** A net picked in the jumper list: its jumpers stand out, the rest fade. */
export const focusNet = signal<string | null>(null);
/** The jumper under the pointer, on the board or in the list. */
export const hoverJumper = signal<string | null>(null);

let request = 0;

/** Derives the layout for the current revision; a newer request wins. */
export async function loadBreadboard(): Promise<void> {
  const mine = ++request;
  try {
    const next = await getBreadboardLayout();
    if (mine !== request) return;
    breadboard.value = next;
    breadboardError.value = null;
    if (
      focusNet.peek() &&
      !next.jumpers.some((jumper) => jumper.net_id === focusNet.peek())
    )
      focusNet.value = null;
  } catch (error) {
    if (mine === request) breadboardError.value = errorMessage(error);
  }
}

export function toggleFocusNet(netId: string): void {
  focusNet.value = focusNet.peek() === netId ? null : netId;
}
