import { signal } from "@preact/signals";

const STORAGE_KEY = "perfboard.leftPanelWidth";
export const LEFT_PANEL_MIN = 200;
export const LEFT_PANEL_MAX = 640;

/** Largest width that still leaves the canvas usable in this window. */
function maxWidth(): number {
  return Math.max(
    LEFT_PANEL_MIN,
    Math.min(LEFT_PANEL_MAX, Math.round(window.innerWidth * 0.45)),
  );
}

export function clampLeftPanelWidth(width: number): number {
  return Math.round(Math.min(maxWidth(), Math.max(LEFT_PANEL_MIN, width)));
}

function loadWidth(): number | null {
  const stored = Number(localStorage.getItem(STORAGE_KEY));
  return Number.isFinite(stored) && stored > 0 ? stored : null;
}

/**
 * The user's left panel width in pixels, or null for the stylesheet default
 * (which shrinks on narrow windows). Kept across sessions.
 */
export const leftPanelWidth = signal<number | null>(loadWidth());

function setWidth(width: number | null) {
  leftPanelWidth.value = width;
  if (width === null) localStorage.removeItem(STORAGE_KEY);
  else localStorage.setItem(STORAGE_KEY, String(width));
}

/**
 * Inline style that overrides `--panel-left-w` when the user chose a width.
 * The `min()` keeps a wide panel from crowding the canvas after the window
 * shrinks, without needing a resize listener.
 */
export function leftPanelStyle(): Record<string, string> | undefined {
  const width = leftPanelWidth.value;
  return width === null
    ? undefined
    : { "--panel-left-w": `min(${width}px, 45vw)` };
}

/**
 * Drag handle on the left panel's right edge. Drag to resize; double-click to
 * return to the default width. Arrow keys nudge it when focused.
 */
export function PanelResizer() {
  function currentWidth(handle: HTMLElement): number {
    const panel = handle.previousElementSibling as HTMLElement | null;
    return panel?.getBoundingClientRect().width ?? LEFT_PANEL_MIN;
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    const startX = event.clientX;
    const startWidth = currentWidth(handle);
    handle.setPointerCapture(event.pointerId);
    document.body.classList.add("resizing-panel");

    const onMove = (move: PointerEvent) => {
      leftPanelWidth.value = clampLeftPanelWidth(
        startWidth + move.clientX - startX,
      );
    };
    const onUp = () => {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
      document.body.classList.remove("resizing-panel");
      setWidth(leftPanelWidth.value);
    };
    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    event.stopPropagation();
    const step = event.shiftKey ? 48 : 16;
    const start = currentWidth(event.currentTarget as HTMLElement);
    setWidth(
      clampLeftPanelWidth(start + (event.key === "ArrowLeft" ? -step : step)),
    );
  }

  return (
    <div
      class="panel-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize left panel"
      title="Drag to resize · double-click to reset"
      tabIndex={0}
      onPointerDown={onPointerDown}
      onDblClick={() => setWidth(null)}
      onKeyDown={onKeyDown}
    />
  );
}
