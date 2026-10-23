import "./styles.css";
import { getBootstrapInfo } from "./api/bootstrap";
import {
  applyOperation,
  createProject,
  getComponentLibrary,
  getProject,
  openProject,
  saveProject,
  type ComponentDefinition,
  type ComponentInstance,
  type ProjectDocument,
  type Rotation,
} from "./api/project";
import { renderBoardSvg } from "./editor/renderer";
import { snappedDragPosition } from "./editor/drag";

const $ = <T extends Element>(selector: string): T => {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Missing UI element ${selector}`);
  return element;
};
let project: ProjectDocument;
let library: ComponentDefinition[] = [];
let selectedId: string | undefined;
let viewBox: [number, number, number, number] | undefined;
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
let suppressClick = false;
let panStart:
  | { x: number; y: number; viewBox: [number, number, number, number] }
  | undefined;
function setStatus(message: string, state = "ready"): void {
  const status = $("#status");
  status.textContent = message;
  status.setAttribute("data-state", state);
}
function selected(): ComponentInstance | undefined {
  return selectedId ? project.design.components?.[selectedId] : undefined;
}
function render(): void {
  $("#canvas").innerHTML = renderBoardSvg(
    project,
    selectedId,
    viewBox?.join(" "),
  );
  $("#library").innerHTML = library
    .map(
      (item) =>
        `<button class="library-item" data-add-definition="${item.id}"><strong>${item.name}</strong><span>${item.description}</span></button>`,
    )
    .join("");
  $("#component-list").innerHTML =
    Object.values(project.design.components ?? {})
      .map(
        (item) =>
          `<button class="component-item${item.id === selectedId ? " active" : ""}" data-select-id="${item.id}">${item.name}<span>${item.position.x}, ${item.position.y} · ${item.rotation}</span></button>`,
      )
      .join("") || "<p>No components placed.</p>";
  const instance = selected();
  $("#inspector").innerHTML = instance
    ? `<label>Name <input id="instance-name" value="${instance.name.replaceAll('"', "&quot;")}" /></label><label>Notes <textarea id="instance-notes">${instance.notes ?? ""}</textarea></label><div class="button-row"><button id="rotate-component">Rotate 90°</button><button id="remove-component" class="danger">Remove</button></div><p>Anchor: ${instance.position.x}, ${instance.position.y}<br>Rotation: ${instance.rotation}</p>`
    : "<p>Select a component.</p>";
  const diagnostics = project.validation ?? [];
  $("#validation").innerHTML = diagnostics.length
    ? diagnostics
        .map(
          (item) =>
            `<article class="diagnostic ${item.severity}"><code>${item.code}</code><p>${item.message}</p></article>`,
        )
        .join("")
    : '<p class="valid">No placement problems.</p>';
}
async function refresh(): Promise<void> {
  project = await getProject();
  render();
  setStatus(
    `Revision ${project.revision} · ${project.design.board.holes_x} × ${project.design.board.holes_y} holes`,
  );
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
function nextRotation(rotation: Rotation): Rotation {
  const rotations: Rotation[] = ["deg0", "deg90", "deg180", "deg270"];
  return rotations[(rotations.indexOf(rotation) + 1) % rotations.length]!;
}
function svgPoint(event: PointerEvent): { x: number; y: number } | undefined {
  const matrix = ($("#board") as SVGSVGElement).getScreenCTM();
  if (!matrix) return undefined;
  const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
    matrix.inverse(),
  );
  return { x: point.x, y: point.y };
}

$("#app").addEventListener("click", (event) => {
  if (suppressClick) {
    suppressClick = false;
    return;
  }
  const target = event.target as HTMLElement;
  const component = target.closest<HTMLElement>(
    "[data-component-id], [data-select-id]",
  );
  if (component) {
    selectedId = component.dataset.componentId ?? component.dataset.selectId;
    render();
    return;
  }
  const add = target.closest<HTMLElement>("[data-add-definition]");
  if (add?.dataset.addDefinition) {
    const definition = library.find(
      (item) => item.id === add.dataset.addDefinition,
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
$("#canvas").addEventListener("pointerdown", (event) => {
  const pointer = event as PointerEvent;
  if (!pointer.isPrimary || pointer.button !== 0 || drag) return;
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
  if (!instance) return;
  const target = event.target as HTMLElement;
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
$("#inspector").addEventListener("change", () => {
  const instance = selected();
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
$("#new-project").addEventListener("click", async () => {
  project = await createProject("Untitled project");
  selectedId = undefined;
  viewBox = undefined;
  render();
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
$("#zoom-in").addEventListener("click", () => zoom(0.8));
$("#zoom-out").addEventListener("click", () => zoom(1.25));
$("#zoom-reset").addEventListener("click", () => {
  viewBox = undefined;
  render();
});
$("#canvas").addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    zoom((event as WheelEvent).deltaY > 0 ? 1.12 : 0.89);
  },
  { passive: false },
);
Promise.all([getBootstrapInfo(), getProject(), getComponentLibrary()])
  .then(([info, loadedProject, loadedLibrary]) => {
    project = loadedProject;
    library = loadedLibrary;
    render();
    setStatus(`Rust engine ready · schema v${info.schema_version}`);
  })
  .catch((error: unknown) =>
    setStatus(`The desktop bridge is unavailable: ${String(error)}`, "error"),
  );
