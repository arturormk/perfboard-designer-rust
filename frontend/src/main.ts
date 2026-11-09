import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { getBootstrapInfo } from "./api/bootstrap";
import {
  applyOperation,
  applyComponentDraftOperation,
  closeRuntimeComponent,
  commitProject,
  compareProjectRevisions,
  createProject,
  discardProjectChanges,
  evaluateProjectRoute,
  getComponentLibrary,
  getComponentDraft,
  getProject,
  getProjectRevision,
  newComponentDraft,
  openProject,
  reloadRuntimeComponent,
  redoProject,
  redoComponentDraft,
  restoreProjectCommit,
  saveProject,
  saveComponentDraft,
  selectRuntimeComponent,
  suggestProjectRoute,
  undoProject,
  undoComponentDraft,
  type AccessRegion,
  type ComponentDefinition,
  type ComponentDraftOperation,
  type ComponentDraftSnapshot,
  type ComponentInstance,
  type ComponentViewerSnapshot,
  type ComponentWatcherStatus,
  type ProjectDocument,
  type Rotation,
  type RouteProposal,
  type RuntimeComponentLoad,
  type LocalRect,
  type HolePos,
  type SemanticDiff,
  type Side,
  type TerminalRef,
  type Wire,
  type WireColor,
} from "./api/project";
import { renderBoardSvg } from "./editor/renderer";
import { snappedDragPosition } from "./editor/drag";
import {
  fullyEnclosedHoleDimensions,
  parseBoardDimensions,
  planBoardResize,
  type BoardResizePlan,
  type ResizeHandle,
} from "./editor/board-size";
import { planWireReroutes, type WireRerouteTarget } from "./editor/net-routing";
import {
  pinLabelPlacement,
  renderComponentPreviewSvg,
  type PinLabelMode,
} from "./component-designer/preview";
import { renderSanitizedMarkdown } from "./component-designer/markdown";
import {
  displayedLoad,
  isActiveViewerSnapshot,
  transitionViewerState,
  type ComponentViewerState,
} from "./component-designer/viewer-state";
import {
  movePinOperation,
  reorderPinOperation,
  splitList,
} from "./component-designer/editor";

