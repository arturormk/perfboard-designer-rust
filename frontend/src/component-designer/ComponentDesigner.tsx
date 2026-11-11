/**
 * Component Designer: graphical editing of a part definition on disk.
 *
 * Rust owns the draft (validation, undo, atomic save, file watching). This view
 * renders the draft with the existing preview renderer and turns pointer and
 * form input into draft operations.
 */
import { signal, useSignalEffect } from "@preact/signals";
import { useEffect, useRef, useState } from "preact/hooks";
import { listen } from "@tauri-apps/api/event";
import {
  applyComponentDraftOperation,
  closeRuntimeComponent,
  getComponentDraft,
  newComponentDraft,
  redoComponentDraft,
  reloadRuntimeComponent,
  saveComponentDraft,
  selectRuntimeComponent,
  undoComponentDraft,
  type AccessRegion,
  type ComponentDefinition,
  type ComponentDraftOperation,
  type ComponentDraftSnapshot,
  type ComponentViewerSnapshot,
  type LocalRect,
  type PinDefinition,
  type Rotation,
} from "../api/project";
import type { ResizeHandle } from "../editor/board-size";
import { errorMessage } from "../error-message";
import { refreshLibrary } from "../state/actions";
import { toast } from "../state/toasts";
import { Icon } from "../ui/Icon";
import { movePinOperation, splitList } from "./editor";
import { planBodyResize, planHoleRegionResize } from "./geometry-resize";
import { renderSanitizedMarkdown } from "./markdown";
import { renderComponentPreviewSvg, type PinLabelMode } from "./preview";
import {
  displayedLoad,
  isActiveViewerSnapshot,
  transitionViewerState,
  type ComponentViewerState,
} from "./viewer-state";

type EditLayer = "pins" | "body" | "keepout" | "access" | "occupied";

const ROTATIONS: Rotation[] = ["deg0", "deg90", "deg180", "deg270"];
const ROTATION_LABEL: Record<Rotation, string> = {
  deg0: "0°",
  deg90: "90°",
  deg180: "180°",
  deg270: "270°",
};

// ── State ──────────────────────────────────────────────────────────────────
const viewerState = signal<ComponentViewerState>({ kind: "no_component" });
const generation = signal<number | undefined>(undefined);
const watcher = signal<{ status: string; error?: string }>({
  status: "stopped",
});
const draft = signal<ComponentDraftSnapshot | undefined>(undefined);
const previewRotation = signal<Rotation>("deg0");
const labelMode = signal<PinLabelMode>("name");
const layer = signal<EditLayer>("pins");
const selectedPinId = signal<string | undefined>(undefined);
const showRegions = signal(true);
/** Live geometry while a pointer drag is in flight. */
const dragDefinition = signal<ComponentDefinition | undefined>(undefined);
let listening = false;

function applySnapshot(snapshot: ComponentViewerSnapshot): void {
  generation.value = snapshot.generation;
  watcher.value = {
    status: snapshot.watcher_status,
    error: snapshot.watcher_error,
  };
  viewerState.value = transitionViewerState(viewerState.value, snapshot);
}

async function ensureListening(): Promise<void> {
  if (listening) return;
  listening = true;
  await listen<ComponentViewerSnapshot>(
    "component-viewer-changed",
    async (event) => {
      const snapshot = event.payload;
      if (!isActiveViewerSnapshot(generation.value, snapshot)) return;
      applySnapshot(snapshot);
      if (snapshot.load?.definition && !draft.value?.dirty) {
        await reloadRuntimeComponent();
        draft.value = (await getComponentDraft()) ?? undefined;
        toast("info", `Reloaded ${snapshot.load.definition.name} from disk`);
      } else if (draft.value?.dirty) {
        toast("error", "The part changed on disk", {
          detail:
            "This draft has unsaved changes. Saving will need conflict resolution.",
        });
      }
    },
  );
}

async function openWith(
  request: () => Promise<ComponentViewerSnapshot | null>,
  verb: string,
): Promise<void> {
  const previous = viewerState.value;
  viewerState.value = { kind: "loading" };
  try {
    const snapshot = await request();
    if (!snapshot?.load) {
      viewerState.value = previous;
      return;
    }
    applySnapshot(snapshot);
    draft.value = (await getComponentDraft()) ?? undefined;
    previewRotation.value =
      snapshot.load.definition?.allowed_rotations[0] ?? "deg0";
    selectedPinId.value = undefined;
    if (snapshot.watcher_status === "error")
      toast("error", `${verb}, but live reload is unavailable`, {
        detail: snapshot.watcher_error,
      });
  } catch (error) {
    viewerState.value = previous;
    toast("error", `Could not ${verb.toLowerCase()}`, {
      detail: errorMessage(error),
    });
  }
}

