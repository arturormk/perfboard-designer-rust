/**
 * Browser preview stand-in for the Rust breadboard layout — development only.
 *
 * It produces a layout of the right shape for UI work: parts stacked down
 * one board (straddling, on the left, or beside the board), and each net
 * chained with jumpers. It assigns no rails and checks nothing; the real
 * rules live in `crates/project/src/breadboard.rs` (ADR 0018).
 */
import type {
  BreadboardGeometry,
  BreadboardJumper,
  BreadboardLayout,
  BreadboardPart,
  HolePos,
  PartPlacement,
  Rotation,
} from "../api/project";
import { rotateLocal, type Design } from "../model/geometry";
import { suggestedWireColor } from "../model/naming";
import { mockReadValue } from "./mock-values";

const LEFT = [3, 4, 5, 6, 7];
const RIGHT = [10, 11, 12, 13, 14];
const ROTATIONS: Rotation[] = ["deg0", "deg90", "deg180", "deg270"];

function geometry(width: number, height: number, offBoard: number) {
  return {
    rows: 63,
    rail_first_row: 2,
    rail_last_row: 60,
    rail_rows: Array.from({ length: 59 }, (_, i) => i + 2).filter(
      (row) => (row - 2) % 6 < 5,
    ),
    board_columns: 18,
    board_stride: 19,
    left_columns: LEFT,
    right_columns: RIGHT,
    rail_columns: [
      { slot: "left_outer", column: 0, polarity: "plus" },
      { slot: "left_inner", column: 1, polarity: "minus" },
      { slot: "right_inner", column: 16, polarity: "plus" },
      { slot: "right_outer", column: 17, polarity: "minus" },
    ],
    off_board_x: offBoard,
    width,
    height,
  } satisfies BreadboardGeometry;
}

export function mockBreadboardLayout(
  design: Design,
  revision: number,
): BreadboardLayout {
  const parts: BreadboardPart[] = [];
  const pinAt = new Map<string, HolePos>();
  const used = new Set<string>();
  const key = (hole: HolePos) => `${hole.x},${hole.y}`;
  let row = 2;
  let offRow = 0;
  const offX = 22;
  let width = 18;
  for (const instance of Object.values(design.components ?? {}).sort((a, b) =>
    a.name.localeCompare(b.name),
  )) {
    const definition = design.component_definitions?.[instance.definition_id];
    if (!definition || definition.kind === "bridge" || !definition.pins.length)
      continue;
    let chosen:
      { rotation: Rotation; placement: PartPlacement; dx: number } | undefined;
    for (const rotation of ROTATIONS) {
      const pins = definition.pins.map((pin) =>
        rotateLocal(rotation, pin.x, pin.y),
      );
      const xs = [...new Set(pins.map((pin) => pin.x))].sort((a, b) => a - b);
      const ys = new Set(pins.map((pin) => pin.y));
      const span = xs.at(-1)! - xs[0]!;
      if (xs.length === 2 && span >= 3 && span <= 11) {
        chosen = { rotation, placement: "straddle", dx: 7 - xs[0]! };
        break;
      }
      if (!chosen && ys.size === pins.length && span <= 4)
        chosen = { rotation, placement: "left", dx: 7 - xs.at(-1)! };
    }
    const rotation = chosen?.rotation ?? instance.rotation;
    const pins = definition.pins.map((pin) => ({
      id: pin.id,
      ...rotateLocal(rotation, pin.x, pin.y),
    }));
    const top = Math.min(...pins.map((pin) => pin.y));
    const bottom = Math.max(...pins.map((pin) => pin.y));
    const left = Math.min(...pins.map((pin) => pin.x));
    const position = chosen
      ? { x: chosen.dx, y: row - top }
      : { x: offX - left, y: offRow - top };
    if (chosen) row += bottom - top + 2;
    else offRow += bottom - top + 3;
    for (const pin of pins) {
      const hole = { x: position.x + pin.x, y: position.y + pin.y };
      pinAt.set(`${instance.id}\u0000${pin.id}`, hole);
      used.add(key(hole));
      width = Math.max(width, hole.x + 1);
    }
    parts.push({
      component_id: instance.id,
      name: instance.name,
      value: mockReadValue(instance.value, instance.band_count),
      definition_id: instance.definition_id,
      position,
      rotation,
      placement: chosen?.placement ?? "off_board",
      board: chosen ? 0 : undefined,
    });
  }

  // A free hole in the strip of an on-board hole; off-board pins as they are.
  const freeBeside = (hole: HolePos): HolePos => {
    if (hole.x >= offX) return hole;
    const columns = LEFT.includes(hole.x) ? LEFT : RIGHT;
    for (const x of [...columns].reverse()) {
      const candidate = { x, y: hole.y };
      if (!used.has(key(candidate))) {
        used.add(key(candidate));
        return candidate;
      }
    }
    return hole;
  };
  const jumpers: BreadboardJumper[] = [];
  const nets = Object.values(design.nets ?? {})
    .filter((net) => net.state === "accepted")
    .sort((a, b) => a.name.localeCompare(b.name));
  for (const net of nets) {
    const holes = (net.pins ?? [])
      .map((pin) => pinAt.get(`${pin.component_id}\u0000${pin.pin_id}`))
      .filter((hole): hole is HolePos => hole !== undefined);
    const color =
      Object.values(design.wires ?? {}).find((wire) => wire.net_id === net.id)
        ?.color ?? suggestedWireColor(net.name);
    for (let index = 1; index < holes.length; index += 1) {
      const a = holes[index - 1]!;
      const b = holes[index]!;
      const lead = a.x >= offX || b.x >= offX;
      const [from, to] = a.x >= offX ? [a, b] : [b, a];
      const number = jumpers.length + 1;
      const start = freeBeside(from);
      const end = freeBeside(to);
      jumpers.push({
        id: `jumper:${number}`,
        number,
        kind: lead ? "lead" : "jumper",
        net_id: net.id,
        net_name: net.name,
        color,
        from: start,
        to: end,
        from_label: `(${start.x}, ${start.y})`,
        to_label: `(${end.x}, ${end.y})`,
      });
    }
  }
  const offBoard = parts.some((part) => part.placement === "off_board");
  return {
    source_revision: revision,
    board_count: 1,
    geometry: geometry(width, Math.max(63, offRow), offBoard ? offX : width),
    parts,
    rails: [],
    jumpers,
    warnings: [
      {
        code: "breadboard.preview",
        message:
          "Browser preview: a simplified layout without rails or checks. Run the desktop app for the real one.",
      },
    ],
  };
}
