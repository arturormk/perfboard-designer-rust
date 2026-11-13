/**
 * Browser preview stand-in for the Rust assembly planner — development only.
 *
 * It produces a plan of the right shape for layout work: one step per object
 * in a plausible order, with holes, instructions and a wire list. It knows no
 * dependencies or crossings; the real rules live in
 * `crates/project/src/assembly.rs` (ADR 0012).
 */
import type {
  AssemblyPlan,
  AssemblyStep,
  Side,
  StepKind,
  WireListEntry,
} from "../api/project";
import { pinBoardPosition, type Design } from "../model/geometry";
import {
  railLabels,
  saddleLabels,
  wireColorName,
  wireLabels,
} from "../model/naming";

function hash(text: string): string {
  let value = 2166136261;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 16777619);
  }
  return (value >>> 0).toString(16).padStart(8, "0");
}

type Draft = Omit<AssemblyStep, "number" | "fingerprint"> & { order: number };

function pathMm(vertices: Array<{ x2: number; y2: number }>, pitch: number) {
  let total = 0;
  vertices.slice(1).forEach((point, index) => {
    const previous = vertices[index]!;
    total += Math.hypot(point.x2 - previous.x2, point.y2 - previous.y2);
  });
  return (total / 2) * pitch;
}

const hole = (point: { x2: number; y2: number }) =>
  `(${point.x2 / 2}, ${point.y2 / 2})`;

