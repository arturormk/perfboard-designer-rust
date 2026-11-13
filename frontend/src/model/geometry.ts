/**
 * Presentation geometry shared by the board renderer, tools and panels.
 *
 * Board space uses one unit per hole pitch: hole (x, y) is drawn at (x, y).
 * Routing space uses half-grid integers (`x2`, `y2`), so a hole maps to
 * (2x, 2y). Mirroring for the solder side is a presentation transform only;
 * every coordinate stored or sent to Rust stays canonical.
 */
import type {
  ComponentDefinition,
  ComponentInstance,
  PinRef,
  ProjectDocument,
  Rotation,
  Side,
  TerminalRef,
} from "../api/project";

export type Design = ProjectDocument["design"];
export interface Point {
  x: number;
  y: number;
}
export interface RoutePoint {
  x2: number;
  y2: number;
}

export const ROTATIONS: Rotation[] = ["deg0", "deg90", "deg180", "deg270"];

export function rotationDegrees(rotation: Rotation): number {
  return { deg0: 0, deg90: 90, deg180: 180, deg270: 270 }[rotation];
}

export function rotateLocal(rotation: Rotation, x: number, y: number): Point {
  const [rx, ry] = {
    deg0: [x, y],
    deg90: [-y, x],
    deg180: [-x, -y],
    deg270: [y, -x],
  }[rotation] as [number, number];
  // Normalise -0 so equality checks and serialisation stay predictable.
  return { x: rx + 0, y: ry + 0 };
}

export function nextRotation(
  rotation: Rotation,
  allowed: Rotation[] = ROTATIONS,
): Rotation {
  const candidates = allowed.length ? allowed : ROTATIONS;
  const start = ROTATIONS.indexOf(rotation);
  for (let step = 1; step <= ROTATIONS.length; step += 1) {
    const candidate = ROTATIONS[(start + step) % ROTATIONS.length]!;
    if (candidates.includes(candidate)) return candidate;
  }
  return rotation;
}

export function definitionOf(
  design: Design,
  instance: ComponentInstance | undefined,
): ComponentDefinition | undefined {
  return instance
    ? design.component_definitions?.[instance.definition_id]
    : undefined;
}

export function isBridge(definition: ComponentDefinition | undefined): boolean {
  return definition?.kind === "bridge";
}

/** A two-pin header that takes a removable shunt. */
export function isJumper(definition: ComponentDefinition | undefined): boolean {
  return definition?.kind === "jumper" && definition.pins.length === 2;
}

/** The board face a component is installed on. Bridges are solder-side links. */
export function componentSide(
  definition: ComponentDefinition | undefined,
): Side {
  return isBridge(definition) ? "solder" : "component";
}

export function pinBoardPosition(
  instance: ComponentInstance,
  pin: { x: number; y: number },
): Point {
  const local = rotateLocal(instance.rotation, pin.x, pin.y);
  return {
    x: instance.position.x + local.x,
    y: instance.position.y + local.y,
  };
}

export function pinPosition(design: Design, ref: PinRef): Point | undefined {
  const instance = design.components?.[ref.component_id];
  const pin = definitionOf(design, instance)?.pins.find(
    (candidate) => candidate.id === ref.pin_id,
  );
  return instance && pin ? pinBoardPosition(instance, pin) : undefined;
}

export function terminalPosition(
  design: Design,
  terminal: TerminalRef,
): Point | undefined {
  if (terminal.kind === "pin") return pinPosition(design, terminal);
  if (terminal.kind === "rail_node")
    return { x: terminal.position.x2 / 2, y: terminal.position.y2 / 2 };
  return undefined;
}

export function toRoute(point: Point): RoutePoint {
  return { x2: Math.round(point.x * 2), y2: Math.round(point.y * 2) };
}

export function fromRoute(point: RoutePoint): Point {
  return { x: point.x2 / 2, y: point.y2 / 2 };
}

