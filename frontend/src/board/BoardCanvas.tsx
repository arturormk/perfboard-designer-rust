/**
 * Interactive board editor: camera, tools, gestures and on-canvas overlays.
 *
 * All edits are sent as engine operations through `state/actions`. Local
 * state here only covers in-progress gestures (drags, drafts, previews).
 */
import { useComputed, useSignal, useSignalEffect } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";
import type {
  ComponentDefinition,
  ElectricalNet,
  Rotation,
  TerminalRef,
} from "../api/project";
import {
  addBridge,
  addWire,
  acceptRoute,
  assignPin,
  createNet,
  mutate,
  placeComponent,
  proposeRoute,
  rejectRoute,
  removeRailNode,
  select,
} from "../state/actions";
import {
  activeNetId,
  airwires,
  changeKinds,
  compareDesign,
  cursorHole,
  design as designSignal,
  emphasisNetId,
  fadeComponents,
  flashIds,
  focusRequest,
  historyHighlight,
  hover,
  pendingRoute,
  pinLabels,
  placing,
  railLabelMap,
  saddleLabelMap,
  selection,
  showChanges,
  showKeepouts,
  showRatsnest,
  side,
  splitCompare,
  tool,
  view,
  wireLabelMap,
  zoomRequest,
  type HoverTarget,
} from "../state/store";
import { toast } from "../state/toasts";
import {
  adjacentHoles,
  bodyBounds,
  bridgeRotation,
  definitionOf,
  nextRotation,
  pinBoardPosition,
  rotationDegrees,
  viewX,
  type Design,
  type Point,
  type RoutePoint,
} from "../model/geometry";
import { snappedDragPosition } from "../editor/drag";
import {
  planBoardResize,
  type BoardResizePlan,
  type ResizeHandle,
} from "../editor/board-size";
import {
  appendOrthogonal,
  closestWireSegmentIndex,
  finishWireNodeDrag,
  insertRailNode,
  insertWireNode,
  snappedRoutePoint,
} from "../editor/wire-routing";
import {
  netForWireStart,
  netsAtPin,
  parsePinKey,
  terminalForWireEnd,
} from "./hit";
import { BoardBase } from "./BoardBase";
import { BoardScene, TEXT_CENTER_DY, defaultViewBox } from "./BoardScene";
import { CanvasHud } from "./CanvasHud";
import { HoverCard } from "./HoverCard";

type ViewBox = [number, number, number, number];

type Gesture =
  | {
      kind: "pan";
      pointerId: number;
      client: Point;
      start: ViewBox;
      moved: boolean;
      deselect: boolean;
    }
  | {
      kind: "move";
      pointerId: number;
      id: string;
      start: Point;
      origin: Point;
      current: Point;
    }
  | {
      kind: "vertex";
      pointerId: number;
      wireId: string;
      index: number;
      vertices: RoutePoint[];
      current: RoutePoint;
      endpoint?: "start" | "end";
    }
  | {
      kind: "resize";
      pointerId: number;
      handle: ResizeHandle;
      start: Point;
      plan: BoardResizePlan;
      dx: number;
      dy: number;
    };

interface WireDraft {
  netId: string;
  /** The start had no net of its own and took the active net from the tool
   * bar, so the wire may still adopt the net of the terminal it ends on. */
  netFromActive?: boolean;
  start: TerminalRef;
  vertices: RoutePoint[];
}

interface NetPrompt {
  x: number;
  y: number;
  pin: { component_id: string; pin_id: string };
}

const MIN_WIDTH = 4;
const MAX_WIDTH = 400;

function clientToCanonical(
  svg: SVGSVGElement,
  clientX: number,
  clientY: number,
): Point | undefined {
  const geometry = svg.querySelector<SVGGElement>(".scene .geometry");
  const matrix = geometry?.getScreenCTM();
  if (!matrix) return undefined;
  const point = new DOMPoint(clientX, clientY).matrixTransform(
    matrix.inverse(),
  );
  return { x: point.x, y: point.y };
}

function clientToView(
  svg: SVGSVGElement,
  clientX: number,
  clientY: number,
): Point | undefined {
  const matrix = svg.getScreenCTM();
  if (!matrix) return undefined;
  const point = new DOMPoint(clientX, clientY).matrixTransform(
    matrix.inverse(),
  );
  return { x: point.x, y: point.y };
}

/**
 * Maps a client point to view units for `box`, computed from the pane's
 * layout box rather than the SVG's CTM: during a zoom preview the SVG carries
 * a CSS transform and an outdated viewBox, but the camera has already moved.
 */
function clientToBox(
  svg: SVGSVGElement,
  box: ViewBox,
  clientX: number,
  clientY: number,
): Point | undefined {
  const host = svg.parentElement?.getBoundingClientRect();
  if (!host || !svg.clientWidth || !svg.clientHeight) return undefined;
  const [x, y, width, height] = box;
  const scale = Math.min(svg.clientWidth / width, svg.clientHeight / height);
  const offsetX = (svg.clientWidth - width * scale) / 2;
  const offsetY = (svg.clientHeight - height * scale) / 2;
  return {
    x: x + (clientX - host.left - offsetX) / scale,
    y: y + (clientY - host.top - offsetY) / scale,
  };
}

/**
 * CSS transform that makes an SVG drawn with viewBox `shown` look like it was
 * drawn with `live` (both under preserveAspectRatio="xMidYMid meet").
 */
function previewTransform(
  svg: SVGSVGElement,
  shown: ViewBox,
  live: ViewBox,
): string {
  const width = svg.clientWidth;
  const height = svg.clientHeight;
  const map = (box: ViewBox) => {
    const scale = Math.min(width / box[2], height / box[3]);
    return {
      scale,
      ox: (width - box[2] * scale) / 2,
      oy: (height - box[3] * scale) / 2,
    };
  };
  const from = map(shown);
  const to = map(live);
  const k = to.scale / from.scale;
  const tx = -k * from.ox + to.scale * (shown[0] - live[0]) + to.ox;
  const ty = -k * from.oy + to.scale * (shown[1] - live[1]) + to.oy;
  return `matrix(${k}, 0, 0, ${k}, ${tx}, ${ty})`;
}

/** Quiet time after the last wheel step before the full-quality redraw. */
const ZOOM_SETTLE_MS = 250;

