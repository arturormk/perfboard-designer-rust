/**
 * Assembly mode: build the board one step at a time (ADR 0012).
 *
 * Read-only. The plan is derived by Rust for the current revision and
 * re-derived whenever the design changes; only checkmarks are written, and
 * they live outside the project file.
 */
import { useSignal, useSignalEffect } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";
import type {
  AssemblyPlan,
  Side,
  StepKind,
  WireListEntry,
} from "../api/project";
import { BoardBase } from "../board/BoardBase";
import { BoardScene, defaultViewBox } from "../board/BoardScene";
import { wireColorCss, wireColorName } from "../model/naming";
import { confirmAction, openDialogKind } from "../shell/dialogs";
import {
  checkmarks,
  completeAndAdvance,
  currentStep,
  goToStep,
  loadAssembly,
  plan,
  planError,
  planUpdate,
  progress,
  resetProgress,
  setDone,
  currentProbe,
  selectedProbe,
  statuses,
  stepBy,
  toggleProbe,
  type StepStatus,
} from "../state/assembly";
import {
  design as designSignal,
  pinLabels,
  railLabelMap,
  saddleLabelMap,
  view,
  wireLabelMap,
} from "../state/store";
import { Icon, type IconName } from "../ui/Icon";
import { assemblyScene } from "./scene";

type ViewBox = [number, number, number, number];

const KIND_ICON: Record<StepKind, IconName> = {
  component: "chip",
  wire: "wire",
  rail: "rail",
  bridge: "bridge",
  saddle: "saddle",
  check: "check",
};

const SIDE_NAME: Record<Side, string> = {
  component: "Component side",
  solder: "Solder side",
};

function isTyping(target: EventTarget | null): boolean {
  return Boolean(
    (target as HTMLElement | null)?.closest?.(
      "input, textarea, select, [contenteditable]",
    ),
  );
}

export function AssemblyMode() {
  // Re-derive the plan whenever the design changes, including MCP edits.
  useSignalEffect(() => {
    void view.value?.revision;
    void loadAssembly();
  });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        openDialogKind.value ||
        isTyping(event.target) ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey
      )
        return;
      const step = currentStep.peek();
      switch (event.key) {
        case "ArrowLeft":
        case "ArrowUp":
        case "PageUp":
          event.preventDefault();
          stepBy(-1);
          return;
        case "ArrowRight":
        case "ArrowDown":
        case "PageDown":
          event.preventDefault();
          stepBy(1);
          return;
        case "Home":
          event.preventDefault();
          stepBy(-Infinity);
          return;
        case "End":
          event.preventDefault();
          stepBy(Infinity);
          return;
        case "Enter":
          event.preventDefault();
          void completeAndAdvance();
          return;
        case " ":
          if (!step) return;
          event.preventDefault();
          void setDone(step, statuses.peek().get(step.id) !== "done");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const current = plan.value;
  const target = designSignal.value;
  if (!current || !target)
    return (
      <main class="assembly assembly-empty">
        {planError.value ? (
          <p class="form-error">
            Could not derive the assembly plan: {planError.value}
          </p>
        ) : (
          <p class="field-note">Working out the build order…</p>
        )}
      </main>
    );
  if (!current.steps.length)
    return (
      <main class="assembly assembly-empty">
        <Icon name="board" size={28} />
        <h2>Nothing to build yet</h2>
        <p class="field-note">
          Place parts and route wires on the Board, and their assembly steps
          appear here.
        </p>
      </main>
    );
  return (
    <main class="assembly">
      <StepList plan={current} />
      <section class="canvas-area assembly-canvas" aria-label="Board">
        <AssemblyCanvas plan={current} />
      </section>
      <StepDetail plan={current} />
    </main>
  );
}

// ── Step list ─────────────────────────────────────────────────────────────

function StatusMark({ status }: { status: StepStatus | undefined }) {
  if (status === "done")
    return (
      <span class="step-mark done" title="Done">
        <Icon name="check" size={12} />
      </span>
    );
  if (status === "changed")
    return (
      <span class="step-mark changed" title="Changed since it was done">
        !
      </span>
    );
  return <span class="step-mark" aria-hidden="true" />;
}

