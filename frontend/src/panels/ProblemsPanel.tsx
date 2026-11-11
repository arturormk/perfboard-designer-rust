import type { Diagnostic } from "../api/project";
import { focusOn, select } from "../state/actions";
import { diagnostics, leftTab, problemCounts } from "../state/store";
import { Icon } from "../ui/Icon";

/** Plain-language next steps for known diagnostic codes. */
const ADVICE: Record<string, string> = {
  "circuit.net_incomplete":
    "Some pins on this net are not joined yet. Follow the dashed hints on the board or ask for a suggested route.",
  "circuit.net_too_small": "A net needs at least two pins to mean anything.",
  "circuit.proposed_net": "Review it in Nets and accept or reject it.",
  "placement.out_of_bounds":
    "Move the part fully onto the board, or enlarge the board.",
  "placement.occupied_hole": "Two parts want the same hole. Move one of them.",
  "placement.keepout_overlap":
    "A part sits inside another part's keepout area.",
  "placement.keepout_occupied": "A hole inside a keepout area is in use.",
  "placement.forbidden_area":
    "The part covers an area marked as forbidden on the board.",
  "placement.mounting_hole_occupied": "The part covers a mounting hole.",
  "placement.required_access_blocked":
    "Something blocks the side of a connector that must stay reachable.",
  "placement.required_access_out_of_bounds":
    "A required access area falls off the board; rotate or move the part.",
  "route.endpoint_moved":
    "A part moved after this wire was drawn. Drag the wire end back onto a pin or ask for a reroute.",
  "route.component_clearance":
    "This wire runs under a part where wires should not go.",
  "route.short_segment":
    "A very short segment is hard to form by hand. Consider removing a bend.",
  "component.definition_missing": "The project lacks this part's definition.",
  "component.rotation_not_allowed": "This part does not allow that rotation.",
};

const TONE = { error: "error", warning: "warning", advisory: "info" } as const;

function locate(item: Diagnostic): void {
  const target = item.objects?.[0];
  if (!target) return;
  const kind =
    target.kind === "component" ||
    target.kind === "wire" ||
    target.kind === "rail" ||
    target.kind === "net"
      ? target.kind
      : undefined;
  if (!kind) return;
  if (kind === "net") leftTab.value = "nets";
  select({ kind, id: target.id });
  focusOn({ kind, id: target.id });
}

export function ProblemsPanel() {
  const items = diagnostics.value;
  const counts = problemCounts.value;
  const order = { error: 0, warning: 1, advisory: 2 };
  const sorted = [...items].sort(
    (a, b) => order[a.severity] - order[b.severity],
  );
  return (
    <div class="panel-body">
      <section class="panel-section">
        <div class="problem-summary">
          <span class={counts.error ? "sev error" : "sev"}>
            <Icon name="error" size={15} /> {counts.error} errors
          </span>
          <span class={counts.warning ? "sev warning" : "sev"}>
            <Icon name="warning" size={15} /> {counts.warning} warnings
          </span>
          <span class="sev">
            <Icon name="info" size={15} /> {counts.advisory} notes
          </span>
        </div>
        {!items.length && (
          <div class="all-clear">
            <Icon name="check" size={22} />
            <strong>No problems found</strong>
            <p>Placement and routing are consistent.</p>
          </div>
        )}
        <ul class="problem-list">
          {sorted.map((item, index) => (
            <li key={`${item.code}:${index}`}>
              <button
                class={`problem ${TONE[item.severity]}`}
                onClick={() => locate(item)}
                disabled={!item.objects?.length}
              >
                <Icon
                  name={
                    item.severity === "error"
                      ? "error"
                      : item.severity === "warning"
                        ? "warning"
                        : "info"
                  }
                  size={16}
                />
                <span class="problem-text">
                  <strong>{item.message.replaceAll("`", "")}</strong>
                  {ADVICE[item.code] && <small>{ADVICE[item.code]}</small>}
                  <code>{item.code}</code>
                </span>
                {item.objects?.length ? (
                  <Icon name="target" size={15} class="problem-go" />
                ) : null}
              </button>
            </li>
          ))}
        </ul>
        <p class="disclaimer">
          These checks cover geometry and connectivity. Passing them does not
          prove the circuit is electrically correct or safe.
        </p>
      </section>
    </div>
  );
}