function contentViewBox(
  target: Design,
  currentSide: "component" | "solder",
): ViewBox {
  const [x, y, width, height] = defaultViewBox(target);
  let minX = x;
  let minY = y;
  let maxX = x + width;
  let maxY = y + height;
  for (const instance of Object.values(target.components ?? {})) {
    const definition = definitionOf(target, instance);
    if (!definition) continue;
    const bounds = bodyBounds(instance, definition, target.board.pitch_mm);
    const left = viewX(
      bounds.x + bounds.width,
      target.board.holes_x,
      currentSide,
    );
    const right = viewX(bounds.x, target.board.holes_x, currentSide);
    minX = Math.min(minX, Math.min(left, right) - 1);
    maxX = Math.max(maxX, Math.max(left, right) + 1);
    minY = Math.min(minY, bounds.y - 1);
    maxY = Math.max(maxY, bounds.y + bounds.height + 1);
  }
  // Head-room for the floating HUD bars above and below the board.
  minY -= 1.6;
  maxY += 1.6;
  return [minX, minY, maxX - minX, maxY - minY];
}

export function BoardCanvas() {
  const svgRef = useRef<SVGSVGElement>(null);
  const compareRef = useRef<SVGSVGElement>(null);
  const viewBox = useSignal<ViewBox | null>(null);
  const gesture = useSignal<Gesture | null>(null);
  const wireDraft = useSignal<WireDraft | null>(null);
  const railDraft = useSignal<RoutePoint[] | null>(null);
  const bridgeStart = useSignal<Point | null>(null);
  const saddleStart = useSignal<Point | null>(null);
  const cursor = useSignal<Point | null>(null);
  const placementRotation = useSignal<Rotation>("deg0");
  const netPrompt = useSignal<NetPrompt | null>(null);
  const hoverClient = useSignal<{ x: number; y: number } | null>(null);
  const spaceDown = useSignal(false);
  // `viewBox` is the live camera. `shownBox` is what the SVG is actually drawn
  // with: re-rasterizing the scene at a new scale is slow in WebKitGTK, so
  // while the wheel is turning the SVG keeps its old viewBox and is scaled
  // with a cheap CSS transform; the real redraw happens once zooming settles.
  // Both are bound straight to the DOM, so camera changes never re-render.
  const shownBox = useSignal<ViewBox | null>(null);
  const zoomPreview = useRef<number | null>(null);
  const viewBoxAttr = useComputed(
    () => (shownBox.value ?? viewBox.value)?.join(" ") ?? "",
  );
  const zoomPercent = useComputed(() => {
    const box = viewBox.value;
    const target = designSignal.value;
    if (!box || !target) return 100;
    return Math.round((contentViewBox(target, side.value)[2] / box[2]) * 100);
  });

  const current = designSignal.value;

  // Reset drafts whenever the tool changes.
  useSignalEffect(() => {
    void tool.value; // subscribe
    wireDraft.value = null;
    railDraft.value = null;
    bridgeStart.value = null;
    saddleStart.value = null;
    netPrompt.value = null;
  });

  useSignalEffect(() => {
    if (placing.value) placementRotation.value = "deg0";
  });

  // Frame the board or an object on request.
  useSignalEffect(() => {
    const request = focusRequest.value;
    const target = designSignal.peek();
    if (!request || !target) return;
    if (request.nonce < 0) {
      viewBox.value = contentViewBox(target, side.peek());
      return;
    }
    const left = viewX(request.x, target.board.holes_x, side.peek());
    const right = viewX(
      request.x + request.width,
      target.board.holes_x,
      side.peek(),
    );
    const pad = 3;
    const width = Math.max(request.width + pad * 2, 10);
    const height = Math.max(request.height + pad * 2, 8);
    const centerX = (left + right) / 2;
    const centerY = request.y + request.height / 2;
    viewBox.value = [centerX - width / 2, centerY - height / 2, width, height];
  });

  useSignalEffect(() => {
    const request = zoomRequest.value;
    if (request) zoomCenter(request.factor);
  });

  // Mirror the camera when flipping sides so the same region stays in view.
  const lastSide = useRef(side.peek());
  useSignalEffect(() => {
    const currentSide = side.value;
    const target = designSignal.peek();
    const box = viewBox.peek();
    if (lastSide.current !== currentSide && target && box) {
      const [x, y, width, height] = box;
      viewBox.value = [
        target.board.holes_x - 1 - (x + width),
        y,
        width,
        height,
      ];
    }
    lastSide.current = currentSide;
  });

  useEffect(() => {
    const down = (event: KeyboardEvent) => {
      const typing = (event.target as HTMLElement | null)?.closest(
        "input, textarea, select, [contenteditable]",
      );
      if (typing) return;
      if (event.key === " ") {
        spaceDown.value = true;
        if (event.target === document.body) event.preventDefault();
      }
      if (event.key === "Escape") {
        if (netPrompt.value) netPrompt.value = null;
        else if (wireDraft.value) wireDraft.value = null;
        else if (railDraft.value) railDraft.value = null;
        else if (bridgeStart.value) bridgeStart.value = null;
        else if (saddleStart.value) saddleStart.value = null;
        else if (pendingRoute.value) rejectRoute();
        else if (placing.value) placing.value = null;
        else if (tool.value !== "select") tool.value = "select";
        else selection.value = null;
        return;
      }
      if (
        event.key === "Backspace" &&
        wireDraft.value &&
        wireDraft.value.vertices.length > 1
      ) {
        event.preventDefault();
        wireDraft.value = {
          ...wireDraft.value,
          vertices: wireDraft.value.vertices.slice(0, -1),
        };
        return;
      }
      if (event.key === "Enter") {
        if (pendingRoute.value) {
          event.preventDefault();
          void acceptRoute();
        } else if (railDraft.value) {
          event.preventDefault();
          finishRail();
        }
        return;
      }
      if (
        (event.key === "r" || event.key === "R") &&
        !event.ctrlKey &&
        !event.metaKey &&
        placing.value
      ) {
        event.preventDefault();
        event.stopPropagation();
        placementRotation.value = nextRotation(
          placementRotation.value,
          placing.value.allowed_rotations,
        );
      }
    };
    const up = (event: KeyboardEvent) => {
      if (event.key === " ") spaceDown.value = false;
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up);
    };
  }, []);

  // Keep the drawn SVGs in step with the camera, or preview while zooming.
  useSignalEffect(() => {
    const live = viewBox.value;
    const svgs = [svgRef.current, compareRef.current];
    if (live && zoomPreview.current !== null) {
      const shown = shownBox.peek() ?? live;
      for (const svg of svgs)
        if (svg) svg.style.transform = previewTransform(svg, shown, live);
      return;
    }
    shownBox.value = live;
    for (const svg of svgs) if (svg) svg.style.transform = "";
  });

  useEffect(
    () => () => {
      if (zoomPreview.current !== null)
        window.clearTimeout(zoomPreview.current);
    },
    [],
  );

  function settleZoom(): void {
    if (zoomPreview.current === null) return;
    window.clearTimeout(zoomPreview.current);
    zoomPreview.current = null;
    const live = viewBox.peek();
    viewBox.value = live && ([...live] as ViewBox); // re-run the sync effect
  }

  // Wheel zoom around the cursor; non-passive so the page does not scroll.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = viewBox.peek();
      const point = box && clientToBox(svg, box, event.clientX, event.clientY);
      if (!box || !point) return;
      if (zoomPreview.current !== null)
        window.clearTimeout(zoomPreview.current);
      zoomPreview.current = window.setTimeout(settleZoom, ZOOM_SETTLE_MS);
      if (
        event.ctrlKey ||
        Math.abs(event.deltaY) >= Math.abs(event.deltaX) ||
        !event.shiftKey
      ) {
        const factor = Math.exp(event.deltaY * (event.ctrlKey ? 0.01 : 0.0015));
        zoomAround(point, factor);
      }
    };
    svg.addEventListener("wheel", onWheel, { passive: false });
    return () => svg.removeEventListener("wheel", onWheel);
  });

  function zoomAround(point: Point, factor: number): void {
    const box = viewBox.peek();
    if (!box) return;
    const [x, y, width, height] = box;
    const nextWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width * factor));
    const applied = nextWidth / width;
    viewBox.value = [
      point.x - (point.x - x) * applied,
      point.y - (point.y - y) * applied,
      width * applied,
      height * applied,
    ];
  }

  function zoomCenter(factor: number): void {
    const box = viewBox.peek();
    if (!box) return;
    zoomAround({ x: box[0] + box[2] / 2, y: box[1] + box[3] / 2 }, factor);
  }

  if (!current) return <div class="canvas-empty">Loading board…</div>;
  if (!viewBox.peek()) viewBox.value = contentViewBox(current, side.peek());
  const target = current;
  const board = target.board;
  const maxX2 = (board.holes_x - 1) * 2;
  const maxY2 = (board.holes_y - 1) * 2;

  function canonical(event: PointerEvent | MouseEvent): Point | undefined {
    return svgRef.current
      ? clientToCanonical(svgRef.current, event.clientX, event.clientY)
      : undefined;
  }
  function holeAt(point: Point): Point | undefined {
    const x = Math.round(point.x);
    const y = Math.round(point.y);
    return x >= 0 && y >= 0 && x < board.holes_x && y < board.holes_y
      ? { x, y }
      : undefined;
  }
  function halfGrid(point: Point): RoutePoint {
    return snappedRoutePoint(point, { maxX2, maxY2 });
  }

  function finishRail(): void {
    const vertices = railDraft.peek();
    const netId = activeNetId.peek();
    if (!vertices || vertices.length < 2 || !netId) {
      toast("info", "A rail needs at least two points");
      return;
    }
    railDraft.value = null;
    void mutate(
      {
        type: "add_rail",
        rail: {
          id: crypto.randomUUID(),
          net_id: netId,
          side: side.peek(),
          color: "white",
          vertices,
          locked: false,
          notes: "",
        },
      },
      { failure: "Could not add the rail" },
    );
  }

  function addSaddle(start: Point, end: Point): void {
    void mutate(
      {
        type: "add_saddle",
        saddle: {
          id: crypto.randomUUID(),
          side: side.peek(),
          start,
          end,
          color: "black",
          notes: "",
        },
      },
      { failure: "Could not add the saddle" },
    );
  }

  async function wireClickTerminal(
    terminal: TerminalRef,
    position: RoutePoint,
    event: PointerEvent,
  ): Promise<void> {
    const draft = wireDraft.peek();
    if (!draft) {
      let netId: string | undefined;
      let netFromActive = false;
      if (terminal.kind === "pin") {
        const resolved = netForWireStart(target, terminal, activeNetId.peek());
        if (resolved.reason) {
          toast("info", resolved.reason);
          return;
        }
        netId = resolved.netId;
        netFromActive = Boolean(resolved.fromActive);
        if (!netId) {
          const host = svgRef.current?.parentElement?.getBoundingClientRect();
          netPrompt.value = {
            x: event.clientX - (host?.left ?? 0),
            y: event.clientY - (host?.top ?? 0),
            pin: {
              component_id: terminal.component_id,
              pin_id: terminal.pin_id,
            },
          };
          return;
        }
      } else if (terminal.kind === "rail_node") {
        netId = target.rails?.[terminal.rail_id]?.net_id;
      }
      if (!netId) return;
      activeNetId.value = netId;
      wireDraft.value = {
        netId,
        netFromActive,
        start: terminal,
        vertices: [position],
      };
      return;
    }
    if (JSON.stringify(terminal) === JSON.stringify(draft.start)) {
      wireDraft.value = null;
      return;
    }
    const endNet = endTerminalNet(terminal);
    if (endNet.reason) {
      toast("error", endNet.reason);
      return;
    }
    let netId = draft.netId;
    if (endNet.net && endNet.net.id !== netId) {
      if (!draft.netFromActive) {
        toast("error", `That ${endNet.what} carries ${endNet.net.name}`, {
          detail: `This wire carries ${target.nets?.[netId]?.name ?? "another net"}.`,
        });
        return;
      }
      if (endNet.net.state !== "accepted") {
        toast(
          "info",
          `Net ${endNet.net.name} is still a proposal; accept it before wiring`,
        );
        return;
      }
      // The start was unconnected and only borrowed the active net.
      netId = endNet.net.id;
      activeNetId.value = netId;
    }
    const vertices = appendOrthogonal(draft.vertices, position);
    wireDraft.value = null;
    await addWire(netId, draft.start, terminal, vertices);
  }

  /** The net already present at a terminal a wire is about to end on. */
  function endTerminalNet(terminal: TerminalRef): {
    net?: ElectricalNet;
    what?: string;
    reason?: string;
  } {
    if (terminal.kind === "pin") {
      const nets = netsAtPin(target, terminal);
      if (nets.length > 1)
        return {
          reason: `That hole joins ${nets.map((net) => net.name).join(" and ")}`,
        };
      return { net: nets[0], what: "pin" };
    }
    if (terminal.kind === "rail_node") {
      const netId = target.rails?.[terminal.rail_id]?.net_id;
      return { net: netId ? target.nets?.[netId] : undefined, what: "rail" };
    }
    return {};
  }

  /** Wire tool on a rail's body: add a connection node there and use it. */
  async function wireToNewRailNode(
    railId: string,
    point: { x: number; y: number },
    event: PointerEvent,
  ): Promise<void> {
    const rail = target.rails?.[railId];
    if (!rail) return;
    const draft = wireDraft.peek();
    if (draft && rail.net_id !== draft.netId && !draft.netFromActive) {
      toast(
        "error",
        `That rail carries ${target.nets?.[rail.net_id]?.name ?? "a different net"}`,
        {
          detail: `This wire carries ${target.nets?.[draft.netId]?.name ?? "another net"}.`,
        },
      );
      return;
    }
    const inserted = insertRailNode(rail.vertices, point);
    if (!inserted) return;
    if (inserted.vertices !== rail.vertices) {
      if (rail.locked) {
        toast("info", "This rail is locked", {
          detail: "Connect at one of its existing nodes instead.",
        });
        return;
      }
      const added = await mutate(
        {
          type: "modify_rail_path",
          rail_id: rail.id,
          vertices: inserted.vertices,
        },
        { failure: "Could not add a node to the rail" },
      );
      if (!added) return;
    }
    await wireClickTerminal(
      { kind: "rail_node", rail_id: rail.id, position: { ...inserted.node } },
      inserted.node,
      event,
    );
  }

  function onPointerDown(event: PointerEvent): void {
    const svg = svgRef.current;
    if (!svg) return;
    // Interactions need the real geometry, not the zoom preview.
    settleZoom();
    netPrompt.value = null;
    const element = event.target as Element;
    const point = canonical(event);
    if (!point) return;
    const panRequested =
      event.button === 1 || (event.button === 0 && spaceDown.peek());
    if (panRequested) {
      startPan(event, false);
      return;
    }
    if (event.button !== 0) {
      if (
        event.button === 2 &&
        (wireDraft.peek() ||
          railDraft.peek() ||
          bridgeStart.peek() ||
          saddleStart.peek())
      ) {
        wireDraft.value = null;
        railDraft.value = null;
        bridgeStart.value = null;
        saddleStart.value = null;
      }
      return;
    }

    const handle = element.closest<SVGElement>("[data-handle]");
    const pinEl = element.closest<SVGElement>("[data-pin]");
    const wireEl = element.closest<SVGElement>("[data-wire]");
    const railEl = element.closest<SVGElement>("[data-rail]");
    const railNodeEl = element.closest<SVGElement>("[data-rail-node]");
    const saddleEl = element.closest<SVGElement>("[data-saddle]");
    const partEl = element.closest<SVGElement>("[data-component]");
    const airEl = element.closest<SVGElement>("[data-airwire]");
    const resizeEl = element.closest<SVGElement>("[data-resize]");

    // Placement of a new part from the library.
    const pending = placing.peek();
    if (pending) {
      const hole = { x: Math.round(point.x), y: Math.round(point.y) };
      placing.value = null;
      void placeComponent(pending, hole, placementRotation.peek());
      return;
    }

    const activeTool = tool.peek();

    if (activeTool === "resize") {
      if (resizeEl) {
        const view = clientToView(svg, event.clientX, event.clientY);
        if (!view) return;
        gesture.value = {
          kind: "resize",
          pointerId: event.pointerId,
          handle: resizeEl.dataset.resize as ResizeHandle,
          start: view,
          plan: {
            holesX: board.holes_x,
            holesY: board.holes_y,
            offsetX: 0,
            offsetY: 0,
          },
          dx: 0,
          dy: 0,
        };
        svg.setPointerCapture(event.pointerId);
      } else startPan(event, false);
      return;
    }

    if (activeTool === "bridge") {
      const hole = holeAt(point);
      if (!hole) return;
      const start = bridgeStart.peek();
      if (!start) {
        bridgeStart.value = hole;
        return;
      }
      if (bridgeRotation(start, hole)) {
        bridgeStart.value = null;
        void addBridge(start, hole);
      } else if (start.x === hole.x && start.y === hole.y) {
        bridgeStart.value = null;
      } else {
        bridgeStart.value = hole;
      }
      return;
    }

    if (activeTool === "saddle") {
      const hole = holeAt(point);
      if (!hole) return;
      const start = saddleStart.peek();
      if (!start) {
        saddleStart.value = hole;
        return;
      }
      saddleStart.value = null;
      if (start.x !== hole.x || start.y !== hole.y) addSaddle(start, hole);
      return;
    }

    if (activeTool === "rail") {
      const net = activeNetId.peek()
        ? target.nets?.[activeNetId.peek()!]
        : undefined;
      if (!net || net.state !== "accepted") {
        toast("info", "Choose an accepted net for the rail", {
          detail: "Pick one in the tool bar above the board.",
        });
        return;
      }
      const snapped = halfGrid(point);
      const draft = railDraft.peek() ?? [];
      const last = draft.at(-1);
      if (last && last.x2 === snapped.x2 && last.y2 === snapped.y2) {
        if (draft.length >= 2) finishRail();
        return;
      }
      railDraft.value = [...draft, snapped];
      return;
    }

    if (airEl && (activeTool === "select" || activeTool === "wire")) {
      const airwire = airwires.peek()[Number(airEl.dataset.airwire)];
      if (airwire)
        void proposeRoute(
          airwire.netId,
          { kind: "pin", ...airwire.from },
          { kind: "pin", ...airwire.to },
        );
      return;
    }

    if (activeTool === "wire") {
      if (pinEl) {
        const ref = parsePinKey(pinEl.dataset.pin!);
        const instance = target.components?.[ref.component_id];
        const pin = definitionOf(target, instance)?.pins.find(
          (item) => item.id === ref.pin_id,
        );
        if (!instance || !pin) return;
        const position = pinBoardPosition(instance, pin);
        void wireClickTerminal(
          { kind: "pin", ...ref },
          { x2: position.x * 2, y2: position.y * 2 },
          event,
        );
        return;
      }
      if (partEl) {
        // Bridge ends are pins too: use the end nearest the pointer.
        const instance = target.components?.[partEl.dataset.component!];
        const definition = definitionOf(target, instance);
        if (instance && definition?.kind === "bridge") {
          const nearest = definition.pins
            .map((pin) => ({ pin, position: pinBoardPosition(instance, pin) }))
            .sort(
              (a, b) =>
                Math.hypot(a.position.x - point.x, a.position.y - point.y) -
                Math.hypot(b.position.x - point.x, b.position.y - point.y),
            )[0];
          if (nearest)
            void wireClickTerminal(
              {
                kind: "pin",
                component_id: instance.id,
                pin_id: nearest.pin.id,
              },
              { x2: nearest.position.x * 2, y2: nearest.position.y * 2 },
              event,
            );
          return;
        }
      }
      if (railNodeEl) {
        const [railId, index] = railNodeEl.dataset.railNode!.split(":");
        const rail = target.rails?.[railId!];
        const position = rail?.vertices[Number(index)];
        if (rail && position)
          void wireClickTerminal(
            { kind: "rail_node", rail_id: rail.id, position: { ...position } },
            position,
            event,
          );
        return;
      }
      if (railEl) {
        void wireToNewRailNode(railEl.dataset.rail!, point, event);
        return;
      }
      const draft = wireDraft.peek();
      if (draft) {
        const snapped = halfGrid(point);
        wireDraft.value = {
          ...draft,
          vertices: appendOrthogonal(draft.vertices, snapped),
        };
        return;
      }
      startPan(event, false);
      return;
    }

    // Select tool.
    if (handle) {
      const wire = target.wires?.[handle.dataset.wire!];
      if (!wire) return;
      if (wire.locked) {
        toast("info", "This wire is locked", {
          detail: "Unlock it in the inspector to edit its path.",
        });
        return;
      }
      const index = Number(handle.dataset.index);
      const endpoint =
        handle.dataset.handle === "start" || handle.dataset.handle === "end"
          ? handle.dataset.handle
          : undefined;
      gesture.value = {
        kind: "vertex",
        pointerId: event.pointerId,
        wireId: wire.id,
        index,
        vertices: wire.vertices.map((vertex) => ({ ...vertex })),
        current: { ...wire.vertices[index]! },
        endpoint,
      };
      svg.setPointerCapture(event.pointerId);
      return;
    }
    if (event.altKey && pinEl) {
      const ref = parsePinKey(pinEl.dataset.pin!);
      const netId = activeNetId.peek();
      const owner = Object.values(target.nets ?? {}).find((net) =>
        net.pins?.some(
          (pin) =>
            pin.component_id === ref.component_id && pin.pin_id === ref.pin_id,
        ),
      );
      if (owner && owner.id === netId) void assignPin(null, ref);
      else if (netId) void assignPin(netId, ref);
      else
        toast("info", "Choose an active net first", {
          detail: "Alt-click toggles a pin on the active net.",
        });
      return;
    }
    if (saddleEl) {
      select({ kind: "saddle", id: saddleEl.dataset.saddle! });
      return;
    }
    if (wireEl || railEl) {
      const id = (wireEl ?? railEl)!.dataset[wireEl ? "wire" : "rail"]!;
      select({ kind: wireEl ? "wire" : "rail", id });
      return;
    }
    const componentId =
      partEl?.dataset.component ??
      (pinEl ? parsePinKey(pinEl.dataset.pin!).component_id : undefined);
    if (componentId) {
      const instance = target.components?.[componentId];
      const definition = definitionOf(target, instance);
      if (!instance) return;
      const ownSide = definition?.kind === "bridge" ? "solder" : "component";
      if (ownSide !== side.peek()) {
        select({ kind: "component", id: componentId });
        return;
      }
      select({ kind: "component", id: componentId });
      gesture.value = {
        kind: "move",
        pointerId: event.pointerId,
        id: componentId,
        start: point,
        origin: { ...instance.position },
        current: { ...instance.position },
      };
      svg.setPointerCapture(event.pointerId);
      return;
    }
    startPan(event, true);
  }

  function startPan(event: PointerEvent, deselect: boolean): void {
    const box = viewBox.peek();
    if (!box || !svgRef.current) return;
    gesture.value = {
      kind: "pan",
      pointerId: event.pointerId,
      client: { x: event.clientX, y: event.clientY },
      start: box,
      moved: false,
      deselect,
    };
    svgRef.current.setPointerCapture(event.pointerId);
  }

  function updateHover(event: PointerEvent): void {
    const element = event.target as Element;
    const pinEl = element.closest<SVGElement>("[data-pin]");
    const wireEl = element.closest<SVGElement>("[data-wire]");
    const railEl = element.closest<SVGElement>("[data-rail]");
    const saddleEl = element.closest<SVGElement>("[data-saddle]");
    const partEl = element.closest<SVGElement>("[data-component]");
    const airEl = element.closest<SVGElement>("[data-airwire]");
    let next: HoverTarget | null = null;
    if (pinEl) next = { kind: "pin", ref: parsePinKey(pinEl.dataset.pin!) };
    else if (airEl) {
      const airwire = airwires.peek()[Number(airEl.dataset.airwire)];
      if (airwire) next = { kind: "airwire", netId: airwire.netId };
    } else if (saddleEl)
      next = { kind: "saddle", id: saddleEl.dataset.saddle! };
    else if (wireEl) next = { kind: "wire", id: wireEl.dataset.wire! };
    else if (railEl) next = { kind: "rail", id: railEl.dataset.rail! };
    else if (partEl)
      next = { kind: "component", id: partEl.dataset.component! };
    const previous = hover.peek();
    if (JSON.stringify(previous) !== JSON.stringify(next)) hover.value = next;
    const host = svgRef.current?.parentElement?.getBoundingClientRect();
    hoverClient.value =
      next && host
        ? { x: event.clientX - host.left, y: event.clientY - host.top }
        : null;
  }

  function onPointerMove(event: PointerEvent): void {
    const svg = svgRef.current;
    if (!svg) return;
    // Hover and cursor feedback wait until the zoom preview settles.
    if (zoomPreview.current !== null) return;
    const active = gesture.peek();
    // While panning, the cursor stays over the same board point; skip the
    // cursor signals so the pan does not re-render the canvas.
    const point = active?.kind === "pan" ? undefined : canonical(event);
    if (point) {
      cursor.value = point;
      const hole = holeAt(point);
      const previous = cursorHole.peek();
      if (hole?.x !== previous?.x || hole?.y !== previous?.y)
        cursorHole.value = hole ?? null;
    }
    if (!active) {
      updateHover(event);
      return;
    }
    if (active.pointerId !== event.pointerId) return;
    if (active.kind === "pan") {
      const rect = svg.getBoundingClientRect();
      const [x, y, width, height] = active.start;
      const scale = Math.max(width / rect.width, height / rect.height);
      const dx = (event.clientX - active.client.x) * scale;
      const dy = (event.clientY - active.client.y) * scale;
      if (!active.moved && Math.hypot(dx, dy) / scale < 3) return;
      if (!active.moved) gesture.value = { ...active, moved: true };
      viewBox.value = [x - dx, y - dy, width, height];
      return;
    }
    if (!point) return;
    if (active.kind === "move") {
      const next = snappedDragPosition(active.origin, active.start, point);
      if (next.x !== active.current.x || next.y !== active.current.y)
        gesture.value = { ...active, current: next };
      return;
    }
    if (active.kind === "vertex") {
      const next = halfGrid(point);
      if (next.x2 !== active.current.x2 || next.y2 !== active.current.y2)
        gesture.value = { ...active, current: next };
      return;
    }
    if (active.kind === "resize") {
      const viewPoint = clientToView(svg, event.clientX, event.clientY);
      if (!viewPoint) return;
      const dx = Math.round(viewPoint.x - active.start.x);
      const dy = Math.round(viewPoint.y - active.start.y);
      if (dx === active.dx && dy === active.dy) return;
      const plan = planBoardResize(
        { holesX: board.holes_x, holesY: board.holes_y },
        active.handle,
        dx,
        dy,
        side.peek() === "solder",
      );
      if (plan) gesture.value = { ...active, plan, dx, dy };
    }
  }

  function onPointerUp(event: PointerEvent): void {
    const active = gesture.peek();
    if (!active || active.pointerId !== event.pointerId) return;
    gesture.value = null;
    if (active.kind === "pan") {
      if (!active.moved && active.deselect) selection.value = null;
      return;
    }
    if (active.kind === "move") {
      if (
        active.current.x !== active.origin.x ||
        active.current.y !== active.origin.y
      )
        void mutate({
          type: "move_component",
          instance_id: active.id,
          position: active.current,
        });
      return;
    }
    if (active.kind === "vertex") {
      const original = active.vertices[active.index]!;
      if (
        original.x2 === active.current.x2 &&
        original.y2 === active.current.y2
      )
        return;
      if (active.endpoint) {
        const terminal = terminalForWireEnd(
          target,
          active.wireId,
          active.endpoint,
          active.current,
        );
        if (!terminal) {
          toast("info", "Wire ends must land on a pin", {
            detail: "The wire was left unchanged.",
          });
          return;
        }
        void mutate({
          type: "reconnect_wire_endpoint",
          wire_id: active.wireId,
          endpoint: active.endpoint,
          terminal,
        });
        return;
      }
      const result = finishWireNodeDrag(
        active.vertices,
        active.index,
        active.current,
      );
      void mutate({
        type: "modify_wire_path",
        wire_id: active.wireId,
        vertices: result.vertices,
      });
      return;
    }
    if (active.kind === "resize") {
      const { plan } = active;
      if (plan.holesX !== board.holes_x || plan.holesY !== board.holes_y)
        void mutate({
          type: "resize_board",
          holes_x: plan.holesX,
          holes_y: plan.holesY,
          offset_x: plan.offsetX,
          offset_y: plan.offsetY,
        });
    }
  }

  function onDoubleClick(event: MouseEvent): void {
    if (tool.peek() === "rail" && railDraft.peek()) {
      finishRail();
      return;
    }
    if (tool.peek() !== "select") return;
    const element = event.target as Element;
    const handle = element.closest<SVGElement>("[data-handle='vertex']");
    if (handle) {
      const wire = target.wires?.[handle.dataset.wire!];
      if (!wire || wire.locked) return;
      const index = Number(handle.dataset.index);
      void mutate({
        type: "modify_wire_path",
        wire_id: wire.id,
        vertices: wire.vertices.filter((_, position) => position !== index),
      });
      return;
    }
    const railNodeEl = element.closest<SVGElement>("[data-rail-node]");
    if (railNodeEl) {
      const [railId, index] = railNodeEl.dataset.railNode!.split(":");
      void removeRailNode(railId!, Number(index));
      return;
    }
    const railEl = element.closest<SVGElement>("[data-rail]");
    const point = canonical(event);
    if (railEl && point) {
      const rail = target.rails?.[railEl.dataset.rail!];
      if (!rail) return;
      if (rail.locked) {
        toast("info", "This rail is locked");
        return;
      }
      const inserted = insertRailNode(rail.vertices, point);
      if (inserted && inserted.vertices !== rail.vertices)
        void mutate(
          {
            type: "modify_rail_path",
            rail_id: rail.id,
            vertices: inserted.vertices,
          },
          { failure: "Could not add a node to the rail" },
        );
      return;
    }
    const wireEl = element.closest<SVGElement>("[data-wire]");
    if (wireEl && point) {
      const wire = target.wires?.[wireEl.dataset.wire!];
      if (!wire || wire.locked) return;
      const segment = closestWireSegmentIndex(wire.vertices, point);
      if (segment === undefined) return;
      const vertices = insertWireNode(wire.vertices, segment, point);
      if (vertices !== wire.vertices)
        void mutate({ type: "modify_wire_path", wire_id: wire.id, vertices });
      return;
    }
    const partEl = element.closest<SVGElement>("[data-component]");
    if (partEl) {
      const item = {
        kind: "component" as const,
        id: partEl.dataset.component!,
      };
      select(item, { focus: false });
    }
  }

  // ── Derived preview design (drag in progress) ─────────────────────────────
  const active = gesture.value;
  let shown: Design = target;
  if (active?.kind === "move") {
    const instance = target.components?.[active.id];
    if (instance)
      shown = {
        ...target,
        components: {
          ...target.components,
          [active.id]: { ...instance, position: active.current },
        },
      };
  } else if (active?.kind === "vertex") {
    const wire = target.wires?.[active.wireId];
    if (wire)
      shown = {
        ...target,
        wires: {
          ...target.wires,
          [wire.id]: {
            ...wire,
            vertices: active.vertices.map((vertex, index) =>
              index === active.index ? active.current : vertex,
            ),
          },
        },
      };
  }

  const selected = selection.value;
  const selectedWire =
    selected?.kind === "wire" ? shown.wires?.[selected.id] : undefined;
  const pending = pendingRoute.value;
  const ghostDefinition = placing.value;
  const cursorPoint = cursor.value;
  const highlight = historyHighlight.value;
  const flashing = highlight.size ? highlight : flashIds.value;

  const overlay = (
    <>
      {cursorHole.value && tool.value !== "select" && (
        <circle
          class="cursor-hole"
          cx={cursorHole.value.x}
          cy={cursorHole.value.y}
          r={0.42}
        />
      )}
      {selectedWire &&
        selectedWire.side === side.value &&
        !selectedWire.locked && (
          <g class="handles">
            {selectedWire.vertices.map((vertex, index) => {
              const endpoint =
                index === 0
                  ? "start"
                  : index === selectedWire.vertices.length - 1
                    ? "end"
                    : undefined;
              return (
                <circle
                  key={index}
                  class={endpoint ? "handle endpoint" : "handle"}
                  cx={vertex.x2 / 2}
                  cy={vertex.y2 / 2}
                  r={endpoint ? 0.24 : 0.2}
                  data-handle={endpoint ?? "vertex"}
                  data-wire={selectedWire.id}
                  data-index={index}
                />
              );
            })}
          </g>
        )}
      {pending && (
        <g class="route-preview">
          <polyline
            class="route-casing"
            points={pending.proposal.vertices
              .map((v) => `${v.x2 / 2},${v.y2 / 2}`)
              .join(" ")}
          />
          <polyline
            class="route-line"
            points={pending.proposal.vertices
              .map((v) => `${v.x2 / 2},${v.y2 / 2}`)
              .join(" ")}
          />
          {pending.proposal.crossings.map((crossing, index) => (
            <circle
              key={index}
              class="crossing"
              cx={crossing.position.x2 / 2}
              cy={crossing.position.y2 / 2}
              r={0.3}
            />
          ))}
        </g>
      )}
      {wireDraft.value && cursorPoint && (
        <WireDraftPreview
          draft={wireDraft.value}
          cursor={halfGrid(cursorPoint)}
        />
      )}
      {railDraft.value && (
        <g class="rail-draft">
          <polyline
            points={[
              ...railDraft.value,
              ...(cursorPoint ? [halfGrid(cursorPoint)] : []),
            ]
              .map((v) => `${v.x2 / 2},${v.y2 / 2}`)
              .join(" ")}
          />
          {railDraft.value.map((v, index) => (
            <circle key={index} cx={v.x2 / 2} cy={v.y2 / 2} r={0.2} />
          ))}
        </g>
      )}
      {tool.value === "saddle" && cursorPoint && (
        <SaddlePreview start={saddleStart.value} end={holeAt(cursorPoint)} />
      )}
      {tool.value === "bridge" && (
        <BridgePreview
          start={bridgeStart.value}
          cursor={cursorPoint}
          holesX={board.holes_x}
          holesY={board.holes_y}
        />
      )}
      {ghostDefinition && cursorPoint && (
        <PlacementGhost
          definition={ghostDefinition}
          position={{
            x: Math.round(cursorPoint.x),
            y: Math.round(cursorPoint.y),
          }}
          rotation={placementRotation.value}
          pitch={board.pitch_mm}
        />
      )}
      {active?.kind === "move" && <MoveReadout design={shown} id={active.id} />}
    </>
  );

  const resizeOverlay =
    tool.value === "resize" ? (
      <ResizeHandles
        holesX={board.holes_x}
        holesY={board.holes_y}
        drag={active?.kind === "resize" ? active : undefined}
      />
    ) : null;

  const sceneProps = {
    side: side.value,
    pinLabels: pinLabels.value,
    fade: fadeComponents.value,
    wireLabels: wireLabelMap.value,
    railLabels: railLabelMap.value,
    saddleLabels: saddleLabelMap.value,
  };
  const compareTarget = compareDesign.value;
  const split = splitCompare.value && compareTarget;
  const cursorClass =
    spaceDown.value || active?.kind === "pan"
      ? active?.kind === "pan"
        ? "grabbing"
        : "grab"
      : placing.value || tool.value !== "select"
        ? "crosshair"
        : undefined;

  return (
    <div class={`canvas-host${split ? " split" : ""}`}>
      {split && (
        <div class="compare-pane">
          <div class="pane-tag">
            Committed ·{" "}
            {view.value?.commits.find((c) => c.id === view.value?.head_commit)
              ?.name ?? "base"}
          </div>
          <BoardBase
            board={compareTarget.board}
            side={side.value}
            viewBox={viewBox}
          />
          <svg
            ref={compareRef}
            class="board-svg readonly"
            viewBox={viewBoxAttr}
            preserveAspectRatio="xMidYMid meet"
          >
            <BoardScene
              {...sceneProps}
              design={compareTarget}
              mode="readonly"
              idPrefix="cmp"
              baseLayer={false}
              changeKinds={changeKinds.value}
              showChanges
            />
          </svg>
        </div>
      )}
      <div class="work-pane">
        {split && <div class="pane-tag working">Working copy</div>}
        <BoardBase board={board} side={side.value} viewBox={viewBox} />
        <svg
          ref={svgRef}
          class={`board-svg${cursorClass ? ` cursor-${cursorClass}` : ""}`}
          viewBox={viewBoxAttr}
          preserveAspectRatio="xMidYMid meet"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={() => (gesture.value = null)}
          onPointerLeave={() => {
            if (!gesture.peek()) {
              hover.value = null;
              hoverClient.value = null;
              cursorHole.value = null;
            }
          }}
          onDblClick={onDoubleClick}
          onContextMenu={(event) => event.preventDefault()}
          role="application"
          aria-label={`${board.name}, ${side.value} side`}
        >
          <BoardScene
            {...sceneProps}
            design={shown}
            mode="edit"
            idPrefix="work"
            baseLayer={false}
            selection={selected}
            hoveredId={
              hover.value && "id" in hover.value ? hover.value.id : undefined
            }
            emphasisNetId={emphasisNetId.value}
            changeKinds={changeKinds.value}
            showChanges={showChanges.value || Boolean(split)}
            removedFrom={compareTarget}
            flashIds={flashing}
            showKeepouts={showKeepouts.value}
            airwires={
              showRatsnest.value && !pending ? airwires.value : undefined
            }
            overlay={overlay}
            viewOverlay={resizeOverlay}
          />
        </svg>
        <CanvasHud
          onZoom={zoomCenter}
          onFit={() => (viewBox.value = contentViewBox(target, side.peek()))}
          zoomPercent={zoomPercent}
          wireDrafting={Boolean(wireDraft.value)}
          railDrafting={Boolean(railDraft.value)}
          onFinishRail={finishRail}
        />
        {netPrompt.value && (
          <NetPromptCard
            prompt={netPrompt.value}
            onCancel={() => (netPrompt.value = null)}
            onCreate={async (name) => {
              const prompt = netPrompt.peek();
              netPrompt.value = null;
              if (!prompt) return;
              const netId = await createNet(name, "accepted", [prompt.pin]);
              if (!netId) return;
              const instance =
                designSignal.peek()?.components?.[prompt.pin.component_id];
              const pin = definitionOf(
                designSignal.peek()!,
                instance,
              )?.pins.find((item) => item.id === prompt.pin.pin_id);
              if (!instance || !pin) return;
              const position = pinBoardPosition(instance, pin);
              wireDraft.value = {
                netId,
                start: { kind: "pin", ...prompt.pin },
                vertices: [{ x2: position.x * 2, y2: position.y * 2 }],
              };
            }}
          />
        )}
        {hoverClient.value && !active && !wireDraft.value && (
          <HoverCard at={hoverClient.value} />
        )}
      </div>
    </div>
  );
}