const $ = <T extends Element>(selector: string): T => {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Missing UI element ${selector}`);
  return element;
};
const escapeHtml = (value: string): string =>
  value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character] ?? character,
  );
let project: ProjectDocument;
let projectReady = false;
let refreshPromise: Promise<void> | undefined;
let comparison: SemanticDiff = {
  components: [],
  nets: [],
  wires: [],
  constraints: [],
  routes: [],
};
let library: ComponentDefinition[] = [];
let selectedLibraryDefinitionId: string | undefined;
let selectedId: string | undefined;
let viewBox: [number, number, number, number] | undefined;
let pendingWheelZoom = 1;
let wheelZoomFrame: number | undefined;
let viewSide: Side = "component";
let activeNetId: string | undefined;
let wireMode = false;
let resizeMode = false;
let routeProposal: RouteProposal | undefined;
let rerouting = false;
let wireDraft:
  | { start: TerminalRef; vertices: Array<{ x2: number; y2: number }> }
  | undefined;
let drag:
  | {
      id: string;
      pointerId: number;
      pointerStart: { x: number; y: number };
      initialPosition: { x: number; y: number };
      position: { x: number; y: number };
      element: SVGGraphicsElement;
    }
  | undefined;
let boardResize:
  | {
      pointerId: number;
      handle: ResizeHandle;
      pointerStart: { x: number; y: number };
      plan: BoardResizePlan;
      boundary: SVGRectElement;
    }
  | undefined;
let suppressClick = false;
let panStart:
  | { x: number; y: number; viewBox: [number, number, number, number] }
  | undefined;
type AppMode = "perfboard" | "component";
let appMode: AppMode = "perfboard";
let componentViewerState: ComponentViewerState = { kind: "no_component" };
let componentDraft: ComponentDraftSnapshot | undefined;
let componentViewerGeneration: number | undefined;
let componentWatcherStatus: ComponentWatcherStatus = "stopped";
let componentLastAttemptMs: number | undefined;
let componentWatcherError: string | undefined;
let componentRotation: Rotation = "deg0";
let componentLabelMode: PinLabelMode = "number";
let selectedComponentPinId: string | undefined;
let componentViewBox: [number, number, number, number] | undefined;
let componentPanStart:
  | { x: number; y: number; viewBox: [number, number, number, number] }
  | undefined;
let componentPinDrag:
  | {
      pinId: string;
      pointerId: number;
      initialX: number;
      initialY: number;
      x: number;
      y: number;
      element: SVGGraphicsElement;
      label?: SVGTextElement;
    }
  | undefined;
let componentPinRowDrag: { pinId: string } | undefined;
function setStatus(message: string, state = "ready"): void {
  const status = $("#status");
  status.textContent = message;
  status.setAttribute("data-state", state);
}
function selected(): ComponentInstance | undefined {
  return selectedId ? project.design.components?.[selectedId] : undefined;
}

function displayedComponentLoad(): RuntimeComponentLoad | undefined {
  return displayedLoad(componentViewerState);
}

function activeComponentDefinition(): ComponentDefinition | undefined {
  return componentDraft?.definition ?? displayedComponentLoad()?.definition;
}

async function mutateComponentDraft(
  operation: ComponentDraftOperation,
): Promise<void> {
  try {
    componentDraft = await applyComponentDraftOperation(operation);
    componentViewBox = undefined;
    renderComponentDesigner();
    setStatus(
      componentDraft.valid
        ? "Component draft updated"
        : "Draft updated with validation errors",
      componentDraft.valid ? "ready" : "error",
    );
  } catch (error) {
    setStatus(`Could not update component draft: ${String(error)}`, "error");
  }
}

function applyComponentSnapshot(
  snapshot: ComponentViewerSnapshot,
  baseState = componentViewerState,
): void {
  componentViewerGeneration = snapshot.generation;
  componentWatcherStatus = snapshot.watcher_status;
  componentLastAttemptMs = snapshot.attempted_at_ms;
  componentWatcherError = snapshot.watcher_error;
  const load = snapshot.load;
  componentViewerState = transitionViewerState(baseState, snapshot);
  if (!load) return;
  if (load.definition) {
    componentRotation = load.definition.allowed_rotations.includes(
      componentRotation,
    )
      ? componentRotation
      : (load.definition.allowed_rotations[0] ?? "deg0");
  }
  selectedComponentPinId = undefined;
  componentViewBox = undefined;
}

function setAppMode(mode: AppMode): void {
  appMode = mode;
  const perfboard = $("#perfboard-mode");
  const component = $("#component-mode");
  const perfboardButton = $("#perfboard-mode-button");
  const componentButton = $("#component-mode-button");
  perfboard.toggleAttribute("hidden", mode !== "perfboard");
  component.toggleAttribute("hidden", mode !== "component");
  perfboardButton.classList.toggle("active", mode === "perfboard");
  componentButton.classList.toggle("active", mode === "component");
  perfboardButton.setAttribute("aria-selected", String(mode === "perfboard"));
  componentButton.setAttribute("aria-selected", String(mode === "component"));
  $("#workspace-title").textContent =
    mode === "perfboard" ? "Perfboard Designer" : "Component Designer";
  if (mode === "component") {
    renderComponentDesigner();
    setStatus("Component Designer · graphical authoring");
  } else if (projectReady) {
    setStatus(
      `Revision ${project.revision} · ${project.design.board.holes_x} × ${project.design.board.holes_y} holes`,
    );
  }
}

function renderComponentDesigner(): void {
  const hasOpenComponent =
    componentViewerState.kind === "current" ||
    componentViewerState.kind === "stale" ||
    componentViewerState.kind === "load_error";
  const loading = componentViewerState.kind === "loading";
  const watchStatus = $("#component-watch-status");
  watchStatus.setAttribute("data-status", componentWatcherStatus);
  const attempted = componentLastAttemptMs
    ? new Date(componentLastAttemptMs).toLocaleTimeString()
    : "not yet";
  watchStatus.textContent = componentWatcherError
    ? `Watcher error: ${componentWatcherError}`
    : componentWatcherStatus === "watching"
      ? `Watching selected directory · last load attempt ${attempted}`
      : "File watcher stopped";
  $("#component-mode").classList.toggle(
    "component-stale",
    componentViewerState.kind === "stale",
  );
  ($("#open-component") as HTMLButtonElement).disabled = loading;
  ($("#new-component") as HTMLButtonElement).disabled = loading;
  ($("#reload-component") as HTMLButtonElement).disabled =
    loading || !hasOpenComponent;
  ($("#close-component") as HTMLButtonElement).disabled =
    loading || !hasOpenComponent;
  ($("#save-component") as HTMLButtonElement).disabled =
    loading || !componentDraft?.dirty || !componentDraft.valid;
  ($("#undo-component") as HTMLButtonElement).disabled =
    loading || !componentDraft?.can_undo;
  ($("#redo-component") as HTMLButtonElement).disabled =
    loading || !componentDraft?.can_redo;
  const draftStatus = $("#component-draft-status");
  draftStatus.textContent = componentDraft
    ? `${componentDraft.dirty ? "Unsaved draft" : "Saved draft"} · ${componentDraft.valid ? "valid" : "invalid"}`
    : "No editable draft";
  draftStatus.setAttribute(
    "data-state",
    componentDraft?.valid === false ? "invalid" : "valid",
  );
  document
    .querySelectorAll<
      HTMLInputElement | HTMLTextAreaElement | HTMLButtonElement
    >(
      "#component-fields input, #component-fields textarea, #component-fields button, #component-pin-form input, #component-pin-form button, #draft-documentation, #apply-draft-documentation",
    )
    .forEach((control) => {
      control.disabled = !componentDraft || loading;
    });
  for (const id of [
    "component-label-mode",
    "component-zoom-out",
    "component-zoom-in",
    "component-fit",
    "show-component-keepouts",
    "show-component-access",
    "show-component-occupied",
  ]) {
    (
      $(`#${id}`) as HTMLButtonElement | HTMLInputElement | HTMLSelectElement
    ).disabled = !activeComponentDefinition() || loading;
  }

  if (componentViewerState.kind === "loading") {
    $("#component-canvas").innerHTML =
      '<div class="empty component-loading">Loading and validating component…</div>';
    $("#component-summary").innerHTML = "";
    $("#component-diagnostics").innerHTML = "";
    $("#component-pin-table").innerHTML = "";
    $("#component-rotations").innerHTML = "";
    $("#component-documentation").innerHTML = "";
    return;
  }

  if (componentViewerState.kind === "no_component") {
    $("#component-canvas").innerHTML =
      '<div class="empty">Open a directory containing <code>component.toml</code>.</div>';
    $("#component-summary").innerHTML =
      '<p class="tool-hint">No component is open.</p>';
    $("#component-diagnostics").innerHTML = "";
    $("#component-pin-table").innerHTML = "";
    $("#component-rotations").innerHTML = "";
    $("#component-documentation").innerHTML =
      '<p class="documentation-absent">No README.md is available.</p>';
    return;
  }

  const attemptedLoad =
    componentViewerState.kind === "stale"
      ? componentViewerState.attempted
      : componentViewerState.load;
  const diagnostics = attemptedLoad.diagnostics;
  const draftDiagnostics = componentDraft?.diagnostics ?? [];
  $("#component-diagnostics").innerHTML = diagnostics.length
    ? `<section class="component-diagnostic-list"><h2>${componentViewerState.kind === "stale" ? "Reload failed — preview is stale" : "Could not load component"}</h2>${diagnostics
        .map((diagnostic) => {
          const location = [diagnostic.file, diagnostic.line, diagnostic.column]
            .filter((part) => part !== undefined)
            .join(":");
          return `<article><code>${escapeHtml(diagnostic.code)}</code><p>${escapeHtml(diagnostic.message)}</p><small>${escapeHtml(location)}${diagnostic.field ? ` · ${escapeHtml(diagnostic.field)}` : ""}</small></article>`;
        })
        .join("")}</section>`
    : draftDiagnostics.length
      ? `<section class="component-diagnostic-list"><h2>Draft needs attention</h2>${draftDiagnostics.map((message) => `<article><code>component.invalid_draft</code><p>${escapeHtml(message)}</p></article>`).join("")}</section>`
      : "";

  const definition = activeComponentDefinition();
  if (!definition) {
    $("#component-canvas").innerHTML =
      '<div class="empty component-load-error">The selected directory has no valid component preview.</div>';
    $("#component-summary").innerHTML =
      `<dl class="component-metadata"><dt>Directory</dt><dd><code>${escapeHtml(attemptedLoad.component_path)}</code></dd><dt>Revision</dt><dd><code>${escapeHtml(attemptedLoad.revision.slice(0, 23))}…</code></dd></dl>`;
    $("#component-pin-table").innerHTML = "";
    $("#component-rotations").innerHTML = "";
    $("#component-documentation").innerHTML = attemptedLoad.documentation
      ? renderSanitizedMarkdown(attemptedLoad.documentation)
      : '<p class="documentation-absent">No README.md is available.</p>';
    return;
  }
  if (!definition.allowed_rotations.includes(componentRotation)) {
    componentRotation = definition.allowed_rotations[0] ?? "deg0";
  }
  for (const [id, value] of [
    ["draft-id", definition.id],
    ["draft-version", definition.definition_version],
    ["draft-name", definition.name],
    ["draft-description", definition.description],
    ["draft-body-x", String(definition.body.x)],
    ["draft-body-y", String(definition.body.y)],
    ["draft-body-width", String(definition.body.width)],
    ["draft-body-height", String(definition.body.height)],
    ["draft-origin-x", String(definition.origin_x ?? 0)],
    ["draft-origin-y", String(definition.origin_y ?? 0)],
  ]) {
    ($(`#${id}`) as HTMLInputElement | HTMLTextAreaElement).value = value ?? "";
  }
  document
    .querySelectorAll<HTMLInputElement>("#draft-rotations input")
    .forEach((input) => {
      input.checked = definition.allowed_rotations.includes(
        input.value as Rotation,
      );
      input.disabled = !componentDraft;
    });
  ($("#draft-occupied") as HTMLTextAreaElement).value = JSON.stringify(
    definition.occupied_holes ?? [],
    null,
    2,
  );
  ($("#draft-keepouts") as HTMLTextAreaElement).value = JSON.stringify(
    definition.keepouts ?? [],
    null,
    2,
  );
  ($("#draft-access") as HTMLTextAreaElement).value = JSON.stringify(
    definition.access ?? [],
    null,
    2,
  );
  if (
    selectedComponentPinId &&
    !definition.pins.some((pin) => pin.id === selectedComponentPinId)
  ) {
    selectedComponentPinId = undefined;
  }
  $("#component-summary").innerHTML = `
    <h2>${escapeHtml(definition.name)}</h2>
    <p>${escapeHtml(definition.description)}</p>
    <dl class="component-metadata">
      <dt>Directory</dt><dd><code>${escapeHtml(displayedComponentLoad()!.component_path)}</code></dd>
      <dt>Stable ID</dt><dd><code>${escapeHtml(definition.id)}</code></dd>
      <dt>Definition</dt><dd>${escapeHtml(definition.definition_version)}</dd>
      <dt>Schema</dt><dd>${definition.schema_version}</dd>
      <dt>Body</dt><dd>${definition.body.width} × ${definition.body.height} mm</dd>
      <dt>Pins</dt><dd>${definition.pins.length}</dd>
      <dt>Revision</dt><dd><code title="${escapeHtml(displayedComponentLoad()!.revision)}">${escapeHtml(displayedComponentLoad()!.revision.slice(0, 23))}…</code></dd>
      ${componentViewerState.kind === "stale" ? `<dt>Attempted revision</dt><dd><code title="${escapeHtml(componentViewerState.attempted.revision)}">${escapeHtml(componentViewerState.attempted.revision.slice(0, 23))}…</code></dd>` : ""}
    </dl>`;
  $("#component-rotations").innerHTML = definition.allowed_rotations
    .map(
      (rotation) =>
        `<button type="button" data-component-rotation="${rotation}" class="${rotation === componentRotation ? "active" : ""}" aria-pressed="${rotation === componentRotation}">${rotation.replace("deg", "")}°</button>`,
    )
    .join("");
  const overlays = {
    showKeepouts: ($("#show-component-keepouts") as HTMLInputElement).checked,
    showAccess: ($("#show-component-access") as HTMLInputElement).checked,
    showOccupiedHoles: ($("#show-component-occupied") as HTMLInputElement)
      .checked,
  };
  $("#component-canvas").innerHTML = renderComponentPreviewSvg(definition, {
    rotation: componentRotation,
    labelMode: componentLabelMode,
    selectedPinId: selectedComponentPinId,
    viewBox: componentViewBox?.join(" "),
    ...overlays,
  });
  $("#component-canvas").classList.toggle(
    "component-invalid-draft",
    componentDraft?.valid === false,
  );
  $("#component-pin-table").innerHTML = `
    <table class="pin-table component-pin-table">
      <thead><tr><th>No.</th><th>Pin</th><th>Aliases / functions</th><th>Position</th></tr></thead>
      <tbody>
        ${definition.pins
          .map(
            (
              pin,
            ) => `<tr tabindex="0" draggable="true" data-component-pin-row="${escapeHtml(pin.id)}" class="${pin.id === selectedComponentPinId ? "selected" : ""}" aria-selected="${pin.id === selectedComponentPinId}" title="Drag to reorder">
              <td>${escapeHtml(pin.number)}</td>
              <td><strong>${escapeHtml(pin.name)}</strong><small>${escapeHtml(pin.id)}</small></td>
              <td>${escapeHtml([...(pin.aliases ?? []), ...(pin.functions ?? [])].join(", ") || "—")}</td>
              <td>(${pin.x}, ${pin.y})</td>
            </tr>`,
          )
          .join("")}
      </tbody>
    </table>`;
  const documentation = displayedComponentLoad()?.documentation;
  const draftDocumentation =
    componentDraft?.documentation ?? documentation ?? "";
  $("#component-documentation").innerHTML = draftDocumentation
    ? renderSanitizedMarkdown(draftDocumentation)
    : '<p class="documentation-absent">No README.md is available.</p>';
  ($("#draft-documentation") as HTMLTextAreaElement).value = draftDocumentation;
  const selectedPin = definition.pins.find(
    (pin) => pin.id === selectedComponentPinId,
  );
  for (const [id, value] of [
    ["draft-pin-id", selectedPin?.id ?? ""],
    ["draft-pin-number", selectedPin?.number ?? ""],
    ["draft-pin-name", selectedPin?.name ?? ""],
    ["draft-pin-x", selectedPin ? String(selectedPin.x) : "0"],
    ["draft-pin-y", selectedPin ? String(selectedPin.y) : "0"],
    ["draft-pin-aliases", selectedPin?.aliases?.join(", ") ?? ""],
    ["draft-pin-functions", selectedPin?.functions?.join(", ") ?? ""],
  ]) {
    ($(`#${id}`) as HTMLInputElement).value = value ?? "";
  }
  ($("#remove-draft-pin") as HTMLButtonElement).disabled =
    !componentDraft || !selectedPin;
}

