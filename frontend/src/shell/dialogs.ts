import { signal } from "@preact/signals";

export type DialogKind = "new-board" | "commit" | "history" | "shortcuts";

export const openDialogKind = signal<DialogKind | null>(null);

export function openDialog(kind: DialogKind): void {
  openDialogKind.value = kind;
}

export function closeDialog(): void {
  openDialogKind.value = null;
}

export interface ConfirmRequest {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  resolve: (value: boolean) => void;
}

export const confirmRequest = signal<ConfirmRequest | null>(null);

/** In-app confirmation; native `window.confirm` is unreliable in webviews. */
export function confirmAction(
  title: string,
  message: string,
  confirmLabel = "Continue",
  danger = false,
): Promise<boolean> {
  return new Promise((resolve) => {
    confirmRequest.value = {
      title,
      message,
      confirmLabel,
      danger,
      resolve: (value) => {
        confirmRequest.value = null;
        resolve(value);
      },
    };
  });
}
