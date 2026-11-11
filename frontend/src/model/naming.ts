/** Human-facing names, labels and colours derived from the design. */
import type {
  ComponentDefinition,
  ComponentInstance,
  ElectricalNet,
  PinRef,
  TerminalRef,
  Wire,
  WireColor,
} from "../api/project";
import { definitionOf, isBridge, type Design } from "./geometry";

export const WIRE_COLORS = [
  "red",
  "black",
  "yellow",
  "white",
  "green",
  "blue",
  "orange",
] as const;
export type NamedWireColor = (typeof WIRE_COLORS)[number];

const WIRE_CSS: Record<string, string> = {
  red: "#e5484d",
  black: "#26282b",
  yellow: "#f5d90a",
  white: "#f2f2ee",
  green: "#30a46c",
  blue: "#3e7bfa",
  orange: "#f76b15",
};

export function wireColorName(color: WireColor): string {
  return typeof color === "string" ? color : color.other;
}

export function wireColorCss(color: WireColor): string {
  const name = wireColorName(color).toLowerCase();
  return WIRE_CSS[name] ?? name;
}

/** Dark wires get a light outline so they stay visible on the board. */
export function isDarkWire(color: WireColor): boolean {
  const name = wireColorName(color).toLowerCase();
  return name === "black" || name === "blue";
}

/**
 * Suggests a conventional insulation colour from a net name. Suggestions only:
 * the blueprint treats colours as advice, never as rules.
 */
export function suggestedWireColor(netName: string | undefined): WireColor {
  const name = (netName ?? "").toUpperCase();
  if (/(^|[^A-Z])(GND|VSS|0V|RETURN)([^A-Z]|$)/.test(name)) return "black";
  if (/(VCC|VDD|VIN|VBUS|5V|3V3|12V|PWR|\+)/.test(name)) return "red";
  if (/(CLK|SCK|SCL|CS|EN|RST|RESET)/.test(name)) return "yellow";
  return "white";
}

/** Stable short labels (W1, W2…) in document order, matching the build drawing. */
export function wireLabels(design: Design): Map<string, string> {
  return new Map(
    Object.keys(design.wires ?? {}).map((id, index) => [id, `W${index + 1}`]),
  );
}

export function railLabels(design: Design): Map<string, string> {
  return new Map(
    Object.keys(design.rails ?? {}).map((id, index) => [id, `R${index + 1}`]),
  );
}

export function netOfPin(
  design: Design,
  ref: PinRef,
): ElectricalNet | undefined {
  return Object.values(design.nets ?? {}).find((net) =>
    net.pins?.some(
      (pin) =>
        pin.component_id === ref.component_id && pin.pin_id === ref.pin_id,
    ),
  );
}

export function pinDisplay(
  design: Design,
  ref: PinRef,
): { component: string; pin: string; pinName: string } {
  const instance = design.components?.[ref.component_id];
  const pin = definitionOf(design, instance)?.pins.find(
    (candidate) => candidate.id === ref.pin_id,
  );
  return {
    component: instance?.name ?? "Missing part",
    pin: pin?.number ?? ref.pin_id,
    pinName: pin?.name ?? ref.pin_id,
  };
}

export function terminalDisplay(design: Design, terminal: TerminalRef): string {
  if (terminal.kind === "pin") {
    const { component, pin, pinName } = pinDisplay(design, terminal);
    return pin === pinName
      ? `${component} · ${pin}`
      : `${component} · ${pin} ${pinName}`;
  }
  if (terminal.kind === "rail_node") {
    const label = railLabels(design).get(terminal.rail_id) ?? "Rail";
    return `${label} node (${terminal.position.x2 / 2}, ${terminal.position.y2 / 2})`;
  }
  return "Junction";
}

export function describeWire(design: Design, wire: Wire): string {
  return `${terminalDisplay(design, wire.start)} → ${terminalDisplay(design, wire.end)}`;
}

export interface PartGroup {
  definition: ComponentDefinition | undefined;
  definitionId: string;
  instances: ComponentInstance[];
  bridge: boolean;
}

/** Groups placed parts by type, bridges last, so long bridge lists stay tidy. */
export function groupPlacedParts(design: Design): PartGroup[] {
  const groups = new Map<string, PartGroup>();
  for (const instance of Object.values(design.components ?? {})) {
    const definition = definitionOf(design, instance);
    const group = groups.get(instance.definition_id) ?? {
      definition,
      definitionId: instance.definition_id,
      instances: [],
      bridge: isBridge(definition),
    };
    group.instances.push(instance);
    groups.set(instance.definition_id, group);
  }
  return [...groups.values()]
    .map((group) => ({
      ...group,
      instances: group.instances.sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { numeric: true }),
      ),
    }))
    .sort(
      (a, b) =>
        Number(a.bridge) - Number(b.bridge) ||
        (a.definition?.name ?? a.definitionId).localeCompare(
          b.definition?.name ?? b.definitionId,
        ),
    );
}

/** Next free instance name such as "Resistor 3" for a definition. */
export function nextInstanceName(
  design: Design,
  definition: ComponentDefinition,
): string {
  const used = new Set(
    Object.values(design.components ?? {}).map((item) => item.name),
  );
  for (let index = 1; ; index += 1) {
    const name = `${definition.name} ${index}`;
    if (!used.has(name)) return name;
  }
}

export function formatMm(value: number): string {
  return `${value.toFixed(value >= 100 ? 0 : 1)} mm`;
}

export function relativeTime(timestampMs: number, now = Date.now()): string {
  const seconds = Math.round((now - timestampMs) / 1000);
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(timestampMs).toLocaleDateString();
}
