import { describe, expect, it } from "vitest";
import {
  dropSlot,
  insertBySeverity,
  moveTarget,
  reordered,
} from "./note-order";

const rects = [
  { top: 0, height: 40 },
  { top: 40, height: 60 },
  { top: 100, height: 20 },
];

describe("dropSlot", () => {
  it("counts the items whose middle is above the pointer", () => {
    expect(dropSlot(rects, -5)).toBe(0);
    expect(dropSlot(rects, 19)).toBe(0);
    expect(dropSlot(rects, 21)).toBe(1);
    expect(dropSlot(rects, 71)).toBe(2);
    expect(dropSlot(rects, 500)).toBe(3);
  });
});

describe("moveTarget", () => {
  const ids = ["a", "b", "c"];

  it("is undefined when the note would land where it is", () => {
    expect(moveTarget(ids, "b", 1)).toBeUndefined();
    expect(moveTarget(ids, "b", 2)).toBeUndefined();
    expect(moveTarget(ids, "x", 0)).toBeUndefined();
  });

  it("names the note to move in front of, or null for the end", () => {
    expect(moveTarget(ids, "c", 0)).toBe("a");
    expect(moveTarget(ids, "a", 2)).toBe("c");
    expect(moveTarget(ids, "a", 3)).toBeNull();
  });
});

describe("reordered", () => {
  const key = (item: string) => item;

  it("matches what the backend does with the target", () => {
    expect(reordered(["a", "b", "c"], key, "c", "a")).toEqual(["c", "a", "b"]);
    expect(reordered(["a", "b", "c"], key, "a", null)).toEqual(["b", "c", "a"]);
    expect(reordered(["a", "b", "c"], key, "a", "c")).toEqual(["b", "a", "c"]);
  });

  it("leaves the list alone for unknown notes", () => {
    expect(reordered(["a", "b"], key, "x", "a")).toEqual(["a", "b"]);
    expect(reordered(["a", "b"], key, "a", "x")).toEqual(["a", "b"]);
  });
});

describe("insertBySeverity", () => {
  const note = (severity: string, title: string) => ({ severity, title });
  const titles = (notes: Array<{ title: string }>) => notes.map((n) => n.title);

  it("puts each note before the first less severe one", () => {
    const list = insertBySeverity(
      [note("warning", "w1"), note("suggestion", "s1"), note("info", "i1")],
      [note("critical", "c1"), note("warning", "w2"), note("info", "i2")],
    );
    expect(titles(list)).toEqual(["c1", "w1", "w2", "s1", "i1", "i2"]);
  });

  it("does not re-sort a list the user reordered", () => {
    const list = insertBySeverity(
      [note("info", "i1"), note("critical", "c1")],
      [note("warning", "w1")],
    );
    expect(titles(list)).toEqual(["w1", "i1", "c1"]);
  });
});
