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

/** A measurement picked in the checks list: its test points are marked. */
export const selectedProbe = signal<{ checkId: string; index: number } | null>(
  null,
);

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
    const picked = selectedProbe.peek();
    if (
      picked &&
      !next.checks?.find((check) => check.id === picked.checkId)?.probes[
        picked.index
      ]
    )
      selectedProbe.value = null;
  } catch (error) {
    if (mine === request) breadboardError.value = errorMessage(error);
  }
}

export function toggleFocusNet(netId: string): void {
  focusNet.value = focusNet.peek() === netId ? null : netId;
}

export function toggleProbe(checkId: string, index: number): void {
  const picked = selectedProbe.peek();
  selectedProbe.value =
    picked?.checkId === checkId && picked.index === index
      ? null
      : { checkId, index };
}
