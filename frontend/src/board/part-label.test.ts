import { describe, expect, it } from "vitest";
import { partLabel } from "./BoardScene";

const identity = (x: number) => x;

describe("partLabel", () => {
  it("labels a standing resistor (2×1 holes) with its name and value", () => {
    const label = partLabel(
      "R1",
      "10k",
      { x: 0, y: 0, width: 2, height: 1 },
      identity,
    );
    expect(label?.text).toBe("R1");
    expect(label?.value).toBe("10k");
  });

  it("still leaves out a long name on a tiny body", () => {
    expect(
      partLabel(
        "Pull-up resistor",
        "10k",
        { x: 0, y: 0, width: 2, height: 1 },
        identity,
      ),
    ).toBeUndefined();
  });
});
