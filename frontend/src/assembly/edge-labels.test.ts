import { describe, expect, it } from "vitest";
import {
  axisLabel,
  DEFAULT_EDGE_LABELS,
  letterName,
  parseEdgeLabels,
} from "./edge-labels";

describe("letterName", () => {
  it("names indices like spreadsheet columns", () => {
    expect(letterName(0)).toBe("A");
    expect(letterName(25)).toBe("Z");
    expect(letterName(26)).toBe("AA");
    expect(letterName(27)).toBe("AB");
    expect(letterName(51)).toBe("AZ");
    expect(letterName(52)).toBe("BA");
    expect(letterName(701)).toBe("ZZ");
    expect(letterName(702)).toBe("AAA");
  });
});

describe("axisLabel", () => {
  it("numbers from 1 by default", () => {
    const axis = DEFAULT_EDGE_LABELS.columns;
    expect(axisLabel(0, 30, axis)).toBe("1");
    expect(axisLabel(29, 30, axis)).toBe("30");
  });

  it("letters rows from A by default", () => {
    const axis = DEFAULT_EDGE_LABELS.rows;
    expect(axisLabel(0, 20, axis)).toBe("A");
    expect(axisLabel(19, 20, axis)).toBe("T");
  });

  it("counts from the far end when reversed", () => {
    expect(axisLabel(0, 30, { style: "numbers", reversed: true })).toBe("30");
    expect(axisLabel(29, 30, { style: "numbers", reversed: true })).toBe("1");
    expect(axisLabel(0, 20, { style: "letters", reversed: true })).toBe("T");
    expect(axisLabel(19, 20, { style: "letters", reversed: true })).toBe("A");
  });
});

describe("parseEdgeLabels", () => {
  it("returns null for missing or broken settings", () => {
    expect(parseEdgeLabels(null)).toBeNull();
    expect(parseEdgeLabels("not json")).toBeNull();
    expect(parseEdgeLabels("3")).toBeNull();
  });

  it("round-trips settings", () => {
    const value = {
      enabled: true,
      columns: { style: "letters" as const, reversed: true },
      rows: { style: "numbers" as const, reversed: false },
    };
    expect(parseEdgeLabels(JSON.stringify(value))).toEqual(value);
  });

  it("fills in defaults for invalid fields", () => {
    expect(
      parseEdgeLabels('{"enabled":true,"columns":{"style":"roman"}}'),
    ).toEqual({ ...DEFAULT_EDGE_LABELS, enabled: true });
  });
});
