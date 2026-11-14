import { signal } from "@preact/signals";

export type DialogKind =
  | "new-board"
  | "commit"
  | "history"
  | "shortcuts"
  | "libraries"
  | "preferences";

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

export interface Choice {
  value: string;
  label: string;
  tone?: "primary" | "danger" | "ghost";
}

export interface ChoiceRequest {
  title: string;
  message: string;
  /** Shown after Cancel, in order; the last one has focus. */
  choices: Choice[];
  resolve: (value: string) => void;
}

export const choiceRequest = signal<ChoiceRequest | null>(null);

/**
 * In-app question with several answers plus Cancel. Resolves the chosen
 * value, or "cancel". A second request while one is open answers the first
 * with "cancel".
 */
export function chooseAction(
  request: Omit<ChoiceRequest, "resolve">,
): Promise<string> {
  choiceRequest.peek()?.resolve("cancel");
  return new Promise((resolve) => {
    const entry: ChoiceRequest = {
      ...request,
      resolve: (value) => {
        if (choiceRequest.peek() === entry) choiceRequest.value = null;
        resolve(value);
      },
    };
    choiceRequest.value = entry;
  });
}
