import { useSignal } from "@preact/signals";
import type {
  ComponentDefinition,
  ComponentInstance,
  Constraint,
  ElectricalNet,
  Rail,
  RouteProposal,
  Saddle,
  Wire,
} from "../api/project";
import {
  assignPin,
  evaluateWire,
  focusOn,
  mutate,
  proposeRoute,
  railNodeLock,
  removeRailNode,
  netInUse,
  mergeNets,
  removeNet,
  removeSelection,
  renameNet,
  select,
} from "../state/actions";
import {
  activeNetId,
  airwires,
  connectivity,
  design,
  hover,
  library,
  railLabelMap,
  saddleLabelMap,
  selection,
  side,
  tool,
  unroutedByNet,
  view,
  wireLabelMap,
} from "../state/store";
import {
  bendCount,
  definitionOf,
  isBridge,
  isJumper,
  nextRotation,
  polylineLengthMm,
  rotationDegrees,
  type Design,
} from "../model/geometry";
import {
  WIRE_COLORS,
  describeWire,
  formatMm,
  netOfPin,
  pinDisplay,
  terminalDisplay,
  wireColorCss,
  wireColorName,
} from "../model/naming";
import {
  isStraightRoute,
  orthogonalRoute,
  straightRoute,
} from "../editor/wire-routing";
import { renderSanitizedMarkdown } from "../component-designer/markdown";
import { Icon } from "../ui/Icon";
import { PartThumb } from "./PartsPanel";
import {
  CONSTRAINT_SPECS,
  defaultValue,
  describeConstraint,
  specFor,
  type FieldSpec,
} from "./constraints";

export function Inspector() {
  const current = design.value;
  const item = selection.value;
  if (!current) return null;
  if (item?.kind === "component") {
    const instance = current.components?.[item.id];
    if (instance)
      return <ComponentInspector instance={instance} design={current} />;
  }
  if (item?.kind === "wire") {
    const wire = current.wires?.[item.id];
    if (wire) return <WireInspector wire={wire} design={current} />;
  }
  if (item?.kind === "rail") {
    const rail = current.rails?.[item.id];
    if (rail) return <RailInspector rail={rail} design={current} />;
  }
  if (item?.kind === "saddle") {
    const saddle = current.saddles?.[item.id];
    if (saddle) return <SaddleInspector saddle={saddle} />;
  }
  if (item?.kind === "net") {
    const net = current.nets?.[item.id];
    if (net) return <NetInspector net={net} design={current} />;
  }
  return <BoardInspector design={current} />;
}

function InspectorHead({
  icon,
  title,
  subtitle,
  onFocus,
}: {
  icon: preact.ComponentChildren;
  title: preact.ComponentChildren;
  subtitle?: preact.ComponentChildren;
  onFocus?: () => void;
}) {
  return (
    <header class="inspector-head">
      <div class="inspector-icon">{icon}</div>
      <div class="inspector-title">
        <h2>{title}</h2>
        {subtitle && <p>{subtitle}</p>}
      </div>
      {onFocus && (
        <button
          class="icon-btn small"
          title="Show on board"
          aria-label="Show on board"
          onClick={onFocus}
        >
          <Icon name="target" size={16} />
        </button>
      )}
      <button
        class="icon-btn small"
        title="Deselect (Esc)"
        aria-label="Deselect"
        onClick={() => (selection.value = null)}
      >
        <Icon name="close" size={16} />
      </button>
    </header>
  );
}

function Section({
  title,
  children,
  aside,
}: {
  title: string;
  children: preact.ComponentChildren;
  aside?: preact.ComponentChildren;
}) {
  return (
    <section class="inspector-section">
      <header>
        <h3>{title}</h3>
        {aside}
      </header>
      {children}
    </section>
  );
}

// ── Component ────────────────────────────────────────────────────────────────

