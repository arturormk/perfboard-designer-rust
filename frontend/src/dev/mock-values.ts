/**
 * Browser preview stand-in for the Rust value reader — development only.
 *
 * It knows a handful of values, enough to see values, colour bands and codes
 * in the UI without Tauri. Anything else is kept as typed. The real rules
 * live in `crates/core/src/value.rs`.
 */
import type { Band, BandCount, ValueReading } from "../api/project";

const KNOWN: Record<string, ValueReading> = {
  "10k": {
    display: "10 kΩ",
    bands: ["brown", "black", "black", "red", "brown"],
  },
  "4k7": {
    display: "4.7 kΩ",
    bands: ["yellow", "violet", "black", "brown", "brown"],
  },
  "220": {
    display: "220 Ω",
    bands: ["red", "red", "black", "black", "brown"],
  },
  "0": { display: "0 Ω", bands: ["black"] },
  "100n": { display: "100 nF", code: "104" },
  "10u": { display: "10 µF" },
};

/** The same resistors' four-band codes. */
const FOUR_BANDS: Record<string, Band[]> = {
  "10 kΩ": ["brown", "black", "orange", "gold"],
  "4.7 kΩ": ["yellow", "violet", "red", "gold"],
  "220 Ω": ["red", "red", "brown", "gold"],
};

const BY_DISPLAY = new Map(
  Object.values(KNOWN).map((reading) => [reading.display, reading]),
);

/** The form the mock stores for a typed value. */
export function mockNormalizeValue(input: string): string {
  const text = input.trim();
  return KNOWN[text]?.display ?? text;
}

/** How the mock reads a stored value. */
export function mockReadValue(
  stored: string | undefined,
  bandCount: BandCount = "five",
): ValueReading | undefined {
  const text = stored?.trim();
  if (!text) return undefined;
  const reading = BY_DISPLAY.get(text);
  if (!reading) return { display: text };
  if (!reading.bands || reading.bands.length < 2) return reading;
  if (bandCount === "four") {
    const bands = FOUR_BANDS[text];
    return bands
      ? { display: text, bands, tolerance: "5%" }
      : { display: text };
  }
  return { ...reading, tolerance: "1%" };
}
