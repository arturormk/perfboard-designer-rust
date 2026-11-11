/**
 * Build drawing for the bench: both board faces plus a wire cut list.
 * Hidden on screen; `@media print` shows only this.
 */
import { BoardScene, defaultViewBox } from "../board/BoardScene";
import { bendCount, polylineLengthMm } from "../model/geometry";
import {
  describeWire,
  formatMm,
  groupPlacedParts,
  wireColorCss,
  wireColorName,
} from "../model/naming";
import { design, railLabelMap, view, wireLabelMap } from "../state/store";

export function PrintSheet() {
  const current = design.value;
  if (!current) return null;
  const box = defaultViewBox(current).join(" ");
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
  const parts = groupPlacedParts(current);
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
    </div>
  );
}
