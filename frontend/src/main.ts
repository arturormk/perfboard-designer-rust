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
  trimProjectUndoHistory,
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
  type Rail,
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
import { errorMessage } from "./error-message";
import { renderBoardSvg, renderPrintLayout } from "./editor/renderer";
import { snappedDragPosition } from "./editor/drag";
import {
  fullyEnclosedHoleDimensions,
  parseBoardDimensions,
  planBoardResize,
  type BoardResizePlan,
  type ResizeHandle,
} from "./editor/board-size";
import { planWireReroutes, type WireRerouteTarget } from "./editor/net-routing";
import { newComponentStagingPosition } from "./editor/component-placement";
import { netPinLabel } from "./editor/net-pins";
import {
  appendOrthogonal,
  closestWireSegmentIndex,
  finishWireNodeDrag,
  insertWireNode,
  isStraightRoute,
  orthogonalRoute,
  snappedRoutePoint,
  straightRoute,
} from "./editor/wire-routing";
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
import {
  bodyGridRect,
  keepoutGridRect,
  planBodyResize,
  planHoleRegionResize,
  planKeepoutResize,
  type GridRect,
} from "./component-designer/geometry-resize";

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
  rails: [],
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
let componentsFaded = false;
let activeNetId: string | undefined;
let wireMode = false;
let railMode = false;
let resizeMode = false;
let routeProposal: RouteProposal | undefined;
let rerouting = false;
let wireDraft:
  | { start: TerminalRef; vertices: Array<{ x2: number; y2: number }> }
  | undefined;