const openPart = () => openWith(selectRuntimeComponent, "Opened part");
const newPart = () => openWith(newComponentDraft, "Created part");
const reloadPart = () => openWith(reloadRuntimeComponent, "Reloaded part");

async function closePart(): Promise<void> {
  try {
    await closeRuntimeComponent();
    viewerState.value = { kind: "no_component" };
    draft.value = undefined;
    generation.value = undefined;
    watcher.value = { status: "stopped" };
    selectedPinId.value = undefined;
  } catch (error) {
    toast("error", "Could not close the part", { detail: errorMessage(error) });
  }
}

async function edit(operation: ComponentDraftOperation): Promise<boolean> {
  try {
    draft.value = await applyComponentDraftOperation(operation);
    return true;
  } catch (error) {
    toast("error", "That change was not applied", {
      detail: errorMessage(error),
    });
    return false;
  }
}

async function draftHistory(redo: boolean): Promise<void> {
  try {
    draft.value = await (redo ? redoComponentDraft() : undoComponentDraft());
  } catch (error) {
    toast("error", redo ? "Nothing to redo" : "Nothing to undo", {
      detail: errorMessage(error),
    });
  }
}

async function savePart(): Promise<void> {
  try {
    const result = await saveComponentDraft();
    draft.value = result.draft;
    viewerState.value = { kind: "current", load: result.load };
    toast("success", `Saved ${result.draft.definition.name}`, {
      detail: "Written atomically; the previous files were kept as .bak.",
    });
    await refreshLibrary();
  } catch (error) {
    toast("error", "Could not save the part", { detail: errorMessage(error) });
  }
}

function nextPinId(pins: PinDefinition[]): { id: string; number: string } {
  let index = pins.length + 1;
  while (
    pins.some((pin) => pin.id === `p${index}` || pin.number === String(index))
  )
    index += 1;
  return { id: `p${index}`, number: String(index) };
}

// ── Preview with direct manipulation ───────────────────────────────────────
type Gesture =
  | { kind: "pin"; pin: PinDefinition; moved: boolean }
  | {
      kind: "resize";
      target: "body" | "keepout" | "access";
      index: number;
      handle: ResizeHandle;
    };