function StepList({ plan: current }: { plan: AssemblyPlan }) {
  const shown = currentStep.value;
  const marks = statuses.value;
  const counts = progress.value;
  const listRef = useRef<HTMLOListElement>(null);
  const warningsOpen = useSignal(false);

  // Keep the step on screen visible in the list.
  useEffect(() => {
    listRef.current
      ?.querySelector(".step-row.on")
      ?.scrollIntoView({ block: "nearest" });
  }, [shown?.id]);

  return (
    <aside class="side-panel left assembly-steps" aria-label="Assembly steps">
      <header class="assembly-progress">
        <div class="progress-line">
          <strong>
            {counts.done} of {counts.total} done
          </strong>
          <button
            class="btn ghost small"
            disabled={counts.done + counts.changed === 0}
            onClick={async () => {
              if (
                await confirmAction(
                  "Start the build again?",
                  "Every checkmark for this project is cleared. The design is not changed.",
                  "Clear checkmarks",
                  true,
                )
              )
                void resetProgress();
            }}
          >
            Reset
          </button>
        </div>
        <div
          class="progress-bar"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={counts.total}
          aria-valuenow={counts.done}
        >
          <span style={{ width: `${(counts.done / counts.total) * 100}%` }} />
        </div>
        {counts.changed > 0 && (
          <p class="field-note warn">
            {counts.changed} step{counts.changed === 1 ? "" : "s"} changed after{" "}
            {counts.changed === 1 ? "it was" : "they were"} done, marked !
          </p>
        )}
      </header>
      {current.warnings.length > 0 && (
        <div class="assembly-warnings">
          <button
            class="warnings-toggle"
            aria-expanded={warningsOpen.value}
            onClick={() => (warningsOpen.value = !warningsOpen.value)}
          >
            <Icon name="warning" size={14} />
            {current.warnings.length} note
            {current.warnings.length === 1 ? "" : "s"} about the order
            <Icon
              name={warningsOpen.value ? "chevronUp" : "chevronDown"}
              size={14}
            />
          </button>
          {warningsOpen.value && (
            <ul>
              {current.warnings.map((warning, index) => {
                const step = current.steps.find((candidate) =>
                  candidate.objects.some((object) =>
                    warning.objects.some((other) => other.id === object.id),
                  ),
                );
                return (
                  <li key={index}>
                    <button
                      disabled={!step}
                      onClick={() => step && goToStep(step.id)}
                    >
                      {warning.message}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
      <ol class="step-rows" ref={listRef}>
        {current.steps.map((step) => (
          <li key={step.id}>
            <button
              class={`step-row${step.id === shown?.id ? " on" : ""} ${
                marks.get(step.id) ?? "todo"
              }`}
              aria-current={step.id === shown?.id ? "step" : undefined}
              onClick={() => goToStep(step.id)}
            >
              <span class="step-number">{step.number}</span>
              <Icon name={KIND_ICON[step.kind]} size={14} />
              <span class="step-title">{step.title}</span>
              {step.side === "solder" && (
                <span class="side-tag" title="Solder side">
                  S
                </span>
              )}
              <StatusMark status={marks.get(step.id)} />
            </button>
          </li>
        ))}
      </ol>
    </aside>
  );
}

// ── Step detail ───────────────────────────────────────────────────────────

function WireFacts({ entry }: { entry: WireListEntry }) {
  return (
    <dl class="wire-facts">
      <dt>Colour</dt>
      <dd>
        <i
          class="color-swatch"
          style={{ background: wireColorCss(entry.color) }}
        />
        {wireColorName(entry.color)}
      </dd>
      <dt>Net</dt>
      <dd>{entry.net}</dd>
      <dt>Cut</dt>
      <dd title="Path length plus bare leads; real bends are not sharp">
        ≈ {Math.round(entry.cut_mm)} mm
      </dd>
      <dt>Bends</dt>
      <dd>{entry.bend_count}</dd>
      <dt>From</dt>
      <dd>{entry.from}</dd>
      <dt>To</dt>
      <dd>{entry.to}</dd>
      {entry.jumper && (
        <>
          <dt>Jumper</dt>
          <dd>{entry.jumper}</dd>
        </>
      )}
    </dl>
  );
}

function PlanUpdateBanner() {
  const update = planUpdate.value;
  if (!update) return null;
  const parts = [
    update.added && `${update.added} new`,
    update.changed && `${update.changed} changed`,
    update.removed && `${update.removed} removed`,
  ].filter(Boolean);
  return (
    <div class="plan-banner" role="status">
      <Icon name="refresh" size={14} />
      <div>
        <strong>The design changed, so the plan was updated.</strong>
        <p>
          {parts.length
            ? `Steps: ${parts.join(", ")}.`
            : "The order or its notes changed."}
          {update.invalidated > 0 &&
            ` ${update.invalidated} step${
              update.invalidated === 1
                ? " you had done asks"
                : "s you had done ask"
            } for something different now.`}
        </p>
      </div>
      <button
        class="icon-btn small"
        aria-label="Dismiss"
        onClick={() => (planUpdate.value = null)}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  );
}

function StepDetail({ plan: current }: { plan: AssemblyPlan }) {
  const step = currentStep.value;
  const status = step ? statuses.value.get(step.id) : undefined;
  void checkmarks.value;
  if (!step) return <aside class="side-panel right" />;
  const byId = new Map(current.steps.map((item) => [item.id, item]));
  const entry =
    step.kind === "wire" || step.kind === "rail"
      ? current.wire_list.find((item) =>
          step.objects.some((object) => object.id === item.id),
        )
      : undefined;
  const last = step.number === current.steps.length;
  return (
    <aside class="side-panel right assembly-detail" aria-label="Current step">
      <PlanUpdateBanner />
      <div class="step-detail-body">
        <p class="step-kicker">
          Step {step.number} of {current.steps.length}
          <span class={`side-badge ${step.side}`}>{SIDE_NAME[step.side]}</span>
        </p>
        <h2>{step.title}</h2>
        {status === "changed" && (
          <p class="step-changed">
            This step changed after you marked it done. Check the board against
            it and mark it again.
          </p>
        )}
        <ol class="step-instructions">
          {step.instructions.map((line, index) => (
            <li key={index}>{line}</li>
          ))}
        </ol>
        {step.probes && step.probes.length > 0 && (
          <ul class="step-probes" aria-label="Measurements">
            {step.probes.map((probe, index) => {
              const on =
                selectedProbe.value?.stepId === step.id &&
                selectedProbe.value.index === index;
              return (
                <li key={index}>
                  <button
                    class={on ? "on" : ""}
                    aria-pressed={on}
                    title={
                      on
                        ? "Hide its test points"
                        : "Show its test points on the board"
                    }
                    onClick={() => toggleProbe(index)}
                  >
                    <Icon name="target" size={14} />
                    <span>{probe.text}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        {entry && <WireFacts entry={entry} />}
        {step.notes && step.notes.length > 0 && (
          <ul class="step-notes">
            {step.notes.map((note, index) => (
              <li key={index}>
                <Icon name="info" size={14} />
                <span>{note}</span>
              </li>
            ))}
          </ul>
        )}
        {step.after && step.after.length > 0 && (
          <section class="step-after">
            <h3>Comes after</h3>
            <ul>
              {step.after.map((dependency) => {
                const before = byId.get(dependency.step_id);
                const done = statuses.value.get(dependency.step_id) === "done";
                return (
                  <li key={dependency.step_id}>
                    <button
                      class={done ? "done" : ""}
                      disabled={!before}
                      onClick={() => before && goToStep(before.id)}
                    >
                      <span class="step-number">{before?.number ?? "?"}</span>
                      <span>
                        <strong>{before?.label ?? dependency.step_id}</strong>
                        {" — "}
                        {dependency.reason}
                      </span>
                      {done && <Icon name="check" size={12} />}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        )}
      </div>
      <footer class="step-controls">
        <button
          class="btn"
          disabled={step.number === 1}
          onClick={() => stepBy(-1)}
          title="Previous step (←)"
        >
          <Icon name="chevron" size={14} class="flip-x" /> Back
        </button>
        {status === "done" ? (
          <button
            class="btn ghost"
            onClick={() => void setDone(step, false)}
            title="Untick this step (Space)"
          >
            Not done
          </button>
        ) : (
          <button
            class="btn primary"
            onClick={() => void completeAndAdvance()}
            title="Mark done and go to the next step (Enter)"
          >
            <Icon name="check" size={14} />
            {status === "changed" ? "Done again" : last ? "Done" : "Done, next"}
          </button>
        )}
        <button
          class="btn"
          disabled={last}
          onClick={() => stepBy(1)}
          title="Next step (→)"
        >
          Next <Icon name="chevron" size={14} />
        </button>
      </footer>
    </aside>
  );
}

// ── Board ─────────────────────────────────────────────────────────────────

function boardBox(holesX: number, holesY: number): ViewBox {
  // Room for the floating side switch above the board.
  return [-2.2, -4.4, holesX - 1 + 4.4, holesY - 1 + 6.6];
}

function AssemblyCanvas({ plan: current }: { plan: AssemblyPlan }) {
  const target = designSignal.value!;
  const step = currentStep.value;
  const svgRef = useRef<SVGSVGElement>(null);
  const viewBox = useSignal<ViewBox | null>(null);
  const shownSide = useSignal<Side>(step?.side ?? "component");
  const pan = useRef<{ x: number; y: number; box: ViewBox } | null>(null);

  // Show each step from the face it is done on.
  useEffect(() => {
    if (step) shownSide.value = step.side;
  }, [step?.id]);

  const { holes_x: holesX, holes_y: holesY } = target.board;
  useEffect(() => {
    viewBox.value = boardBox(holesX, holesY);
  }, [holesX, holesY]);
  if (!viewBox.peek()) viewBox.value = boardBox(holesX, holesY);

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = viewBox.peek();
      const matrix = svg.getScreenCTM();
      if (!box || !matrix) return;
      const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
        matrix.inverse(),
      );
      const factor = Math.exp(event.deltaY * (event.ctrlKey ? 0.01 : 0.0015));
      const width = Math.min(200, Math.max(4, box[2] * factor));
      const applied = width / box[2];
      viewBox.value = [
        point.x - (point.x - box[0]) * applied,
        point.y - (point.y - box[1]) * applied,
        box[2] * applied,
        box[3] * applied,
      ];
    };
    svg.addEventListener("wheel", onWheel, { passive: false });
    return () => svg.removeEventListener("wheel", onWheel);
  }, []);

  const probe = currentProbe.value;
  const scene = assemblyScene(target, current, step, probe);
  const side = shownSide.value;
  const box = viewBox.value ?? defaultViewBox(target);
  const markers = (probe?.holes ?? step?.holes)?.map((hole, index) => (
    <g key={index} class="step-hole">
      <circle class="casing" cx={hole.x2 / 2} cy={hole.y2 / 2} r={0.42} />
      <circle class="ring" cx={hole.x2 / 2} cy={hole.y2 / 2} r={0.42} />
    </g>
  ));

  return (
    <div class="canvas-host">
      <div class="work-pane">
        <BoardBase board={target.board} side={side} viewBox={viewBox} />
        <svg
          ref={svgRef}
          class={`board-svg readonly${pan.current ? " cursor-grabbing" : ""}`}
          viewBox={box.join(" ")}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={`${step?.title ?? "Board"}, ${SIDE_NAME[side].toLowerCase()}`}
          onPointerDown={(event) => {
            if (event.button !== 0 && event.button !== 1) return;
            (event.currentTarget as SVGSVGElement).setPointerCapture(
              event.pointerId,
            );
            pan.current = { x: event.clientX, y: event.clientY, box };
          }}
          onPointerMove={(event) => {
            const start = pan.current;
            const matrix = svgRef.current?.getScreenCTM();
            if (!start || !matrix) return;
            viewBox.value = [
              start.box[0] - (event.clientX - start.x) / matrix.a,
              start.box[1] - (event.clientY - start.y) / matrix.d,
              start.box[2],
              start.box[3],
            ];
          }}
          onPointerUp={() => (pan.current = null)}
          onPointerCancel={() => (pan.current = null)}
        >
          <BoardScene
            design={scene.design}
            side={side}
            mode="readonly"
            idPrefix="asm"
            baseLayer={false}
            dimIds={scene.dimIds}
            focusIds={scene.focusIds}
            pinLabels={pinLabels.value}
            wireLabels={wireLabelMap.value}
            railLabels={railLabelMap.value}
            saddleLabels={saddleLabelMap.value}
            overlay={<g class="step-holes">{markers}</g>}
          />
        </svg>
        <div class="hud hud-top-center">
          <div
            class={`side-switch side-${side}`}
            role="radiogroup"
            aria-label="Board face"
          >
            {(["component", "solder"] as const).map((face) => (
              <button
                key={face}
                role="radio"
                aria-checked={side === face}
                class={side === face ? "on" : ""}
                onClick={() => (shownSide.value = face)}
                title={
                  face === "solder"
                    ? "Solder side, seen from below (mirrored)"
                    : "Component side, seen from above"
                }
              >
                {SIDE_NAME[face]}
              </button>
            ))}
            {side === "solder" && <span class="mirror-tag">mirrored</span>}
          </div>
        </div>
        <div class="hud hud-top-right">
          <div class="zoom-group">
            <button
              class="icon-btn"
              title="Show the whole board"
              aria-label="Show the whole board"
              onClick={() => (viewBox.value = boardBox(holesX, holesY))}
            >
              <Icon name="fit" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