export function mockAssemblyPlan(
  design: Design,
  revision: number,
): AssemblyPlan {
  const pitch = design.board.pitch_mm;
  const drafts: Draft[] = [];
  const wireList: Array<Omit<WireListEntry, "step">> = [];
  const nets = design.nets ?? {};
  const wireNames = wireLabels(design);
  const railNames = railLabels(design);
  const saddleNames = saddleLabels(design);
  const sideOrder = (side: Side, component: number, solder: number) =>
    side === "component" ? component : solder;

  for (const instance of Object.values(design.components ?? {})) {
    const definition = design.component_definitions?.[instance.definition_id];
    if (!definition) continue;
    const bridge = definition.kind === "bridge";
    const label = instance.name || definition.name;
    const holes = definition.pins.map((pin) => {
      const point = pinBoardPosition(instance, pin);
      return { x2: point.x * 2, y2: point.y * 2 };
    });
    const kind: StepKind = bridge ? "bridge" : "component";
    drafts.push({
      order: bridge ? 50 : 10 + Math.min(definition.pins.length, 30),
      id: `${kind}:${instance.id}`,
      kind,
      label,
      title: bridge ? `Solder bridge ${label}` : `Fit ${label}`,
      side: bridge ? "solder" : "component",
      objects: [{ kind: "component", id: instance.id }],
      holes,
      instructions: bridge
        ? [`Run solder across ${holes.map(hole).join(" and ")}.`]
        : [
            `Insert ${label} (${definition.name}) with pin 1 at ${holes[0] ? hole(holes[0]) : "its place"}.`,
            "Turn the board over and solder every pin.",
          ],
    });
  }

  for (const wire of Object.values(design.wires ?? {})) {
    const label = wireNames.get(wire.id) ?? "W";
    const first = wire.vertices[0]!;
    const last = wire.vertices.at(-1)!;
    const path = pathMm(wire.vertices, pitch);
    const cut = Math.round(path + 10);
    drafts.push({
      order: sideOrder(wire.side, 1, 40),
      id: `wire:${wire.id}`,
      kind: "wire",
      label,
      title: `Lay wire ${label}`,
      side: wire.side,
      objects: [{ kind: "wire", id: wire.id }],
      holes: [first, last],
      instructions: [
        `Cut ${wireColorName(wire.color).toLowerCase()} wire ≈ ${cut} mm and strip 5 mm at each end.`,
        `Lay it from ${hole(first)} to ${hole(last)} on the ${wire.side} side.`,
      ],
    });
    wireList.push({
      kind: "wire",
      id: wire.id,
      label,
      from: hole(first),
      to: hole(last),
      net: nets[wire.net_id]?.name ?? "",
      color: wire.color,
      side: wire.side,
      path_mm: path,
      cut_mm: cut,
      bend_count: Math.max(0, wire.vertices.length - 2),
    });
  }

  for (const rail of Object.values(design.rails ?? {})) {
    const label = railNames.get(rail.id) ?? "R";
    const path = pathMm(rail.vertices, pitch);
    drafts.push({
      order: sideOrder(rail.side, 0, 30),
      id: `rail:${rail.id}`,
      kind: "rail",
      label,
      title: `Fit rail ${label}`,
      side: rail.side,
      objects: [{ kind: "rail", id: rail.id }],
      holes: rail.vertices,
      instructions: [
        `Cut, bend and slice rail ${label} using its wire template.`,
      ],
      notes: ["Browser preview: a stand-in plan without ordering rules."],
    });
    wireList.push({
      kind: "rail",
      id: rail.id,
      label,
      from: hole(rail.vertices[0]!),
      to: hole(rail.vertices.at(-1)!),
      net: nets[rail.net_id]?.name ?? "",
      color: rail.color,
      side: rail.side,
      path_mm: path,
      cut_mm: Math.round(path + 10),
      bend_count: Math.max(0, rail.vertices.length - 2),
    });
  }

  for (const saddle of Object.values(design.saddles ?? {})) {
    const label = saddleNames.get(saddle.id) ?? "S";
    const holes = [saddle.start, saddle.end].map((point) => ({
      x2: point.x * 2,
      y2: point.y * 2,
    }));
    drafts.push({
      order: sideOrder(saddle.side, 2, 60),
      id: `saddle:${saddle.id}`,
      kind: "saddle",
      label,
      title: `Fit saddle ${label}`,
      side: saddle.side,
      objects: [{ kind: "saddle", id: saddle.id }],
      holes,
      instructions: [
        `Push the strap through ${holes.map(hole).join(" and ")} over the wires.`,
      ],
    });
  }

  // Bridges join leads, so they wait for the parts with pins in their holes.
  const key = (point: { x2: number; y2: number }) => `${point.x2},${point.y2}`;
  for (const bridge of drafts.filter((draft) => draft.kind === "bridge")) {
    const holes = new Set(bridge.holes.map(key));
    bridge.after = drafts
      .filter(
        (draft) =>
          draft.kind === "component" &&
          draft.holes.some((point) => holes.has(key(point))),
      )
      .map((draft) => ({
        step_id: draft.id,
        reason: `${draft.label} has a pin in its holes`,
      }));
  }
  drafts.sort((a, b) => a.order - b.order || a.label.localeCompare(b.label));
  drafts.push({
    order: 99,
    id: "check:inspect",
    kind: "check",
    label: "Inspect",
    title: "Inspect the solder side",
    side: "solder",
    objects: [],
    holes: [],
    instructions: [
      "Look for solder bridges between neighbouring pads that the design does not have.",
    ],
  });

  const probes = Object.values(nets)
    .map((net) => {
      const points = (net.pins ?? []).flatMap((ref) => {
        const instance = design.components?.[ref.component_id];
        const definition =
          instance && design.component_definitions?.[instance.definition_id];
        const pin = definition?.pins.find((item) => item.id === ref.pin_id);
        if (!instance || !pin || definition?.kind === "bridge") return [];
        const point = pinBoardPosition(instance, pin);
        return [
          {
            name: `${instance.name} · ${pin.number}`,
            hole: { x2: point.x * 2, y2: point.y * 2 },
          },
        ];
      });
      return {
        text: `${net.name}: ${points.map((point) => point.name).join(", ")}.`,
        nets: [net.id],
        holes: points.map((point) => point.hole),
      };
    })
    .filter((probe) => probe.holes.length >= 2);
  if (probes.length)
    drafts.push({
      order: 100,
      id: "check:continuity",
      kind: "check",
      label: "Continuity",
      title: "Check continuity",
      side: "solder",
      objects: [],
      holes: [],
      instructions: [
        "With a multimeter on continuity, check that each net below connects all its pins.",
      ],
      probes,
    });

  const steps: AssemblyStep[] = drafts.map(
    ({ order: _order, ...draft }, index) => ({
      ...draft,
      number: index + 1,
      fingerprint: hash(JSON.stringify({ ...draft, after: undefined })),
    }),
  );
  const stepOf = new Map(
    steps.flatMap((step) =>
      step.objects.map((object) => [object.id, step.number]),
    ),
  );
  const wire_list = wireList
    .map((entry) => ({ ...entry, step: stepOf.get(entry.id) ?? 0 }))
    .sort((a, b) => a.step - b.step);
  return {
    source_revision: revision,
    fingerprint: hash(JSON.stringify(steps) + JSON.stringify(wire_list)),
    steps,
    wire_list,
    warnings: [],
  };
}