let railDraft: Array<{ x2: number; y2: number }> | undefined;
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
let wireNodeDrag:
  | {
      wireId: string;
      vertexIndex: number;
      pointerId: number;
      vertices: Array<{ x2: number; y2: number }>;
      position: { x2: number; y2: number };
      element: SVGCircleElement;
      endpoint?: "start" | "end";
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
const wireSegmentLongPressDelayMs = 500;
const wireSegmentLongPressMovementPx = 8;
let wireSegmentPress:
  | {
      pointerId: number;
      clientX: number;
      clientY: number;
      point: { x: number; y: number };
      wireId: string;
      segmentIndex: number;
      viewBox: [number, number, number, number];
      wireGroup: SVGGElement;
      timer: number | undefined;
      triggered: boolean;
    }
  | undefined;
type AppMode = "perfboard" | "component";
let appMode: AppMode = "perfboard";
type DesignView = "working-copy" | "committed";
let designView: DesignView = "working-copy";
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
let componentResizeMode: "body" | "keepout" | "access" | undefined;
let componentGeometryResize:
  | {
      pointerId: number;
      handle: ResizeHandle;
      kind: "body" | "keepout" | "access";
      index?: number;
      bounds: GridRect;
      initialBody?: ComponentDefinition["body"];
      body?: ComponentDefinition["body"];
      initialKeepout?: LocalRect;
      keepout?: LocalRect;
      initialAccess?: AccessRegion;
      access?: AccessRegion;
      boundary: SVGRectElement;
      feedback: SVGTextElement;
    }
  | undefined;
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
function objectSide(objectId: string): Side | undefined {
  const wire = project.design.wires?.[objectId];
  if (wire) return wire.side;
  const rail = project.design.rails?.[objectId];
  if (rail) return rail.side;
  const instance = project.design.components?.[objectId];
  const definition =
    instance && project.design.component_definitions?.[instance.definition_id];
  if (!instance) return undefined;
  return definition?.kind === "bridge" ? "solder" : "component";
}
function selectVisibleObject(objectId: string): void {
  const side = objectSide(objectId);
  if (side && side !== viewSide) {
    setStatus(`Switch to the ${side} side to select this item`, "error");
    return;
  }
  selectedId = objectId;
  render();
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
    setStatus(
      `Could not update component draft: ${errorMessage(error)}`,
      "error",
    );
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

function setDesignView(view: DesignView, moveFocus = false): void {
  designView = view;
  const workingCopyTab = $("#working-copy-tab") as HTMLButtonElement;
  const committedTab = $("#committed-tab") as HTMLButtonElement;
  const workingCopyPane = $("#working-copy-pane");
  const committedPane = $("#committed-pane");
  const showWorkingCopy = view === "working-copy";

  workingCopyPane.toggleAttribute("hidden", !showWorkingCopy);
  committedPane.toggleAttribute("hidden", showWorkingCopy);
  workingCopyTab.classList.toggle("active", showWorkingCopy);
  committedTab.classList.toggle("active", !showWorkingCopy);
  workingCopyTab.setAttribute("aria-selected", String(showWorkingCopy));
  committedTab.setAttribute("aria-selected", String(!showWorkingCopy));
  workingCopyTab.tabIndex = showWorkingCopy ? 0 : -1;
  committedTab.tabIndex = showWorkingCopy ? -1 : 0;

  if (moveFocus) (showWorkingCopy ? workingCopyTab : committedTab).focus();
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
    "show-component-wire-keepouts",
    "show-component-access",
    "show-component-occupied",
  ]) {
    (
      $(`#${id}`) as HTMLButtonElement | HTMLInputElement | HTMLSelectElement
    ).disabled = !activeComponentDefinition() || loading;
  }
  for (const id of [
    "resize-component-body",
    "resize-component-keepouts",
    "resize-component-access",
  ]) {
    ($(`#${id}`) as HTMLButtonElement).disabled = !componentDraft || loading;
  }
  const resizeBody = $("#resize-component-body");
  const resizeKeepouts = $("#resize-component-keepouts");
  const resizeAccess = $("#resize-component-access");
  resizeBody.classList.toggle("active", componentResizeMode === "body");
  resizeKeepouts.classList.toggle("active", componentResizeMode === "keepout");
  resizeAccess.classList.toggle("active", componentResizeMode === "access");
  resizeBody.setAttribute(
    "aria-pressed",
    String(componentResizeMode === "body"),
  );
  resizeKeepouts.setAttribute(
    "aria-pressed",
    String(componentResizeMode === "keepout"),
  );
  resizeAccess.setAttribute(
    "aria-pressed",
    String(componentResizeMode === "access"),
  );

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
  ($("#draft-wire-keepouts") as HTMLTextAreaElement).value = JSON.stringify(
    definition.wire_keepouts ?? [],
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
      <dt>Kind</dt><dd>${escapeHtml(definition.kind ?? "standard")}</dd>
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
    showWireKeepouts: ($("#show-component-wire-keepouts") as HTMLInputElement)
      .checked,
    showAccess: ($("#show-component-access") as HTMLInputElement).checked,
    showOccupiedHoles: ($("#show-component-occupied") as HTMLInputElement)
      .checked,
  };
  $("#component-canvas").innerHTML = renderComponentPreviewSvg(definition, {
    rotation: componentRotation,
    labelMode: componentLabelMode,
    selectedPinId: selectedComponentPinId,
    resizeGeometry: componentDraft ? componentResizeMode : undefined,
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
    [...comparison.components, ...comparison.wires, ...comparison.rails].map(
      (item) => [item.object_id, item.kind],
    ),
  );
  let previewProject: ProjectDocument = routeProposal
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
  if (railDraft?.length && activeNetId) {
    previewProject = {
      ...previewProject,
      design: {
        ...previewProject.design,
        rails: {
          ...(previewProject.design.rails ?? {}),
          "rail-draft-preview": {
            id: "rail-draft-preview",
            net_id: activeNetId,
            side: viewSide,
            color: "orange",
            vertices: railDraft,
            locked: true,
            notes: "Rail draft",
          },
        },
      },
    };
  }
  $("#print-sheet").innerHTML = renderPrintLayout(project);
  $("#canvas").innerHTML = renderBoardSvg(
    previewProject,
    selectedId,
    viewBox?.join(" "),
    viewSide,
    changes,
    "board",
    resizeMode,
    false,
    componentsFaded,
  );
  $("#canvas").classList.toggle("resize-mode", resizeMode);
  const head = project.head_commit
    ? project.commits?.[project.head_commit]
    : undefined;
  $("#committed-canvas").innerHTML = head
    ? renderBoardSvg(
        { ...project, design: head.design, validation: head.validation },
        undefined,
        undefined,
        viewSide,
        changes,
        "committed-board",
        false,
        false,
        componentsFaded,
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
      .map((net) => {
        const pins = net.pins ?? [];
        const pinList = pins.length
          ? `<ul class="net-pin-list">${pins
              .map(
                (pin) =>
                  `<li>${escapeHtml(netPinLabel(project.design, pin))}</li>`,
              )
              .join("")}</ul>`
          : '<p class="net-empty">No pins assigned</p>';
        return `<div class="net-item ${net.state}"><div class="net-heading"><strong>${escapeHtml(net.name)}</strong><span>${pins.length} ${pins.length === 1 ? "pin" : "pins"}</span></div>${pinList}<div>${net.state === "proposed" ? `<button data-accept-net="${net.id}">Accept</button><button data-reject-net="${net.id}">Reject</button>` : `<button data-remove-net="${net.id}">Remove</button>`}</div></div>`;
      })
      .join("") || "<p>No nets defined.</p>";
  const wires = Object.values(project.design.wires ?? {});
  $("#wire-list").innerHTML =
    wires
      .map((wire, index) => {
        const net = project.design.nets?.[wire.net_id];
        const color =
          typeof wire.color === "string" ? wire.color : wire.color.other;
        return `<div class="wire-item${wire.id === selectedId ? " active" : ""}"><button class="wire-select" data-select-wire="${escapeHtml(wire.id)}"><strong>W${index + 1}</strong><span>${escapeHtml(net?.name ?? wire.net_id)} · ${escapeHtml(wire.side)} · ${escapeHtml(color)}</span></button><button class="danger wire-remove" data-remove-wire="${escapeHtml(wire.id)}" aria-label="Remove wire W${index + 1}" title="Remove wire">Remove</button></div>`;
      })
      .join("") || '<p class="tool-hint">No wires placed.</p>';
  const rails = Object.values(project.design.rails ?? {});
  $("#rail-list").innerHTML =
    rails
      .map((rail, index) => {
        const net = project.design.nets?.[rail.net_id];
        return `<div class="wire-item${rail.id === selectedId ? " active" : ""}"><button class="wire-select" data-select-rail="${escapeHtml(rail.id)}"><strong>R${index + 1}</strong><span>${escapeHtml(net?.name ?? rail.net_id)} · ${escapeHtml(rail.side)} · ${rail.vertices.length} nodes</span></button><button class="danger wire-remove" data-remove-rail="${escapeHtml(rail.id)}">Remove</button></div>`;
      })
      .join("") || '<p class="tool-hint">No rails placed.</p>';
  $("#route-proposal").innerHTML = routeProposal
    ? `<article class="route-result"><strong>${routeProposal.length_mm.toFixed(2)} mm · ${routeProposal.bend_count} bends</strong><p>Cost ${routeProposal.cost.total}: length ${routeProposal.cost.length}, bends ${routeProposal.cost.bends}, crossings ${routeProposal.cost.crossings}, board edge ${routeProposal.cost.board_edges}, jumper ${routeProposal.cost.jumper_adjustment}</p>${routeProposal.jumper_match ? `<p>Jumper: ${escapeHtml(routeProposal.jumper_match.inventory_id)} (${routeProposal.jumper_match.kind})</p>` : ""}${(routeProposal.warnings ?? []).map((warning) => `<p class="warning">${escapeHtml(warning)}</p>`).join("")}${routeProposal.crossings.length ? '<label>At crossings <select id="crossing-order"><option value="over">New route over</option><option value="under">New route under</option></select></label>' : ""}<div class="button-row"><button id="accept-route">Accept</button><button id="reject-route">Reject</button></div></article>`
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
  const fadeComponents = $("#fade-components") as HTMLButtonElement;
  fadeComponents.hidden = viewSide !== "component";
  fadeComponents.classList.toggle("active", componentsFaded);
  fadeComponents.setAttribute("aria-pressed", String(componentsFaded));
  $("#wire-mode").classList.toggle("active", wireMode);
  $("#rail-mode").classList.toggle("active", railMode);
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
  const rail = selectedId ? project.design.rails?.[selectedId] : undefined;
  const wireName = wire
    ? `W${wires.findIndex((candidate) => candidate.id === wire.id) + 1}`
    : undefined;
  const selectedDefinition = instance
    ? project.design.component_definitions?.[instance.definition_id]
    : undefined;
  const libraryDefinition = instance
    ? library.find((definition) => definition.id === instance.definition_id)
    : undefined;
  const pinTable = selectedDefinition
    ? `<h3>Pin reference</h3><table class="pin-table"><thead><tr><th>Pin</th><th>Name</th></tr></thead><tbody>${selectedDefinition.pins.map((pin) => `<tr><td>${escapeHtml(pin.number)}</td><td>${escapeHtml(pin.name)}</td></tr>`).join("")}</tbody></table>`
    : "";
  $("#inspector").innerHTML = instance
    ? `<label>Name <input id="instance-name" value="${escapeHtml(instance.name)}" /></label><label>Notes <textarea id="instance-notes">${escapeHtml(instance.notes ?? "")}</textarea></label><div class="button-row"><button id="rotate-component">Rotate 90°</button><button id="update-component-metadata"${libraryDefinition ? "" : " disabled"}>Update metadata</button><button id="remove-component" class="danger">Remove</button></div><p>Anchor: ${instance.position.x}, ${instance.position.y}<br>Rotation: ${instance.rotation}${selectedDefinition ? `<br>Definition: ${escapeHtml(selectedDefinition.definition_version)}` : ""}</p>${libraryDefinition ? '<p class="tool-hint">Updates the definition version, name, description, and documentation. Pins and geometry stay unchanged.</p>' : '<p class="tool-hint">This component is not available in the current library.</p>'}${pinTable}`
    : wire
      ? `<p>Wire ${wireName}</p><label>Side <select id="selected-wire-side"><option${wire.side === "component" ? " selected" : ""}>component</option><option${wire.side === "solder" ? " selected" : ""}>solder</option></select></label><label>Color <select id="selected-wire-color">${["green", "red", "black", "yellow", "white", "blue", "orange"].map((color) => `<option${wire.color === color ? " selected" : ""}>${color}</option>`).join("")}</select></label>${wire.vertices.map((point, index) => `<div class="wire-vertex"><label>x2 <input data-vertex-x="${index}" type="number" value="${point.x2}" /></label><label>y2 <input data-vertex-y="${index}" type="number" value="${point.y2}" /></label>${index > 0 && index < wire.vertices.length - 1 ? `<button data-remove-vertex="${index}">Remove bend</button>` : ""}</div>`).join("")}<div class="button-row"><button id="toggle-wire-route"${wire.locked ? " disabled" : ""}>${isStraightRoute(wire.vertices) ? "Use 90° route" : "Make straight"}</button><button id="reroute-wire"${wire.locked || rerouting ? " disabled" : ""}>Reroute selected</button><button id="evaluate-wire">Evaluate</button><button id="toggle-wire-lock">${wire.locked ? "Unlock" : "Lock"}</button><button id="remove-wire" class="danger">Remove wire</button></div>${wire.locked ? '<p class="tool-hint">Unlock this wire before changing its route.</p>' : ""}`
      : rail
        ? `<p>Rail R${rails.findIndex((candidate) => candidate.id === rail.id) + 1}</p><p>${escapeHtml(project.design.nets?.[rail.net_id]?.name ?? rail.net_id)} · ${rail.side} · ${rail.vertices.length} explicit connection nodes</p>${rail.vertices.map((point) => `<div class="wire-vertex"><span>x ${point.x2 / 2}, y ${point.y2 / 2}</span></div>`).join("")}<button data-remove-rail="${escapeHtml(rail.id)}" class="danger">Remove rail</button>`
        : "<p>Select a component, wire, or rail.</p>";
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
  const redoChanges = project.redo_stack ?? [];
  const historyEntryCount = workingChanges.length + redoChanges.length;
  $("#undo-history-summary").textContent = historyEntryCount
    ? `${workingChanges.length} undo · ${redoChanges.length} redo entries stored in the project file.`
    : "No undo or redo history is stored.";
  ($("#manage-undo-history") as HTMLButtonElement).disabled =
    historyEntryCount === 0;
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
    : {
        components: [],
        nets: [],
        wires: [],
        rails: [],
        constraints: [],
        routes: [],
      };
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
    setStatus(
      `Could not refresh component library: ${errorMessage(error)}`,
      "error",
    );
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
    setStatus(errorMessage(error), "error");
  }
}

async function updateComponentMetadata(
  instance: ComponentInstance,
): Promise<void> {
  setStatus("Refreshing component metadata…");
  try {
    library = await getComponentLibrary();
    const definition = library.find(
      (candidate) => candidate.id === instance.definition_id,
    );
    if (!definition) {
      render();
      setStatus(
        "This component is not available in the current library",
        "error",
      );
      return;
    }
    await applyOperation(project.revision, {
      type: "update_component_metadata",
      instance_id: instance.id,
      definition,
    });
    await refresh();
    setStatus(
      `Updated ${definition.name} metadata to ${definition.definition_version}; pins and geometry were preserved`,
    );
  } catch (error) {
    setStatus(
      `Could not update component metadata: ${errorMessage(error)}`,
      "error",
    );
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

function previewWireNodeDrag(state: NonNullable<typeof wireNodeDrag>): boolean {
  const preview = state.vertices.map((point, index) =>
    index === state.vertexIndex ? state.position : point,
  );
  const canvas = $("#canvas");
  const group = canvas.querySelector<SVGGElement>(
    `.wire[data-wire-id="${CSS.escape(state.wireId)}"]`,
  );
  const points = preview
    .map((point) => `${point.x2 / 2},${point.y2 / 2}`)
    .join(" ");
  group
    ?.querySelector<SVGPolylineElement>(".wire-path")
    ?.setAttribute("points", points);
  group
    ?.querySelector<SVGPolylineElement>(".wire-selection")
    ?.setAttribute("points", points);
  group
    ?.querySelectorAll<SVGLineElement>("[data-wire-segment]")
    .forEach((segment, index) => {
      const start = preview[index]!;
      const end = preview[index + 1]!;
      segment.setAttribute("x1", String(start.x2 / 2));
      segment.setAttribute("y1", String(start.y2 / 2));
      segment.setAttribute("x2", String(end.x2 / 2));
      segment.setAttribute("y2", String(end.y2 / 2));
    });
  state.element.setAttribute("cx", String(state.position.x2 / 2));
  state.element.setAttribute("cy", String(state.position.y2 / 2));

  const merge = state.vertices.some(
    (point, index) =>
      index !== state.vertexIndex &&
      point.x2 === state.position.x2 &&
      point.y2 === state.position.y2,
  );
  canvas
    .querySelectorAll<SVGCircleElement>(
      `[data-wire-id="${CSS.escape(state.wireId)}"] [data-wire-vertex-index], [data-wire-id="${CSS.escape(state.wireId)}"][data-wire-vertex-index]`,
    )
    .forEach((node) => {
      const index = Number(node.dataset.wireVertexIndex);
      node.classList.toggle(
        "merge-target",
        index !== state.vertexIndex &&
          state.vertices[index]?.x2 === state.position.x2 &&
          state.vertices[index]?.y2 === state.position.y2,
      );
    });
  return merge;
}

function pinTerminalAtPosition(
  position: { x2: number; y2: number },
  wireId: string,
  endpoint: "start" | "end",
): TerminalRef | undefined {
  const wire = project.design.wires?.[wireId];
  const current = wire?.[endpoint];
  const candidates = Object.values(project.design.components ?? {}).flatMap(
    (component) => {
      const definition =
        project.design.component_definitions?.[component.definition_id];
      return (definition?.pins ?? [])
        .filter((pin) => {
          const pinPosition = pinRoutePosition(component.id, pin.id);
          return (
            pinPosition?.x2 === position.x2 && pinPosition.y2 === position.y2
          );
        })
        .map<TerminalRef>((pin) => ({
          kind: "pin",
          component_id: component.id,
          pin_id: pin.id,
        }));
    },
  );
  return (
    candidates.find(
      (candidate) => JSON.stringify(candidate) === JSON.stringify(current),
    ) ??
    candidates.find(
      (candidate) =>
        candidate.kind === "pin" &&
        project.design.nets?.[wire?.net_id ?? ""]?.pins?.some(
          (pin) =>
            pin.component_id === candidate.component_id &&
            pin.pin_id === candidate.pin_id,
        ),
    ) ??
    candidates[0]
  );
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
  wireDraft.vertices = appendOrthogonal(wireDraft.vertices, target);
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
  await handleWireTerminal(terminal, position);
}

async function handleWireTerminal(
  terminal: TerminalRef,
  position: { x2: number; y2: number },
): Promise<void> {
  const netId = activeNetId;
  if (!netId) return;
  if (!wireDraft) {
    wireDraft = { start: terminal, vertices: [position] };
    setStatus("Starting node selected. Add bends or click the ending node.");
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

async function handleWireRailNode(
  rail: Rail,
  nodeIndex: number,
): Promise<void> {
  const net = activeNetId ? project.design.nets?.[activeNetId] : undefined;
  if (!net || net.state !== "accepted") {
    setStatus("Create and select an accepted net first", "error");
    return;
  }
  if (rail.net_id !== net.id) {
    setStatus("That rail belongs to a different net", "error");
    return;
  }
  if (rail.side !== viewSide) {
    setStatus("View the rail's side before connecting to it", "error");
    return;
  }
  const position = rail.vertices[nodeIndex];
  if (!position) return;
  await handleWireTerminal(
    { kind: "rail_node", rail_id: rail.id, position: { ...position } },
    position,
  );
}

async function addAndUseRailNode(
  rail: Rail,
  pointer: PointerEvent,
): Promise<void> {
  if (rail.locked) {
    setStatus("Unlock this rail before adding a connection node", "error");
    return;
  }
  const point = svgPoint(pointer);
  if (!point) return;
  const position = snappedRoutePoint(point, {
    maxX2: (project.design.board.holes_x - 1) * 2,
    maxY2: (project.design.board.holes_y - 1) * 2,
  });
  const existingIndex = rail.vertices.findIndex(
    (node) => node.x2 === position.x2 && node.y2 === position.y2,
  );
  if (existingIndex >= 0) {
    await handleWireRailNode(rail, existingIndex);
    return;
  }
  const segmentIndex = closestWireSegmentIndex(rail.vertices, point);
  if (segmentIndex === undefined) return;
  const vertices = [
    ...rail.vertices.slice(0, segmentIndex + 1),
    position,
    ...rail.vertices.slice(segmentIndex + 1),
  ];
  try {
    await applyOperation(project.revision, {
      type: "modify_rail_path",
      rail_id: rail.id,
      vertices,
    });
    await refresh();
    const updated = project.design.rails?.[rail.id];
    if (updated) await handleWireRailNode(updated, segmentIndex + 1);
  } catch (error) {
    setStatus(errorMessage(error), "error");
  }
}

$("#perfboard-mode-button").addEventListener("click", () =>
  setAppMode("perfboard"),
);
$("#component-mode-button").addEventListener("click", () =>
  setAppMode("component"),
);
$("#working-copy-tab").addEventListener("click", () =>
  setDesignView("working-copy"),
);
$("#committed-tab").addEventListener("click", () => setDesignView("committed"));
$(".design-tabs").addEventListener("keydown", (event) => {
  const keyboardEvent = event as KeyboardEvent;
  let nextView: DesignView | undefined;
  if (keyboardEvent.key === "ArrowLeft" || keyboardEvent.key === "ArrowRight")
    nextView = designView === "working-copy" ? "committed" : "working-copy";
  if (keyboardEvent.key === "Home") nextView = "working-copy";
  if (keyboardEvent.key === "End") nextView = "committed";
  if (!nextView) return;
  keyboardEvent.preventDefault();
  setDesignView(nextView, true);
});
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
      componentResizeMode = undefined;
      componentGeometryResize = undefined;
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
$("#apply-draft-wire-keepouts").addEventListener("click", () =>
  applyJsonDraftField<LocalRect[]>("draft-wire-keepouts", (keepouts) => ({
    type: "set_wire_keepouts",
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
  "show-component-wire-keepouts",
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
function componentLocalPointer(
  pointer: PointerEvent,
): { x: number; y: number } | undefined {
  const svg = document.querySelector<SVGSVGElement>("#component-preview");
  const matrix = svg?.getScreenCTM()?.inverse();
  if (!svg || !matrix) return undefined;
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
  return { x, y };
}

function updateComponentResizeFeedback(
  resize: NonNullable<typeof componentGeometryResize>,
): void {
  const { x, y, width, height } = resize.bounds;
  resize.boundary.setAttribute("x", String(x));
  resize.boundary.setAttribute("y", String(y));
  resize.boundary.setAttribute("width", String(width));
  resize.boundary.setAttribute("height", String(height));
  resize.feedback.setAttribute("x", String(x + width / 2));
  resize.feedback.setAttribute("y", String(y + height / 2));
  resize.feedback.textContent =
    resize.kind === "body"
      ? `${width} × ${height} holes`
      : resize.kind === "keepout"
        ? `${width} × ${height} holes kept out`
        : `${width} × ${height} holes accessible`;
}

$("#component-canvas").addEventListener("pointerdown", (event) => {
  const pointer = event as PointerEvent;
  const handle = (pointer.target as Element).closest<SVGElement>(
    "[data-component-resize-handle]",
  );
  if (!handle || !componentDraft || pointer.button !== 0) return;
  const kind = handle.dataset.componentResizeKind as
    "body" | "keepout" | "access";
  const handleName = handle.dataset.componentResizeHandle as ResizeHandle;
  const indexValue = handle.dataset.componentResizeIndex;
  const index = indexValue === undefined ? undefined : Number(indexValue);
  const controls = handle.closest<SVGGElement>(
    "[data-component-resize-controls]",
  );
  const boundary = controls?.querySelector<SVGRectElement>(
    ".component-resize-boundary",
  );
  const feedback = controls?.querySelector<SVGTextElement>(
    ".component-resize-feedback",
  );
  if (!boundary || !feedback) return;
  if (kind === "body") {
    const body = componentDraft.definition.body;
    componentGeometryResize = {
      pointerId: pointer.pointerId,
      handle: handleName,
      kind,
      bounds: bodyGridRect(body),
      initialBody: body,
      body,
      boundary,
      feedback,
    };
  } else if (kind === "keepout") {
    const keepout = componentDraft.definition.keepouts?.[index ?? -1];
    if (!keepout) return;
    componentGeometryResize = {
      pointerId: pointer.pointerId,
      handle: handleName,
      kind,
      index,
      bounds: keepoutGridRect(keepout),
      initialKeepout: keepout,
      keepout,
      boundary,
      feedback,
    };
  } else {
    const access = componentDraft.definition.access?.[index ?? -1];
    if (!access) return;
    componentGeometryResize = {
      pointerId: pointer.pointerId,
      handle: handleName,
      kind,
      index,
      bounds: keepoutGridRect(access),
      initialAccess: access,
      access,
      boundary,
      feedback,
    };
  }
  pointer.preventDefault();
  (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
});

$("#component-canvas").addEventListener("pointermove", (event) => {
  const pointer = event as PointerEvent;
  const resize = componentGeometryResize;
  if (!resize || resize.pointerId !== pointer.pointerId) return;
  const point = componentLocalPointer(pointer);
  if (!point) return;
  if (resize.kind === "body" && resize.initialBody) {
    const body = planBodyResize(
      resize.initialBody,
      resize.handle,
      point.x,
      point.y,
    );
    if (!body) return;
    resize.body = body;
    resize.bounds = bodyGridRect(body);
    setStatus(
      `Resize body to ${resize.bounds.width} × ${resize.bounds.height} holes`,
    );
  } else if (resize.kind === "keepout" && resize.initialKeepout) {
    const keepout = planKeepoutResize(
      resize.initialKeepout,
      resize.handle,
      point.x,
      point.y,
    );
    if (!keepout) return;
    resize.keepout = keepout;
    resize.bounds = keepoutGridRect(keepout);
    setStatus(
      `Keep out ${resize.bounds.width} × ${resize.bounds.height} holes`,
    );
  } else if (resize.initialAccess) {
    const access = planHoleRegionResize(
      resize.initialAccess,
      resize.handle,
      point.x,
      point.y,
    );
    if (!access) return;
    resize.access = access;
    resize.bounds = keepoutGridRect(access);
    setStatus(
      `Access region covers ${resize.bounds.width} × ${resize.bounds.height} holes`,
    );
  }
  updateComponentResizeFeedback(resize);
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
  const point = componentLocalPointer(pointer);
  if (!point) return;
  const { x, y } = point;
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
for (const [id, mode] of [
  ["resize-component-body", "body"],
  ["resize-component-keepouts", "keepout"],
  ["resize-component-access", "access"],
] as const) {
  $(`#${id}`).addEventListener("click", () => {
    componentResizeMode = componentResizeMode === mode ? undefined : mode;
    componentGeometryResize = undefined;
    componentPanStart = undefined;
    renderComponentDesigner();
    setStatus(
      componentResizeMode === "body"
        ? "Drag a body edge or corner; borders snap every half hole"
        : componentResizeMode === "keepout"
          ? "Drag a keepout edge or corner; borders stay between holes"
          : componentResizeMode === "access"
            ? "Drag an access-region edge or corner; borders stay between holes"
            : "Component geometry resize finished",
    );
  });
}
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
    (pointer.target as Element).closest(
      "[data-preview-pin-id], [data-component-resize-handle]",
    )
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
  $("#component-canvas").addEventListener(eventName, (event) => {
    const pointer = event as PointerEvent;
    if (
      componentGeometryResize &&
      pointer.pointerId === componentGeometryResize.pointerId
    ) {
      const resize = componentGeometryResize;
      componentGeometryResize = undefined;
      if (eventName === "pointerup" && componentDraft) {
        if (
          resize.kind === "body" &&
          resize.body &&
          resize.initialBody &&
          JSON.stringify(resize.body) !== JSON.stringify(resize.initialBody)
        ) {
          void mutateComponentDraft({ type: "set_body", body: resize.body });
        } else if (
          resize.kind === "keepout" &&
          resize.keepout &&
          resize.initialKeepout &&
          resize.index !== undefined &&
          JSON.stringify(resize.keepout) !==
            JSON.stringify(resize.initialKeepout)
        ) {
          const keepouts = [...(componentDraft.definition.keepouts ?? [])];
          keepouts[resize.index] = resize.keepout;
          void mutateComponentDraft({ type: "set_keepouts", keepouts });
        } else if (
          resize.kind === "access" &&
          resize.access &&
          resize.initialAccess &&
          resize.index !== undefined &&
          JSON.stringify(resize.access) !== JSON.stringify(resize.initialAccess)
        ) {
          const access = [...(componentDraft.definition.access ?? [])];
          access[resize.index] = resize.access;
          void mutateComponentDraft({ type: "set_access_regions", access });
        }
      }
      if (eventName === "pointercancel") renderComponentDesigner();
    }
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
  const removeWire =
    target.closest<HTMLElement>("[data-remove-wire]")?.dataset.removeWire;
  if (removeWire) {
    if (selectedId === removeWire) selectedId = undefined;
    void mutate({ type: "remove_wire", wire_id: removeWire });
    return;
  }
  const removeRail =
    target.closest<HTMLElement>("[data-remove-rail]")?.dataset.removeRail;
  if (removeRail) {
    if (selectedId === removeRail) selectedId = undefined;
    void mutate({ type: "remove_rail", rail_id: removeRail });
    return;
  }
  const railNode = target.closest<HTMLElement>("[data-rail-node-index]");
  const railNodeId = railNode?.dataset.railId;
  if (wireMode && railNodeId) {
    const rail = project.design.rails?.[railNodeId];
    if (rail)
      void handleWireRailNode(rail, Number(railNode.dataset.railNodeIndex));
    return;
  }
  const clickedRailId = target.closest<HTMLElement>(".rail[data-rail-id]")
    ?.dataset.railId;
  if (wireMode && clickedRailId) {
    const rail = project.design.rails?.[clickedRailId];
    if (rail) void addAndUseRailNode(rail, event as PointerEvent);
    return;
  }
  const selectWire =
    target.closest<HTMLElement>("[data-select-wire]")?.dataset.selectWire;
  if (selectWire) {
    selectVisibleObject(selectWire);
    return;
  }
  const selectRail =
    target.closest<HTMLElement>("[data-select-rail]")?.dataset.selectRail;
  if (selectRail) {
    selectVisibleObject(selectRail);
    return;
  }
  const selectedWire =
    target.closest<HTMLElement>("[data-wire-id]")?.dataset.wireId;
  if (selectedWire) {
    selectVisibleObject(selectedWire);
    return;
  }
  const selectedRail =
    target.closest<HTMLElement>("[data-rail-id]")?.dataset.railId;
  if (selectedRail) {
    selectVisibleObject(selectedRail);
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
    const componentId =
      component.dataset.componentId ?? component.dataset.selectId;
    if (componentId) selectVisibleObject(componentId);
    return;
  }
  if (target.id === "add-library-component") {
    const definition = library.find(
      (item) => item.id === selectedLibraryDefinitionId,
    );
    if (!definition) return;
    const count = Object.keys(project.design.components ?? {}).length;
    const instanceId = crypto.randomUUID();
    viewSide = definition.kind === "bridge" ? "solder" : "component";
    selectedId = instanceId;
    void mutate({
      type: "add_component",
      definition,
      instance: {
        id: instanceId,
        definition_id: definition.id,
        name: `${definition.name} ${count + 1}`,
        position: newComponentStagingPosition(project.design.board),
        rotation: "deg0",
        properties: {},
        notes: "",
      },
    });
    return;
  }
  if (target.closest("#canvas, #committed-canvas") && selectedId) {
    selectedId = undefined;
    render();
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
  if (
    !pointer.isPrimary ||
    pointer.button !== 0 ||
    drag ||
    wireNodeDrag ||
    boardResize
  )
    return;
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
  const pointerTarget = pointer.target as Element;
  if (wireMode && pointerTarget.closest("[data-rail-id]")) return;
  if (railMode) {
    const point = svgPoint(pointer);
    if (!point) return;
    const snapped = snappedRoutePoint(point, {
      maxX2: (project.design.board.holes_x - 1) * 2,
      maxY2: (project.design.board.holes_y - 1) * 2,
    });
    if (!railDraft) railDraft = [snapped];
    else if (
      railDraft.at(-1)?.x2 !== snapped.x2 ||
      railDraft.at(-1)?.y2 !== snapped.y2
    )
      railDraft.push(snapped);
    setStatus(
      railDraft.length === 1
        ? "Rail started; click more half-grid nodes"
        : `${railDraft.length} rail nodes; finish when ready`,
    );
    render();
    pointer.preventDefault();
    return;
  }
  const wireSegment = pointerTarget.closest<SVGLineElement>(
    "[data-wire-segment]",
  );
  const segmentWireGroup = pointerTarget.closest<SVGGElement>("[data-wire-id]");
  const segmentWireId = segmentWireGroup?.dataset.wireId;
  const segmentWire = segmentWireId
    ? project.design.wires?.[segmentWireId]
    : undefined;
  if (
    !wireMode &&
    !pointerTarget.closest("[data-wire-vertex-index]") &&
    segmentWireGroup &&
    segmentWire &&
    segmentWire.side === viewSide
  ) {
    const point = svgPoint(pointer);
    const svg = $("#board") as SVGSVGElement;
    const currentViewBox = (svg.getAttribute("viewBox") ?? "0 0 25 18")
      .split(" ")
      .map(Number) as [number, number, number, number];
    if (!point) return;
    const segmentIndex = wireSegment
      ? Number(wireSegment.dataset.wireSegment)
      : closestWireSegmentIndex(segmentWire.vertices, point);
    if (segmentIndex === undefined || !Number.isFinite(segmentIndex)) return;
    const press = {
      pointerId: pointer.pointerId,
      clientX: pointer.clientX,
      clientY: pointer.clientY,
      point,
      wireId: segmentWire.id,
      segmentIndex,
      viewBox: currentViewBox,
      wireGroup: segmentWireGroup,
      timer: undefined as number | undefined,
      triggered: false,
    };
    press.timer = window.setTimeout(() => {
      if (wireSegmentPress !== press) return;
      press.timer = undefined;
      press.triggered = true;
      press.wireGroup.classList.remove("long-press-pending");
      const wire = project.design.wires?.[press.wireId];
      if (!wire || wire.side !== viewSide) return;
      if (wire.locked) {
        setStatus("Unlock this wire before adding a node", "error");
        return;
      }
      const vertices = insertWireNode(
        wire.vertices,
        press.segmentIndex,
        press.point,
      );
      if (vertices === wire.vertices) {
        setStatus(
          "This segment has no free half-grid position for another node",
        );
        return;
      }
      selectedId = wire.id;
      void mutate({ type: "modify_wire_path", wire_id: wire.id, vertices });
    }, wireSegmentLongPressDelayMs);
    wireSegmentPress = press;
    segmentWireGroup.classList.add("long-press-pending");
    (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
    pointer.preventDefault();
    return;
  }
  const wireNode = (pointer.target as Element).closest<SVGCircleElement>(
    "[data-wire-vertex-index]",
  );
  const wireGroup = wireNode?.closest<SVGGElement>("[data-wire-id]");
  const editedWire = wireGroup?.dataset.wireId
    ? project.design.wires?.[wireGroup.dataset.wireId]
    : undefined;
  if (wireNode && editedWire) {
    if (editedWire.locked) {
      setStatus("Unlock this wire before editing its nodes", "error");
      return;
    }
    const vertexIndex = Number(wireNode.dataset.wireVertexIndex);
    wireNodeDrag = {
      wireId: editedWire.id,
      vertexIndex,
      pointerId: pointer.pointerId,
      vertices: editedWire.vertices.map((point) => ({ ...point })),
      position: { ...editedWire.vertices[vertexIndex]! },
      element: wireNode,
      endpoint:
        wireNode.dataset.wireEndpoint === "start" ||
        wireNode.dataset.wireEndpoint === "end"
          ? wireNode.dataset.wireEndpoint
          : undefined,
    };
    wireNode.classList.add("dragging");
    (pointer.currentTarget as Element).setPointerCapture?.(pointer.pointerId);
    pointer.preventDefault();
    return;
  }
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
    if (objectSide(component.dataset.componentId) !== viewSide) return;
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
$("#canvas").addEventListener("contextmenu", (event) => {
  if ((event.target as Element).closest("[data-wire-id]"))
    event.preventDefault();
});
$("#canvas").addEventListener("pointermove", (event) => {
  const pointer = event as PointerEvent;
  if (wireSegmentPress) {
    if (pointer.pointerId !== wireSegmentPress.pointerId) return;
    const distance = Math.hypot(
      pointer.clientX - wireSegmentPress.clientX,
      pointer.clientY - wireSegmentPress.clientY,
    );
    if (
      !wireSegmentPress.triggered &&
      distance > wireSegmentLongPressMovementPx
    ) {
      if (wireSegmentPress.timer !== undefined)
        window.clearTimeout(wireSegmentPress.timer);
      wireSegmentPress.wireGroup.classList.remove("long-press-pending");
      panStart = {
        x: wireSegmentPress.clientX,
        y: wireSegmentPress.clientY,
        viewBox: wireSegmentPress.viewBox,
      };
      wireSegmentPress = undefined;
    } else {
      return;
    }
  }
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
  if (wireNodeDrag) {
    if (pointer.pointerId !== wireNodeDrag.pointerId) return;
    const point = svgPoint(pointer);
    if (!point) return;
    wireNodeDrag.position = snappedRoutePoint(point, {
      maxX2: (project.design.board.holes_x - 1) * 2,
      maxY2: (project.design.board.holes_y - 1) * 2,
    });
    const merge = previewWireNodeDrag(wireNodeDrag);
    setStatus(
      wireNodeDrag.endpoint
        ? "Release over a component pin to reconnect this wire end"
        : merge
          ? "Release to remove this node"
          : `Move wire node to ${wireNodeDrag.position.x2 / 2}, ${wireNodeDrag.position.y2 / 2}`,
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
  if (wireSegmentPress) {
    if ((event as PointerEvent).pointerId !== wireSegmentPress.pointerId)
      return;
    if (wireSegmentPress.timer !== undefined)
      window.clearTimeout(wireSegmentPress.timer);
    wireSegmentPress.wireGroup.classList.remove("long-press-pending");
    const triggered = wireSegmentPress.triggered;
    wireSegmentPress = undefined;
    if (triggered) {
      suppressClick = true;
      window.setTimeout(() => (suppressClick = false), 0);
    }
    return;
  }
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
  if (wireNodeDrag) {
    if ((event as PointerEvent).pointerId !== wireNodeDrag.pointerId) return;
    const finished = wireNodeDrag;
    wireNodeDrag = undefined;
    const original = finished.vertices[finished.vertexIndex]!;
    const moved =
      original.x2 !== finished.position.x2 ||
      original.y2 !== finished.position.y2;
    finished.element.classList.remove("dragging");
    suppressClick = moved;
    if (moved) window.setTimeout(() => (suppressClick = false), 0);
    if (moved) {
      if (finished.endpoint) {
        const terminal = pinTerminalAtPosition(
          finished.position,
          finished.wireId,
          finished.endpoint,
        );
        if (!terminal) {
          render();
          setStatus(
            "Wire end was not changed; drop it on a component pin",
            "error",
          );
          return;
        }
        void mutate({
          type: "reconnect_wire_endpoint",
          wire_id: finished.wireId,
          endpoint: finished.endpoint,
          terminal,
        }).then(render);
        return;
      }
      const result = finishWireNodeDrag(
        finished.vertices,
        finished.vertexIndex,
        finished.position,
      );
      void mutate({
        type: "modify_wire_path",
        wire_id: finished.wireId,
        vertices: result.vertices,
      }).then(render);
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
  if (wireSegmentPress) {
    if ((event as PointerEvent).pointerId !== wireSegmentPress.pointerId)
      return;
    if (wireSegmentPress.timer !== undefined)
      window.clearTimeout(wireSegmentPress.timer);
    wireSegmentPress.wireGroup.classList.remove("long-press-pending");
    wireSegmentPress = undefined;
    return;
  }
  if (boardResize) {
    if ((event as PointerEvent).pointerId !== boardResize.pointerId) return;
    boardResize = undefined;
    render();
    return;
  }
  if (wireNodeDrag) {
    if ((event as PointerEvent).pointerId !== wireNodeDrag.pointerId) return;
    wireNodeDrag = undefined;
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
    if (target.id === "toggle-wire-route") {
      const vertices = isStraightRoute(wire.vertices)
        ? orthogonalRoute(wire.vertices)
        : straightRoute(wire.vertices);
      void mutate({ type: "modify_wire_path", wire_id: wire.id, vertices });
    } else if (target.id === "reroute-wire") {
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
  if (target.id === "update-component-metadata") {
    void updateComponentMetadata(instance);
  }
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
      const side = (target as HTMLSelectElement).value as Side;
      if (side !== viewSide) selectedId = undefined;
      void mutate({
        type: "set_wire_side",
        wire_id: wire.id,
        side,
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
        rails: [],
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
function formatByteSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}
function undoHistoryStorageBytes(): number {
  return new Blob([
    JSON.stringify(
      {
        undo_stack: project.undo_stack ?? [],
        redo_stack: project.redo_stack ?? [],
      },
      null,
      2,
    ),
  ]).size;
}
$("#manage-undo-history").addEventListener("click", () => {
  const undoCount = project.undo_stack?.length ?? 0;
  const redoCount = project.redo_stack?.length ?? 0;
  const largestStack = Math.max(undoCount, redoCount);
  const keep = $("#undo-history-keep") as HTMLInputElement;
  keep.max = String(largestStack);
  keep.value = String(Math.min(25, largestStack));
  $("#undo-history-error").textContent = "";
  $("#undo-history-size").textContent =
    `${undoCount + redoCount} entries use approximately ${formatByteSize(undoHistoryStorageBytes())}. ` +
    "This data is embedded in every saved project file.";
  ($("#undo-history-dialog") as HTMLDialogElement).showModal();
  keep.select();
});
$("#cancel-undo-history").addEventListener("click", () => {
  ($("#undo-history-dialog") as HTMLDialogElement).close();
});
async function applyUndoHistoryLimit(keepEntries: number): Promise<void> {
  const dialog = $("#undo-history-dialog") as HTMLDialogElement;
  const error = $("#undo-history-error");
  try {
    const result = await trimProjectUndoHistory(project.revision, keepEntries);
    await refresh();
    dialog.close();
    setStatus(`${result.summary}. Save the project to reclaim disk space.`);
  } catch (trimError) {
    error.textContent = errorMessage(trimError);
  }
}
$("#trim-undo-history").addEventListener("click", () => {
  const keepEntries = Number(
    ($("#undo-history-keep") as HTMLInputElement).value,
  );
  if (!Number.isSafeInteger(keepEntries) || keepEntries < 1) {
    $("#undo-history-error").textContent =
      "Enter a positive whole number of entries to keep.";
    return;
  }
  void applyUndoHistoryLimit(keepEntries);
});
$("#delete-undo-history").addEventListener("click", () => {
  void applyUndoHistoryLimit(0);
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
  if (selectedId && objectSide(selectedId) !== viewSide) selectedId = undefined;
  render();
});
$("#fade-components").addEventListener("click", () => {
  componentsFaded = !componentsFaded;
  render();
});
$("#print-project").addEventListener("click", () => window.print());
$("#active-net").addEventListener("change", (event) => {
  activeNetId = (event.target as HTMLSelectElement).value;
  wireDraft = undefined;
  railDraft = undefined;
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
  railMode = false;
  railDraft = undefined;
  wireMode = true;
  wireDraft = undefined;
  setStatus("Click a starting pin or rail node for the active net");
  render();
});
$("#rail-mode").addEventListener("click", () => {
  const net = activeNetId ? project.design.nets?.[activeNetId] : undefined;
  if (!net || net.state !== "accepted") {
    setStatus("Create and select an accepted net first", "error");
    return;
  }
  resizeMode = false;
  wireMode = false;
  wireDraft = undefined;
  railMode = true;
  railDraft = undefined;
  setStatus("Click the first half-grid rail node");
  render();
});
$("#finish-rail").addEventListener("click", () => {
  const netId = activeNetId;
  if (!netId || !railDraft || railDraft.length < 2) {
    setStatus("A rail needs at least two distinct nodes", "error");
    return;
  }
  const vertices = railDraft;
  railDraft = undefined;
  railMode = false;
  void mutate({
    type: "add_rail",
    rail: {
      id: crypto.randomUUID(),
      net_id: netId,
      side: viewSide,
      color: ($("#wire-color") as HTMLSelectElement).value as WireColor,
      vertices,
      locked: false,
      notes: "",
    },
  });
});
$("#cancel-rail").addEventListener("click", () => {
  railMode = false;
  railDraft = undefined;
  render();
});
$("#resize-board").addEventListener("click", () => {
  resizeMode = !resizeMode;
  wireMode = false;
  wireDraft = undefined;
  railMode = false;
  railDraft = undefined;
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
    : {
        components: [],
        nets: [],
        wires: [],
        rails: [],
        constraints: [],
        routes: [],
      };
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
