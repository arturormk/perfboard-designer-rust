export type RoutePoint = { x2: number; y2: number };

export function snappedRoutePoint(
  point: { x: number; y: number },
  bounds?: { maxX2: number; maxY2: number },
): RoutePoint {
  const x2 = Math.round(point.x * 2);
  const y2 = Math.round(point.y * 2);
  return {
    x2: bounds ? Math.max(0, Math.min(bounds.maxX2, x2)) : x2,
    y2: bounds ? Math.max(0, Math.min(bounds.maxY2, y2)) : y2,
  };
}

function gcd(left: number, right: number): number {
  let a = Math.abs(left);
  let b = Math.abs(right);
  while (b !== 0) [a, b] = [b, a % b];
  return a;
}

/** Returns the segment nearest to a pointer that is already known to be over a wire. */
export function closestWireSegmentIndex(
  vertices: RoutePoint[],
  pointer: { x: number; y: number },
): number | undefined {
  if (vertices.length < 2) return undefined;
  const pointerX2 = pointer.x * 2;
  const pointerY2 = pointer.y * 2;
  let closestIndex = 0;
  let closestDistanceSquared = Number.POSITIVE_INFINITY;
  for (let index = 0; index < vertices.length - 1; index += 1) {
    const start = vertices[index]!;
    const end = vertices[index + 1]!;
    const dx = end.x2 - start.x2;
    const dy = end.y2 - start.y2;
    const lengthSquared = dx * dx + dy * dy;
    const projection = lengthSquared
      ? Math.max(
          0,
          Math.min(
            1,
            ((pointerX2 - start.x2) * dx + (pointerY2 - start.y2) * dy) /
              lengthSquared,
          ),
        )
      : 0;
    const offsetX = pointerX2 - (start.x2 + dx * projection);
    const offsetY = pointerY2 - (start.y2 + dy * projection);
    const distanceSquared = offsetX * offsetX + offsetY * offsetY;
    if (distanceSquared < closestDistanceSquared) {
      closestDistanceSquared = distanceSquared;
      closestIndex = index;
    }
  }
  return closestIndex;
}

function closestInteriorGridPoint(
  start: RoutePoint,
  end: RoutePoint,
  pointer: { x: number; y: number },
): RoutePoint | undefined {
  const dx = end.x2 - start.x2;
  const dy = end.y2 - start.y2;
  if (dx === 0 || dy === 0) return undefined;

  const pointerX2 = pointer.x * 2;
  const pointerY2 = pointer.y * 2;
  const minX2 = Math.min(start.x2, end.x2);
  const maxX2 = Math.max(start.x2, end.x2);
  const minY2 = Math.min(start.y2, end.y2);
  const maxY2 = Math.max(start.y2, end.y2);
  const nearbyCoordinates = (value: number, min: number, max: number) =>
    [
      ...new Set([
        min,
        max,
        ...[-1, 0, 1].map((offset) =>
          Math.max(min, Math.min(max, Math.round(value) + offset)),
        ),
      ]),
    ].sort((left, right) => left - right);
  const xCandidates = nearbyCoordinates(pointerX2, minX2, maxX2);
  const yCandidates = nearbyCoordinates(pointerY2, minY2, maxY2);
  const lengthSquared = dx * dx + dy * dy;
  let closest: RoutePoint | undefined;
  let closestDistanceSquared = Number.POSITIVE_INFINITY;

  for (const x2 of xCandidates) {
    for (const y2 of yCandidates) {
      if (
        (x2 === start.x2 && y2 === start.y2) ||
        (x2 === end.x2 && y2 === end.y2)
      )
        continue;
      const projection =
        ((x2 - start.x2) * dx + (y2 - start.y2) * dy) / lengthSquared;
      if (projection <= 0 || projection >= 1) continue;
      const distanceSquared = (x2 - pointerX2) ** 2 + (y2 - pointerY2) ** 2;
      if (distanceSquared < closestDistanceSquared) {
        closest = { x2, y2 };
        closestDistanceSquared = distanceSquared;
      }
    }
  }
  return closest;
}

/** Inserts the closest half-grid node, preserving the segment when possible. */
export function insertWireNode(
  vertices: RoutePoint[],
  segmentIndex: number,
  pointer: { x: number; y: number },
): RoutePoint[] {
  const start = vertices[segmentIndex];
  const end = vertices[segmentIndex + 1];
  if (!start || !end) return vertices;

  const dx = end.x2 - start.x2;
  const dy = end.y2 - start.y2;
  const steps = gcd(dx, dy);
  let node: RoutePoint | undefined;
  if (steps >= 2) {
    const stepX = dx / steps;
    const stepY = dy / steps;
    const pointerX2 = pointer.x * 2;
    const pointerY2 = pointer.y * 2;
    const stepLengthSquared = stepX * stepX + stepY * stepY;
    const projectedStep = Math.round(
      ((pointerX2 - start.x2) * stepX + (pointerY2 - start.y2) * stepY) /
        stepLengthSquared,
    );
    const selectedStep = Math.max(1, Math.min(steps - 1, projectedStep));
    node = {
      x2: start.x2 + stepX * selectedStep,
      y2: start.y2 + stepY * selectedStep,
    };
  } else {
    node = closestInteriorGridPoint(start, end, pointer);
  }
  if (!node) return vertices;
  return [
    ...vertices.slice(0, segmentIndex + 1),
    node,
    ...vertices.slice(segmentIndex + 1),
  ];
}