function WireDraftPreview({
  draft,
  cursor,
}: {
  draft: WireDraft;
  cursor: RoutePoint;
}) {
  const vertices = appendOrthogonal(draft.vertices, cursor);
  return (
    <g class="wire-draft">
      <polyline
        points={vertices.map((v) => `${v.x2 / 2},${v.y2 / 2}`).join(" ")}
      />
      {draft.vertices.map((v, index) => (
        <circle
          key={index}
          cx={v.x2 / 2}
          cy={v.y2 / 2}
          r={index === 0 ? 0.26 : 0.16}
        />
      ))}
    </g>
  );
}

function BridgePreview({
  start,
  cursor,
  holesX,
  holesY,
}: {
  start: Point | null;
  cursor: Point | null;
  holesX: number;
  holesY: number;
}) {
  if (!start) return null;
  const options = adjacentHoles(start, holesX, holesY);
  const target =
    cursor &&
    options
      .map((hole) => ({
        hole,
        d: Math.hypot(hole.x - cursor.x, hole.y - cursor.y),
      }))
      .sort((a, b) => a.d - b.d)[0]?.hole;
  return (
    <g class="bridge-preview">
      {options.map((hole) => (
        <circle
          key={`${hole.x},${hole.y}`}
          class="bridge-option"
          cx={hole.x}
          cy={hole.y}
          r={0.34}
        />
      ))}
      <circle class="bridge-anchor" cx={start.x} cy={start.y} r={0.3} />
      {target && <line x1={start.x} y1={start.y} x2={target.x} y2={target.y} />}
    </g>
  );
}

