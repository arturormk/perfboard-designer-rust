/**
 * Zoom preview for SVG board views. Re-rasterizing a big scene at every wheel
 * step is slow in WebKitGTK, so while the wheel turns a view keeps drawing
 * with its old viewBox and is scaled with a cheap CSS transform; it redraws
 * at full quality once zooming settles.
 */
import type { Point } from "../model/geometry";

export type ViewBox = [number, number, number, number];

/**
 * Maps a client point to view units for `box`, computed from the pane's
 * layout box rather than the SVG's CTM: during a zoom preview the SVG carries
 * a CSS transform and an outdated viewBox, but the camera has already moved.
 */
export function clientToBox(
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
export function previewTransform(
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
export const ZOOM_SETTLE_MS = 250;