export function finishWireNodeDrag(
  vertices: RoutePoint[],
  vertexIndex: number,
  position: RoutePoint,
): { vertices: RoutePoint[]; deleted: boolean } {
  if (vertexIndex <= 0 || vertexIndex >= vertices.length - 1)
    return { vertices, deleted: false };
  if (
    vertices.some(
      (point, index) =>
        index !== vertexIndex &&
        point.x2 === position.x2 &&
        point.y2 === position.y2,
    )
  ) {
    return {
      vertices: vertices.filter((_, index) => index !== vertexIndex),
      deleted: true,
    };
  }
  return {
    vertices: vertices.map((point, index) =>
      index === vertexIndex ? position : point,
    ),
    deleted: false,
  };
}

export function isStraightRoute(vertices: RoutePoint[]): boolean {
  if (vertices.length < 3) return true;
  const first = vertices[0]!;
  const last = vertices.at(-1)!;
  const dx = last.x2 - first.x2;
  const dy = last.y2 - first.y2;
  return vertices
    .slice(1, -1)
    .every(
      (point) => (point.x2 - first.x2) * dy === (point.y2 - first.y2) * dx,
    );
}

export function straightRoute(vertices: RoutePoint[]): RoutePoint[] {
  if (vertices.length < 2) return [...vertices];
  return [vertices[0]!, vertices.at(-1)!];
}

export function orthogonalRoute(vertices: RoutePoint[]): RoutePoint[] {
  if (vertices.length < 2) return [...vertices];
  const first = vertices[0]!;
  const last = vertices.at(-1)!;
  const bend = { x2: last.x2, y2: first.y2 };
  return [first, bend, last].filter(
    (point, index, points) =>
      index === 0 ||
      point.x2 !== points[index - 1]!.x2 ||
      point.y2 !== points[index - 1]!.y2,
  );
}

export function appendOrthogonal(
  vertices: RoutePoint[],
  target: RoutePoint,
): RoutePoint[] {
  if (vertices.length === 0) return [target];
  const result = [...vertices];
  const last = result.at(-1)!;
  if (last.x2 !== target.x2 && last.y2 !== target.y2) {
    result.push({ x2: target.x2, y2: last.y2 });
  }
  const end = result.at(-1)!;
  if (end.x2 !== target.x2 || end.y2 !== target.y2) result.push(target);
  return result;
}

/**
 * Inserts a connection node into a rail at the half-grid point on its nearest
 * segment closest to the pointer. Unlike wire bend points, rail nodes must lie
 * exactly on the existing path so the rail's shape never changes. When the
 * pointer is already on a stored node, that node is returned unchanged.
 * Returns `undefined` when the segment has no interior grid point.
 */
export function insertRailNode(
  vertices: RoutePoint[],
  pointer: { x: number; y: number },
): { vertices: RoutePoint[]; node: RoutePoint; index: number } | undefined {
  const segmentIndex = closestWireSegmentIndex(vertices, pointer);
  if (segmentIndex === undefined) return undefined;
  const start = vertices[segmentIndex]!;
  const end = vertices[segmentIndex + 1]!;
  const dx = end.x2 - start.x2;
  const dy = end.y2 - start.y2;
  const steps = gcd(dx, dy);
  if (steps === 0) return undefined;
  const stepX = dx / steps;
  const stepY = dy / steps;
  const projected = Math.round(
    ((pointer.x * 2 - start.x2) * stepX + (pointer.y * 2 - start.y2) * stepY) /
      (stepX * stepX + stepY * stepY),
  );
  const step = Math.max(0, Math.min(steps, projected));
  if (step === 0 || step === steps) {
    const index = segmentIndex + (step === 0 ? 0 : 1);
    return { vertices, node: vertices[index]!, index };
  }
  const node = { x2: start.x2 + stepX * step, y2: start.y2 + stepY * step };
  return {
    vertices: [
      ...vertices.slice(0, segmentIndex + 1),
      node,
      ...vertices.slice(segmentIndex + 1),
    ],
    node,
    index: segmentIndex + 1,
  };
}

/**
 * Where dragging rail node `index` towards `pointer` puts the rail, or
 * undefined when that spot is not allowed (keep the last good one).
 *
 * A node in the middle of a straight run is a connection tap: it slides
 * along the run and stays strictly between its neighbours, so the rail keeps
 * its shape. Ends and corners reshape the rail and may go anywhere except
 * onto a neighbouring node, which would leave a zero-length segment.
 */
export function dragRailNode(
  vertices: RoutePoint[],
  index: number,
  pointer: RoutePoint,
): RoutePoint[] | undefined {
  const node = vertices[index];
  if (!node) return undefined;
  const before = vertices[index - 1];
  const after = vertices[index + 1];
  const replace = (position: RoutePoint) =>
    vertices.map((point, at) => (at === index ? position : point));
  const isTap =
    before &&
    after &&
    (node.x2 - before.x2) * (after.y2 - node.y2) ===
      (node.y2 - before.y2) * (after.x2 - node.x2);
  if (isTap) {
    const dx = after.x2 - before.x2;
    const dy = after.y2 - before.y2;
    const steps = gcd(dx, dy);
    if (steps < 2) return undefined;
    const stepX = dx / steps;
    const stepY = dy / steps;
    const projected = Math.round(
      ((pointer.x2 - before.x2) * stepX + (pointer.y2 - before.y2) * stepY) /
        (stepX * stepX + stepY * stepY),
    );
    const step = Math.max(1, Math.min(steps - 1, projected));
    return replace({
      x2: before.x2 + stepX * step,
      y2: before.y2 + stepY * step,
    });
  }
  const same = (point: RoutePoint | undefined) =>
    point !== undefined && point.x2 === pointer.x2 && point.y2 === pointer.y2;
  if (same(before) || same(after)) return undefined;
  return replace({ ...pointer });
}
