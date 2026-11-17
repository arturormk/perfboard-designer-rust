/**
 * Browser preview stand-in for the Rust value reader — development only.
 *
 * It knows a handful of values, enough to see values, colour bands and codes
 * in the UI without Tauri. Anything else is kept as typed. The real rules
 * live in `crates/core/src/value.rs`.
 */
import type { ValueReading } from "../api/project";

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
): ValueReading | undefined {
  const text = stored?.trim();
  if (!text) return undefined;
  return BY_DISPLAY.get(text) ?? { display: text };
}
