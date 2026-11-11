import type {
  PinRef,
  ProjectDocument,
  TerminalRef,
  Wire,
} from "../api/project";

export interface WireRerouteTarget {
  wireId: string;
  start: TerminalRef;
  end: TerminalRef;
}

interface PositionedTerminal {
  key: string;
  terminal: TerminalRef;
  position: { x2: number; y2: number };
}

interface Edge {
  first: PositionedTerminal;
  second: PositionedTerminal;
  weight: number;
}

const pinKey = (pin: PinRef): string =>
  `${pin.component_id}\u0000${pin.pin_id}`;

const terminalPinKey = (terminal: TerminalRef): string | undefined =>
  terminal.kind === "pin"
    ? pinKey({
        component_id: terminal.component_id,
        pin_id: terminal.pin_id,
      })
    : undefined;

const edgeKey = (first: string, second: string): string =>
  first < second ? `${first}\u0001${second}` : `${second}\u0001${first}`;

function pinPosition(
  project: ProjectDocument,
  pin: PinRef,
): { x2: number; y2: number } | undefined {
  const instance = project.design.components?.[pin.component_id];
  const definition =
    instance && project.design.component_definitions?.[instance.definition_id];
  const definitionPin = definition?.pins.find(
    (candidate) => candidate.id === pin.pin_id,
  );
  if (!instance || !definitionPin) return undefined;
  const rotated: Record<typeof instance.rotation, readonly [number, number]> = {
    deg0: [definitionPin.x, definitionPin.y],
    deg90: [-definitionPin.y, definitionPin.x],
    deg180: [-definitionPin.x, -definitionPin.y],
    deg270: [definitionPin.y, -definitionPin.x],
  };
  const [x, y] = rotated[instance.rotation];
  return {
    x2: (instance.position.x + x) * 2,
    y2: (instance.position.y + y) * 2,
  };
}

function routeEstimate(
  first: PositionedTerminal,
  second: PositionedTerminal,
): number {
  const dx = Math.abs(first.position.x2 - second.position.x2);
  const dy = Math.abs(first.position.y2 - second.position.y2);
  return (dx + dy) * 10;
}

function minimumSpanningTree(nodes: PositionedTerminal[]): Edge[] {
  const visited = new Set([nodes[0]!.key]);
  const result: Edge[] = [];
  while (visited.size < nodes.length) {
    const candidates: Edge[] = [];
    for (const first of nodes) {
      if (!visited.has(first.key)) continue;
      for (const second of nodes) {
        if (visited.has(second.key)) continue;
        candidates.push({
          first,
          second,
          weight: routeEstimate(first, second),
        });
      }
    }
    candidates.sort(
      (left, right) =>
        left.weight - right.weight ||
        edgeKey(left.first.key, left.second.key).localeCompare(
          edgeKey(right.first.key, right.second.key),
        ),
    );
    const best = candidates[0];
    if (!best) return [];
    result.push(best);
    visited.add(best.second.key);
  }
  return result;
}

function isSimpleConnectedTree(
  wires: Wire[],
  nodes: PositionedTerminal[],
): boolean {
  if (wires.length !== nodes.length - 1) return false;
  const nodeKeys = new Set(nodes.map((node) => node.key));
  const adjacency = new Map<string, Set<string>>();
  for (const wire of wires) {
    const first = terminalPinKey(wire.start);
    const second = terminalPinKey(wire.end);
    if (!first || !second || !nodeKeys.has(first) || !nodeKeys.has(second))
      return false;
    const firstNeighbors = adjacency.get(first) ?? new Set<string>();
    firstNeighbors.add(second);
    adjacency.set(first, firstNeighbors);
    const secondNeighbors = adjacency.get(second) ?? new Set<string>();
    secondNeighbors.add(first);
    adjacency.set(second, secondNeighbors);
  }
  const reached = new Set<string>();
  const pending = [nodes[0]!.key];
  while (pending.length) {
    const current = pending.pop()!;
    if (reached.has(current)) continue;
    reached.add(current);
    pending.push(...(adjacency.get(current) ?? []));
  }
  return reached.size === nodes.length;
}

/** Plans endpoint changes for unlocked, pin-only net trees when an MST is shorter. */
export function planWireReroutes(
  project: ProjectDocument,
): WireRerouteTarget[] {
  const wires = Object.values(project.design.wires ?? {}).sort((left, right) =>
    left.id.localeCompare(right.id),
  );
  const result = new Map(
    wires.map((wire) => [
      wire.id,
      { wireId: wire.id, start: wire.start, end: wire.end },
    ]),
  );

  for (const net of Object.values(project.design.nets ?? {})) {
    const netWires = wires.filter((wire) => wire.net_id === net.id);
    const pins = [...(net.pins ?? [])].sort((left, right) =>
      pinKey(left).localeCompare(pinKey(right)),
    );
    if (pins.length < 3 || netWires.some((wire) => wire.locked)) continue;
    const nodes = pins
      .map((pin): PositionedTerminal | undefined => {
        const position = pinPosition(project, pin);
        return position
          ? {
              key: pinKey(pin),
              terminal: { kind: "pin", ...pin },
              position,
            }
          : undefined;
      })
      .filter((node): node is PositionedTerminal => Boolean(node));
    if (nodes.length !== pins.length || !isSimpleConnectedTree(netWires, nodes))
      continue;

    const byKey = new Map(nodes.map((node) => [node.key, node]));
    const currentWeight = netWires.reduce((total, wire) => {
      const first = byKey.get(terminalPinKey(wire.start)!);
      const second = byKey.get(terminalPinKey(wire.end)!);
      return total + routeEstimate(first!, second!);
    }, 0);
    const tree = minimumSpanningTree(nodes);
    const treeWeight = tree.reduce((total, edge) => total + edge.weight, 0);
    if (tree.length !== nodes.length - 1 || treeWeight >= currentWeight)
      continue;

    const unusedWires = new Set(netWires.map((wire) => wire.id));
    const unassignedEdges = new Set(tree);
    for (const edge of tree) {
      const wanted = edgeKey(edge.first.key, edge.second.key);
      const exact = netWires.find(
        (wire) =>
          unusedWires.has(wire.id) &&
          edgeKey(terminalPinKey(wire.start)!, terminalPinKey(wire.end)!) ===
            wanted,
      );
      if (!exact) continue;
      unusedWires.delete(exact.id);
      unassignedEdges.delete(edge);
      result.set(exact.id, {
        wireId: exact.id,
        start: edge.first.terminal,
        end: edge.second.terminal,
      });
    }
    const remainingWires = [...unusedWires].sort();
    const remainingEdges = [...unassignedEdges].sort((left, right) =>
      edgeKey(left.first.key, left.second.key).localeCompare(
        edgeKey(right.first.key, right.second.key),
      ),
    );
    remainingWires.forEach((wireId, index) => {
      const edge = remainingEdges[index]!;
      result.set(wireId, {
        wireId,
        start: edge.first.terminal,
        end: edge.second.terminal,
      });
    });
  }
  return [...result.values()];
}