function zoomComponent(factor: number): void {
  const svg = document.querySelector<SVGSVGElement>("#component-preview");
  if (!svg) return;
  const current = (
    svg.getAttribute("viewBox") ??
    svg.dataset.defaultViewBox ??
    "-2 -2 10 10"
  )
    .split(" ")
    .map(Number) as [number, number, number, number];
  const [x, y, width, height] = current;
  const nextWidth = width * factor;
  const nextHeight = height * factor;
  componentViewBox = [
    x + (width - nextWidth) / 2,
    y + (height - nextHeight) / 2,
    nextWidth,
    nextHeight,
  ];
  svg.setAttribute("viewBox", componentViewBox.join(" "));
}
function render(): void {
  const changes = Object.fromEntries(
    [...comparison.components, ...comparison.wires].map((item) => [
      item.object_id,
      item.kind,
    ]),
  );
  const previewProject = routeProposal
    ? {
        ...project,
        design: {
          ...project.design,
          wires: {
            ...(project.design.wires ?? {}),
            "route-proposal-preview": {
              id: "route-proposal-preview",
              net_id: routeProposal.net_id,
              start: routeProposal.start,
              end: routeProposal.end,
              side: routeProposal.side,
              color: "orange" as const,
              vertices: routeProposal.vertices,
              notes: "Route proposal",
            },
          },
        },
      }
    : project;
  $("#canvas").innerHTML = renderBoardSvg(
    previewProject,
    selectedId,
    viewBox?.join(" "),
    viewSide,
    changes,
    "board",
    resizeMode,
  );
  $("#canvas").classList.toggle("resize-mode", resizeMode);
  const head = project.head_commit
    ? project.commits?.[project.head_commit]
    : undefined;
  $("#committed-pane").classList.toggle("empty-commit", !head);
  $("#committed-canvas").innerHTML = head
    ? renderBoardSvg(
        { ...project, design: head.design, validation: head.validation },
        undefined,
        undefined,
        viewSide,
        changes,
        "committed-board",
      )
    : '<p class="tool-hint">No commit yet.</p>';
  if (
    !selectedLibraryDefinitionId ||
    !library.some((item) => item.id === selectedLibraryDefinitionId)
  )
    selectedLibraryDefinitionId = library[0]?.id;
  const selectedLibraryDefinition = library.find(
    (item) => item.id === selectedLibraryDefinitionId,
  );
  $("#library").innerHTML = library.length
    ? `<label class="stacked">Component type
        <select id="component-library-select">
          ${library
            .map(
              (item) =>
                `<option value="${escapeHtml(item.id)}"${item.id === selectedLibraryDefinitionId ? " selected" : ""}>${escapeHtml(item.name)}</option>`,
            )
            .join("")}
        </select>
      </label>
      <p id="component-library-description" class="tool-hint">${escapeHtml(selectedLibraryDefinition?.description ?? "")}</p>
      <button id="add-library-component" type="button">Add component</button>`
    : '<p class="tool-hint">No component types available.</p>';
  $("#component-list").innerHTML =
    Object.values(project.design.components ?? {})
      .map(
        (item) =>
          `<button class="component-item${item.id === selectedId ? " active" : ""}" data-select-id="${item.id}">${item.name}<span>${item.position.x}, ${item.position.y} · ${item.rotation}</span></button>`,
      )
      .join("") || "<p>No components placed.</p>";
  const nets = Object.values(project.design.nets ?? {});
  if (!activeNetId || !project.design.nets?.[activeNetId])
    activeNetId = nets[0]?.id;
  $("#active-net").innerHTML = nets
    .map(
      (net) =>
        `<option value="${net.id}"${net.id === activeNetId ? " selected" : ""}>${net.name} (${net.state})</option>`,
    )
    .join("");
  $("#net-list").innerHTML =
    nets
      .map(
        (net) =>
          `<div class="net-item ${net.state}"><strong>${net.name}</strong> · ${net.pins?.length ?? 0} pins<div>${net.state === "proposed" ? `<button data-accept-net="${net.id}">Accept</button><button data-reject-net="${net.id}">Reject</button>` : `<button data-remove-net="${net.id}">Remove</button>`}</div></div>`,
      )
      .join("") || "<p>No nets defined.</p>";
  $("#route-proposal").innerHTML = routeProposal
    ? `<article class="route-result"><strong>${routeProposal.length_mm.toFixed(2)} mm · ${routeProposal.bend_count} bends</strong><p>Cost ${routeProposal.cost.total}: length ${routeProposal.cost.length}, bends ${routeProposal.cost.bends}, crossings ${routeProposal.cost.crossings}, jumper ${routeProposal.cost.jumper_adjustment}</p>${routeProposal.jumper_match ? `<p>Jumper: ${escapeHtml(routeProposal.jumper_match.inventory_id)} (${routeProposal.jumper_match.kind})</p>` : ""}${(routeProposal.warnings ?? []).map((warning) => `<p class="warning">${escapeHtml(warning)}</p>`).join("")}${routeProposal.crossings.length ? '<label>At crossings <select id="crossing-order"><option value="over">New route over</option><option value="under">New route under</option></select></label>' : ""}<div class="button-row"><button id="accept-route">Accept</button><button id="reject-route">Reject</button></div></article>`
    : "<p>No active proposal.</p>";
  $("#jumper-list").innerHTML =
    Object.values(project.design.jumper_inventory ?? {})
      .map(
        (item) =>
          `<div class="net-item"><strong>${escapeHtml(item.id)}</strong> · ${item.span_holes} intervals ±${item.tolerance_holes} · ${escapeHtml(item.color)}${item.quantity === undefined ? "" : ` · qty ${item.quantity}`}<button data-remove-jumper="${escapeHtml(item.id)}">Remove</button></div>`,
      )
      .join("") || "<p>No preferred jumpers.</p>";
  $("#constraint-list").innerHTML =
    Object.values(project.design.constraints ?? {})
      .map(
        (item) =>
          `<article class="operation-item"><strong>${escapeHtml(item.type.replaceAll("_", " "))}</strong><span>${item.severity} · ${escapeHtml(item.rationale ?? "No rationale")}</span><div><button data-edit-constraint="${item.id}">Edit</button><button data-remove-constraint="${item.id}">Remove</button></div></article>`,
      )
      .join("") || "<p>No constraints.</p>";
  $("#view-side").textContent = `View: ${viewSide}`;
  $("#wire-mode").classList.toggle("active", wireMode);
  $("#resize-board").classList.toggle("active", resizeMode);
  $("#resize-board").setAttribute("aria-pressed", String(resizeMode));
  const unlockedWireCount = Object.values(project.design.wires ?? {}).filter(
    (candidate) => !candidate.locked,
  ).length;
  const rerouteAll = $("#reroute-all") as HTMLButtonElement;
  rerouteAll.disabled = rerouting || unlockedWireCount === 0;
  rerouteAll.textContent = rerouting ? "Rerouting…" : "Reroute all wires";
  const instance = selected();
  const wire = selectedId ? project.design.wires?.[selectedId] : undefined;
  const selectedDefinition = instance
    ? project.design.component_definitions?.[instance.definition_id]
    : undefined;
  const pinTable = selectedDefinition
    ? `<h3>Pin reference</h3><table class="pin-table"><thead><tr><th>Pin</th><th>Name</th></tr></thead><tbody>${selectedDefinition.pins.map((pin) => `<tr><td>${escapeHtml(pin.number)}</td><td>${escapeHtml(pin.name)}</td></tr>`).join("")}</tbody></table>`
    : "";
  $("#inspector").innerHTML = instance
    ? `<label>Name <input id="instance-name" value="${escapeHtml(instance.name)}" /></label><label>Notes <textarea id="instance-notes">${escapeHtml(instance.notes ?? "")}</textarea></label><div class="button-row"><button id="rotate-component">Rotate 90°</button><button id="remove-component" class="danger">Remove</button></div><p>Anchor: ${instance.position.x}, ${instance.position.y}<br>Rotation: ${instance.rotation}</p>${pinTable}`
    : wire
      ? `<p>Wire ${wire.id.slice(0, 8)}</p><label>Side <select id="selected-wire-side"><option${wire.side === "component" ? " selected" : ""}>component</option><option${wire.side === "solder" ? " selected" : ""}>solder</option></select></label><label>Color <select id="selected-wire-color">${["green", "red", "black", "yellow", "white", "blue", "orange"].map((color) => `<option${wire.color === color ? " selected" : ""}>${color}</option>`).join("")}</select></label>${wire.vertices.map((point, index) => `<div class="wire-vertex"><label>x2 <input data-vertex-x="${index}" type="number" value="${point.x2}" /></label><label>y2 <input data-vertex-y="${index}" type="number" value="${point.y2}" /></label>${index > 0 && index < wire.vertices.length - 1 ? `<button data-remove-vertex="${index}">Remove bend</button>` : ""}</div>`).join("")}<div class="button-row"><button id="reroute-wire"${wire.locked || rerouting ? " disabled" : ""}>Reroute selected</button><button id="evaluate-wire">Evaluate</button><button id="toggle-wire-lock">${wire.locked ? "Unlock" : "Lock"}</button><button id="remove-wire" class="danger">Remove wire</button></div>${wire.locked ? '<p class="tool-hint">Unlock this wire before rerouting it.</p>' : ""}`
      : "<p>Select a component or wire.</p>";
  const diagnostics = project.validation ?? [];
  $("#validation").innerHTML = diagnostics.length
    ? diagnostics
        .map(
          (item) =>
            `<article class="diagnostic ${item.severity}"><code>${item.code}</code><p>${item.message}</p></article>`,
        )
        .join("")
    : '<p class="valid">No placement problems.</p>';
  const commits = [...(project.commit_order ?? [])].reverse();
  $("#revision-history").innerHTML = commits.length
    ? commits
        .map((id) => {
          const commit = project.commits?.[id];
          if (!commit) return "";
          const origins = (commit.changes ?? [])
            .map(
              (change) =>
                `${change.origin}${change.rationale ? `: ${escapeHtml(change.rationale)}` : ""}`,
            )
            .join(" · ");
          return `<article class="history-item"><strong>${escapeHtml(commit.name)}</strong><span>r${commit.source_revision} · ${new Date(commit.timestamp_ms).toLocaleString()}</span>${origins ? `<span>${origins}</span>` : ""}<div><button data-compare-commit="${id}">Compare</button><button data-restore-commit="${id}">Restore</button></div></article>`;
        })
        .join("")
    : "<p>No commits yet.</p>";
  const diffEntries = [
    ...comparison.components,
    ...comparison.nets,
    ...comparison.wires,
    ...comparison.constraints,
  ];
  $("#semantic-diff").innerHTML = diffEntries.length
    ? diffEntries
        .map(
          (item) =>
            `<p class="diff-entry ${item.kind}"><span>${item.kind.replace("_", " ")}</span>${escapeHtml(item.summary)}</p>`,
        )
        .join("")
    : "<p>No differences from the selected commit.</p>";
  const workingChanges = project.undo_stack ?? [];
  $("#operation-history").innerHTML = workingChanges.length
    ? workingChanges
        .slice()
        .reverse()
        .map(
          (item) =>
            `<article class="operation-item"><strong>${escapeHtml(item.summary)}</strong><span>${item.origin}${item.rationale ? ` · ${escapeHtml(item.rationale)}` : ""}</span></article>`,
        )
        .join("")
    : "<p>No uncommitted operations.</p>";
  ($("#undo-project") as HTMLButtonElement).disabled =
    !project.undo_stack?.length;
  ($("#redo-project") as HTMLButtonElement).disabled =
    !project.redo_stack?.length;
  ($("#discard-project") as HTMLButtonElement).disabled =
    !project.undo_stack?.length;
}
async function refresh(): Promise<void> {
  routeProposal = undefined;
  const loadedProject = await getProject();
  const loadedComparison = loadedProject.head_commit
    ? await compareProjectRevisions(loadedProject.head_commit)
    : { components: [], nets: [], wires: [], constraints: [], routes: [] };
  if (projectReady && loadedProject.revision < project.revision) return;
  project = loadedProject;
  comparison = loadedComparison;
  projectReady = true;
  render();
  if (appMode === "perfboard") {
    setStatus(
      `Revision ${project.revision} · ${project.design.board.holes_x} × ${project.design.board.holes_y} holes`,
    );
  }
}
function requestRefresh(): Promise<void> {
  refreshPromise ??= refresh().finally(() => {
    refreshPromise = undefined;
  });
  return refreshPromise;
}
async function refreshComponentLibrary(): Promise<void> {
  const button = $("#refresh-component-library") as HTMLButtonElement;
  button.disabled = true;
  button.classList.add("refreshing");
  try {
    library = await getComponentLibrary();
    if (!library.some((item) => item.id === selectedLibraryDefinitionId))
      selectedLibraryDefinitionId = library[0]?.id;
    render();
    setStatus(
      `Component library refreshed · ${library.length} component type${library.length === 1 ? "" : "s"}`,
    );
  } catch (error) {
    setStatus(`Could not refresh component library: ${String(error)}`, "error");
  } finally {
    button.disabled = false;
    button.classList.remove("refreshing");
  }
}
async function refreshIfChanged(): Promise<void> {
  if (!projectReady || document.visibilityState === "hidden") return;
  const revision = await getProjectRevision();
  if (revision !== project.revision) await requestRefresh();
}
async function mutate(
  payload: Parameters<typeof applyOperation>[1],
): Promise<void> {
  try {
    await applyOperation(project.revision, payload);
    await refresh();
  } catch (error) {
    setStatus(String(error), "error");
  }
}