function DesignerPreview({
  definition,
  editable,
}: {
  definition: ComponentDefinition;
  editable: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const gesture = useRef<Gesture | null>(null);
  const shown = dragDefinition.value ?? definition;
  const active = editable ? layer.value : "pins";
  const resizeGeometry =
    active === "body" || active === "keepout" || active === "access"
      ? active
      : undefined;
  const markup = renderComponentPreviewSvg(shown, {
    rotation: previewRotation.value,
    labelMode: labelMode.value,
    selectedPinId: selectedPinId.value,
    showKeepouts: showRegions.value,
    showWireKeepouts: showRegions.value,
    showAccess: showRegions.value,
    showOccupiedHoles: showRegions.value || active === "occupied",
    resizeGeometry,
  });

  function localPoint(
    event: PointerEvent,
  ): { x: number; y: number } | undefined {
    const group = host.current?.querySelector<SVGGElement>(".preview-geometry");
    const matrix = group?.getScreenCTM();
    if (!group || !matrix) return undefined;
    const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
      matrix.inverse(),
    );
    return { x: point.x, y: point.y };
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    const target = event.target as Element;
    const resize = target.closest<SVGElement>("[data-component-resize-handle]");
    if (editable && resize) {
      gesture.current = {
        kind: "resize",
        target: resize.dataset.componentResizeKind as
          "body" | "keepout" | "access",
        index: Number(resize.dataset.componentResizeIndex ?? 0),
        handle: resize.dataset.componentResizeHandle as ResizeHandle,
      };
    } else {
      const pinElement = target.closest<SVGElement>("[data-preview-pin-id]");
      const pin =
        pinElement &&
        definition.pins.find(
          (item) => item.id === pinElement.dataset.previewPinId,
        );
      if (pin) {
        selectedPinId.value = pin.id;
        if (editable && active === "pins")
          gesture.current = { kind: "pin", pin, moved: false };
      } else if (editable && active === "occupied") {
        const point = localPoint(event);
        if (!point) return;
        const hole = { x: Math.round(point.x), y: Math.round(point.y) };
        const holes = definition.occupied_holes ?? [];
        const exists = holes.some(
          (item) => item.x === hole.x && item.y === hole.y,
        );
        void edit({
          type: "set_occupied_holes",
          holes: exists
            ? holes.filter((item) => item.x !== hole.x || item.y !== hole.y)
            : [...holes, hole],
        });
        return;
      } else {
        selectedPinId.value = undefined;
      }
    }
    if (gesture.current) host.current?.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    const current = gesture.current;
    if (!current) return;
    const point = localPoint(event);
    if (!point) return;
    const base = dragDefinition.value ?? definition;
    if (current.kind === "pin") {
      const x = Math.round(point.x);
      const y = Math.round(point.y);
      const existing = base.pins.find((pin) => pin.id === current.pin.id);
      if (!existing || (existing.x === x && existing.y === y)) return;
      current.moved = true;
      dragDefinition.value = {
        ...base,
        pins: base.pins.map((pin) =>
          pin.id === current.pin.id ? { ...pin, x, y } : pin,
        ),
      };
    } else if (current.target === "body") {
      const body = planBodyResize(
        definition.body,
        current.handle,
        point.x,
        point.y,
      );
      if (body) dragDefinition.value = { ...base, body };
    } else {
      const key = current.target === "keepout" ? "keepouts" : "access";
      const regions = [...((definition[key] ?? []) as LocalRect[])];
      const region = regions[current.index];
      const resized =
        region &&
        planHoleRegionResize(region, current.handle, point.x, point.y);
      if (!resized) return;
      regions[current.index] = resized;
      dragDefinition.value = { ...base, [key]: regions };
    }
  }

  async function onPointerUp() {
    const current = gesture.current;
    gesture.current = null;
    const result = dragDefinition.value;
    if (!current || !result) {
      dragDefinition.value = undefined;
      return;
    }
    if (current.kind === "pin") {
      const moved = result.pins.find((pin) => pin.id === current.pin.id);
      if (current.moved && moved)
        await edit(movePinOperation(current.pin, moved.x, moved.y));
    } else if (current.target === "body")
      await edit({ type: "set_body", body: result.body });
    else if (current.target === "keepout")
      await edit({ type: "set_keepouts", keepouts: result.keepouts ?? [] });
    else
      await edit({ type: "set_access_regions", access: result.access ?? [] });
    dragDefinition.value = undefined;
  }

  async function onDoubleClick(event: MouseEvent) {
    if (!editable || active !== "pins") return;
    if ((event.target as Element).closest("[data-preview-pin-id]")) return;
    const point = localPoint(event as PointerEvent);
    if (!point) return;
    const { id, number } = nextPinId(definition.pins);
    const pin = {
      id,
      number,
      name: number,
      x: Math.round(point.x),
      y: Math.round(point.y),
    };
    if (await edit({ type: "upsert_pin", pin })) selectedPinId.value = id;
  }

  return (
    <div
      ref={host}
      class={`designer-preview layer-${active}${editable ? " editable" : ""}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => void onPointerUp()}
      onPointerCancel={() => {
        gesture.current = null;
        dragDefinition.value = undefined;
      }}
      onDblClick={(event) => void onDoubleClick(event)}
      // The preview renderer escapes every user-provided string.
      dangerouslySetInnerHTML={{ __html: markup }}
    />
  );
}

// ── Form sections ──────────────────────────────────────────────────────────
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

function IdentityForm({
  definition,
  editable,
}: {
  definition: ComponentDefinition;
  editable: boolean;
}) {
  const [fields, setFields] = useState(definition);
  useEffect(() => setFields(definition), [definition]);
  const changed =
    fields.id !== definition.id ||
    fields.name !== definition.name ||
    fields.definition_version !== definition.definition_version ||
    fields.description !== definition.description;
  const set = (key: keyof ComponentDefinition) => (event: Event) =>
    setFields({ ...fields, [key]: (event.target as HTMLInputElement).value });
  return (
    <Section title="Identity">
      <form
        class="form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          void edit({
            type: "set_metadata",
            id: fields.id,
            definition_version: fields.definition_version,
            name: fields.name,
            description: fields.description,
          });
        }}
      >
        <label class="span-2">
          Name
          <input
            value={fields.name}
            disabled={!editable}
            onInput={set("name")}
          />
        </label>
        <label>
          Stable ID
          <input
            value={fields.id}
            disabled={!editable}
            onInput={set("id")}
            spellcheck={false}
          />
        </label>
        <label>
          Version
          <input
            value={fields.definition_version}
            disabled={!editable}
            onInput={set("definition_version")}
          />
        </label>
        <label class="span-2">
          Description
          <textarea
            rows={2}
            value={fields.description}
            disabled={!editable}
            onInput={set("description")}
          />
        </label>
        {editable && changed && (
          <div class="form-actions span-2">
            <button
              type="button"
              class="btn ghost"
              onClick={() => setFields(definition)}
            >
              Reset
            </button>
            <button type="submit" class="btn primary">
              Apply
            </button>
          </div>
        )}
      </form>
    </Section>
  );
}

function NumberField({
  label,
  value,
  step = 1,
  disabled,
  onCommit,
}: {
  label: string;
  value: number;
  step?: number;
  disabled?: boolean;
  onCommit: (value: number) => void;
}) {
  return (
    <label>
      {label}
      <input
        type="number"
        step={step}
        value={value}
        disabled={disabled}
        onChange={(event) => {
          const next = Number((event.target as HTMLInputElement).value);
          if (Number.isFinite(next) && next !== value) onCommit(next);
        }}
      />
    </label>
  );
}

function GeometryForm({
  definition,
  editable,
}: {
  definition: ComponentDefinition;
  editable: boolean;
}) {
  const body = definition.body;
  const setBody = (patch: Partial<typeof body>) =>
    void edit({ type: "set_body", body: { ...body, ...patch } });
  const rotations = definition.allowed_rotations;
  return (
    <Section title="Body and orientation">
      <div class="form-grid four">
        <NumberField
          label="X mm"
          step={0.01}
          value={body.x}
          disabled={!editable}
          onCommit={(x) => setBody({ x })}
        />
        <NumberField
          label="Y mm"
          step={0.01}
          value={body.y}
          disabled={!editable}
          onCommit={(y) => setBody({ y })}
        />
        <NumberField
          label="W mm"
          step={0.01}
          value={body.width}
          disabled={!editable}
          onCommit={(width) => setBody({ width })}
        />
        <NumberField
          label="H mm"
          step={0.01}
          value={body.height}
          disabled={!editable}
          onCommit={(height) => setBody({ height })}
        />
        <NumberField
          label="Origin X"
          value={definition.origin_x ?? 0}
          disabled={!editable}
          onCommit={(x) =>
            void edit({ type: "set_origin", x, y: definition.origin_y ?? 0 })
          }
        />
        <NumberField
          label="Origin Y"
          value={definition.origin_y ?? 0}
          disabled={!editable}
          onCommit={(y) =>
            void edit({ type: "set_origin", x: definition.origin_x ?? 0, y })
          }
        />
      </div>
      <div class="field-label">Allowed rotations</div>
      <div class="chip-row">
        {ROTATIONS.map((rotation) => {
          const on = rotations.includes(rotation);
          return (
            <button
              key={rotation}
              class={`chip${on ? " on" : ""}`}
              aria-pressed={on}
              disabled={!editable || (on && rotations.length === 1)}
              onClick={() =>
                void edit({
                  type: "set_rotations",
                  rotations: on
                    ? rotations.filter((item) => item !== rotation)
                    : ROTATIONS.filter(
                        (item) => item === rotation || rotations.includes(item),
                      ),
                })
              }
            >
              {ROTATION_LABEL[rotation]}
            </button>
          );
        })}
      </div>
    </Section>
  );
}

function PinEditor({
  pin,
  editable,
}: {
  pin: PinDefinition;
  editable: boolean;
}) {
  const [fields, setFields] = useState({
    id: pin.id,
    number: pin.number,
    name: pin.name,
    aliases: (pin.aliases ?? []).join(", "),
    functions: (pin.functions ?? []).join(", "),
  });
  useEffect(
    () =>
      setFields({
        id: pin.id,
        number: pin.number,
        name: pin.name,
        aliases: (pin.aliases ?? []).join(", "),
        functions: (pin.functions ?? []).join(", "),
      }),
    [pin],
  );
  const set = (key: keyof typeof fields) => (event: Event) =>
    setFields({ ...fields, [key]: (event.target as HTMLInputElement).value });
  return (
    <form
      class="form-grid pin-editor"
      onSubmit={async (event) => {
        event.preventDefault();
        const ok = await edit({
          type: "upsert_pin",
          previous_id: pin.id,
          pin: {
            id: fields.id.trim(),
            number: fields.number.trim(),
            name: fields.name.trim(),
            x: pin.x,
            y: pin.y,
            aliases: splitList(fields.aliases),
            functions: splitList(fields.functions),
          },
        });
        if (ok) selectedPinId.value = fields.id.trim();
      }}
    >
      <label>
        Number
        <input
          value={fields.number}
          disabled={!editable}
          onInput={set("number")}
        />
      </label>
      <label>
        Name
        <input value={fields.name} disabled={!editable} onInput={set("name")} />
      </label>
      <label>
        ID
        <input
          value={fields.id}
          disabled={!editable}
          onInput={set("id")}
          spellcheck={false}
        />
      </label>
      <label>
        Position
        <input
          value={`${pin.x}, ${pin.y}`}
          disabled
          title="Drag the pin in the preview to move it"
        />
      </label>
      <label class="span-2">
        Aliases
        <input
          value={fields.aliases}
          disabled={!editable}
          onInput={set("aliases")}
          placeholder="comma separated"
        />
      </label>
      <label class="span-2">
        Functions
        <input
          value={fields.functions}
          disabled={!editable}
          onInput={set("functions")}
          placeholder="e.g. TX, PWM"
        />
      </label>
      {editable && (
        <div class="form-actions span-2">
          <button
            type="button"
            class="btn ghost danger"
            onClick={async () => {
              if (await edit({ type: "remove_pin", id: pin.id }))
                selectedPinId.value = undefined;
            }}
          >
            <Icon name="trash" size={14} /> Remove pin
          </button>
          <button type="submit" class="btn primary">
            Apply
          </button>
        </div>
      )}
    </form>
  );
}

function PinsSection({
  definition,
  editable,
}: {
  definition: ComponentDefinition;
  editable: boolean;
}) {
  const selected = definition.pins.find(
    (pin) => pin.id === selectedPinId.value,
  );
  return (
    <Section
      title={`Pins · ${definition.pins.length}`}
      aside={
        editable && (
          <button
            class="btn ghost small"
            onClick={async () => {
              const { id, number } = nextPinId(definition.pins);
              const last = definition.pins[definition.pins.length - 1];
              const pin = {
                id,
                number,
                name: number,
                x: last ? last.x + 1 : 0,
                y: last?.y ?? 0,
              };
              if (await edit({ type: "upsert_pin", pin }))
                selectedPinId.value = id;
            }}
          >
            <Icon name="plus" size={14} /> Add
          </button>
        )
      }
    >
      <div class="pin-list" role="listbox" aria-label="Pins">
        {definition.pins.map((pin, index) => (
          <div
            key={pin.id}
            role="option"
            aria-selected={pin.id === selectedPinId.value}
            class={`pin-row${pin.id === selectedPinId.value ? " on" : ""}`}
            onClick={() =>
              (selectedPinId.value =
                pin.id === selectedPinId.value ? undefined : pin.id)
            }
          >
            <span class="pin-num">{pin.number}</span>
            <span class="pin-name">{pin.name}</span>
            <span class="pin-fn">{(pin.functions ?? []).join(" · ")}</span>
            <span class="pin-pos">
              {pin.x},{pin.y}
            </span>
            {editable && (
              <span class="pin-order">
                <button
                  class="icon-btn tiny"
                  aria-label={`Move pin ${pin.number} up`}
                  disabled={index === 0}
                  onClick={(event) => {
                    event.stopPropagation();
                    void edit({
                      type: "reorder_pin",
                      id: pin.id,
                      index: index - 1,
                    });
                  }}
                >
                  <Icon name="chevronUp" size={12} />
                </button>
                <button
                  class="icon-btn tiny"
                  aria-label={`Move pin ${pin.number} down`}
                  disabled={index === definition.pins.length - 1}
                  onClick={(event) => {
                    event.stopPropagation();
                    void edit({
                      type: "reorder_pin",
                      id: pin.id,
                      index: index + 1,
                    });
                  }}
                >
                  <Icon name="chevronDown" size={12} />
                </button>
              </span>
            )}
          </div>
        ))}
      </div>
      {selected && <PinEditor pin={selected} editable={editable} />}
      {editable && !selected && (
        <p class="field-note">
          Double-click an empty hole in the preview to add a pin there. Drag
          pins to move them.
        </p>
      )}
    </Section>
  );
}

function RegionList<T extends LocalRect>({
  title,
  regions,
  editable,
  make,
  commit,
  extra,
}: {
  title: string;
  regions: T[];
  editable: boolean;
  make: () => T;
  commit: (next: T[]) => void;
  extra?: (
    region: T,
    update: (patch: Partial<T>) => void,
  ) => preact.ComponentChildren;
}) {
  const update = (index: number, patch: Partial<T>) =>
    commit(
      regions.map((region, at) =>
        at === index ? { ...region, ...patch } : region,
      ),
    );
  return (
    <div class="region-list">
      <div class="region-head">
        <span>{title}</span>
        {editable && (
          <button
            class="icon-btn tiny"
            aria-label={`Add ${title.toLowerCase()}`}
            onClick={() => commit([...regions, make()])}
          >
            <Icon name="plus" size={13} />
          </button>
        )}
      </div>
      {regions.length === 0 && <p class="field-note">None</p>}
      {regions.map((region, index) => (
        <div class="region-row" key={index}>
          <input
            class="region-label"
            value={region.label}
            disabled={!editable}
            aria-label="Label"
            onChange={(event) =>
              update(index, {
                label: (event.target as HTMLInputElement).value,
              } as Partial<T>)
            }
          />
          <span class="region-span">
            {region.min.x},{region.min.y} → {region.max.x},{region.max.y}
          </span>
          {extra?.(region, (patch) => update(index, patch))}
          {editable && (
            <button
              class="icon-btn tiny"
              aria-label="Remove region"
              onClick={() => commit(regions.filter((_, at) => at !== index))}
            >
              <Icon name="close" size={12} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

function RegionsSection({
  definition,
  editable,
}: {
  definition: ComponentDefinition;
  editable: boolean;
}) {
  const box = {
    min: {
      x: Math.round(definition.body.x / 2.54),
      y: Math.round(definition.body.y / 2.54),
    },
    max: {
      x: Math.round(definition.body.x / 2.54),
      y: Math.round(definition.body.y / 2.54),
    },
  };
  return (
    <Section title="Clearance and access">
      <RegionList
        title="Keepouts"
        regions={definition.keepouts ?? []}
        editable={editable}
        make={() => ({ ...box, label: "Keepout" })}
        commit={(keepouts) => void edit({ type: "set_keepouts", keepouts })}
      />
      <RegionList
        title="Wire keepouts"
        regions={definition.wire_keepouts ?? []}
        editable={editable}
        make={() => ({ ...box, label: "No wires" })}
        commit={(keepouts) =>
          void edit({ type: "set_wire_keepouts", keepouts })
        }
      />
      <RegionList<AccessRegion>
        title="Access regions"
        regions={definition.access ?? []}
        editable={editable}
        make={() => ({
          ...box,
          label: "Access",
          direction: "north",
          required: true,
          allow_board_overhang: false,
        })}
        commit={(access) => void edit({ type: "set_access_regions", access })}
        extra={(region, update) => (
          <select
            value={region.direction}
            disabled={!editable}
            aria-label="Direction"
            onChange={(event) =>
              update({
                direction: (event.target as HTMLSelectElement)
                  .value as AccessRegion["direction"],
              })
            }
          >
            <option value="north">↑ north</option>
            <option value="east">→ east</option>
            <option value="south">↓ south</option>
            <option value="west">← west</option>
          </select>
        )}
      />
      <div class="region-head">
        <span>Occupied holes · {(definition.occupied_holes ?? []).length}</span>
      </div>
      <p class="field-note">
        Choose the Occupied layer above the preview and click holes to toggle
        them.
      </p>
    </Section>
  );
}

function DocumentationSection({
  documentation,
  editable,
}: {
  documentation: string;
  editable: boolean;
}) {
  const [text, setText] = useState(documentation);
  const [editing, setEditing] = useState(false);
  useEffect(() => setText(documentation), [documentation]);
  return (
    <Section
      title="Documentation"
      aside={
        editable && (
          <button class="btn ghost small" onClick={() => setEditing(!editing)}>
            {editing ? "Preview" : "Edit"}
          </button>
        )
      }
    >
      {editing ? (
        <>
          <textarea
            class="doc-editor"
            rows={12}
            value={text}
            onInput={(event) =>
              setText((event.target as HTMLTextAreaElement).value)
            }
          />
          <div class="form-actions">
            <button
              class="btn primary"
              disabled={text === documentation}
              onClick={() =>
                void edit({ type: "set_documentation", documentation: text })
              }
            >
              Apply
            </button>
          </div>
        </>
      ) : documentation.trim() ? (
        // Markdown is rendered through the sanitizer, which escapes HTML.
        <div
          class="doc-body"
          dangerouslySetInnerHTML={{
            __html: renderSanitizedMarkdown(documentation),
          }}
        />
      ) : (
        <p class="field-note">
          No documentation yet. Agents read this when they use the part.
        </p>
      )}
    </Section>
  );
}

// ── Layout ─────────────────────────────────────────────────────────────────
function EmptyState() {
  return (
    <div class="designer-empty">
      <Icon name="component" size={40} />
      <h2>Design a part</h2>
      <p>
        Parts live in their own folder as <code>component.toml</code> plus
        optional <code>README.md</code>. Open one to edit it here; changes made
        on disk by you or an agent reload live.
      </p>
      <div class="button-row">
        <button class="btn primary" onClick={() => void openPart()}>
          <Icon name="folder" size={16} /> Open part folder…
        </button>
        <button class="btn" onClick={() => void newPart()}>
          <Icon name="plus" size={16} /> New part…
        </button>
      </div>
    </div>
  );
}

export function ComponentDesigner() {
  useEffect(() => {
    void ensureListening().catch(() => (listening = false));
  }, []);
  useSignalEffect(() => {
    // Keep the preview rotation valid when the allowed set changes.
    const allowed = draft.value?.definition.allowed_rotations;
    if (allowed?.length && !allowed.includes(previewRotation.value))
      previewRotation.value = allowed[0]!;
  });

  const state = viewerState.value;
  if (state.kind === "no_component") return <EmptyState />;
  if (state.kind === "loading")
    return (
      <div class="designer-empty">
        <div class="spinner" />
        <p>Waiting for a folder…</p>
      </div>
    );

  const load = displayedLoad(state);
  const current = draft.value;
  const definition = current?.definition ?? load?.definition;
  const documentation = current?.documentation ?? load?.documentation ?? "";
  const editable = Boolean(current);
  const loadErrors =
    state.kind === "load_error"
      ? state.load.diagnostics
      : state.kind === "stale"
        ? state.attempted.diagnostics
        : [];
  const path =
    state.kind === "load_error"
      ? state.load.component_path
      : load?.component_path;

  return (
    <main class="designer">
      <header class="designer-bar">
        <div class="designer-title">
          <Icon name="component" size={18} />
          <div>
            <strong>{definition?.name ?? "Invalid part"}</strong>
            <span class="mono">{path}</span>
          </div>
          {current && (
            <span
              class={`pill ${current.valid ? (current.dirty ? "warn" : "ok") : "error"}`}
            >
              {!current.valid ? "Invalid" : current.dirty ? "Unsaved" : "Saved"}
            </span>
          )}
          <span
            class={`watch ${watcher.value.status}`}
            title={
              watcher.value.error ?? "Reloads when the files change on disk"
            }
          >
            <i />{" "}
            {watcher.value.status === "watching"
              ? "Live"
              : watcher.value.status === "error"
                ? "Not watching"
                : "Stopped"}
          </span>
        </div>
        <div class="designer-actions">
          <button
            class="icon-btn"
            aria-label="Undo part change"
            title="Undo"
            disabled={!current?.can_undo}
            onClick={() => void draftHistory(false)}
          >
            <Icon name="undo" size={17} />
          </button>
          <button
            class="icon-btn"
            aria-label="Redo part change"
            title="Redo"
            disabled={!current?.can_redo}
            onClick={() => void draftHistory(true)}
          >
            <Icon name="redo" size={17} />
          </button>
          <button class="btn ghost" onClick={() => void reloadPart()}>
            <Icon name="refresh" size={15} /> Reload
          </button>
          <button class="btn ghost" onClick={() => void openPart()}>
            <Icon name="folder" size={15} /> Open…
          </button>
          <button class="btn ghost" onClick={() => void closePart()}>
            Close
          </button>
          <button
            class="btn primary"
            disabled={!current?.valid || !current.dirty}
            onClick={() => void savePart()}
          >
            <Icon name="save" size={15} /> Save part
          </button>
        </div>
      </header>

      <div class="designer-body">
        <section class="designer-stage">
          <div class="stage-toolbar">
            {editable && (
              <div class="segmented" role="radiogroup" aria-label="Edit layer">
                {(
                  [
                    ["pins", "Pins"],
                    ["body", "Body"],
                    ["keepout", "Keepouts"],
                    ["access", "Access"],
                    ["occupied", "Occupied"],
                  ] as const
                ).map(([id, label]) => (
                  <button
                    key={id}
                    role="radio"
                    aria-checked={layer.value === id}
                    class={layer.value === id ? "on" : ""}
                    onClick={() => (layer.value = id)}
                  >
                    {label}
                  </button>
                ))}
              </div>
            )}
            <div class="stage-spacer" />
            <label class="inline-select">
              Labels
              <select
                value={labelMode.value}
                onChange={(event) =>
                  (labelMode.value = (event.target as HTMLSelectElement)
                    .value as PinLabelMode)
                }
              >
                <option value="name">Name</option>
                <option value="number">Number</option>
                <option value="combined">Everything</option>
                <option value="function">Function</option>
                <option value="hidden">Hidden</option>
              </select>
            </label>
            <div
              class="segmented"
              role="radiogroup"
              aria-label="Preview rotation"
            >
              {(definition?.allowed_rotations ?? ["deg0"]).map((rotation) => (
                <button
                  key={rotation}
                  role="radio"
                  aria-checked={previewRotation.value === rotation}
                  class={previewRotation.value === rotation ? "on" : ""}
                  onClick={() => (previewRotation.value = rotation)}
                >
                  {ROTATION_LABEL[rotation]}
                </button>
              ))}
            </div>
            <button
              class={`icon-btn${showRegions.value ? " on" : ""}`}
              aria-pressed={showRegions.value}
              title="Show keepouts and access regions"
              aria-label="Show keepouts and access regions"
              onClick={() => (showRegions.value = !showRegions.value)}
            >
              <Icon name="layers" size={17} />
            </button>
          </div>
          {definition ? (
            <DesignerPreview definition={definition} editable={editable} />
          ) : (
            <div class="designer-empty compact">
              <Icon name="error" size={28} />
              <p>
                The part on disk could not be read. Fix the file and it will
                reload.
              </p>
            </div>
          )}
          {(loadErrors.length > 0 ||
            (current && current.diagnostics.length > 0)) && (
            <div class="designer-problems">
              {state.kind === "stale" && (
                <strong>
                  Showing the last valid version. The file on disk has errors:
                </strong>
              )}
              {loadErrors.map((item, index) => (
                <div key={`l${index}`} class="problem-line error">
                  <Icon name="error" size={14} />
                  <span>
                    {item.message}
                    <small class="mono">
                      {item.file}
                      {item.line ? `:${item.line}` : ""}
                      {item.field ? ` · ${item.field}` : ""}
                    </small>
                  </span>
                </div>
              ))}
              {current?.diagnostics.map((message, index) => (
                <div key={`d${index}`} class="problem-line warning">
                  <Icon name="warning" size={14} />
                  <span>{message}</span>
                </div>
              ))}
            </div>
          )}
        </section>

        <aside class="designer-inspector side-panel right">
          {definition && (
            <div class="panel-scroll">
              <IdentityForm definition={definition} editable={editable} />
              <PinsSection definition={definition} editable={editable} />
              <GeometryForm definition={definition} editable={editable} />
              <RegionsSection definition={definition} editable={editable} />
              <DocumentationSection
                documentation={documentation}
                editable={editable}
              />
            </div>
          )}
        </aside>
      </div>
    </main>
  );
}
