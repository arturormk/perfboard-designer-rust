/**
 * Build drawing for the bench: both board faces, the wire list in build
 * order, the parts, and the build-order checklist from the assembly plan.
 * Hidden on screen; `@media print` shows only this.
 */
import type { AssemblyPlan, Checkmarks } from "../api/project";
import { BoardScene, defaultViewBox } from "../board/BoardScene";
import { bendCount, polylineLengthMm, type Design } from "../model/geometry";
import {
  describeWire,
  formatMm,
  groupPlacedParts,
  wireColorCss,
  wireColorName,
} from "../model/naming";
import { design, railLabelMap, view, wireLabelMap } from "../state/store";
import { checklistRows, printPlan, wireListInBuildOrder } from "./checklist";

const SIDE_SHORT = { component: "Component", solder: "Solder" } as const;

function Swatch({ color }: { color: Parameters<typeof wireColorCss>[0] }) {
  return (
    <>
      <i class="swatch" style={{ background: wireColorCss(color) }} />{" "}
      {wireColorName(color)}
    </>
  );
}

/** Wires and rails in build order, with estimated cut lengths. */
function PlanWireList({ plan }: { plan: AssemblyPlan }) {
  const entries = wireListInBuildOrder(plan);
  const rails = entries.filter((entry) => entry.kind === "rail").length;
  return (
    <section>
      <h2>
        Wires · {entries.length - rails}
        {rails > 0 && ` · rails · ${rails}`}
      </h2>
      <table class="print-steps">
        <colgroup>
          <col class="col-tick" />
          <col class="col-number" />
          <col />
          <col class="col-side" />
          <col class="col-length" />
          <col class="col-length" />
          <col class="col-bends" />
        </colgroup>
        <thead>
          <tr>
            <th class="tick-head">✓</th>
            <th class="num">Step</th>
            <th>Wire</th>
            <th>Side</th>
            <th class="num">Cut ≈</th>
            <th class="num">Path</th>
            <th class="num">Bends</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((entry) => (
            <tr key={entry.id}>
              <td class="tick" />
              <td class="num mono">{entry.step}</td>
              <td>
                <strong>
                  {entry.label}
                  {entry.net && ` · ${entry.net}`}
                </strong>
                <div class="print-step-text">
                  <Swatch color={entry.color} /> · {entry.from} → {entry.to}
                  {entry.jumper && ` · jumper: ${entry.jumper}`}
                </div>
              </td>
              <td>{SIDE_SHORT[entry.side]}</td>
              <td class="num">{Math.round(entry.cut_mm)} mm</td>
              <td class="num">{formatMm(entry.path_mm)}</td>
              <td class="num">{entry.bend_count}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p class="print-note">
        Cut lengths are estimates: the routed path plus a 5 mm bare lead at each
        end that goes into a hole (1.5 mm tips for rail ends between holes).
        Rounded bends use a little more, so cut generously. Rails follow their
        wire template for bends and insulation windows.
      </p>
    </section>
  );
}

/** Every step in build order with a box to tick. */
function BuildChecklist({
  plan,
  marks,
}: {
  plan: AssemblyPlan;
  marks: Checkmarks;
}) {
  const rows = checklistRows(plan, marks);
  const done = rows.filter((row) => row.done).length;
  return (
    <section class="print-checklist">
      <h2>
        Build order · {rows.length} steps
        {done > 0 && (
          <span class="print-note"> · {done} already ticked in the app</span>
        )}
      </h2>
      <table class="print-steps">
        <colgroup>
          <col class="col-tick" />
          <col class="col-number" />
          <col />
          <col class="col-side" />
          <col class="col-after" />
        </colgroup>
        <thead>
          <tr>
            <th class="tick-head">✓</th>
            <th class="num">#</th>
            <th>Step</th>
            <th>Side</th>
            <th>After</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ step, done: ticked, after }) => (
            <tr key={step.id} class={step.kind === "check" ? "check-step" : ""}>
              <td class={`tick${ticked ? " ticked" : ""}`}>
                {ticked ? "✓" : ""}
              </td>
              <td class="num mono">{step.number}</td>
              <td>
                <strong>{step.title}</strong>
                <div class="print-step-text">{step.instructions.join(" ")}</div>
                {step.probes && step.probes.length > 0 && (
                  <ul class="print-probes">
                    {step.probes.map((probe, index) => (
                      <li key={index}>
                        <i class="tick-box" /> {probe.text}
                      </li>
                    ))}
                  </ul>
                )}
                {step.notes?.map((note, index) => (
                  <div key={index} class="print-note">
                    Note: {note}
                  </div>
                ))}
              </td>
              <td>{SIDE_SHORT[step.side]}</td>
              <td class="mono">{after.join(", ")}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {plan.warnings.length > 0 && (
        <>
          <h3>Notes about the order</h3>
          <ul class="print-warnings">
            {plan.warnings.map((warning, index) => (
              <li key={index}>{warning.message}</li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

export function PrintSheet() {
  const current = design.value;
  if (!current) return null;
  const box = defaultViewBox(current).join(" ");
  const pitch = current.board.pitch_mm;
  const labels = wireLabelMap.value;
  const parts = groupPlacedParts(current);
  const fetched = printPlan.value;
  // A plan from an older revision would describe a different board.
  const fresh =
    fetched && fetched.plan.source_revision === view.value?.revision
      ? fetched
      : null;
  return (
    <div class="print-sheet" aria-hidden="true">
      <header>
        <h1>{current.board.name || view.value?.name}</h1>
        <p>
          {current.board.holes_x} × {current.board.holes_y} holes · {pitch} mm
          pitch · revision {view.value?.revision}
          {" · "}
          {new Date().toLocaleDateString()}
        </p>
      </header>
      <div class="print-faces">
        {(["component", "solder"] as const).map((face) => (
          <figure key={face}>
            <svg viewBox={box} class="board-svg print">
              <BoardScene
                design={current}
                side={face}
                mode="print"
                idPrefix={`print-${face}`}
                pinLabels="numbers"
                wireLabels={labels}
                railLabels={railLabelMap.value}
              />
            </svg>
            <figcaption>
              {face === "component"
                ? "Component side"
                : "Solder side (mirrored, as seen when flipped)"}
            </figcaption>
          </figure>
        ))}
      </div>
      {fresh ? (
        <PlanWireList plan={fresh.plan} />
      ) : (
        <FallbackWireTable design={current} />
      )}
      <section>
        <h2>Parts</h2>
        <ul class="print-parts">
          {parts.map((group) => (
            <li key={group.definitionId}>
              <strong>{group.instances.length} ×</strong>{" "}
              {group.definition?.name ?? group.definitionId}
              {!group.bridge && (
                <span>
                  {" "}
                  — {group.instances.map((item) => item.name).join(", ")}
                </span>
              )}
            </li>
          ))}
        </ul>
      </section>
      {fresh && <BuildChecklist plan={fresh.plan} marks={fresh.marks} />}
    </div>
  );
}

/** Without a current plan: wires by label with their routed length. */
function FallbackWireTable({ design: current }: { design: Design }) {
  const pitch = current.board.pitch_mm;
  const labels = wireLabelMap.value;
  const nets = current.nets ?? {};
  const wires = Object.values(current.wires ?? {}).sort((a, b) =>
    (labels.get(a.id) ?? a.id).localeCompare(
      labels.get(b.id) ?? b.id,
      undefined,
      { numeric: true },
    ),
  );
  return (
    <section>
      <h2>Wires · {wires.length}</h2>
      <table>
        <thead>
          <tr>
            <th>Wire</th>
            <th>Colour</th>
            <th>Net</th>
            <th>Side</th>
            <th>From → to</th>
            <th>Length</th>
            <th>Bends</th>
            <th>✓</th>
          </tr>
        </thead>
        <tbody>
          {wires.map((wire) => (
            <tr key={wire.id}>
              <td class="mono">{labels.get(wire.id) ?? wire.id}</td>
              <td>
                <i
                  class="swatch"
                  style={{ background: wireColorCss(wire.color) }}
                />{" "}
                {wireColorName(wire.color)}
              </td>
              <td>{nets[wire.net_id]?.name ?? wire.net_id}</td>
              <td>{wire.side}</td>
              <td>{describeWire(current, wire)}</td>
              <td>{formatMm(polylineLengthMm(wire.vertices, pitch))}</td>
              <td>{bendCount(wire.vertices)}</td>
              <td class="tick" />
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