async function rerouteWire(
  wire: Wire,
  target: WireRerouteTarget,
): Promise<void> {
  const proposal = await suggestProjectRoute({
    net_id: wire.net_id,
    start: target.start,
    end: target.end,
    preferred_side: wire.side,
    replace_wire_id: wire.id,
  });
  await applyOperation(project.revision, {
    type: "accept_route_proposal",
    proposal,
    wire_id: wire.id,
    color: wire.color,
    notes: wire.notes ?? "",
    replace_wire_id: wire.id,
    new_route_over: proposal.crossings.length ? true : undefined,
  });
  await refresh();
}

async function rerouteWires(targets: WireRerouteTarget[]): Promise<void> {
  if (rerouting) return;
  rerouting = true;
  render();
  let completed = 0;
  let skipped = 0;
  try {
    for (const target of targets) {
      const wire = project.design.wires?.[target.wireId];
      if (!wire || wire.locked) {
        skipped += 1;
        continue;
      }
      await rerouteWire(wire, target);
      completed += 1;
    }
    setStatus(
      `Rerouted ${completed} wire${completed === 1 ? "" : "s"}; bends are anchored at free holes${skipped ? ` · left ${skipped} locked` : ""}`,
    );
  } catch (error) {
    setStatus(
      `Rerouted ${completed} wire${completed === 1 ? "" : "s"}, then stopped: ${String(error)}`,
      "error",
    );
  } finally {
    rerouting = false;
    render();
  }
}
function nextRotation(rotation: Rotation): Rotation {
  const rotations: Rotation[] = ["deg0", "deg90", "deg180", "deg270"];
  return rotations[(rotations.indexOf(rotation) + 1) % rotations.length]!;
}
function svgVisualPoint(
  event: PointerEvent,
): { x: number; y: number } | undefined {
  const matrix = ($("#board") as SVGSVGElement).getScreenCTM();
  if (!matrix) return undefined;
  const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
    matrix.inverse(),
  );
  return { x: point.x, y: point.y };
}

function svgPoint(event: PointerEvent): { x: number; y: number } | undefined {
  const point = svgVisualPoint(event);
  if (!point) return undefined;
  return {
    x:
      viewSide === "solder"
        ? project.design.board.holes_x - 1 - point.x
        : point.x,
    y: point.y,
  };
}

function pinRoutePosition(
  componentId: string,
  pinId: string,
): { x2: number; y2: number } | undefined {
  const instance = project.design.components?.[componentId];
  const definition =
    instance && project.design.component_definitions?.[instance.definition_id];
  const pin = definition?.pins.find((item) => item.id === pinId);
  if (!instance || !pin) return undefined;
  const rotated: Record<Rotation, [number, number]> = {
    deg0: [pin.x, pin.y],
    deg90: [-pin.y, pin.x],
    deg180: [-pin.x, -pin.y],
    deg270: [pin.y, -pin.x],
  };
  const [x, y] = rotated[instance.rotation];
  return {
    x2: (instance.position.x + x) * 2,
    y2: (instance.position.y + y) * 2,
  };
}

function appendRoutable(target: { x2: number; y2: number }): void {
  if (!wireDraft) return;
  const last = wireDraft.vertices.at(-1)!;
  const dx = target.x2 - last.x2;
  const dy = target.y2 - last.y2;
  if (dx !== 0 && dy !== 0 && Math.abs(dx) !== Math.abs(dy)) {
    const diagonal = Math.min(Math.abs(dx), Math.abs(dy));
    wireDraft.vertices.push({
      x2: last.x2 + Math.sign(dx) * diagonal,
      y2: last.y2 + Math.sign(dy) * diagonal,
    });
  }
  const end = wireDraft.vertices.at(-1)!;
  if (end.x2 !== target.x2 || end.y2 !== target.y2)
    wireDraft.vertices.push(target);
}

async function handleWirePin(
  componentId: string,
  pinId: string,
): Promise<void> {
  const netId = activeNetId;
  const net = netId ? project.design.nets?.[netId] : undefined;
  if (!net || !netId) {
    setStatus("Create and select an accepted net first", "error");
    return;
  }
  if (net.state !== "accepted") {
    setStatus("Accept the active net proposal before routing it", "error");
    return;
  }
  const ref = { component_id: componentId, pin_id: pinId };
  const owner = Object.values(project.design.nets ?? {}).find((candidate) =>
    candidate.pins?.some(
      (pin) => pin.component_id === componentId && pin.pin_id === pinId,
    ),
  );
  if (owner && owner.id !== netId) {
    setStatus(`This pin already belongs to net ${owner.name}`, "error");
    return;
  }
  if (!owner) {
    try {
      await applyOperation(project.revision, {
        type: "assign_pin",
        net_id: netId,
        pin: ref,
      });
      await refresh();
    } catch (error) {
      setStatus(`Could not assign endpoint: ${String(error)}`, "error");
      return;
    }
  }
  const position = pinRoutePosition(componentId, pinId);
  if (!position) return;
  const terminal: TerminalRef = {
    kind: "pin",
    component_id: componentId,
    pin_id: pinId,
  };
  if (!wireDraft) {
    wireDraft = { start: terminal, vertices: [position] };
    setStatus("Starting pin assigned. Add bends or click the ending pin.");
    return;
  }
  if (JSON.stringify(terminal) === JSON.stringify(wireDraft.start)) {
    setStatus("Choose a different ending pin", "error");
    return;
  }
  appendRoutable(position);
  const color = ($("#wire-color") as HTMLSelectElement).value as WireColor;
  const draft = wireDraft;
  wireDraft = undefined;
  wireMode = false;
  await mutate({
    type: "add_wire",
    wire: {
      id: crypto.randomUUID(),
      net_id: netId,
      start: draft.start,
      end: terminal,
      side: viewSide,
      color,
      vertices: draft.vertices,
      locked: false,
      notes: "",
    },
  });
}