/**
 * Saddle tool preview: the hole under the pointer before the first click,
 * then the strap from the first hole to the hole under the pointer. Whether
 * the holes are free is decided by Rust when the saddle is added.
 */
function SaddlePreview({
  start,
  end,
}: {
  start: Point | null;
  end: Point | undefined;
}) {
  if (!end) return null;
  return (
    <g class="saddle-draft">
      {start && <line x1={start.x} y1={start.y} x2={end.x} y2={end.y} />}
      {start && <circle cx={start.x} cy={start.y} r={0.2} />}
      <circle cx={end.x} cy={end.y} r={0.2} />
    </g>
  );
}

function PlacementGhost({
  definition,
  position,
  rotation,
  pitch,
}: {
  definition: ComponentDefinition;
  position: Point;
  rotation: Rotation;
  pitch: number;
}) {
  const { body } = definition;
  return (
    <g
      class="placement-ghost"
      transform={`translate(${position.x} ${position.y}) rotate(${rotationDegrees(rotation)})`}
    >
      <rect
        x={body.x / pitch}
        y={body.y / pitch}
        width={body.width / pitch}
        height={body.height / pitch}
        rx={0.3}
      />
      {definition.pins.map((pin) => (
        <circle key={pin.id} cx={pin.x} cy={pin.y} r={0.3} />
      ))}
    </g>
  );
}