/** Axis-aligned board-space rectangle of a component body after rotation. */
export function bodyBounds(
  instance: ComponentInstance,
  definition: ComponentDefinition,
  pitchMm: number,
): { x: number; y: number; width: number; height: number } {
  const { body } = definition;
  const corners = [
    [body.x, body.y],
    [body.x + body.width, body.y],
    [body.x, body.y + body.height],
    [body.x + body.width, body.y + body.height],
  ].map(([x, y]) => rotateLocal(instance.rotation, x! / pitchMm, y! / pitchMm));
  const xs = corners.map((corner) => corner.x);
  const ys = corners.map((corner) => corner.y);
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  return {
    x: instance.position.x + minX,
    y: instance.position.y + minY,
    width: Math.max(...xs) - minX,
    height: Math.max(...ys) - minY,
  };
}

/** Horizontal presentation transform for a canonical x coordinate. */
export function viewX(x: number, holesX: number, side: Side): number {
  return side === "solder" ? holesX - 1 - x : x;
}

export function polylineLengthMm(
  vertices: RoutePoint[],
  pitchMm: number,
): number {
  let total = 0;
  for (let index = 1; index < vertices.length; index += 1) {
    const a = vertices[index - 1]!;
    const b = vertices[index]!;
    total += Math.hypot(b.x2 - a.x2, b.y2 - a.y2) / 2;
  }
  return total * pitchMm;
}

export function bendCount(vertices: RoutePoint[]): number {
  let bends = 0;
  for (let index = 1; index < vertices.length - 1; index += 1) {
    const a = vertices[index - 1]!;
    const b = vertices[index]!;
    const c = vertices[index + 1]!;
    const cross = (b.x2 - a.x2) * (c.y2 - b.y2) - (b.y2 - a.y2) * (c.x2 - b.x2);
    if (cross !== 0) bends += 1;
  }
  return bends;
}

/** Midpoint along a polyline, plus the direction of the segment it falls on. */
export function polylineMidpoint(vertices: RoutePoint[]): {
  point: Point;
  horizontal: boolean;
} {
  const points = vertices.map(fromRoute);
  if (points.length < 2)
    return { point: points[0] ?? { x: 0, y: 0 }, horizontal: true };
  const lengths = points
    .slice(1)
    .map((point, index) =>
      Math.hypot(point.x - points[index]!.x, point.y - points[index]!.y),
    );
  const half = lengths.reduce((sum, length) => sum + length, 0) / 2;
  let travelled = 0;
  for (let index = 0; index < lengths.length; index += 1) {
    const length = lengths[index]!;
    const start = points[index]!;
    const end = points[index + 1]!;
    if (length > 0 && travelled + length >= half) {
      const ratio = (half - travelled) / length;
      return {
        point: {
          x: start.x + (end.x - start.x) * ratio,
          y: start.y + (end.y - start.y) * ratio,
        },
        horizontal: Math.abs(end.x - start.x) >= Math.abs(end.y - start.y),
      };
    }
    travelled += length;
  }
  return { point: points.at(-1)!, horizontal: true };
}

/** Holes adjacent to `hole` in the four grid directions that lie on the board. */
export function adjacentHoles(
  hole: Point,
  holesX: number,
  holesY: number,
): Point[] {
  return [
    { x: hole.x + 1, y: hole.y },
    { x: hole.x, y: hole.y + 1 },
    { x: hole.x - 1, y: hole.y },
    { x: hole.x, y: hole.y - 1 },
  ].filter(
    (point) =>
      point.x >= 0 && point.y >= 0 && point.x < holesX && point.y < holesY,
  );
}

/** Rotation that points a bridge's B pin from `from` toward the adjacent `to`. */
export function bridgeRotation(from: Point, to: Point): Rotation | undefined {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  if (dx === 1 && dy === 0) return "deg0";
  if (dx === 0 && dy === 1) return "deg90";
  if (dx === -1 && dy === 0) return "deg180";
  if (dx === 0 && dy === -1) return "deg270";
  return undefined;
}

export function samePoint(a: Point, b: Point): boolean {
  return a.x === b.x && a.y === b.y;
}

export function pinKey(ref: PinRef): string {
  return `${ref.component_id}\u0000${ref.pin_id}`;
}
