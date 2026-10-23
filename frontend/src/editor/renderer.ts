import type {
  ComponentDefinition,
  ComponentInstance,
  ProjectDocument,
} from "../api/project";
const PAD = 2;
function escape(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ] ?? c,
  );
}
function degrees(rotation: ComponentInstance["rotation"]): number {
  return { deg0: 0, deg90: 90, deg180: 180, deg270: 270 }[rotation];
}
function pinTooltip(pin: ComponentDefinition["pins"][number]): string {
  const details = [`${pin.name} (${pin.id})`];
  if (pin.aliases?.length) details.push(`Aliases: ${pin.aliases.join(", ")}`);
  if (pin.functions?.length)
    details.push(`Functions: ${pin.functions.join(", ")}`);
  return details.join(" · ");
}
function rectPoints(
  min: { x: number; y: number },
  max: { x: number; y: number },
): string {
  return `${min.x - 0.42} ${min.y - 0.42} ${max.x - min.x + 0.84} ${max.y - min.y + 0.84}`;
}
function renderComponent(
  instance: ComponentInstance,
  definition: ComponentDefinition,
  selected: boolean,
  pitch: number,
): string {
  const body = definition.body;
  const pins = definition.pins
    .map(
      (pin) =>
        `<g class="pin" data-pin-id="${escape(pin.id)}"><circle cx="${pin.x}" cy="${pin.y}" r=".22"/><title>${escape(pinTooltip(pin))}</title></g>`,
    )
    .join("");
  const keepouts = (definition.keepouts ?? [])
    .map((area) => {
      const [x, y, width, height] = rectPoints(area.min, area.max).split(" ");
      return `<rect class="keepout" x="${x}" y="${y}" width="${width}" height="${height}" rx=".15"><title>${escape(area.label)}</title></rect>`;
    })
    .join("");
  const access = (definition.access ?? [])
    .map((area) => {
      const [x, y, width, height] = rectPoints(area.min, area.max).split(" ");
      const middleX = (area.min.x + area.max.x) / 2;
      const middleY = (area.min.y + area.max.y) / 2;
      const arrow = { north: "↑", east: "→", south: "↓", west: "←" }[
        area.direction
      ];
      return `<g class="access"><rect x="${x}" y="${y}" width="${width}" height="${height}"/><text x="${middleX}" y="${middleY + 0.25}">${arrow}</text><title>${escape(area.label)}</title></g>`;
    })
    .join("");
  return `<g class="component${selected ? " selected" : ""}" data-component-id="${instance.id}" data-rotation-degrees="${degrees(instance.rotation)}" transform="translate(${instance.position.x} ${instance.position.y}) rotate(${degrees(instance.rotation)})">${keepouts}<rect class="body" x="${body.x / pitch}" y="${body.y / pitch}" width="${body.width / pitch}" height="${body.height / pitch}" rx=".35"/><path class="orientation" d="M .25 .25 L 1 .25 L .25 1 Z"><title>Component orientation</title></path>${pins}${access}<text class="component-label" x=".5" y="1.45">${escape(instance.name)}</text></g>`;
}
export function renderBoardSvg(
  document: ProjectDocument,
  selectedId?: string,
  viewBox?: string,
): string {
  const board = document.design.board;
  const definitions = document.design.component_definitions ?? {};
  const holes = Array.from({ length: board.holes_y }, (_, y) =>
    Array.from(
      { length: board.holes_x },
      (_, x) => `<circle class="hole" cx="${x}" cy="${y}" r=".12"/>`,
    ).join(""),
  ).join("");
  const forbidden = (board.forbidden_areas ?? [])
    .map(
      (area) =>
        `<rect class="invalid-region" x="${area.min.x - 0.45}" y="${area.min.y - 0.45}" width="${area.max.x - area.min.x + 0.9}" height="${area.max.y - area.min.y + 0.9}"><title>${escape(area.label)}</title></rect>`,
    )
    .join("");
  const mountingHoles = (board.mounting_holes ?? [])
    .map(
      (hole) =>
        `<circle class="mounting-hole" cx="${hole.position.x}" cy="${hole.position.y}" r="${hole.diameter_mm / board.pitch_mm / 2}"><title>${escape(hole.label)}</title></circle>`,
    )
    .join("");
  const components = Object.values(document.design.components ?? {})
    .sort((left, right) => {
      if (left.id === selectedId) return 1;
      if (right.id === selectedId) return -1;
      return 0;
    })
    .map((instance) => {
      const definition = definitions[instance.definition_id];
      return definition
        ? renderComponent(
            instance,
            definition,
            instance.id === selectedId,
            board.pitch_mm,
          )
        : "";
    })
    .join("");
  const defaultViewBox = `${-PAD} ${-PAD} ${board.holes_x - 1 + PAD * 2} ${board.holes_y - 1 + PAD * 2}`;
  const margins = board.margins_mm ?? { top: 0, right: 0, bottom: 0, left: 0 };
  const left = margins.left / board.pitch_mm;
  const top = margins.top / board.pitch_mm;
  const width =
    board.holes_x - 1 + (margins.left + margins.right) / board.pitch_mm;
  const height =
    board.holes_y - 1 + (margins.top + margins.bottom) / board.pitch_mm;
  return `<svg id="board" aria-label="${escape(board.name)}" role="img" viewBox="${viewBox ?? defaultViewBox}" data-default-view-box="${defaultViewBox}"><rect class="board-outline" x="${-left}" y="${-top}" width="${width}" height="${height}" rx=".5"/><g class="holes">${holes}</g>${mountingHoles}${forbidden}<g class="components">${components}</g></svg>`;
}
