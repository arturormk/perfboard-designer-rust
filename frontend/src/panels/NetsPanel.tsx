import { useSignal } from "@preact/signals";
import type { ElectricalNet } from "../api/project";
import {
  createNet,
  mutate,
  netInUse,
  proposeRoute,
  removeNet,
  select,
  focusOn,
} from "../state/actions";
import {
  activeNetId,
  airwires,
  design,
  hover,
  proposedNets,
  selection,
  tool,
  unroutedByNet,
} from "../state/store";
import { pinDisplay, wireColorCss, wireColorName } from "../model/naming";
import { Icon } from "../ui/Icon";

function NetStatus({ net }: { net: ElectricalNet }) {
  const pins = net.pins?.length ?? 0;
  const unrouted = unroutedByNet.value.get(net.id) ?? 0;
  if (pins < 2) return <span class="net-status muted">{pins} pin</span>;
  if (unrouted) return <span class="net-status warn">{unrouted} open</span>;
  return (
    <span class="net-status ok">
      <Icon name="check" size={13} /> routed
    </span>
  );
}

export function NetsPanel() {
  const name = useSignal("");
  const current = design.value;
  const nets = Object.values(current?.nets ?? {})
    .filter((net) => net.state === "accepted")
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  const proposals = proposedNets.value;
  const selectedNet =
    selection.value?.kind === "net" ? selection.value.id : undefined;

  async function add(event: Event) {
    event.preventDefault();
    const value = name.value.trim();
    if (!value) return;
    name.value = "";
    const id = await createNet(value);
    if (id) select({ kind: "net", id });
  }

  return (
    <div class="panel-body">
      {proposals.length > 0 && (
        <section class="panel-section proposals">
          <header class="section-head">
            <h3>
              <Icon name="agent" size={15} /> Needs your approval
            </h3>
            <span class="count">{proposals.length}</span>
          </header>
          <p class="section-note">
            Proposed connections change the circuit. They stay inactive until
            you accept them.
          </p>
          {proposals.map((net) => (
            <article class="proposal-card" key={net.id}>
              <header>
                <strong>{net.name}</strong>
                <span>{net.pins?.length ?? 0} pins</span>
              </header>
              {net.notes && <p class="proposal-notes">{net.notes}</p>}
              {current && (net.pins?.length ?? 0) > 0 && (
                <ul class="proposal-pins">
                  {net.pins!.map((pin) => {
                    const label = pinDisplay(current, pin);
                    return (
                      <li key={`${pin.component_id}:${pin.pin_id}`}>
                        {label.component} · <strong>{label.pin}</strong>{" "}
                        {label.pinName !== label.pin && label.pinName}
                      </li>
                    );
                  })}
                </ul>
              )}
              <div class="row end">
                <button
                  class="btn ghost small"
                  onClick={() =>
                    void mutate({ type: "reject_net_proposal", net_id: net.id })
                  }
                >
                  Reject
                </button>
                <button
                  class="btn primary small"
                  onClick={() =>
                    void mutate({ type: "accept_net_proposal", net_id: net.id })
                  }
                >
                  Accept
                </button>
              </div>
            </article>
          ))}
        </section>
      )}

      <section class="panel-section grow">
        <header class="section-head">
          <h3>Nets</h3>
          <span class="count">{nets.length}</span>
        </header>
        <form class="inline-add" onSubmit={add}>
          <input
            placeholder="New net name"
            value={name.value}
            onInput={(event) =>
              (name.value = (
                event.target as HTMLInputElement
              ).value.toUpperCase())
            }
            aria-label="New net name"
          />
          <button class="btn small" type="submit" disabled={!name.value.trim()}>
            <Icon name="plus" size={14} /> Add
          </button>
        </form>
        {!nets.length && (
          <p class="empty-note">
            Nets name what must be connected. Create one here, or start a wire
            from an unconnected pin with the wire tool.
          </p>
        )}
        <ul class="net-list">
          {nets.map((net) => {
            const colors = [
              ...new Set(
                Object.values(current?.wires ?? {})
                  .filter((wire) => wire.net_id === net.id)
                  .map((wire) => wireColorName(wire.color)),
              ),
            ];
            const firstAirwire = airwires.value.find(
              (airwire) => airwire.netId === net.id,
            );
            return (
              <li key={net.id}>
                <div
                  class={`net-row${selectedNet === net.id ? " on" : ""}${activeNetId.value === net.id ? " active" : ""}`}
                  onMouseEnter={() =>
                    (hover.value = { kind: "net", id: net.id })
                  }
                  onMouseLeave={() => (hover.value = null)}
                >
                  <button
                    class="net-main"
                    onClick={() => {
                      select({ kind: "net", id: net.id });
                      focusOn({ kind: "net", id: net.id });
                    }}
                  >
                    <span class="net-swatches">
                      {colors.length ? (
                        colors.slice(0, 3).map((color) => (
                          <i
                            key={color}
                            style={{
                              background: wireColorCss({ other: color }),
                            }}
                          />
                        ))
                      ) : (
                        <i class="empty" />
                      )}
                    </span>
                    <span class="net-name">{net.name}</span>
                    <span class="net-count">{net.pins?.length ?? 0}</span>
                    <NetStatus net={net} />
                  </button>
                  <div class="net-actions">
                    {firstAirwire && (
                      <button
                        class="icon-btn small"
                        title="Suggest a route for the next open connection"
                        aria-label={`Suggest route for ${net.name}`}
                        onClick={() =>
                          void proposeRoute(
                            net.id,
                            { kind: "pin", ...firstAirwire.from },
                            { kind: "pin", ...firstAirwire.to },
                          )
                        }
                      >
                        <Icon name="route" size={15} />
                      </button>
                    )}
                    <button
                      class="icon-btn small"
                      title="Draw a wire on this net (W)"
                      aria-label={`Draw a wire on ${net.name}`}
                      onClick={() => {
                        activeNetId.value = net.id;
                        tool.value = "wire";
                      }}
                    >
                      <Icon name="wire" size={15} />
                    </button>
                    {current && !netInUse(current, net.id) && (
                      <button
                        class="icon-btn small"
                        title="Remove this unused net"
                        aria-label={`Remove ${net.name}`}
                        onClick={() => void removeNet(net.id)}
                      >
                        <Icon name="trash" size={15} />
                      </button>
                    )}
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      </section>
    </div>
  );
}