function ComponentInspector({
  instance,
  design: current,
}: {
  instance: ComponentInstance;
  design: Design;
}) {
  const definition = definitionOf(current, instance);
  const libraryDefinition = library.value.find(
    (candidate) => candidate.id === instance.definition_id,
  );
  const showDocs = useSignal(false);
  const bridge = isBridge(definition);
  const jumper = isJumper(definition);
  const canBecomeJumper =
    !jumper &&
    isJumper(libraryDefinition) &&
    JSON.stringify(libraryDefinition?.pins) ===
      JSON.stringify(definition?.pins);
  const rotations = definition?.allowed_rotations.length
    ? definition.allowed_rotations
    : (["deg0", "deg90", "deg180", "deg270"] as const);
  const outdated =
    libraryDefinition &&
    definition &&
    libraryDefinition.definition_version !== definition.definition_version;

  const rename = (value: string) => {
    const name = value.trim();
    if (!name || name === instance.name) return;
    void mutate({
      type: "change_instance_properties",
      instance_id: instance.id,
      name,
      notes: instance.notes ?? "",
      properties: instance.properties ?? {},
    });
  };
  const move = (axis: "x" | "y", value: string) => {
    const number = Math.round(Number(value));
    if (
      !Number.isFinite(number) ||
      number < 0 ||
      number === instance.position[axis]
    )
      return;
    void mutate({
      type: "move_component",
      instance_id: instance.id,
      position: { ...instance.position, [axis]: number },
    });
  };

  return (
    <div class="inspector">
      <InspectorHead
        icon={
          definition ? (
            <PartThumb definition={definition} size={38} />
          ) : (
            <Icon name="chip" />
          )
        }
        title={
          <input
            key={`${instance.id}:${instance.name}`}
            class="title-input"
            defaultValue={instance.name}
            aria-label="Part name"
            onBlur={(event) => rename((event.target as HTMLInputElement).value)}
            onKeyDown={(event) =>
              event.key === "Enter" && (event.target as HTMLInputElement).blur()
            }
          />
        }
        subtitle={`${definition?.name ?? instance.definition_id}${bridge ? " · solder side" : ""}`}
        onFocus={() => focusOn({ kind: "component", id: instance.id })}
      />

      <Section title="Placement">
        <div class="field-grid three">
          <label class="field">
            <span>X</span>
            <input
              key={`x${instance.position.x}`}
              type="number"
              min={0}
              defaultValue={String(instance.position.x)}
              onChange={(event) =>
                move("x", (event.target as HTMLInputElement).value)
              }
            />
          </label>
          <label class="field">
            <span>Y</span>
            <input
              key={`y${instance.position.y}`}
              type="number"
              min={0}
              defaultValue={String(instance.position.y)}
              onChange={(event) =>
                move("y", (event.target as HTMLInputElement).value)
              }
            />
          </label>
          <div class="field">
            <span>Rotate</span>
            <button
              class="btn small"
              title="Rotate 90° (R)"
              onClick={() =>
                void mutate({
                  type: "rotate_component",
                  instance_id: instance.id,
                  rotation: nextRotation(
                    instance.rotation,
                    definition?.allowed_rotations,
                  ),
                })
              }
            >
              <Icon name="rotate" size={14} />{" "}
              {rotationDegrees(instance.rotation)}°
            </button>
          </div>
        </div>
        {rotations.length < 4 && (
          <p class="field-note">
            This part allows{" "}
            {rotations.map((r) => `${rotationDegrees(r)}°`).join(", ")}.
          </p>
        )}
        <p class="field-note">
          Drag on the board, or use arrow keys to nudge one hole.
        </p>
      </Section>

      {jumper && definition && (
        <Section title="Shunt">
          <div class="segmented">
            {([false, true] as const).map((value) => (
              <button
                key={String(value)}
                class={Boolean(instance.shunted) === value ? "on" : ""}
                onClick={() => {
                  if (Boolean(instance.shunted) === value) return;
                  void mutate({
                    type: "set_jumper_shunt",
                    instance_id: instance.id,
                    shunted: value,
                  });
                }}
              >
                {value ? "Shunted (closed)" : "Open"}
              </button>
            ))}
          </div>
          <p class="field-note">{shuntNote(current, instance, definition)}</p>
        </Section>
      )}
      {canBecomeJumper && (
        <Section title="Shunt">
          <p class="field-note">
            The library now marks this part as a jumper that can take a shunt.
            Use <strong>Update metadata</strong> below to enable it.
          </p>
        </Section>
      )}

      {definition && !bridge && (
        <Section
          title="Pins"
          aside={<span class="count">{definition.pins.length}</span>}
        >
          <PinTable instance={instance} design={current} />
        </Section>
      )}

      <Section title="Notes">
        <textarea
          key={`${instance.id}:${instance.notes ?? ""}`}
          class="notes"
          rows={3}
          placeholder="Assembly notes, values, reminders…"
          defaultValue={instance.notes ?? ""}
          onBlur={(event) => {
            const notes = (event.target as HTMLTextAreaElement).value;
            if (notes !== (instance.notes ?? ""))
              void mutate({
                type: "change_instance_properties",
                instance_id: instance.id,
                name: instance.name,
                notes,
                properties: instance.properties ?? {},
              });
          }}
        />
      </Section>

      {definition && (
        <Section title="Definition">
          <dl class="kv">
            <dt>Type</dt>
            <dd>{definition.name}</dd>
            <dt>Version</dt>
            <dd>
              {definition.definition_version}
              {outdated && (
                <span class="badge warn">
                  library has {libraryDefinition!.definition_version}
                </span>
              )}
            </dd>
            <dt>Body</dt>
            <dd>
              {definition.body.width} × {definition.body.height} mm
            </dd>
          </dl>
          {definition.description && (
            <p class="field-note">{definition.description}</p>
          )}
          <div class="row">
            {libraryDefinition && (
              <button
                class="btn small"
                title="Refresh name, description and documentation from the library, and whether a two-pin part takes a shunt. Pins and geometry stay as they are."
                onClick={() =>
                  void mutate({
                    type: "update_component_metadata",
                    instance_id: instance.id,
                    definition: libraryDefinition,
                  })
                }
              >
                Update metadata
              </button>
            )}
            {definition.documentation && (
              <button
                class="btn small ghost"
                onClick={() => (showDocs.value = !showDocs.value)}
              >
                {showDocs.value ? "Hide" : "Show"} documentation
              </button>
            )}
          </div>
          {showDocs.value && definition.documentation && (
            <div
              class="markdown"
              dangerouslySetInnerHTML={{
                __html: renderSanitizedMarkdown(definition.documentation),
              }}
            />
          )}
        </Section>
      )}

      <footer class="inspector-foot">
        <button class="btn danger" onClick={removeSelection}>
          <Icon name="trash" size={15} /> Remove part
        </button>
      </footer>
    </div>
  );
}