function MoveReadout({ design, id }: { design: Design; id: string }) {
  const instance = design.components?.[id];
  const definition = definitionOf(design, instance);
  if (!instance || !definition) return null;
  const bounds = bodyBounds(instance, definition, design.board.pitch_mm);
  return (
    <g class="move-outline">
      <rect
        x={bounds.x - 0.15}
        y={bounds.y - 0.15}
        width={bounds.width + 0.3}
        height={bounds.height + 0.3}
        rx={0.35}
      />
    </g>
  );
}

function ResizeHandles({
  holesX,
  holesY,
  drag,
}: {
  holesX: number;
  holesY: number;
  drag?: {
    handle: ResizeHandle;
    dx: number;
    dy: number;
    plan: BoardResizePlan;
  };
}) {
  // View space: edges follow the pointer; the plan carries canonical offsets.
  const moves = (edge: string) => Boolean(drag?.handle.includes(edge));
  const left = moves("left") ? drag!.dx : 0;
  const top = moves("top") ? drag!.dy : 0;
  const right = holesX - 1 + (moves("right") ? drag!.dx : 0);
  const bottom = holesY - 1 + (moves("bottom") ? drag!.dy : 0);
  const width = right - left;
  const height = bottom - top;
  const corners: Array<[ResizeHandle, number, number]> = [
    ["top-left", left, top],
    ["top-right", right, top],
    ["bottom-right", right, bottom],
    ["bottom-left", left, bottom],
  ];
  return (
    <g class="resize-handles">
      <rect
        class="resize-outline"
        x={left}
        y={top}
        width={width}
        height={height}
      />
      <line data-resize="top" x1={left} y1={top} x2={right} y2={top} />
      <line data-resize="bottom" x1={left} y1={bottom} x2={right} y2={bottom} />
      <line data-resize="left" x1={left} y1={top} x2={left} y2={bottom} />
      <line data-resize="right" x1={right} y1={top} x2={right} y2={bottom} />
      {corners.map(([handle, x, y]) => (
        <rect
          key={handle}
          class="resize-corner"
          data-resize={handle}
          x={x - 0.35}
          y={y - 0.35}
          width={0.7}
          height={0.7}
          rx={0.15}
        />
      ))}
      <g
        class="resize-readout"
        transform={`translate(${left + width / 2} ${top + height / 2})`}
      >
        <rect x={-3.2} y={-0.7} width={6.4} height={1.4} rx={0.7} />
        <text dy={TEXT_CENTER_DY}>
          {drag?.plan.holesX ?? holesX} × {drag?.plan.holesY ?? holesY} holes
        </text>
      </g>
    </g>
  );
}

function NetPromptCard({
  prompt,
  onCancel,
  onCreate,
}: {
  prompt: NetPrompt;
  onCancel: () => void;
  onCreate: (name: string) => void;
}) {
  const name = useSignal("");
  return (
    <form
      class="net-prompt"
      style={{ left: `${prompt.x + 12}px`, top: `${prompt.y + 12}px` }}
      onSubmit={(event) => {
        event.preventDefault();
        const value = name.value.trim();
        if (value) onCreate(value);
      }}
    >
      <label>
        <span>This pin has no net yet. Name a new one:</span>
        <input
          autoFocus
          value={name.value}
          placeholder="e.g. GND, VCC, TX"
          onInput={(event) =>
            (name.value = (
              event.target as HTMLInputElement
            ).value.toUpperCase())
          }
          onKeyDown={(event) => event.key === "Escape" && onCancel()}
        />
      </label>
      <div class="row">
        <button type="button" class="btn ghost small" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="submit"
          class="btn primary small"
          disabled={!name.value.trim()}
        >
          Create net & start wire
        </button>
      </div>
    </form>
  );
}
