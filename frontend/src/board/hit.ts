/** Pure helpers used by the canvas to interpret pointer positions. */
import type { ElectricalNet, PinRef, Side, TerminalRef } from "../api/project";
import {
  definitionOf,
  isBridge,
  pinBoardPosition,
  type Design,
  type Point,
  type RoutePoint,
} from "../model/geometry";
import { netOfPin } from "../model/naming";

export function parsePinKey(key: string): PinRef {
  const [component_id, pin_id] = key.split("\u0000");
  return { component_id: component_id!, pin_id: pin_id! };
}

/** Pins (of any part, including bridges) whose hole is at `point`. */
export function pinsAt(design: Design, point: Point): PinRef[] {
  const result: PinRef[] = [];
  for (const instance of Object.values(design.components ?? {})) {
    for (const pin of definitionOf(design, instance)?.pins ?? []) {
      const position = pinBoardPosition(instance, pin);
      if (position.x === point.x && position.y === point.y)
        result.push({ component_id: instance.id, pin_id: pin.id });
    }
  }
  return result;
}

/**
 * Chooses the terminal a dragged wire end should attach to: the current one
 * if still under the pointer, otherwise a pin already on the wire's net,
 * otherwise any pin at that hole.
 */
export function terminalForWireEnd(
  design: Design,
  wireId: string,
  endpoint: "start" | "end",
  position: RoutePoint,
): TerminalRef | undefined {
  if (position.x2 % 2 !== 0 || position.y2 % 2 !== 0) return undefined;
  const wire = design.wires?.[wireId];
  const candidates = pinsAt(design, { x: position.x2 / 2, y: position.y2 / 2 });
  const current = wire?.[endpoint];
  const same = candidates.find(
    (pin) =>
      current?.kind === "pin" &&
      current.component_id === pin.component_id &&
      current.pin_id === pin.pin_id,
  );
  const onNet = candidates.find(
    (pin) => netOfPin(design, pin)?.id === wire?.net_id,
  );
  const chosen = same ?? onNet ?? candidates[0];
  return chosen ? { kind: "pin", ...chosen } : undefined;
}

/** Which net a new wire should use, given the pin it starts at. */
/**
 * The nets electrically present at a pin, as the backend sees them: the pin's
 * own net, the nets of other pins sharing its hole, and, across bridges
 * (whose two ends are joined), everything at the far end, chained through
 * further bridges. A bridge end is usually not in a net itself; it takes the
 * net of whatever it is soldered to.
 */
export function netsAtPin(design: Design, pin: PinRef): ElectricalNet[] {
  const own = netOfPin(design, pin);
  const instance = design.components?.[pin.component_id];
  const definition = definitionOf(design, instance);
  const start = definition?.pins.find((item) => item.id === pin.pin_id);
  if (!instance || !start) return own ? [own] : [];

  const bridges = Object.values(design.components ?? {}).flatMap((part) => {
    const bridge = definitionOf(design, part);
    if (!isBridge(bridge) || bridge!.pins.length < 2) return [];
    return [bridge!.pins.map((item) => pinBoardPosition(part, item))];
  });
  const key = (point: Point) => `${point.x},${point.y}`;
  const first = pinBoardPosition(instance, start);
  const seen = new Set([key(first)]);
  const queue = [first];
  while (queue.length) {
    const point = queue.shift()!;
    for (const ends of bridges) {
      if (!ends.some((end) => key(end) === key(point))) continue;
      for (const end of ends) {
        if (seen.has(key(end))) continue;
        seen.add(key(end));
        queue.push(end);
      }
    }
  }

  const found = new Map<string, ElectricalNet>();
  if (own) found.set(own.id, own);
  for (const position of seen) {
    const [x, y] = position.split(",").map(Number);
    for (const ref of pinsAt(design, { x: x!, y: y! })) {
      const net = netOfPin(design, ref);
      if (net) found.set(net.id, net);
    }
  }
  return [...found.values()];
}

export function netForWireStart(
  design: Design,
  pin: PinRef,
  activeNetId: string | undefined,
): { netId?: string; reason?: string; fromActive?: boolean } {
  const nets = netsAtPin(design, pin);
  if (nets.length > 1)
    return {
      reason: `This hole joins ${nets.map((net) => net.name).join(" and ")}; fix that short first`,
    };
  const owner = nets[0];
  if (owner) {
    if (owner.state !== "accepted")
      return {
        reason: `Net ${owner.name} is still a proposal; accept it before wiring`,
      };
    return { netId: owner.id };
  }
  const active = activeNetId ? design.nets?.[activeNetId] : undefined;
  if (active?.state === "accepted")
    return { netId: active.id, fromActive: true };
  return {};
}

/** A pin can take a wire only on the face where the wire is being drawn. */
export function pinUsableOnSide(
  design: Design,
  pin: PinRef,
  side: Side,
): boolean {
  const instance = design.components?.[pin.component_id];
  // Through-hole pins are reachable from both faces; bridges live on the solder side.
  return !isBridge(definitionOf(design, instance)) || side === "solder";
}
