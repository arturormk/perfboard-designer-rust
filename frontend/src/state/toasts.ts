import { signal } from "@preact/signals";

export type ToastTone = "info" | "success" | "error" | "agent";
export interface ToastAction {
  label: string;
  run: () => void;
}
export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  detail?: string;
  actions?: ToastAction[];
}

export const toasts = signal<Toast[]>([]);
let nextId = 1;
const timers = new Map<number, number>();

export function dismissToast(id: number): void {
  toasts.value = toasts.value.filter((toast) => toast.id !== id);
  const timer = timers.get(id);
  if (timer !== undefined) window.clearTimeout(timer);
  timers.delete(id);
}

export function toast(
  tone: ToastTone,
  title: string,
  options: {
    detail?: string;
    actions?: ToastAction[];
    durationMs?: number;
  } = {},
): number {
  const id = nextId++;
  const entry: Toast = {
    id,
    tone,
    title,
    detail: options.detail,
    actions: options.actions,
  };
  // Keep the stack short; the newest message matters most.
  toasts.value = [...toasts.value.slice(-3), entry];
  const duration =
    options.durationMs ??
    (tone === "error" ? 8000 : tone === "agent" ? 9000 : 3500);
  if (duration > 0)
    timers.set(
      id,
      window.setTimeout(() => dismissToast(id), duration),
    );
  return id;
}