function PinTable({
  instance,
  design: current,
}: {
  instance: ComponentInstance;
  design: Design;
}) {
  const definition = definitionOf(current, instance)!;
  const nets = Object.values(current.nets ?? {});
  return (
    <table class="pin-table">
      <thead>
        <tr>
          <th>No.</th>
          <th>Name</th>
          <th>Net</th>
        </tr>
      </thead>
      <tbody>
        {definition.pins.map((pin) => {
          const ref = { component_id: instance.id, pin_id: pin.id };
          const net = netOfPin(current, ref);
          return (
            <tr
              key={pin.id}
              onMouseEnter={() => (hover.value = { kind: "pin", ref })}
              onMouseLeave={() => (hover.value = null)}
            >
              <td class="mono">{pin.number}</td>
              <td
                title={[...(pin.aliases ?? []), ...(pin.functions ?? [])].join(
                  ", ",
                )}
              >
                {pin.name}
              </td>
              <td>
                <select
                  class={`net-select${net ? "" : " empty"}`}
                  value={net?.id ?? ""}
                  aria-label={`Net for pin ${pin.number}`}
                  onChange={(event) =>
                    void assignPin(
                      (event.target as HTMLSelectElement).value || null,
                      ref,
                    )
                  }
                >
                  <option value="">—</option>
                  {nets.map((candidate) => (
                    <option key={candidate.id} value={candidate.id}>
                      {candidate.name}
                      {candidate.state === "proposed" ? " (proposed)" : ""}
                    </option>
                  ))}
                </select>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

// ── Wire ─────────────────────────────────────────────────────────────────────

function WireInspector({
  wire,
  design: current,
}: {
  wire: Wire;
  design: Design;
}) {
  const evaluation = useSignal<RouteProposal | null>(null);
  const showPath = useSignal(false);
  const net = current.nets?.[wire.net_id];
  const label = wireLabelMap.value.get(wire.id) ?? "Wire";
  const length = polylineLengthMm(wire.vertices, current.board.pitch_mm);
  const straight = isStraightRoute(wire.vertices);

  return (
    <div class="inspector">
      <InspectorHead
        icon={
          <span
            class="wire-swatch"
            style={{ background: wireColorCss(wire.color) }}
          />
        }
        title={
          <>
            {label} <span class="title-net">{net?.name}</span>
          </>
        }
        subtitle={describeWire(current, wire)}
        onFocus={() => focusOn({ kind: "wire", id: wire.id })}
      />

      <Section title="Route">
        <dl class="stat-row">
          <div>
            <dt>Length</dt>
            <dd>{formatMm(length)}</dd>
          </div>
          <div>
            <dt>Bends</dt>
            <dd>{bendCount(wire.vertices)}</dd>
          </div>
          <div>
            <dt>Points</dt>
            <dd>{wire.vertices.length}</dd>
          </div>
        </dl>
        <div class="row wrap">
          <button
            class="btn small"
            disabled={wire.locked}
            title="Ask the router for a better path; you can review it before accepting"
            onClick={() =>
              void proposeRoute(wire.net_id, wire.start, wire.end, wire)
            }
          >
            <Icon name="route" size={14} /> Suggest reroute
          </button>
          <button
            class="btn small"
            disabled={wire.locked}
            onClick={() =>
              void mutate({
                type: "modify_wire_path",
                wire_id: wire.id,
                vertices: straight
                  ? orthogonalRoute(wire.vertices)
                  : straightRoute(wire.vertices),
              })
            }
          >
            {straight ? "Use one 90° bend" : "Make straight"}
          </button>
          <button
            class="btn small ghost"
            onClick={async () =>
              (evaluation.value = (await evaluateWire(wire)) ?? null)
            }
          >
            Evaluate
          </button>
        </div>
        {evaluation.value && (
          <div class="evaluation">
            <p>
              Router cost <strong>{evaluation.value.cost.total}</strong> ·{" "}
              {evaluation.value.crossings.length} crossings
              {evaluation.value.jumper_match
                ? ` · fits ${evaluation.value.jumper_match.inventory_id}`
                : ""}
            </p>
            {(evaluation.value.warnings ?? []).length ? (
              (evaluation.value.warnings ?? []).map((warning) => (
                <p key={warning} class="route-warning">
                  <Icon name="warning" size={13} /> {warning}
                </p>
              ))
            ) : (
              <p class="ok-note">
                <Icon name="check" size={13} /> No soft warnings
              </p>
            )}
          </div>
        )}
        <p class="field-note">
          Drag the round handles to bend. Double-click a segment to add a point,
          a point to remove it.
        </p>
      </Section>

      <Section title="Appearance">
        <div class="field">
          <span>Insulation colour</span>
          <div class="swatches">
            {WIRE_COLORS.map((color) => (
              <button
                key={color}
                class={`swatch${wireColorName(wire.color) === color ? " on" : ""}`}
                style={{ "--swatch": wireColorCss(color) }}
                title={color}
                aria-label={color}
                onClick={() =>
                  void mutate({
                    type: "set_wire_color",
                    wire_id: wire.id,
                    color,
                  })
                }
              />
            ))}
          </div>
        </div>
        <div class="field">
          <span>Board side</span>
          <div class="segmented">
            {(["component", "solder"] as const).map((value) => (
              <button
                key={value}
                class={wire.side === value ? "on" : ""}
                onClick={() => {
                  if (wire.side === value) return;
                  side.value = value;
                  void mutate({
                    type: "set_wire_side",
                    wire_id: wire.id,
                    side: value,
                  });
                }}
              >
                {value === "component" ? "Component (top)" : "Solder (bottom)"}
              </button>
            ))}
          </div>
        </div>
        <label class="check">
          <input
            type="checkbox"
            checked={Boolean(wire.locked)}
            onChange={() =>
              void mutate({
                type: "set_wire_lock",
                wire_id: wire.id,
                locked: !wire.locked,
              })
            }
          />
          <span>
            <Icon name={wire.locked ? "lock" : "unlock"} size={14} /> Locked —
            automatic rerouting leaves it alone
          </span>
        </label>
      </Section>

      <Section title="Connects">
        <ul class="terminal-list">
          <li>
            <span class="terminal-tag">from</span>{" "}
            {terminalDisplay(current, wire.start)}
          </li>
          <li>
            <span class="terminal-tag">to</span>{" "}
            {terminalDisplay(current, wire.end)}
          </li>
        </ul>
      </Section>

      {wire.notes && (
        <Section title="Notes">
          <p class="notes-text">{wire.notes}</p>
        </Section>
      )}

      <Section
        title="Path points"
        aside={
          <button
            class="link"
            onClick={() => (showPath.value = !showPath.value)}
          >
            {showPath.value ? "hide" : "edit numerically"}
          </button>
        }
      >
        {showPath.value && <PathEditor wire={wire} />}
      </Section>

      <footer class="inspector-foot">
        <button class="btn danger" onClick={removeSelection}>
          <Icon name="trash" size={15} /> Remove wire
        </button>
      </footer>
    </div>
  );
}

function PathEditor({ wire }: { wire: Wire }) {
  const update = (index: number, axis: "x2" | "y2", value: string) => {
    const number = Math.round(Number(value) * 2);
    if (!Number.isFinite(number)) return;
    const vertices = wire.vertices.map((vertex, position) =>
      position === index ? { ...vertex, [axis]: number } : vertex,
    );
    void mutate({ type: "modify_wire_path", wire_id: wire.id, vertices });
  };
  return (
    <table class="path-table">
      <thead>
        <tr>
          <th>#</th>
          <th>x</th>
          <th>y</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {wire.vertices.map((vertex, index) => {
          const end = index === 0 || index === wire.vertices.length - 1;
          return (
            <tr key={`${index}:${vertex.x2}:${vertex.y2}`}>
              <td class="mono">
                {index === 0 ? "start" : end ? "end" : index}
              </td>
              <td>
                <input
                  type="number"
                  step={0.5}
                  disabled={end || wire.locked}
                  defaultValue={String(vertex.x2 / 2)}
                  onChange={(event) =>
                    update(
                      index,
                      "x2",
                      (event.target as HTMLInputElement).value,
                    )
                  }
                />
              </td>
              <td>
                <input
                  type="number"
                  step={0.5}
                  disabled={end || wire.locked}
                  defaultValue={String(vertex.y2 / 2)}
                  onChange={(event) =>
                    update(
                      index,
                      "y2",
                      (event.target as HTMLInputElement).value,
                    )
                  }
                />
              </td>
              <td>
                {!end && !wire.locked && (
                  <button
                    class="icon-btn small"
                    aria-label={`Remove point ${index}`}
                    onClick={() =>
                      void mutate({
                        type: "modify_wire_path",
                        wire_id: wire.id,
                        vertices: wire.vertices.filter(
                          (_, position) => position !== index,
                        ),
                      })
                    }
                  >
                    <Icon name="close" size={13} />
                  </button>
                )}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

// ── Rail ─────────────────────────────────────────────────────────────────────

function RailInspector({
  rail,
  design: current,
}: {
  rail: Rail;
  design: Design;
}) {
  const net = current.nets?.[rail.net_id];
  return (
    <div class="inspector">
      <InspectorHead
        icon={<Icon name="rail" />}
        title={
          <>
            {railLabelMap.value.get(rail.id) ?? "Rail"}{" "}
            <span class="title-net">{net?.name}</span>
          </>
        }
        subtitle={`${rail.vertices.length} connection nodes · ${rail.side} side`}
        onFocus={() => focusOn({ kind: "rail", id: rail.id })}
      />
      <Section title="Appearance">
        <div class="field">
          <span>Insulation colour</span>
          <div class="swatches">
            {WIRE_COLORS.map((color) => (
              <button
                key={color}
                class={`swatch${wireColorName(rail.color) === color ? " on" : ""}`}
                style={{ "--swatch": wireColorCss(color) }}
                title={color}
                aria-label={color}
                onClick={() =>
                  void mutate(
                    { type: "set_rail_color", rail_id: rail.id, color },
                    { failure: "Could not change the rail colour" },
                  )
                }
              />
            ))}
          </div>
        </div>
      </Section>
      <Section title="Connection nodes">
        <table class="path-table">
          <thead>
            <tr>
              <th>#</th>
              <th>x</th>
              <th>y</th>
              <th>reach</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rail.vertices.map((vertex, index) => {
              const lock = railNodeLock(current, rail.id, index);
              const throughHole = vertex.x2 % 2 === 0 && vertex.y2 % 2 === 0;
              return (
                <tr key={`${index}:${vertex.x2}:${vertex.y2}`}>
                  <td class="mono">{index + 1}</td>
                  <td class="mono">{vertex.x2 / 2}</td>
                  <td class="mono">{vertex.y2 / 2}</td>
                  <td
                    title={
                      throughHole
                        ? "On a hole: wires on either side can connect"
                        : "Between holes: only wires on the rail's side"
                    }
                  >
                    {throughHole ? "both sides" : `${rail.side} only`}
                  </td>
                  <td>
                    {!lock && (
                      <button
                        class="icon-btn small"
                        aria-label={`Remove node ${index + 1}`}
                        onClick={() => void removeRailNode(rail.id, index)}
                      >
                        <Icon name="close" size={13} />
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <p class="field-note">
          A rail is a net-owned conductor; all its nodes are connected. With the
          Select tool, double-click the rail to add a node and double-click a
          node to remove it. With the Wire tool, click anywhere on the rail to
          add a node and connect to it. Nodes on a hole act like a pin: wires on
          either side of the board can connect there.
        </p>
      </Section>
      <footer class="inspector-foot">
        <button class="btn danger" onClick={removeSelection}>
          <Icon name="trash" size={15} /> Remove rail
        </button>
      </footer>
    </div>
  );
}

// ── Saddle ───────────────────────────────────────────────────────────────────

function SaddleInspector({ saddle }: { saddle: Saddle }) {
  const span = Math.hypot(
    saddle.end.x - saddle.start.x,
    saddle.end.y - saddle.start.y,
  );
  const holes = (point: { x: number; y: number }) => `${point.x},${point.y}`;
  return (
    <div class="inspector">
      <InspectorHead
        icon={<Icon name="saddle" />}
        title={saddleLabelMap.value.get(saddle.id) ?? "Saddle"}
        subtitle={`Wire tie · holes ${holes(saddle.start)} → ${holes(saddle.end)} · ${span.toFixed(span % 1 ? 1 : 0)} pitch`}
        onFocus={() => focusOn({ kind: "saddle", id: saddle.id })}
      />
      <Section title="Placement">
        <div class="field">
          <span>Strap lies on</span>
          <div class="segmented">
            {(["component", "solder"] as const).map((value) => (
              <button
                key={value}
                class={saddle.side === value ? "on" : ""}
                onClick={() => {
                  if (saddle.side === value) return;
                  side.value = value;
                  void mutate(
                    {
                      type: "update_saddle",
                      saddle: { ...saddle, side: value },
                    },
                    { failure: "Could not move the saddle" },
                  );
                }}
              >
                {value === "component" ? "Component (top)" : "Solder (bottom)"}
              </button>
            ))}
          </div>
        </div>
      </Section>
      <Section title="About saddles">
        <p class="field-note">
          A saddle is a short strap soldered into two free holes on either side
          of a wire or bundle, holding it to the board. It belongs to no net:
          its holes must stay clear of pins, wires, rails and junctions.
        </p>
      </Section>
      <footer class="inspector-foot">
        <button class="btn danger" onClick={removeSelection}>
          <Icon name="trash" size={15} /> Remove saddle
        </button>
      </footer>
    </div>
  );
}

/** What the shunt on a jumper does with its current setting and nets. */
function shuntNote(
  current: Design,
  instance: ComponentInstance,
  definition: ComponentDefinition,
): string {
  const [first, second] = definition.pins;
  if (!instance.shunted)
    return "Without a shunt the two pins are not connected.";
  const nets = [first, second].map((pin) =>
    netOfPin(current, { component_id: instance.id, pin_id: pin!.id }),
  );
  if (nets[0] && nets[1] && nets[0].id !== nets[1].id)
    return `The shunt joins net ${nets[0].name} to net ${nets[1].name}. Each stays its own net, routed on its own copper.`;
  return `The shunt joins pins ${first!.number} and ${second!.number}, so they need no wire between them.`;
}

// ── Net ──────────────────────────────────────────────────────────────────────

function NetInspector({
  net,
  design: current,
}: {
  net: ElectricalNet;
  design: Design;
}) {
  const wires = Object.values(current.wires ?? {}).filter(
    (wire) => wire.net_id === net.id,
  );
  const unrouted = unroutedByNet.value.get(net.id) ?? 0;
  const firstAirwire = airwires.value.find(
    (airwire) => airwire.netId === net.id,
  );
  const proposed = net.state === "proposed";
  const inUse = netInUse(current, net.id);
  return (
    <div class="inspector">
      <InspectorHead
        icon={<Icon name="net" />}
        title={
          <input
            key={`${net.id}:${net.name}`}
            class="title-input"
            defaultValue={net.name}
            aria-label="Net name"
            onInput={(event) => {
              const input = event.target as HTMLInputElement;
              const caret = input.selectionStart;
              input.value = input.value.toUpperCase();
              input.setSelectionRange(caret, caret);
            }}
            onBlur={(event) => {
              const input = event.target as HTMLInputElement;
              void renameNet(net.id, input.value).then((ok) => {
                if (!ok) input.value = net.name;
              });
            }}
            onKeyDown={(event) => {
              const input = event.target as HTMLInputElement;
              if (event.key === "Enter") input.blur();
              else if (event.key === "Escape") {
                input.value = net.name;
                input.blur();
              }
            }}
          />
        }
        subtitle={
          proposed
            ? "Proposed · not part of the circuit yet"
            : unrouted
              ? `${unrouted} open connection${unrouted === 1 ? "" : "s"}`
              : (net.pins?.length ?? 0) >= 2
                ? "Fully routed"
                : "Needs at least two pins"
        }
        onFocus={() => focusOn({ kind: "net", id: net.id })}
      />
      {proposed && (
        <div class="callout agent">
          <Icon name="agent" size={16} />
          <div>
            <p>
              This connection was proposed{net.notes ? `: ${net.notes}` : "."}
            </p>
            <div class="row">
              <button
                class="btn primary small"
                onClick={() =>
                  void mutate({ type: "accept_net_proposal", net_id: net.id })
                }
              >
                Accept
              </button>
              <button
                class="btn ghost small"
                onClick={() =>
                  void mutate({ type: "reject_net_proposal", net_id: net.id })
                }
              >
                Reject
              </button>
            </div>
          </div>
        </div>
      )}
      {!proposed && (
        <div class="row wrap pad">
          <button
            class="btn small"
            onClick={() => {
              activeNetId.value = net.id;
              tool.value = "wire";
            }}
          >
            <Icon name="wire" size={14} /> Draw wire
          </button>
          {firstAirwire && (
            <button
              class="btn small"
              onClick={() =>
                void proposeRoute(
                  net.id,
                  { kind: "pin", ...firstAirwire.from },
                  { kind: "pin", ...firstAirwire.to },
                )
              }
            >
              <Icon name="route" size={14} /> Suggest route
            </button>
          )}
        </div>
      )}
      <Section
        title="Pins"
        aside={<span class="count">{net.pins?.length ?? 0}</span>}
      >
        {!net.pins?.length && (
          <p class="field-note">
            Assign pins from a part's pin table, or start a wire from a pin.
          </p>
        )}
        <ul class="member-list">
          {(net.pins ?? []).map((pin) => {
            const label = pinDisplay(current, pin);
            return (
              <li key={`${pin.component_id}:${pin.pin_id}`}>
                <button
                  class="member"
                  onClick={() =>
                    select(
                      { kind: "component", id: pin.component_id },
                      { focus: true },
                    )
                  }
                  onMouseEnter={() => (hover.value = { kind: "pin", ref: pin })}
                  onMouseLeave={() => (hover.value = null)}
                >
                  <span>{label.component}</span>
                  <strong>{label.pin}</strong>
                  {label.pinName !== label.pin && (
                    <small>{label.pinName}</small>
                  )}
                </button>
                <button
                  class="icon-btn small"
                  aria-label="Remove from net"
                  title="Remove from net"
                  onClick={() => void assignPin(null, pin)}
                >
                  <Icon name="close" size={13} />
                </button>
              </li>
            );
          })}
        </ul>
      </Section>
      <Section title="Wires" aside={<span class="count">{wires.length}</span>}>
        <ul class="member-list">
          {wires.map((wire) => (
            <li key={wire.id}>
              <button
                class="member"
                onClick={() =>
                  select({ kind: "wire", id: wire.id }, { focus: true })
                }
              >
                <i
                  class="dot"
                  style={{ background: wireColorCss(wire.color) }}
                />
                <strong>{wireLabelMap.value.get(wire.id)}</strong>
                <small>
                  {terminalDisplay(current, wire.start)} →{" "}
                  {terminalDisplay(current, wire.end)}
                </small>
              </button>
            </li>
          ))}
        </ul>
      </Section>
      {!proposed && <JoinedNetsSection net={net} design={current} />}
      {net.notes && !proposed && (
        <Section title="Notes">
          <p class="notes-text">{net.notes}</p>
        </Section>
      )}
      {!proposed && <MergeNetSection net={net} design={current} />}
      <footer class="inspector-foot">
        <button
          class="btn danger"
          disabled={!!inUse}
          title={inUse ? `${inUse}; remove them first` : undefined}
          onClick={() => void removeNet(net.id)}
        >
          <Icon name="trash" size={15} /> Remove net
        </button>
      </footer>
    </div>
  );
}

/**
 * Nets this one is currently joined with through fitted shunts: the computed
 * net it belongs to with the present jumper settings. Each net is still routed
 * and checked on its own copper.
 */
function JoinedNetsSection({
  net,
  design: current,
}: {
  net: ElectricalNet;
  design: Design;
}) {
  const entry = connectivity.value.find((item) => item.net_id === net.id);
  const joined = (entry?.joined_nets ?? [])
    .map((id) => current.nets?.[id])
    .filter((other): other is ElectricalNet => !!other)
    .sort((a, b) => a.name.localeCompare(b.name));
  if (!joined.length) return null;
  const shunts = (entry?.shunts ?? [])
    .map((id) => current.components?.[id]?.name)
    .filter(Boolean)
    .sort();
  return (
    <Section
      title="Joined by shunt"
      aside={<span class="count">{joined.length}</span>}
    >
      <ul class="member-list">
        {joined.map((other) => (
          <li key={other.id}>
            <button
              class="member"
              onClick={() =>
                select({ kind: "net", id: other.id }, { focus: true })
              }
            >
              <strong>{other.name}</strong>
            </button>
          </li>
        ))}
      </ul>
      <p class="field-note">
        With the shunt{shunts.length === 1 ? "" : "s"} on {shunts.join(", ")}{" "}
        fitted, these nets are one connection. Each is still routed and checked
        on its own copper.
      </p>
    </Section>
  );
}

function MergeNetSection({
  net,
  design: current,
}: {
  net: ElectricalNet;
  design: Design;
}) {
  const targetId = useSignal("");
  const others = Object.values(current.nets ?? {})
    .filter((other) => other.id !== net.id && other.state === "accepted")
    .sort((a, b) => a.name.localeCompare(b.name));
  if (!others.length) return null;
  const target = others.find((other) => other.id === targetId.value);
  return (
    <Section title="Merge">
      <div class="row">
        <select
          aria-label="Net to merge into"
          value={targetId.value}
          onChange={(event) =>
            (targetId.value = (event.target as HTMLSelectElement).value)
          }
        >
          <option value="">Merge into…</option>
          {others.map((other) => (
            <option key={other.id} value={other.id}>
              {other.name}
            </option>
          ))}
        </select>
        <button
          class="btn small"
          disabled={!target}
          onClick={() => {
            if (target) void mergeNets(net.id, target.id);
          }}
        >
          <Icon name="net" size={14} /> Merge
        </button>
      </div>
      <p class="field-note">
        Moves this net's pins, wires and rails onto the chosen net and removes{" "}
        {net.name}. Renaming a net to an existing name does the same.
      </p>
    </Section>
  );
}

// ── Board (nothing selected) ────────────────────────────────────────────────

function BoardInspector({ design: current }: { design: Design }) {
  const board = current.board;
  const widthMm = (board.holes_x - 1) * board.pitch_mm;
  const heightMm = (board.holes_y - 1) * board.pitch_mm;
  const parts = Object.values(current.components ?? {});
  const bridges = parts.filter((instance) =>
    isBridge(definitionOf(current, instance)),
  ).length;
  return (
    <div class="inspector">
      <InspectorHead
        icon={<Icon name="board" />}
        title={view.value?.name ?? "Project"}
        subtitle={`${board.holes_x} × ${board.holes_y} holes · ${board.pitch_mm} mm pitch`}
      />
      <Section title="Board">
        <dl class="stat-row">
          <div>
            <dt>Size, mm</dt>
            <dd>
              {widthMm.toFixed(1)} × {heightMm.toFixed(1)}
            </dd>
          </div>
          <div>
            <dt>Parts</dt>
            <dd>{parts.length - bridges}</dd>
          </div>
          <div>
            <dt>Bridges</dt>
            <dd>{bridges}</dd>
          </div>
          <div>
            <dt>Wires</dt>
            <dd>{Object.keys(current.wires ?? {}).length}</dd>
          </div>
        </dl>
        <p class="field-note">
          Hole-centre span. Select something on the board to inspect it.
        </p>
        <button class="btn small" onClick={() => (tool.value = "resize")}>
          <Icon name="resize" size={14} /> Resize board
        </button>
      </Section>
      <ConstraintsSection design={current} />
      <JumperSection design={current} />
    </div>
  );
}

function ConstraintsSection({ design: current }: { design: Design }) {
  const editing = useSignal<Constraint | null>(null);
  const constraints = Object.values(current.constraints ?? {});
  return (
    <Section
      title="Design rules"
      aside={
        !editing.value && (
          <button
            class="link"
            onClick={() =>
              (editing.value = {
                id: "",
                type: "proximity",
                severity: "preferred",
                origin: "user",
                rationale: "",
                ...Object.fromEntries(
                  specFor("proximity")!.fields.map((field) => [
                    field.key,
                    defaultValue(field),
                  ]),
                ),
              })
            }
          >
            + add rule
          </button>
        )
      }
    >
      {!constraints.length && !editing.value && (
        <p class="field-note">
          Rules capture intent such as “keep the capacitor near the
          transceiver”. The router and checks take them into account.
        </p>
      )}
      <ul class="rule-list">
        {constraints.map((constraint) => (
          <li key={constraint.id} class={`rule ${constraint.severity}`}>
            <div>
              <strong>{describeConstraint(constraint, current)}</strong>
              <small>
                {constraint.severity}
                {constraint.origin === "mcp" ? " · from agent" : ""}
                {constraint.rationale ? ` · ${constraint.rationale}` : ""}
              </small>
            </div>
            <button
              class="icon-btn small"
              aria-label="Edit rule"
              onClick={() => (editing.value = { ...constraint })}
            >
              <Icon name="tag" size={13} />
            </button>
            <button
              class="icon-btn small"
              aria-label="Remove rule"
              onClick={() =>
                void mutate({
                  type: "remove_constraint",
                  constraint_id: constraint.id,
                })
              }
            >
              <Icon name="close" size={13} />
            </button>
          </li>
        ))}
      </ul>
      {editing.value && (
        <ConstraintForm
          draft={editing.value}
          design={current}
          onDone={() => (editing.value = null)}
        />
      )}
    </Section>
  );
}

function ConstraintForm({
  draft,
  design: current,
  onDone,
}: {
  draft: Constraint;
  design: Design;
  onDone: () => void;
}) {
  const value = useSignal<Constraint>(draft);
  const spec = specFor(value.value.type);
  const set = (key: string, next: unknown) =>
    (value.value = { ...value.value, [key]: next });
  const parts = Object.values(current.components ?? {}).filter(
    (instance) => !isBridge(definitionOf(current, instance)),
  );
  const nets = Object.values(current.nets ?? {});
  const wires = Object.entries(current.wires ?? {});

  const field = (spec: FieldSpec) => {
    const raw = value.value[spec.key];
    switch (spec.kind) {
      case "component":
        return (
          <select
            value={String(raw ?? "")}
            onChange={(event) =>
              set(spec.key, (event.target as HTMLSelectElement).value)
            }
          >
            <option value="">Choose…</option>
            {parts.map((part) => (
              <option key={part.id} value={part.id}>
                {part.name}
              </option>
            ))}
          </select>
        );
      case "net":
        return (
          <select
            value={String(raw ?? "")}
            onChange={(event) =>
              set(spec.key, (event.target as HTMLSelectElement).value)
            }
          >
            <option value="">Choose…</option>
            {nets.map((net) => (
              <option key={net.id} value={net.id}>
                {net.name}
              </option>
            ))}
          </select>
        );
      case "wire":
        return (
          <select
            value={String(raw ?? "")}
            onChange={(event) =>
              set(spec.key, (event.target as HTMLSelectElement).value)
            }
          >
            <option value="">Choose…</option>
            {wires.map(([id]) => (
              <option key={id} value={id}>
                {wireLabelMap.value.get(id)} ·{" "}
                {current.nets?.[current.wires![id]!.net_id]?.name}
              </option>
            ))}
          </select>
        );
      case "rotation":
        return (
          <select
            value={String(raw)}
            onChange={(event) =>
              set(spec.key, (event.target as HTMLSelectElement).value)
            }
          >
            {["deg0", "deg90", "deg180", "deg270"].map((rotation) => (
              <option key={rotation} value={rotation}>
                {rotation.replace("deg", "")}°
              </option>
            ))}
          </select>
        );
      case "side":
        return (
          <select
            value={String(raw)}
            onChange={(event) =>
              set(spec.key, (event.target as HTMLSelectElement).value)
            }
          >
            <option value="component">Component</option>
            <option value="solder">Solder</option>
          </select>
        );
      case "number":
      case "integer":
        return (
          <input
            type="number"
            min={spec.min}
            step={spec.kind === "integer" ? 1 : 0.5}
            value={String(raw ?? "")}
            onInput={(event) =>
              set(spec.key, Number((event.target as HTMLInputElement).value))
            }
          />
        );
      case "position": {
        const position = (raw as { x: number; y: number }) ?? { x: 0, y: 0 };
        return (
          <div class="pair">
            <input
              type="number"
              min={0}
              value={String(position.x)}
              onInput={(event) =>
                set(spec.key, {
                  ...position,
                  x: Number((event.target as HTMLInputElement).value),
                })
              }
            />
            <input
              type="number"
              min={0}
              value={String(position.y)}
              onInput={(event) =>
                set(spec.key, {
                  ...position,
                  y: Number((event.target as HTMLInputElement).value),
                })
              }
            />
          </div>
        );
      }
      case "area": {
        const area = (raw as {
          min: { x: number; y: number };
          max: { x: number; y: number };
        }) ?? { min: { x: 0, y: 0 }, max: { x: 1, y: 1 } };
        const corner = (which: "min" | "max", axis: "x" | "y") => (
          <input
            type="number"
            min={0}
            aria-label={`${which} ${axis}`}
            value={String(area[which][axis])}
            onInput={(event) =>
              set(spec.key, {
                ...area,
                [which]: {
                  ...area[which],
                  [axis]: Number((event.target as HTMLInputElement).value),
                },
              })
            }
          />
        );
        return (
          <div class="pair four">
            {corner("min", "x")}
            {corner("min", "y")}
            <span>to</span>
            {corner("max", "x")}
            {corner("max", "y")}
          </div>
        );
      }
      default:
        return (
          <input
            value={String(raw ?? "")}
            onInput={(event) =>
              set(spec.key, (event.target as HTMLInputElement).value)
            }
          />
        );
    }
  };

  const save = async () => {
    const constraint = { ...value.value };
    const isNew = !constraint.id;
    if (isNew) constraint.id = crypto.randomUUID();
    const ok = await mutate({
      type: isNew ? "add_constraint" : "update_constraint",
      constraint,
    });
    if (ok) onDone();
  };

  return (
    <div class="rule-form">
      <label class="field">
        <span>Rule</span>
        <select
          value={value.value.type}
          disabled={Boolean(draft.id)}
          onChange={(event) => {
            const type = (event.target as HTMLSelectElement).value;
            const next = specFor(type)!;
            value.value = {
              id: value.value.id,
              severity: value.value.severity,
              origin: value.value.origin,
              rationale: value.value.rationale,
              type,
              ...Object.fromEntries(
                next.fields.map((item) => [item.key, defaultValue(item)]),
              ),
            };
          }}
        >
          {CONSTRAINT_SPECS.map((item) => (
            <option key={item.type} value={item.type}>
              {item.label}
            </option>
          ))}
        </select>
      </label>
      {spec && <p class="field-note">{spec.help}</p>}
      {spec?.fields.map((item) => (
        <label class="field" key={item.key}>
          <span>{item.label}</span>
          {field(item)}
        </label>
      ))}
      <label class="field">
        <span>Strength</span>
        <select
          value={value.value.severity}
          onChange={(event) =>
            set("severity", (event.target as HTMLSelectElement).value)
          }
        >
          <option value="required">Required — block violations</option>
          <option value="preferred">Preferred — warn and penalise</option>
          <option value="advisory">Advisory — a note for humans</option>
        </select>
      </label>
      <label class="field">
        <span>Why</span>
        <input
          value={value.value.rationale ?? ""}
          placeholder="Optional reason"
          onInput={(event) =>
            set("rationale", (event.target as HTMLInputElement).value)
          }
        />
      </label>
      <div class="row end">
        <button class="btn ghost small" onClick={onDone}>
          Cancel
        </button>
        <button class="btn primary small" onClick={() => void save()}>
          {draft.id ? "Save rule" : "Add rule"}
        </button>
      </div>
    </div>
  );
}

function JumperSection({ design: current }: { design: Design }) {
  const adding = useSignal(false);
  const draft = useSignal({
    id: "",
    span: 3,
    tolerance: 0,
    color: "red",
    quantity: "",
  });
  const jumpers = Object.values(current.jumper_inventory ?? {}).sort(
    (a, b) => a.span_holes - b.span_holes,
  );
  const save = async () => {
    const value = draft.value;
    const id = value.id.trim() || `U-${value.span}`;
    const ok = await mutate({
      type: "upsert_jumper_inventory",
      entry: {
        id,
        span_holes: value.span,
        tolerance_holes: value.tolerance,
        color: value.color,
        quantity: value.quantity === "" ? undefined : Number(value.quantity),
      },
    });
    if (ok) adding.value = false;
  };
  return (
    <Section
      title="Jumper stock"
      aside={
        !adding.value && (
          <button class="link" onClick={() => (adding.value = true)}>
            + add
          </button>
        )
      }
    >
      {!jumpers.length && !adding.value && (
        <p class="field-note">
          List the pre-formed U-jumpers you own; the router prefers spans you
          have.
        </p>
      )}
      {jumpers.length > 0 && (
        <ul class="jumper-list">
          {jumpers.map((jumper) => (
            <li key={jumper.id}>
              <i
                class="dot"
                style={{ background: wireColorCss(jumper.color as never) }}
              />
              <strong>{jumper.id}</strong>
              <span>
                {jumper.span_holes} pitches
                {jumper.tolerance_holes ? ` ±${jumper.tolerance_holes}` : ""}
                {jumper.quantity !== undefined
                  ? ` · ${jumper.quantity} pcs`
                  : ""}
              </span>
              <button
                class="icon-btn small"
                aria-label={`Remove ${jumper.id}`}
                onClick={() =>
                  void mutate({
                    type: "remove_jumper_inventory",
                    inventory_id: jumper.id,
                  })
                }
              >
                <Icon name="close" size={13} />
              </button>
            </li>
          ))}
        </ul>
      )}
      {adding.value && (
        <div class="rule-form">
          <div class="field-grid">
            <label class="field">
              <span>Span (pitches)</span>
              <input
                type="number"
                min={1}
                value={String(draft.value.span)}
                onInput={(event) =>
                  (draft.value = {
                    ...draft.value,
                    span: Number((event.target as HTMLInputElement).value),
                  })
                }
              />
            </label>
            <label class="field">
              <span>Colour</span>
              <select
                value={draft.value.color}
                onChange={(event) =>
                  (draft.value = {
                    ...draft.value,
                    color: (event.target as HTMLSelectElement).value,
                  })
                }
              >
                {WIRE_COLORS.map((color) => (
                  <option key={color}>{color}</option>
                ))}
              </select>
            </label>
            <label class="field">
              <span>Tolerance</span>
              <input
                type="number"
                min={0}
                value={String(draft.value.tolerance)}
                onInput={(event) =>
                  (draft.value = {
                    ...draft.value,
                    tolerance: Number((event.target as HTMLInputElement).value),
                  })
                }
              />
            </label>
            <label class="field">
              <span>Quantity</span>
              <input
                type="number"
                min={0}
                placeholder="∞"
                value={draft.value.quantity}
                onInput={(event) =>
                  (draft.value = {
                    ...draft.value,
                    quantity: (event.target as HTMLInputElement).value,
                  })
                }
              />
            </label>
          </div>
          <label class="field">
            <span>Name</span>
            <input
              placeholder={`U-${draft.value.span}`}
              value={draft.value.id}
              onInput={(event) =>
                (draft.value = {
                  ...draft.value,
                  id: (event.target as HTMLInputElement).value,
                })
              }
            />
          </label>
          <div class="row end">
            <button
              class="btn ghost small"
              onClick={() => (adding.value = false)}
            >
              Cancel
            </button>
            <button class="btn primary small" onClick={() => void save()}>
              Add jumper
            </button>
          </div>
        </div>
      )}
    </Section>
  );
}
