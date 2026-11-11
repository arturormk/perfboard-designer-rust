/** Rich tooltip for whatever is under the pointer on the board. */
import {
  airwires,
  design,
  hover,
  unroutedByNet,
  wireLabelMap,
} from "../state/store";
import { definitionOf, polylineLengthMm, bendCount } from "../model/geometry";
import {
  formatMm,
  netOfPin,
  terminalDisplay,
  wireColorCss,
  wireColorName,
} from "../model/naming";

export function HoverCard({ at }: { at: { x: number; y: number } }) {
  const target = hover.value;
  const current = design.value;
  if (!target || !current) return null;
  let body: preact.ComponentChildren = null;

  if (target.kind === "pin") {
    const instance = current.components?.[target.ref.component_id];
    const definition = definitionOf(current, instance);
    const pin = definition?.pins.find((item) => item.id === target.ref.pin_id);
    if (!instance || !pin) return null;
    const net = netOfPin(current, target.ref);
    body = (
      <>
        <div class="hc-title">
          <span class="hc-pin">{pin.number}</span>
          <strong>{pin.name}</strong>
        </div>
        <div class="hc-sub">{instance.name}</div>
        {(pin.functions?.length || pin.aliases?.length) && (
          <div class="hc-tags">
            {[...(pin.aliases ?? []), ...(pin.functions ?? [])]
              .slice(0, 6)
              .map((tag) => (
                <span key={tag}>{tag}</span>
              ))}
          </div>
        )}
        <div class={`hc-net${net ? "" : " none"}`}>
          {net
            ? `Net ${net.name}${net.state === "proposed" ? " (proposed)" : ""}`
            : "Not connected"}
        </div>
      </>
    );
  } else if (target.kind === "component") {
    const instance = current.components?.[target.id];
    const definition = definitionOf(current, instance);
    if (!instance) return null;
    body = (
      <>
        <div class="hc-title">
          <strong>{instance.name}</strong>
        </div>
        <div class="hc-sub">
          {definition?.name ?? instance.definition_id} ·{" "}
          {definition?.pins.length ?? 0} pins
        </div>
        {instance.notes && <p class="hc-notes">{instance.notes}</p>}
      </>
    );
  } else if (target.kind === "wire") {
    const wire = current.wires?.[target.id];
    if (!wire) return null;
    const net = current.nets?.[wire.net_id];
    body = (
      <>
        <div class="hc-title">
          <span
            class="hc-swatch"
            style={{ background: wireColorCss(wire.color) }}
          />
          <strong>{wireLabelMap.value.get(wire.id)}</strong>
          <span class="hc-net-inline">{net?.name}</span>
        </div>
        <div class="hc-sub">
          {terminalDisplay(current, wire.start)} →{" "}
          {terminalDisplay(current, wire.end)}
        </div>
        <div class="hc-sub">
          {formatMm(polylineLengthMm(wire.vertices, current.board.pitch_mm))} ·{" "}
          {bendCount(wire.vertices)} bends · {wireColorName(wire.color)}
          {wire.locked ? " · locked" : ""}
        </div>
        {wire.notes && <p class="hc-notes">{wire.notes}</p>}
      </>
    );
  } else if (target.kind === "airwire") {
    const net = current.nets?.[target.netId];
    const remaining =
      unroutedByNet.value.get(target.netId) ?? airwires.value.length;
    body = (
      <>
        <div class="hc-title">
          <strong>Unrouted · {net?.name}</strong>
        </div>
        <div class="hc-sub">
          {remaining} connection{remaining === 1 ? "" : "s"} still missing.
          Click for a suggested route.
        </div>
      </>
    );
  } else if (target.kind === "rail") {
    const rail = current.rails?.[target.id];
    if (!rail) return null;
    body = (
      <div class="hc-title">
        <strong>Rail · {current.nets?.[rail.net_id]?.name}</strong>
      </div>
    );
  }
  if (!body) return null;
  return (
    <div
      class="hover-card"
      style={{ left: `${at.x + 16}px`, top: `${at.y + 16}px` }}
      role="tooltip"
    >
      {body}
    </div>
  );
}
