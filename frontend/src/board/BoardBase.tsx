/**
 * The static base of the board (substrate and hole grid) drawn on a canvas
 * behind the interactive SVG scene.
 *
 * As an SVG `<pattern>` the hole grid was re-tiled at the current zoom on
 * every frame, which WebKitGTK does slowly. Here only the holes inside the
 * viewport are drawn, batched into a few path fills, and only when the camera,
 * board or side changes. The canvas maps view units to pixels exactly like the
 * sibling SVG (`preserveAspectRatio="xMidYMid meet"`), so the two line up.
 */
import { useSignalEffect, type ReadonlySignal } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";
import type { Side } from "../api/project";
import type { Design } from "../model/geometry";

type Board = Design["board"];
type ViewBox = [number, number, number, number];

/** Keep in sync with `BOARD_MARGIN` in BoardScene. */
const BOARD_MARGIN = 0.7;

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): void {
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + width, y, x + width, y + height, radius);
  ctx.arcTo(x + width, y + height, x, y + height, radius);
  ctx.arcTo(x, y + height, x, y, radius);
  ctx.arcTo(x, y, x + width, y, radius);
  ctx.closePath();
}

function draw(
  canvas: HTMLCanvasElement,
  board: Board,
  side: Side,
  box: ViewBox,
): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const width = Math.round(canvas.clientWidth * dpr);
  const height = Math.round(canvas.clientHeight * dpr);
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (!width || !height) return;

  const [vx, vy, vw, vh] = box;
  const scale = Math.min(width / vw, height / vh);
  const ox = (width - vw * scale) / 2 - vx * scale;
  const oy = (height - vh * scale) / 2 - vy * scale;
  ctx.setTransform(scale, 0, 0, scale, ox, oy);

  // Both layers are symmetric under the solder-side mirror, so canonical
  // coordinates can be drawn directly in view space.
  const style = getComputedStyle(canvas);
  const color = (name: string) => style.getPropertyValue(name).trim();
  const w = board.holes_x - 1 + BOARD_MARGIN * 2;
  const h = board.holes_y - 1 + BOARD_MARGIN * 2;

  roundRect(ctx, -BOARD_MARGIN, -BOARD_MARGIN + 0.22, w, h, 0.45);
  ctx.fillStyle = "rgb(0 0 0 / 30%)";
  ctx.fill();
  ctx.lineWidth = 0.35;
  ctx.strokeStyle = "rgb(0 0 0 / 14%)";
  ctx.stroke();

  roundRect(ctx, -BOARD_MARGIN, -BOARD_MARGIN, w, h, 0.45);
  ctx.fillStyle = color(
    side === "solder" ? "--substrate-solder" : "--substrate",
  );
  ctx.fill();
  ctx.lineWidth = 0.08;
  ctx.strokeStyle = color("--substrate-edge");
  ctx.stroke();

  // Only the holes inside the viewport.
  const x0 = Math.max(0, Math.floor(-ox / scale));
  const x1 = Math.min(board.holes_x - 1, Math.ceil((width - ox) / scale));
  const y0 = Math.max(0, Math.floor(-oy / scale));
  const y1 = Math.min(board.holes_y - 1, Math.ceil((height - oy) / scale));
  if (x0 > x1 || y0 > y1) return;

  const holes = (radius: number) => {
    ctx.beginPath();
    for (let y = y0; y <= y1; y++)
      for (let x = x0; x <= x1; x++) {
        ctx.moveTo(x + radius, y);
        ctx.arc(x, y, radius, 0, Math.PI * 2);
      }
  };
  if (side === "solder") {
    holes(0.36);
    ctx.fillStyle = color("--pad-ring-solder");
    ctx.fill();
    ctx.lineWidth = 0.03;
    ctx.strokeStyle = "#9d6a3b";
    ctx.stroke();
  } else {
    holes(0.27);
    ctx.fillStyle = color("--pad-ring-component");
    ctx.fill();
  }
  holes(0.14);
  ctx.fillStyle = color("--hole");
  ctx.fill();
}

export function BoardBase(props: {
  board: Board;
  side: Side;
  viewBox: ReadonlySignal<ViewBox | null>;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const latest = useRef(props);
  latest.current = props;
  const frame = useRef(0);

  const schedule = () => {
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      const canvas = canvasRef.current;
      const { board, side, viewBox } = latest.current;
      const box = viewBox.peek();
      if (canvas && box) draw(canvas, board, side, box);
    });
  };

  // Camera changes: subscribe to the signal without re-rendering.
  useSignalEffect(() => {
    void props.viewBox.value;
    schedule();
  });

  // Board or side changes.
  useEffect(schedule, [props.board, props.side]);

  // Pane resizes.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(schedule);
    observer.observe(canvas);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame.current);
      frame.current = 0;
    };
  }, []);

  return <canvas ref={canvasRef} class="board-base" aria-hidden="true" />;
}
