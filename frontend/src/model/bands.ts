/**
 * How resistor colour bands look. Which bands a value has is decided by the
 * Rust engine (`crates/core/src/value.rs`); this file only paints them.
 */
import type { Band, Rotation } from "../api/project";
import { rotationDegrees } from "./geometry";

/** Paint for each band, tuned so brown, red and orange stay apart. */
export const BAND_COLORS: Record<Band, string> = {
  black: "#1c1c1c",
  brown: "#7b4a22",
  red: "#cf3a2c",
  orange: "#ee8a2a",
  yellow: "#f4d33f",
  green: "#3f9b4a",
  blue: "#2f63cc",
  violet: "#8b52c2",
  grey: "#8e8e8e",
  white: "#f5f5f0",
  gold: "#c9a43c",
  silver: "#c3c7cc",
};

/** Bands that need an outline to show against a light or dark body. */
export const OUTLINED_BANDS: ReadonlySet<Band> = new Set(["black", "white"]);

/**
 * Where each band's centre sits along the body, as a fraction of its length,
 * by how many bands there are.
 */
const BAND_CENTRES: Record<number, readonly number[]> = {
  1: [0.5],
  4: [0.26, 0.4, 0.54, 0.8],
  5: [0.22, 0.34, 0.46, 0.58, 0.8],
};

/** Bodies at least this many times longer than wide are drawn with bands. */
const MIN_BANDED_ASPECT = 2.5;

export interface BandRect {
  band: Band;
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The bands on a lying resistor's body, in part-local units. The value bands
 * (three digits and a multiplier, or two and a multiplier on a four-band
 * code) are grouped, then a gap, then the tolerance band. The tolerance band
 * goes to the right (or bottom) end as seen on screen, so the code reads left
 * to right or top to bottom whatever way the part is turned. Bodies too short
 * to be a lying resistor (a standing one) get none.
 */
export function bandRects(
  bands: readonly Band[],
  body: { x: number; y: number; width: number; height: number },
  rotation: Rotation,
): BandRect[] {
  if (!bands.length) return [];
  const along = body.width >= body.height;
  const length = along ? body.width : body.height;
  const thickness = along ? body.height : body.width;
  if (length < thickness * MIN_BANDED_ASPECT) return [];

  // Where the part's long axis points on screen: reverse when it points left
  // or up, so the first band is always on the left or top.
  const degrees = rotationDegrees(rotation) + (along ? 0 : 90);
  const radians = (degrees * Math.PI) / 180;
  const dx = Math.round(Math.cos(radians));
  const dy = Math.round(Math.sin(radians));
  const reversed = dx < 0 || dy < 0;

  const width = length * 0.075;
  const centres = BAND_CENTRES[bands.length] ?? [];
  return bands.slice(0, centres.length).map((band, index) => {
    const fraction = reversed ? 1 - centres[index]! : centres[index]!;
    const start = (along ? body.x : body.y) + fraction * length - width / 2;
    return along
      ? { band, x: start, y: body.y, width, height: thickness }
      : { band, x: body.x, y: start, width: thickness, height: width };
  });
}