$("#perfboard-mode-button").addEventListener("click", () =>
  setAppMode("perfboard"),
);
$("#component-mode-button").addEventListener("click", () =>
  setAppMode("component"),
);
$("#refresh-component-library").addEventListener("click", () => {
  void refreshComponentLibrary();
});
$("#open-component").addEventListener("click", () => {
  const previous = componentViewerState;
  componentViewerState = { kind: "loading" };
  renderComponentDesigner();
  setStatus("Selecting a component directory…");
  void selectRuntimeComponent()
    .then(async (snapshot) => {
      if (!snapshot?.load) {
        componentViewerState = previous;
        renderComponentDesigner();
        setStatus("Component selection cancelled");
        return;
      }
      const load = snapshot.load;
      applyComponentSnapshot(snapshot);
      componentDraft = (await getComponentDraft()) ?? undefined;
      componentRotation = load.definition?.allowed_rotations[0] ?? "deg0";
      renderComponentDesigner();
      setStatus(
        snapshot.watcher_status === "error"
          ? `Loaded ${load.definition?.name ?? "component"}, but live reload is unavailable`
          : load.definition
            ? `Loaded ${load.definition.name} · editable draft`
            : "The selected component is invalid",
        load.definition && snapshot.watcher_status !== "error"
          ? "ready"
          : "error",
      );
    })
    .catch((error: unknown) => {
      componentViewerState = previous;
      renderComponentDesigner();
      setStatus(`Could not open component: ${String(error)}`, "error");
    });
});
$("#new-component").addEventListener("click", () => {
  const previous = componentViewerState;
  componentViewerState = { kind: "loading" };
  renderComponentDesigner();
  setStatus("Selecting a directory for the new component…");
  void newComponentDraft()
    .then(async (snapshot) => {
      if (!snapshot?.load) {
        componentViewerState = previous;
        renderComponentDesigner();
        setStatus("New component cancelled");
        return;
      }
      applyComponentSnapshot(snapshot);
      componentDraft = (await getComponentDraft()) ?? undefined;
      componentRotation = "deg0";
      renderComponentDesigner();
      setStatus(
        "New component draft created · choose a stable ID before saving",
      );
    })
    .catch((error: unknown) => {
      componentViewerState = previous;
      renderComponentDesigner();
      setStatus(`Could not create component draft: ${String(error)}`, "error");
    });
});
$("#reload-component").addEventListener("click", () => {
  const previous = componentViewerState;
  componentViewerState = { kind: "loading" };
  renderComponentDesigner();
  setStatus("Reloading component…");
  void reloadRuntimeComponent()
    .then(async (snapshot) => {
      if (!snapshot?.load) {
        componentViewerState = { kind: "no_component" };
        renderComponentDesigner();
        setStatus("No component is open", "error");
        return;
      }
      const load = snapshot.load;
      applyComponentSnapshot(snapshot, previous);
      componentDraft = (await getComponentDraft()) ?? componentDraft;
      renderComponentDesigner();
      setStatus(
        snapshot.watcher_status === "error"
          ? `Reloaded ${load.definition?.name ?? "component"}, but live reload is unavailable`
          : load.definition
            ? `Reloaded ${load.definition.name} · editable draft`
            : componentViewerState.kind === "stale"
              ? "Reload failed · showing the last valid preview"
              : "The component is still invalid",
        load.definition && snapshot.watcher_status !== "error"
          ? "ready"
          : "error",
      );
    })
    .catch((error: unknown) => {
      componentViewerState = previous;
      renderComponentDesigner();
      setStatus(`Could not reload component: ${String(error)}`, "error");
    });
});
$("#close-component").addEventListener("click", () => {
  void closeRuntimeComponent()
    .then(() => {
      componentViewerState = { kind: "no_component" };
      componentDraft = undefined;
      componentViewerGeneration = undefined;
      componentWatcherStatus = "stopped";
      componentLastAttemptMs = undefined;
      componentWatcherError = undefined;
      componentRotation = "deg0";
      selectedComponentPinId = undefined;
      componentViewBox = undefined;
      renderComponentDesigner();
      setStatus("Component closed");
    })
    .catch((error: unknown) =>
      setStatus(`Could not close component: ${String(error)}`, "error"),
    );
});
$("#save-component").addEventListener("click", () => {
  setStatus("Saving component draft…");
  void saveComponentDraft()
    .then((result) => {
      componentDraft = result.draft;
      componentViewerState = { kind: "current", load: result.load };
      renderComponentDesigner();
      setStatus("Component saved atomically · previous files retained as .bak");
    })
    .catch((error: unknown) =>
      setStatus(`Could not save component: ${String(error)}`, "error"),
    );
});
$("#undo-component").addEventListener("click", () => {
  void undoComponentDraft()
    .then((draft) => {
      componentDraft = draft;
      renderComponentDesigner();
      setStatus("Component draft change undone");
    })
    .catch((error: unknown) =>
      setStatus(`Could not undo: ${String(error)}`, "error"),
    );
});
$("#redo-component").addEventListener("click", () => {
  void redoComponentDraft()
    .then((draft) => {
      componentDraft = draft;
      renderComponentDesigner();
      setStatus("Component draft change redone");
    })
    .catch((error: unknown) =>
      setStatus(`Could not redo: ${String(error)}`, "error"),
    );
});
$("#component-metadata-form").addEventListener("submit", (event) => {
  event.preventDefault();
  void mutateComponentDraft({
    type: "set_metadata",
    id: ($("#draft-id") as HTMLInputElement).value,
    definition_version: ($("#draft-version") as HTMLInputElement).value,
    name: ($("#draft-name") as HTMLInputElement).value,
    description: ($("#draft-description") as HTMLTextAreaElement).value,
  });
});
$("#component-body-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const number = (id: string): number =>
    Number(($(`#${id}`) as HTMLInputElement).value);
  void mutateComponentDraft({
    type: "set_body",
    body: {
      x: number("draft-body-x"),
      y: number("draft-body-y"),
      width: number("draft-body-width"),
      height: number("draft-body-height"),
    },
  }).then(() =>
    mutateComponentDraft({
      type: "set_origin",
      x: Math.round(number("draft-origin-x")),
      y: Math.round(number("draft-origin-y")),
    }),
  );
});
$("#draft-rotations").addEventListener("change", () => {
  const rotations = [
    ...document.querySelectorAll<HTMLInputElement>(
      "#draft-rotations input:checked",
    ),
  ].map((input) => input.value as Rotation);
  void mutateComponentDraft({ type: "set_rotations", rotations });
});
function applyJsonDraftField<T>(
  textareaId: string,
  operation: (value: T) => ComponentDraftOperation,
): void {
  try {
    const value = JSON.parse(
      ($(`#${textareaId}`) as HTMLTextAreaElement).value,
    ) as T;
    void mutateComponentDraft(operation(value));
  } catch (error) {
    setStatus(`Invalid JSON: ${String(error)}`, "error");
  }
}
$("#apply-draft-occupied").addEventListener("click", () =>
  applyJsonDraftField<HolePos[]>("draft-occupied", (holes) => ({
    type: "set_occupied_holes",
    holes,
  })),
);
$("#apply-draft-keepouts").addEventListener("click", () =>
  applyJsonDraftField<LocalRect[]>("draft-keepouts", (keepouts) => ({
    type: "set_keepouts",
    keepouts,
  })),
);
$("#apply-draft-access").addEventListener("click", () =>
  applyJsonDraftField<AccessRegion[]>("draft-access", (access) => ({
    type: "set_access_regions",
    access,
  })),
);
$("#component-pin-form").addEventListener("submit", (event) => {
  event.preventDefault();
  void mutateComponentDraft({
    type: "upsert_pin",
    previous_id: selectedComponentPinId,
    pin: {
      id: ($("#draft-pin-id") as HTMLInputElement).value,
      number: ($("#draft-pin-number") as HTMLInputElement).value,
      name: ($("#draft-pin-name") as HTMLInputElement).value,
      x: Math.round(Number(($("#draft-pin-x") as HTMLInputElement).value)),
      y: Math.round(Number(($("#draft-pin-y") as HTMLInputElement).value)),
      aliases: splitList(($("#draft-pin-aliases") as HTMLInputElement).value),
      functions: splitList(
        ($("#draft-pin-functions") as HTMLInputElement).value,
      ),
    },
  });
});
$("#remove-draft-pin").addEventListener("click", () => {
  if (selectedComponentPinId)
    void mutateComponentDraft({
      type: "remove_pin",
      id: selectedComponentPinId,
    }).then(() => {
      selectedComponentPinId = undefined;
      renderComponentDesigner();
    });
});
$("#apply-draft-documentation").addEventListener("click", () => {
  void mutateComponentDraft({
    type: "set_documentation",
    documentation: ($("#draft-documentation") as HTMLTextAreaElement).value,
  });
});
$("#component-label-mode").addEventListener("change", (event) => {
  componentLabelMode = (event.target as HTMLSelectElement)
    .value as PinLabelMode;
  selectedComponentPinId = undefined;
  renderComponentDesigner();
});
for (const id of [
  "show-component-keepouts",
  "show-component-access",
  "show-component-occupied",
]) {
  $(`#${id}`).addEventListener("change", renderComponentDesigner);
}
$("#component-rotations").addEventListener("click", (event) => {
  const rotation = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-component-rotation]",
  )?.dataset.componentRotation as Rotation | undefined;
  if (!rotation) return;
  componentRotation = rotation;
  componentViewBox = undefined;
  renderComponentDesigner();
});
$("#component-pin-table").addEventListener("click", (event) => {
  const pinId = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-component-pin-row]",
  )?.dataset.componentPinRow;
  if (!pinId) return;
  selectedComponentPinId = pinId;
  renderComponentDesigner();
});
$("#component-pin-table").addEventListener("keydown", (event) => {
  const keyboard = event as KeyboardEvent;
  if (keyboard.key !== "Enter" && keyboard.key !== " ") return;
  const pinId = (keyboard.target as HTMLElement).closest<HTMLElement>(
    "[data-component-pin-row]",
  )?.dataset.componentPinRow;
  if (!pinId) return;
  keyboard.preventDefault();
  selectedComponentPinId = pinId;
  renderComponentDesigner();
});
function clearPinRowDropIndicators(): void {
  document
    .querySelectorAll("#component-pin-table tr")
    .forEach((row) =>
      row.classList.remove("dragging", "drop-before", "drop-after"),
    );
}
$("#component-pin-table").addEventListener("dragstart", (event) => {
  const drag = event as DragEvent;
  const row = (drag.target as HTMLElement).closest<HTMLTableRowElement>(
    "[data-component-pin-row]",
  );
  const pinId = row?.dataset.componentPinRow;
  if (!row || !pinId || !componentDraft) {
    drag.preventDefault();
    return;
  }
  componentPinRowDrag = { pinId };
  row.classList.add("dragging");
  drag.dataTransfer?.setData("text/plain", pinId);
  if (drag.dataTransfer) drag.dataTransfer.effectAllowed = "move";
});
$("#component-pin-table").addEventListener("dragover", (event) => {
  if (!componentPinRowDrag) return;
  const drag = event as DragEvent;
  const row = (drag.target as HTMLElement).closest<HTMLTableRowElement>(
    "[data-component-pin-row]",
  );
  if (!row || row.dataset.componentPinRow === componentPinRowDrag.pinId) return;
  drag.preventDefault();
  document
    .querySelectorAll("#component-pin-table tr")
    .forEach((candidate) =>
      candidate.classList.remove("drop-before", "drop-after"),
    );
  const afterTarget =
    drag.clientY >= row.getBoundingClientRect().top + row.offsetHeight / 2;
  row.classList.add(afterTarget ? "drop-after" : "drop-before");
  if (drag.dataTransfer) drag.dataTransfer.dropEffect = "move";
});
$("#component-pin-table").addEventListener("drop", (event) => {
  const drag = event as DragEvent;
  const row = (drag.target as HTMLElement).closest<HTMLTableRowElement>(
    "[data-component-pin-row]",
  );
  const pinId = componentPinRowDrag?.pinId;
  const targetId = row?.dataset.componentPinRow;
  if (!row || !pinId || !targetId || !componentDraft || pinId === targetId)
    return;
  drag.preventDefault();
  const afterTarget = row.classList.contains("drop-after");
  const operation = reorderPinOperation(
    componentDraft.definition.pins,
    pinId,
    targetId,
    afterTarget,
  );
  clearPinRowDropIndicators();
  componentPinRowDrag = undefined;
  if (operation) void mutateComponentDraft(operation);
});
$("#component-pin-table").addEventListener("dragend", () => {
  clearPinRowDropIndicators();
  componentPinRowDrag = undefined;
});
$("#component-canvas").addEventListener("click", (event) => {
  const pinId = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-preview-pin-id]",
  )?.dataset.previewPinId;
  if (!pinId) return;
  selectedComponentPinId = pinId;
  renderComponentDesigner();
});
$("#component-canvas").addEventListener("pointerdown", (event) => {
  const pointer = event as PointerEvent;
  const pinElement = (pointer.target as Element).closest<SVGGraphicsElement>(
    "[data-preview-pin-id]",
  );
  const pinId = pinElement?.dataset.previewPinId;
  if (!pinId || !componentDraft || pointer.button !== 0) return;
  const pin = componentDraft.definition.pins.find((item) => item.id === pinId);
  if (!pin || !pinElement) return;
  selectedComponentPinId = pinId;
  const label = Array.from(
    document.querySelectorAll<SVGTextElement>("[data-preview-pin-label-id]"),
  ).find((item) => item.dataset.previewPinLabelId === pinId);
  componentPinDrag = {
    pinId,
    pointerId: pointer.pointerId,
    initialX: pin.x,
    initialY: pin.y,
    x: pin.x,
    y: pin.y,
    element: pinElement,
    label,
  };
  pinElement.classList.add("dragging");
  (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
});
$("#component-canvas").addEventListener("pointermove", (event) => {
  const pointer = event as PointerEvent;
  if (!componentPinDrag || pointer.pointerId !== componentPinDrag.pointerId)
    return;
  const svg = document.querySelector<SVGSVGElement>("#component-preview");
  const matrix = svg?.getScreenCTM()?.inverse();
  if (!svg || !matrix) return;
  const point = svg.createSVGPoint();
  point.x = pointer.clientX;
  point.y = pointer.clientY;
  const rotated = point.matrixTransform(matrix);
  const [x, y] =
    componentRotation === "deg0"
      ? [rotated.x, rotated.y]
      : componentRotation === "deg90"
        ? [rotated.y, -rotated.x]
        : componentRotation === "deg180"
          ? [-rotated.x, -rotated.y]
          : [-rotated.y, rotated.x];
  componentPinDrag.x = Math.round(x);
  componentPinDrag.y = Math.round(y);
  componentPinDrag.element.setAttribute(
    "transform",
    `translate(${componentPinDrag.x - componentPinDrag.initialX} ${componentPinDrag.y - componentPinDrag.initialY})`,
  );
  if (componentPinDrag.label) {
    const placement = pinLabelPlacement(
      componentRotation,
      componentPinDrag.x,
      componentPinDrag.y,
    );
    componentPinDrag.label.setAttribute("x", String(placement.x));
    componentPinDrag.label.setAttribute("y", String(placement.y));
    componentPinDrag.label.setAttribute("text-anchor", placement.textAnchor);
  }
  setStatus(
    `Move pin ${componentPinDrag.pinId} to (${componentPinDrag.x}, ${componentPinDrag.y})`,
  );
});
$("#component-zoom-in").addEventListener("click", () => zoomComponent(0.8));
$("#component-zoom-out").addEventListener("click", () => zoomComponent(1.25));
$("#component-fit").addEventListener("click", () => {
  componentViewBox = undefined;
  renderComponentDesigner();
});
$("#component-canvas").addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    zoomComponent((event as WheelEvent).deltaY > 0 ? 1.12 : 0.89);
  },
  { passive: false },
);
$("#component-canvas").addEventListener("pointerdown", (event) => {
  const pointer = event as PointerEvent;
  if (
    !pointer.isPrimary ||
    pointer.button !== 0 ||
    (pointer.target as Element).closest("[data-preview-pin-id]")
  )
    return;
  const svg = document.querySelector<SVGSVGElement>("#component-preview");
  if (!svg) return;
  const values = (svg.getAttribute("viewBox") ?? "-2 -2 10 10")
    .split(" ")
    .map(Number) as [number, number, number, number];
  componentPanStart = {
    x: pointer.clientX,
    y: pointer.clientY,
    viewBox: values,
  };
  (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
});
$("#component-canvas").addEventListener("pointermove", (event) => {
  if (!componentPanStart) return;
  const pointer = event as PointerEvent;
  const svg = document.querySelector<SVGSVGElement>("#component-preview");
  if (!svg) return;
  const bounds = svg.getBoundingClientRect();
  const dx =
    ((pointer.clientX - componentPanStart.x) * componentPanStart.viewBox[2]) /
    bounds.width;
  const dy =
    ((pointer.clientY - componentPanStart.y) * componentPanStart.viewBox[3]) /
    bounds.height;
  componentViewBox = [
    componentPanStart.viewBox[0] - dx,
    componentPanStart.viewBox[1] - dy,
    componentPanStart.viewBox[2],
    componentPanStart.viewBox[3],
  ];
  svg.setAttribute("viewBox", componentViewBox.join(" "));
});
for (const eventName of ["pointerup", "pointercancel"]) {
  $("#component-canvas").addEventListener(eventName, () => {
    if (componentPinDrag) {
      const drag = componentPinDrag;
      drag.element.removeAttribute("transform");
      drag.element.classList.remove("dragging");
      if (drag.label) {
        const placement = pinLabelPlacement(
          componentRotation,
          drag.initialX,
          drag.initialY,
        );
        drag.label.setAttribute("x", String(placement.x));
        drag.label.setAttribute("y", String(placement.y));
        drag.label.setAttribute("text-anchor", placement.textAnchor);
      }
      if (eventName === "pointerup" && componentDraft) {
        const pin = componentDraft.definition.pins.find(
          (item) => item.id === drag.pinId,
        );
        if (pin && (pin.x !== drag.x || pin.y !== drag.y))
          void mutateComponentDraft(movePinOperation(pin, drag.x, drag.y));
      }
    }
    componentPinDrag = undefined;
    componentPanStart = undefined;
  });
}
window.addEventListener("keydown", (event) => {
  if (appMode !== "component" || !componentDraft) return;
  const modifier = event.ctrlKey || event.metaKey;
  if (!modifier) return;
  if (event.key.toLowerCase() === "s") {
    event.preventDefault();
    ($("#save-component") as HTMLButtonElement).click();
  } else if (event.key.toLowerCase() === "z" && event.shiftKey) {
    event.preventDefault();
    ($("#redo-component") as HTMLButtonElement).click();
  } else if (event.key.toLowerCase() === "z") {
    event.preventDefault();
    ($("#undo-component") as HTMLButtonElement).click();
  }
});

$("#app").addEventListener("click", (event) => {
  if (suppressClick) {
    suppressClick = false;
    return;
  }
  const target = event.target as HTMLElement;
  if (target.id === "accept-route" && routeProposal) {
    const proposal = routeProposal;
    routeProposal = undefined;
    void mutate({
      type: "accept_route_proposal",
      proposal,
      wire_id: crypto.randomUUID(),
      color: ($("#wire-color") as HTMLSelectElement).value as WireColor,
      notes: "Accepted automatic route",
      new_route_over: proposal.crossings.length
        ? ($("#crossing-order") as HTMLSelectElement).value === "over"
        : undefined,
    });
    return;
  }
  if (target.id === "reject-route") {
    routeProposal = undefined;
    render();
    setStatus("Route proposal rejected; accepted wires were not changed");
    return;
  }
  const removeJumper = target.closest<HTMLElement>("[data-remove-jumper]")
    ?.dataset.removeJumper;
  if (removeJumper) {
    void mutate({
      type: "remove_jumper_inventory",
      inventory_id: removeJumper,
    });
    return;
  }
  const removeConstraint = target.closest<HTMLElement>(
    "[data-remove-constraint]",
  )?.dataset.removeConstraint;
  if (removeConstraint) {
    void mutate({ type: "remove_constraint", constraint_id: removeConstraint });
    return;
  }
  const editConstraint = target.closest<HTMLElement>("[data-edit-constraint]")
    ?.dataset.editConstraint;
  if (editConstraint) {
    const constraint = project.design.constraints?.[editConstraint];
    if (constraint) {
      ($("#constraint-json") as HTMLTextAreaElement).value = JSON.stringify(
        constraint,
        null,
        2,
      );
      ($("#save-constraint") as HTMLButtonElement).textContent =
        "Update constraint";
    }
    return;
  }
  const accept =
    target.closest<HTMLElement>("[data-accept-net]")?.dataset.acceptNet;
  if (accept) {
    void mutate({ type: "accept_net_proposal", net_id: accept });
    return;
  }
  const reject =
    target.closest<HTMLElement>("[data-reject-net]")?.dataset.rejectNet;
  if (reject) {
    void mutate({ type: "reject_net_proposal", net_id: reject });
    return;
  }
  const removeNet =
    target.closest<HTMLElement>("[data-remove-net]")?.dataset.removeNet;
  if (removeNet) {
    void mutate({ type: "remove_net", net_id: removeNet });
    return;
  }
  const selectedWire =
    target.closest<HTMLElement>("[data-wire-id]")?.dataset.wireId;
  if (selectedWire) {
    selectedId = selectedWire;
    render();
    return;
  }
  const pin = target.closest<HTMLElement>("[data-pin-id]");
  const pinComponent = pin?.closest<HTMLElement>("[data-component-id]")?.dataset
    .componentId;
  if (pin?.dataset.pinId && pinComponent) {
    if (wireMode) {
      void handleWirePin(pinComponent, pin.dataset.pinId);
      return;
    }
    if (!activeNetId) {
      setStatus("Create and select a net before assigning pins", "error");
      return;
    }
    const ref = { component_id: pinComponent, pin_id: pin.dataset.pinId };
    const net = project.design.nets?.[activeNetId];
    if (!net) return;
    const assigned = net.pins?.some(
      (item) =>
        item.component_id === ref.component_id && item.pin_id === ref.pin_id,
    );
    void mutate({
      type: assigned ? "unassign_pin" : "assign_pin",
      net_id: activeNetId,
      pin: ref,
    });
    return;
  }
  const component = target.closest<HTMLElement>(
    "[data-component-id], [data-select-id]",
  );
  if (component) {
    selectedId = component.dataset.componentId ?? component.dataset.selectId;
    render();
    return;
  }
  if (target.id === "add-library-component") {
    const definition = library.find(
      (item) => item.id === selectedLibraryDefinitionId,
    );
    if (!definition) return;
    const count = Object.keys(project.design.components ?? {}).length;
    const instanceId = crypto.randomUUID();
    selectedId = instanceId;
    void mutate({
      type: "add_component",
      definition,
      instance: {
        id: instanceId,
        definition_id: definition.id,
        name: `${definition.name} ${count + 1}`,
        position: { x: 2 + count * 8, y: 2 },
        rotation: "deg0",
        properties: {},
        notes: "",
      },
    });
  }
});
$("#library").addEventListener("change", (event) => {
  const target = event.target as HTMLSelectElement;
  if (target.id !== "component-library-select") return;
  selectedLibraryDefinitionId = target.value;
  const definition = library.find((item) => item.id === target.value);
  $("#component-library-description").textContent =
    definition?.description ?? "";
});
$("#canvas").addEventListener("pointerdown", (event) => {
  const pointer = event as PointerEvent;
  if (!pointer.isPrimary || pointer.button !== 0 || drag || boardResize) return;
  const resizeHandle = (pointer.target as Element).closest<SVGElement>(
    "[data-resize-handle]",
  )?.dataset.resizeHandle as ResizeHandle | undefined;
  if (resizeMode && resizeHandle) {
    const point = svgVisualPoint(pointer);
    const boundary = $(".board-resize-boundary") as SVGRectElement;
    if (!point) return;
    boardResize = {
      pointerId: pointer.pointerId,
      handle: resizeHandle,
      pointerStart: point,
      plan: {
        holesX: project.design.board.holes_x,
        holesY: project.design.board.holes_y,
        offsetX: 0,
        offsetY: 0,
      },
      boundary,
    };
    (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
    pointer.preventDefault();
    return;
  }
  if (resizeMode) return;
  if (wireMode && (pointer.target as Element).closest("[data-pin-id]")) return;
  if (
    wireMode &&
    wireDraft &&
    !(pointer.target as Element).closest("[data-pin-id]")
  ) {
    const point = svgPoint(pointer);
    if (point) {
      appendRoutable({
        x2: Math.round(point.x) * 2,
        y2: Math.round(point.y) * 2,
      });
      setStatus(`${wireDraft.vertices.length - 1} bend point(s)`);
    }
    return;
  }
  const component = (pointer.target as Element).closest<HTMLElement>(
    "[data-component-id]",
  );
  if (component?.dataset.componentId) {
    const point = svgPoint(pointer);
    const instance = project.design.components?.[component.dataset.componentId];
    if (!point || !instance) return;
    drag = {
      id: instance.id,
      pointerId: pointer.pointerId,
      pointerStart: point,
      initialPosition: { ...instance.position },
      position: { ...instance.position },
      element: component as unknown as SVGGraphicsElement,
    };
    selectedId = instance.id;
    document
      .querySelectorAll(".component.selected")
      .forEach((item) => item.classList.remove("selected"));
    component.classList.add("selected", "dragging");
    pointer.preventDefault();
  } else {
    const svg = $("#board") as SVGSVGElement;
    const values = (svg.getAttribute("viewBox") ?? "0 0 25 18")
      .split(" ")
      .map(Number) as [number, number, number, number];
    panStart = { x: pointer.clientX, y: pointer.clientY, viewBox: values };
  }
  (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
});
$("#canvas").addEventListener("pointermove", (event) => {
  const pointer = event as PointerEvent;
  if (boardResize) {
    if (pointer.pointerId !== boardResize.pointerId) return;
    const point = svgVisualPoint(pointer);
    if (!point) return;
    const deltaX = Math.round(point.x - boardResize.pointerStart.x);
    const deltaY = Math.round(point.y - boardResize.pointerStart.y);
    const plan = planBoardResize(
      {
        holesX: project.design.board.holes_x,
        holesY: project.design.board.holes_y,
      },
      boardResize.handle,
      deltaX,
      deltaY,
      viewSide === "solder",
    );
    if (!plan) return;
    boardResize.plan = plan;
    const movesLeft = boardResize.handle.includes("left");
    const movesRight = boardResize.handle.includes("right");
    const movesTop = boardResize.handle.includes("top");
    const movesBottom = boardResize.handle.includes("bottom");
    boardResize.boundary.setAttribute("x", String(movesLeft ? deltaX : 0));
    boardResize.boundary.setAttribute("y", String(movesTop ? deltaY : 0));
    boardResize.boundary.setAttribute(
      "width",
      String(
        project.design.board.holes_x -
          1 +
          (movesRight ? deltaX : 0) -
          (movesLeft ? deltaX : 0),
      ),
    );
    boardResize.boundary.setAttribute(
      "height",
      String(
        project.design.board.holes_y -
          1 +
          (movesBottom ? deltaY : 0) -
          (movesTop ? deltaY : 0),
      ),
    );
    const boundaryX = Number(boardResize.boundary.getAttribute("x"));
    const boundaryY = Number(boardResize.boundary.getAttribute("y"));
    const boundaryWidth = Number(boardResize.boundary.getAttribute("width"));
    const boundaryHeight = Number(boardResize.boundary.getAttribute("height"));
    const feedback = $(".board-resize-feedback") as SVGTextElement;
    const fullHoles = fullyEnclosedHoleDimensions(plan);
    feedback.setAttribute("x", String(boundaryX + boundaryWidth / 2));
    feedback.setAttribute("y", String(boundaryY + boundaryHeight / 2));
    feedback.textContent = `${fullHoles.holesX} full holes wide × ${fullHoles.holesY} tall`;
    setStatus(
      `Resize to ${fullHoles.holesX} full holes wide × ${fullHoles.holesY} tall`,
    );
    return;
  }
  if (drag) {
    if (pointer.pointerId !== drag.pointerId) return;
    const point = svgPoint(pointer);
    if (!point) return;
    drag.position = snappedDragPosition(
      drag.initialPosition,
      drag.pointerStart,
      point,
    );
    const rotation = drag.element.dataset.rotationDegrees ?? "0";
    drag.element.setAttribute(
      "transform",
      `translate(${drag.position.x} ${drag.position.y}) rotate(${rotation})`,
    );
    setStatus(`Move to ${drag.position.x}, ${drag.position.y}`);
    return;
  }
  if (!panStart) return;
  const svg = $("#board") as SVGSVGElement;
  const rect = svg.getBoundingClientRect();
  const dx =
    ((pointer.clientX - panStart.x) * panStart.viewBox[2]) / rect.width;
  const dy =
    ((pointer.clientY - panStart.y) * panStart.viewBox[3]) / rect.height;
  viewBox = [
    panStart.viewBox[0] - dx,
    panStart.viewBox[1] - dy,
    panStart.viewBox[2],
    panStart.viewBox[3],
  ];
  svg.setAttribute("viewBox", viewBox.join(" "));
});
$("#canvas").addEventListener("pointerup", (event) => {
  if (boardResize) {
    if ((event as PointerEvent).pointerId !== boardResize.pointerId) return;
    const { plan } = boardResize;
    boardResize = undefined;
    if (
      plan.holesX !== project.design.board.holes_x ||
      plan.holesY !== project.design.board.holes_y
    ) {
      void mutate({
        type: "resize_board",
        holes_x: plan.holesX,
        holes_y: plan.holesY,
        offset_x: plan.offsetX,
        offset_y: plan.offsetY,
      });
      render();
    } else {
      render();
    }
    return;
  }
  if (drag) {
    if ((event as PointerEvent).pointerId !== drag.pointerId) return;
    const finished = drag;
    drag = undefined;
    finished.element.classList.remove("dragging");
    const moved =
      finished.position.x !== finished.initialPosition.x ||
      finished.position.y !== finished.initialPosition.y;
    suppressClick = moved;
    if (moved) window.setTimeout(() => (suppressClick = false), 0);
    if (moved)
      void mutate({
        type: "move_component",
        instance_id: finished.id,
        position: finished.position,
      });
  }
  panStart = undefined;
});
$("#canvas").addEventListener("pointercancel", (event) => {
  if (boardResize) {
    if ((event as PointerEvent).pointerId !== boardResize.pointerId) return;
    boardResize = undefined;
    render();
    return;
  }
  if (drag) {
    if ((event as PointerEvent).pointerId !== drag.pointerId) return;
    const rotation = drag.element.dataset.rotationDegrees ?? "0";
    drag.element.setAttribute(
      "transform",
      `translate(${drag.initialPosition.x} ${drag.initialPosition.y}) rotate(${rotation})`,
    );
    drag.element.classList.remove("dragging");
    drag = undefined;
  }
  panStart = undefined;
});
$("#inspector").addEventListener("click", (event) => {
  const instance = selected();
  const target = event.target as HTMLElement;
  const wire = selectedId ? project.design.wires?.[selectedId] : undefined;
  if (wire) {
    const removeIndex = target.closest<HTMLElement>("[data-remove-vertex]")
      ?.dataset.removeVertex;
    if (target.id === "reroute-wire") {
      void rerouteWires([
        { wireId: wire.id, start: wire.start, end: wire.end },
      ]);
    } else if (target.id === "evaluate-wire") {
      void evaluateProjectRoute(
        {
          net_id: wire.net_id,
          start: wire.start,
          end: wire.end,
          preferred_side: wire.side,
          replace_wire_id: wire.id,
        },
        wire.side,
        wire.vertices,
      )
        .then((evaluation) => {
          const warning =
            evaluation.warnings?.join(" · ") || "No soft warnings";
          setStatus(
            `Wire cost ${evaluation.cost.total} · ${evaluation.length_mm.toFixed(2)} mm · ${warning}`,
          );
        })
        .catch((error: unknown) => setStatus(String(error), "error"));
    } else if (removeIndex) {
      const vertices = wire.vertices.filter(
        (_, index) => index !== Number(removeIndex),
      );
      void mutate({ type: "modify_wire_path", wire_id: wire.id, vertices });
    } else if (target.id === "toggle-wire-lock") {
      void mutate({
        type: "set_wire_lock",
        wire_id: wire.id,
        locked: !wire.locked,
      });
    } else if (target.id === "remove-wire") {
      selectedId = undefined;
      void mutate({ type: "remove_wire", wire_id: wire.id });
    }
    return;
  }
  if (!instance) return;
  if (target.id === "rotate-component")
    void mutate({
      type: "rotate_component",
      instance_id: instance.id,
      rotation: nextRotation(instance.rotation),
    });
  if (target.id === "remove-component") {
    selectedId = undefined;
    void mutate({ type: "remove_component", instance_id: instance.id });
  }
});
$("#inspector").addEventListener("change", (event) => {
  const instance = selected();
  const wire = selectedId ? project.design.wires?.[selectedId] : undefined;
  if (wire) {
    const target = event.target as HTMLElement;
    if (target.id === "selected-wire-side") {
      void mutate({
        type: "set_wire_side",
        wire_id: wire.id,
        side: (target as HTMLSelectElement).value as Side,
      });
      return;
    }
    if (target.id === "selected-wire-color") {
      void mutate({
        type: "set_wire_color",
        wire_id: wire.id,
        color: (target as HTMLSelectElement).value as WireColor,
      });
      return;
    }
    const vertices = wire.vertices.map((point, index) => ({
      x2: Number(
        (
          document.querySelector(
            `[data-vertex-x="${index}"]`,
          ) as HTMLInputElement
        )?.value ?? point.x2,
      ),
      y2: Number(
        (
          document.querySelector(
            `[data-vertex-y="${index}"]`,
          ) as HTMLInputElement
        )?.value ?? point.y2,
      ),
    }));
    void mutate({ type: "modify_wire_path", wire_id: wire.id, vertices });
    return;
  }
  if (!instance) return;
  const name = $("#instance-name") as HTMLInputElement;
  const notes = $("#instance-notes") as HTMLTextAreaElement;
  void mutate({
    type: "change_instance_properties",
    instance_id: instance.id,
    name: name.value,
    notes: notes.value,
    properties: instance.properties ?? {},
  });
});
$("#new-project").addEventListener("click", () => {
  const dialog = $("#new-board-dialog") as HTMLDialogElement;
  const holesX = $("#board-holes-x") as HTMLInputElement;
  const holesY = $("#board-holes-y") as HTMLInputElement;
  holesX.value = String(project.design.board.holes_x);
  holesY.value = String(project.design.board.holes_y);
  $("#new-board-error").textContent = "";
  dialog.showModal();
  holesX.select();
});
$("#cancel-new-board").addEventListener("click", () => {
  ($("#new-board-dialog") as HTMLDialogElement).close();
});
$("#new-board-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const dialog = $("#new-board-dialog") as HTMLDialogElement;
  const error = $("#new-board-error");
  const dimensions = parseBoardDimensions(
    ($("#board-holes-x") as HTMLInputElement).value,
    ($("#board-holes-y") as HTMLInputElement).value,
  );
  if (!dimensions) {
    error.textContent = "Width and height must be positive whole numbers.";
    return;
  }
  error.textContent = "";
  void createProject("Untitled project", dimensions.holesX, dimensions.holesY)
    .then((createdProject) => {
      project = createdProject;
      comparison = {
        components: [],
        nets: [],
        wires: [],
        constraints: [],
        routes: [],
      };
      selectedId = undefined;
      activeNetId = undefined;
      wireMode = false;
      wireDraft = undefined;
      routeProposal = undefined;
      viewBox = undefined;
      dialog.close();
      render();
      setStatus(`Created ${dimensions.holesX} × ${dimensions.holesY} board`);
    })
    .catch((createError: unknown) => {
      error.textContent = String(createError);
    });
});
$("#save-project").addEventListener("click", async () => {
  await saveProject(($("#project-path") as HTMLInputElement).value);
  setStatus("Project saved");
});
$("#open-project").addEventListener("click", async () => {
  project = await openProject(($("#project-path") as HTMLInputElement).value);
  selectedId = undefined;
  viewBox = undefined;
  render();
  setStatus("Project reopened");
});
async function runRevisionAction(
  action: () => Promise<unknown>,
): Promise<void> {
  try {
    await action();
    await refresh();
  } catch (error) {
    setStatus(String(error), "error");
  }
}
$("#undo-project").addEventListener("click", () => {
  void runRevisionAction(() => undoProject(project.revision));
});
$("#redo-project").addEventListener("click", () => {
  void runRevisionAction(() => redoProject(project.revision));
});
$("#discard-project").addEventListener("click", () => {
  void runRevisionAction(() => discardProjectChanges(project.revision));
});
$("#commit-project").addEventListener("click", () => {
  const name = ($("#commit-name") as HTMLInputElement).value.trim();
  const description = ($("#commit-description") as HTMLTextAreaElement).value;
  if (!name) {
    setStatus("Enter a commit name", "error");
    return;
  }
  void runRevisionAction(async () => {
    await commitProject(project.revision, name, description);
    ($("#commit-name") as HTMLInputElement).value = "";
    ($("#commit-description") as HTMLTextAreaElement).value = "";
  });
});
$("#revision-history").addEventListener("click", (event) => {
  const target = event.target as HTMLElement;
  const compareId = target.closest<HTMLElement>("[data-compare-commit]")
    ?.dataset.compareCommit;
  if (compareId) {
    void compareProjectRevisions(compareId).then((diff) => {
      comparison = diff;
      render();
      setStatus(
        `Comparing commit ${compareId.slice(0, 8)} to the working copy`,
      );
    });
    return;
  }
  const restoreId = target.closest<HTMLElement>("[data-restore-commit]")
    ?.dataset.restoreCommit;
  if (restoreId) {
    void runRevisionAction(() =>
      restoreProjectCommit(project.revision, restoreId),
    );
  }
});
function zoom(factor: number): void {
  const svg = $("#board") as SVGSVGElement;
  const current = (
    svg.getAttribute("viewBox") ??
    svg.dataset.defaultViewBox ??
    "-2 -2 29 22"
  )
    .split(" ")
    .map(Number) as [number, number, number, number];
  const [x, y, width, height] = current;
  const nextWidth = width * factor;
  const nextHeight = height * factor;
  viewBox = [
    x + (width - nextWidth) / 2,
    y + (height - nextHeight) / 2,
    nextWidth,
    nextHeight,
  ];
  svg.setAttribute("viewBox", viewBox.join(" "));
}
function scheduleWheelZoom(factor: number): void {
  pendingWheelZoom *= factor;
  if (wheelZoomFrame !== undefined) return;
  wheelZoomFrame = window.requestAnimationFrame(() => {
    const accumulatedFactor = pendingWheelZoom;
    pendingWheelZoom = 1;
    wheelZoomFrame = undefined;
    zoom(accumulatedFactor);
  });
}
$("#zoom-in").addEventListener("click", () => zoom(0.8));
$("#zoom-out").addEventListener("click", () => zoom(1.25));
$("#zoom-reset").addEventListener("click", () => {
  viewBox = undefined;
  render();
});
$("#view-side").addEventListener("click", () => {
  viewSide = viewSide === "component" ? "solder" : "component";
  render();
});
$("#active-net").addEventListener("change", (event) => {
  activeNetId = (event.target as HTMLSelectElement).value;
  wireDraft = undefined;
  render();
});
function addNet(state: "accepted" | "proposed"): void {
  const input = $("#net-name") as HTMLInputElement;
  const name = input.value.trim();
  if (!name) {
    setStatus("Enter a net name", "error");
    return;
  }
  const id = crypto.randomUUID();
  activeNetId = id;
  input.value = "";
  void mutate({
    type: "define_net",
    net: { id, name, state, pins: [], notes: "" },
  });
}
$("#add-net").addEventListener("click", () => addNet("accepted"));
$("#propose-net").addEventListener("click", () => addNet("proposed"));
$("#wire-mode").addEventListener("click", () => {
  resizeMode = false;
  wireMode = true;
  wireDraft = undefined;
  setStatus("Click the starting pin for the active net");
  render();
});
$("#resize-board").addEventListener("click", () => {
  resizeMode = !resizeMode;
  wireMode = false;
  wireDraft = undefined;
  render();
  setStatus(
    resizeMode
      ? "Drag a board side or corner; resizing snaps to one-hole increments"
      : `Revision ${project.revision} · ${project.design.board.holes_x} × ${project.design.board.holes_y} holes`,
  );
});
$("#cancel-wire").addEventListener("click", () => {
  wireMode = false;
  wireDraft = undefined;
  render();
});
$("#reroute-all").addEventListener("click", () => {
  void rerouteWires(planWireReroutes(project));
});
$("#suggest-route").addEventListener("click", () => {
  const net = activeNetId ? project.design.nets?.[activeNetId] : undefined;
  if (!net || net.state !== "accepted" || (net.pins?.length ?? 0) < 2) {
    setStatus("Choose an accepted net with at least two pins", "error");
    return;
  }
  const [start, end] = net.pins!;
  void suggestProjectRoute({
    net_id: net.id,
    start: {
      kind: "pin",
      component_id: start!.component_id,
      pin_id: start!.pin_id,
    },
    end: { kind: "pin", component_id: end!.component_id, pin_id: end!.pin_id },
    preferred_side: viewSide,
  })
    .then((proposal) => {
      routeProposal = proposal;
      render();
      setStatus(
        "Route proposal ready; review its cost and warnings before accepting",
      );
    })
    .catch((error: unknown) => setStatus(String(error), "error"));
});
$("#save-jumper").addEventListener("click", () => {
  const id = ($("#jumper-id") as HTMLInputElement).value.trim();
  const color = ($("#jumper-color") as HTMLInputElement).value.trim();
  if (!id || !color) {
    setStatus("Jumper ID and color are required", "error");
    return;
  }
  void mutate({
    type: "upsert_jumper_inventory",
    entry: {
      id,
      span_holes: Number(($("#jumper-span") as HTMLInputElement).value),
      tolerance_holes: Number(
        ($("#jumper-tolerance") as HTMLInputElement).value,
      ),
      color,
    },
  });
});
$("#save-constraint").addEventListener("click", () => {
  try {
    const parsed = JSON.parse(
      ($("#constraint-json") as HTMLTextAreaElement).value,
    ) as import("./api/project").Constraint;
    const exists = parsed.id && project.design.constraints?.[parsed.id];
    parsed.id ||= crypto.randomUUID();
    void mutate({
      type: exists ? "update_constraint" : "add_constraint",
      constraint: parsed,
    });
    ($("#save-constraint") as HTMLButtonElement).textContent = "Add constraint";
  } catch (error) {
    setStatus(`Invalid constraint JSON: ${String(error)}`, "error");
  }
});
$("#canvas").addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    scheduleWheelZoom((event as WheelEvent).deltaY > 0 ? 1.12 : 0.89);
  },
  { passive: false },
);
async function initialize(): Promise<void> {
  let changedDuringStartup = false;
  await listen<ComponentViewerSnapshot>(
    "component-viewer-changed",
    async (event) => {
      const snapshot = event.payload;
      if (!isActiveViewerSnapshot(componentViewerGeneration, snapshot)) return;
      applyComponentSnapshot(snapshot);
      if (snapshot.load?.definition && !componentDraft?.dirty) {
        await reloadRuntimeComponent();
        componentDraft = (await getComponentDraft()) ?? undefined;
      }
      renderComponentDesigner();
      if (snapshot.watcher_status === "error") {
        setStatus(
          `Component watcher unavailable: ${snapshot.watcher_error ?? "unknown error"}`,
          "error",
        );
        return;
      }
      const load = snapshot.load;
      if (!load) return;
      setStatus(
        componentDraft?.dirty
          ? "Component changed on disk while this draft has unsaved changes · save will require conflict resolution"
          : load.definition
            ? `Updated ${load.definition.name} from disk`
            : displayedComponentLoad()?.definition
              ? "External edit is invalid · showing the last valid preview"
              : "External component edit is invalid",
        load.definition && !componentDraft?.dirty ? "ready" : "error",
      );
    },
  );
  await listen<{ revision: number }>("project-changed", (event) => {
    if (!projectReady) {
      changedDuringStartup = true;
      return;
    }
    if (event.payload.revision > project.revision) {
      void requestRefresh().catch((error: unknown) =>
        setStatus(
          `Could not refresh external change: ${String(error)}`,
          "error",
        ),
      );
    }
  });
  const [info, loadedProject, loadedLibrary] = await Promise.all([
    getBootstrapInfo(),
    getProject(),
    getComponentLibrary(),
  ]);
  project = loadedProject;
  library = loadedLibrary;
  comparison = project.head_commit
    ? await compareProjectRevisions(project.head_commit)
    : { components: [], nets: [], wires: [], constraints: [], routes: [] };
  projectReady = true;
  render();
  renderComponentDesigner();
  setStatus(
    appMode === "component"
      ? "Component Designer · graphical authoring"
      : `Rust engine ready · schema v${info.schema_version}`,
  );
  if (changedDuringStartup) await requestRefresh();
  await refreshIfChanged();
}

void initialize().catch((error: unknown) =>
  setStatus(`The desktop bridge is unavailable: ${String(error)}`, "error"),
);
window.addEventListener("focus", () => {
  void refreshIfChanged().catch((error: unknown) =>
    console.error("Could not check for external project changes", error),
  );
});
window.setInterval(() => {
  void refreshIfChanged().catch((error: unknown) =>
    console.error("Could not check for external project changes", error),
  );
}, 1_000);
