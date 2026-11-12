use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

use perfboard_core::{
    ConstraintKind, ConstraintSeverity, JumperMatch, JumperMatchKind, NetId, RouteCostBreakdown,
    RouteCrossing, RoutePos, RouteProposal, Side, TerminalRef, WireId, measure_route,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Design;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteRequest {
    pub net_id: NetId,
    pub start: TerminalRef,
    pub end: TerminalRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_side: Option<Side>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_wire_id: Option<WireId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoutingCostConfig {
    pub orthogonal_step: u64,
    pub bend: u64,
    pub cramped_bend: u64,
    pub crossing: u64,
    pub access: u64,
    pub side_transition: u64,
    pub parallel_run: u64,
    pub board_edge_step: u64,
    pub exact_jumper_bonus: u64,
    pub approximate_jumper_bonus: u64,
    pub maximum_visited_states: usize,
}

impl Default for RoutingCostConfig {
    fn default() -> Self {
        Self {
            orthogonal_step: 10,
            bend: 25,
            cramped_bend: 80,
            crossing: 60,
            access: 40,
            side_transition: 100,
            parallel_run: 8,
            // Edge holes remain usable, but long runs there are awkward to
            // solder and easy to damage while trimming the board.
            board_edge_step: 12,
            exact_jumper_bonus: 100,
            approximate_jumper_bonus: 30,
            maximum_visited_states: 250_000,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum RouteError {
    #[error("route terminal is invalid: {0}")]
    InvalidTerminal(String),
    #[error("route endpoints must differ")]
    SameEndpoint,
    #[error("no route found: {reasons:?}")]
    NoPath { reasons: Vec<String> },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct State {
    x2: i64,
    y2: i64,
    side: Side,
    direction: u8,
    since_bend: u32,
}

const DIRECTIONS: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const NO_DIRECTION: u8 = 4;

/// Produces a deterministic proposal without changing the design.
///
/// # Errors
/// Returns an invalid-terminal error or an explained no-path result.
#[allow(clippy::too_many_lines)] // Search setup and loop are kept together to make ordering auditable.
pub fn suggest_route(
    design: &Design,
    request: &RouteRequest,
    config: &RoutingCostConfig,
) -> Result<RouteProposal, RouteError> {
    let start = crate::connectivity::terminal_position(design, request.net_id, &request.start)
        .map_err(RouteError::InvalidTerminal)?;
    let end = crate::connectivity::terminal_position(design, request.net_id, &request.end)
        .map_err(RouteError::InvalidTerminal)?;
    if start == end {
        return Err(RouteError::SameEndpoint);
    }
    let constrained_side =
        design
            .constraints
            .values()
            .find_map(|constraint| match constraint.kind {
                ConstraintKind::WireSide { net_id, side }
                    if net_id == request.net_id
                        && constraint.severity == ConstraintSeverity::Required =>
                {
                    Some(side)
                }
                _ => None,
            });
    let preferred_constraint_side =
        design
            .constraints
            .values()
            .find_map(|constraint| match constraint.kind {
                ConstraintKind::WireSide { net_id, side } if net_id == request.net_id => Some(side),
                _ => None,
            });
    let side = constrained_side
        .or(request.preferred_side)
        .or(preferred_constraint_side)
        .unwrap_or(Side::Solder);
    let minimum_bend = design
        .constraints
        .values()
        .filter_map(|constraint| match constraint.kind {
            ConstraintKind::BendSpacing {
                min_half_grid_steps,
            } => Some((min_half_grid_steps, constraint.severity)),
            _ => None,
        })
        .max_by_key(|item| item.0);
    let bend_distance_cap = minimum_bend.map_or(1, |item| item.0.max(1));
    let blocked = blocked_positions(design, side, start, end);
    let unavailable_bend_holes = component_occupied_holes(design);
    let soft_areas = soft_area_positions(design);
    let existing = occupied_wire_positions(design, request.replace_wire_id);
    let max_x = i64::from(design.board.size.intervals_x()) * 2;
    let max_y = i64::from(design.board.size.intervals_y()) * 2;
    let initial = State {
        x2: start.x2,
        y2: start.y2,
        side,
        direction: NO_DIRECTION,
        since_bend: u32::MAX,
    };
    let mut queue = BinaryHeap::from([Reverse((heuristic(start, end, config), 0_u64, initial))]);
    let mut best = BTreeMap::from([(initial, 0_u64)]);
    let mut previous = BTreeMap::<State, State>::new();
    let mut final_state = None;
    while let Some(Reverse((_estimate, cost, current))) = queue.pop() {
        if best.get(&current).is_some_and(|known| cost > *known) {
            continue;
        }
        if current.x2 == end.x2 && current.y2 == end.y2 {
            final_state = Some(current);
            break;
        }
        if best.len() >= config.maximum_visited_states {
            break;
        }
        for (direction, (dx, dy)) in (0_u8..).zip(DIRECTIONS) {
            let point = RoutePos::new(current.x2 + dx, current.y2 + dy);
            if point.x2 < 0
                || point.y2 < 0
                || point.x2 > max_x
                || point.y2 > max_y
                || blocked.contains(&point)
            {
                continue;
            }
            let turning = current.direction < NO_DIRECTION && current.direction != direction;
            // A formed bend needs a physical anchoring hole. Component pins,
            // occupied holes, and junctions are already excluded by `blocked`.
            let bend = RoutePos::new(current.x2, current.y2);
            if turning && (!bend.is_hole() || unavailable_bend_holes.contains(&bend)) {
                continue;
            }
            if turning
                && minimum_bend.is_some_and(|(spacing, severity)| {
                    severity == ConstraintSeverity::Required && current.since_bend < spacing
                })
            {
                continue;
            }
            let since_bend = if turning {
                1
            } else {
                current.since_bend.saturating_add(1).min(bend_distance_cap)
            };
            let next = State {
                x2: point.x2,
                y2: point.y2,
                side,
                direction,
                since_bend,
            };
            let mut step = config.orthogonal_step;
            if turning {
                step += config.bend;
            }
            if turning
                && minimum_bend.is_some_and(|(spacing, severity)| {
                    severity != ConstraintSeverity::Required && current.since_bend < spacing
                })
            {
                step += config.cramped_bend;
            }
            if existing.contains_key(&point) {
                step += config.crossing;
            }
            if soft_areas.contains(&point) {
                step += config.access;
            }
            if is_parallel(point, direction, &existing) {
                step += config.parallel_run;
            }
            if is_board_edge(point, max_x, max_y) {
                step += config.board_edge_step;
            }
            let next_cost = cost.saturating_add(step);
            if best.get(&next).is_none_or(|known| next_cost < *known) {
                best.insert(next, next_cost);
                previous.insert(next, current);
                let estimate = next_cost.saturating_add(heuristic(point, end, config));
                queue.push(Reverse((estimate, next_cost, next)));
            }
        }
    }
    let Some(mut cursor) = final_state else {
        let mut reasons = vec![
            "Board bounds, occupied component areas, and required reserved areas were excluded"
                .into(),
        ];
        if minimum_bend.is_some_and(|(_, severity)| severity == ConstraintSeverity::Required) {
            reasons.push("Required minimum bend spacing was enforced".into());
        }
        reasons.push(format!("Search considered {} states", best.len()));
        return Err(RouteError::NoPath { reasons });
    };
    let mut points = vec![RoutePos::new(cursor.x2, cursor.y2)];
    while cursor != initial {
        cursor = previous[&cursor];
        points.push(RoutePos::new(cursor.x2, cursor.y2));
    }
    points.reverse();
    let vertices = compress_path(&points);
    Ok(evaluate_route(design, request, side, vertices, config))
}

/// Evaluates an explicit manual or generated route using the same cost model.
#[must_use]
#[allow(clippy::too_many_lines)] // Keeping every named cost in one function prevents policy drift.
pub fn evaluate_route(
    design: &Design,
    request: &RouteRequest,
    side: Side,
    vertices: Vec<RoutePos>,
    config: &RoutingCostConfig,
) -> RouteProposal {
    let measurements = measure_route(&vertices, design.board.pitch_mm);
    let positions = expand_path(&vertices);
    let existing = occupied_wire_positions(design, request.replace_wire_id);
    let mut crossings = BTreeSet::new();
    for point in positions
        .iter()
        .skip(1)
        .take(positions.len().saturating_sub(2))
    {
        if let Some(ids) = existing.get(point) {
            for id in ids {
                crossings.insert(RouteCrossing {
                    other_wire_id: *id,
                    position: *point,
                });
            }
        }
    }
    let length_cost: u64 = vertices
        .windows(2)
        .map(|pair| {
            let dx = (pair[1].x2 - pair[0].x2).unsigned_abs();
            let dy = (pair[1].y2 - pair[0].y2).unsigned_abs();
            euclidean_cost(dx, dy, config.orthogonal_step)
        })
        .sum();
    let bend_cost = u64::from(measurements.bend_count) * config.bend;
    let crossing_cost = u64::try_from(crossings.len()).unwrap_or(u64::MAX) * config.crossing;
    let soft_areas = soft_area_positions(design);
    let access_count = positions
        .iter()
        .filter(|point| soft_areas.contains(point))
        .count();
    let access_cost = u64::try_from(access_count).unwrap_or(u64::MAX) * config.access;
    let parallel_count = positions
        .windows(2)
        .filter(|pair| {
            let dx = (pair[1].x2 - pair[0].x2).signum();
            let dy = (pair[1].y2 - pair[0].y2).signum();
            DIRECTIONS
                .iter()
                .position(|candidate| *candidate == (dx, dy))
                .and_then(|direction| u8::try_from(direction).ok())
                .is_some_and(|direction| is_parallel(pair[1], direction, &existing))
        })
        .count();
    let parallel_cost = u64::try_from(parallel_count).unwrap_or(u64::MAX) * config.parallel_run;
    let max_x = i64::from(design.board.size.intervals_x()) * 2;
    let max_y = i64::from(design.board.size.intervals_y()) * 2;
    let board_edge_count = positions
        .iter()
        .skip(1)
        .filter(|point| is_board_edge(**point, max_x, max_y))
        .count();
    let board_edge_cost =
        u64::try_from(board_edge_count).unwrap_or(u64::MAX) * config.board_edge_step;
    let preferred_bend_spacing = design
        .constraints
        .values()
        .filter_map(|constraint| match constraint.kind {
            ConstraintKind::BendSpacing {
                min_half_grid_steps,
            } if constraint.severity != ConstraintSeverity::Required => Some(min_half_grid_steps),
            _ => None,
        })
        .max();
    let cramped_bend_cost = preferred_bend_spacing.map_or(0, |spacing| {
        let count = vertices
            .windows(3)
            .filter(|points| {
                let preceding = (points[1].x2 - points[0].x2)
                    .abs()
                    .max((points[1].y2 - points[0].y2).abs());
                u32::try_from(preceding).unwrap_or(u32::MAX) < spacing
            })
            .count();
        u64::try_from(count).unwrap_or(u64::MAX) * config.cramped_bend
    });
    let jumper_match = jumper_match(design, &vertices);
    let jumper_adjustment = match jumper_match.as_ref().map(|value| value.kind) {
        Some(JumperMatchKind::Exact) => {
            -i64::try_from(config.exact_jumper_bonus).unwrap_or(i64::MAX)
        }
        Some(JumperMatchKind::Approximate) => {
            -i64::try_from(config.approximate_jumper_bonus).unwrap_or(i64::MAX)
        }
        _ => 0,
    };
    let base = length_cost
        .saturating_add(bend_cost)
        .saturating_add(cramped_bend_cost)
        .saturating_add(crossing_cost)
        .saturating_add(access_cost)
        .saturating_add(parallel_cost)
        .saturating_add(board_edge_cost);
    let total = if jumper_adjustment < 0 {
        base.saturating_sub(jumper_adjustment.unsigned_abs())
    } else {
        base.saturating_add(jumper_adjustment.unsigned_abs())
    };
    let mut warnings = Vec::new();
    if !crossings.is_empty() {
        warnings.push(format!(
            "Route crosses {} accepted wire(s); choose over/under during assembly",
            crossings.len()
        ));
    }
    if matches!(
        jumper_match.as_ref().map(|value| value.kind),
        Some(JumperMatchKind::Approximate | JumperMatchKind::Reshaped)
    ) {
        warnings.push("Jumper match is approximate or requires reshaping".into());
    }
    for constraint in design.constraints.values() {
        if let ConstraintKind::PreferredMaximumLength {
            net_id,
            max_length_mm,
        } = constraint.kind
            && net_id == request.net_id
            && measurements.length_mm > max_length_mm
        {
            warnings.push(format!(
                "Route exceeds preferred maximum length of {max_length_mm:.2} mm"
            ));
        }
        if let ConstraintKind::WireSide {
            net_id,
            side: wanted,
        } = constraint.kind
            && net_id == request.net_id
            && wanted != side
            && constraint.severity != ConstraintSeverity::Required
        {
            warnings.push(format!("Route does not use preferred {wanted:?} side"));
        }
    }
    RouteProposal {
        net_id: request.net_id,
        start: request.start.clone(),
        end: request.end.clone(),
        side,
        vertices,
        length_mm: measurements.length_mm,
        bend_count: measurements.bend_count,
        crossings: crossings.into_iter().collect(),
        jumper_match,
        cost: RouteCostBreakdown {
            length: length_cost,
            bends: bend_cost,
            cramped_bends: cramped_bend_cost,
            crossings: crossing_cost,
            access: access_cost,
            parallel_runs: parallel_cost,
            board_edges: board_edge_cost,
            jumper_adjustment,
            total,
            ..RouteCostBreakdown::default()
        },
        warnings,
    }
}

#[allow(clippy::cast_precision_loss, clippy::similar_names)] // Coordinate pairs are clearer with parallel names.
fn blocked_positions(
    design: &Design,
    side: Side,
    start: RoutePos,
    end: RoutePos,
) -> BTreeSet<RoutePos> {
    let mut blocked = BTreeSet::new();
    for area in &design.board.features.forbidden_areas {
        add_rect(&mut blocked, area.min, area.max);
    }
    for constraint in design.constraints.values() {
        if constraint.severity == ConstraintSeverity::Required
            && let ConstraintKind::ReservedArea { area, .. }
            | ConstraintKind::AssemblyAccess { area, .. } = &constraint.kind
        {
            add_rect(&mut blocked, area.min, area.max);
        }
    }
    if side == Side::Component {
        for instance in design.components.values() {
            let definition = &design.component_definitions[&instance.definition_id];
            if definition.kind == perfboard_core::ComponentKind::Bridge {
                continue;
            }
            for keepout in &definition.wire_keepouts {
                for x2 in i64::from(keepout.min.x) * 2..=i64::from(keepout.max.x) * 2 {
                    for y2 in i64::from(keepout.min.y) * 2..=i64::from(keepout.max.y) * 2 {
                        let (x2, y2) = rotate_half_grid(instance.rotation, x2, y2);
                        let point = RoutePos::new(
                            i64::from(instance.position.x) * 2 + x2,
                            i64::from(instance.position.y) * 2 + y2,
                        );
                        blocked.insert(point);
                    }
                }
            }
            for local in &definition.occupied_holes {
                let (x, y) = instance.rotation.rotate_local((local.x, local.y));
                blocked.insert(RoutePos::new(
                    (i64::from(instance.position.x) + i64::from(x)) * 2,
                    (i64::from(instance.position.y) + i64::from(y)) * 2,
                ));
            }
            for access in &definition.access {
                if access.required {
                    add_local_rect(&mut blocked, instance, access.min, access.max);
                }
            }
        }
    }
    for instance in design.components.values() {
        let definition = &design.component_definitions[&instance.definition_id];
        // Pins physically occupy their holes on both faces. Other occupied
        // positions describe holes covered by the component body, so they are
        // obstacles only on the component face; solder-side wire may pass
        // beneath that body.
        let occupied = definition
            .pins
            .iter()
            .map(|pin| perfboard_core::LocalPos::new(pin.x, pin.y));
        for local in occupied {
            let (x, y) = instance.rotation.rotate_local((local.x, local.y));
            blocked.insert(RoutePos::new(
                (i64::from(instance.position.x) + i64::from(x)) * 2,
                (i64::from(instance.position.y) + i64::from(y)) * 2,
            ));
        }
    }
    for junction in design.junctions.values() {
        blocked.insert(junction.position.as_route_pos());
    }
    blocked.remove(&start);
    blocked.remove(&end);
    blocked
}

const fn rotate_half_grid(rotation: perfboard_core::Rotation, x2: i64, y2: i64) -> (i64, i64) {
    match rotation {
        perfboard_core::Rotation::Deg0 => (x2, y2),
        perfboard_core::Rotation::Deg90 => (-y2, x2),
        perfboard_core::Rotation::Deg180 => (-x2, -y2),
        perfboard_core::Rotation::Deg270 => (y2, -x2),
    }
}

fn soft_area_positions(design: &Design) -> BTreeSet<RoutePos> {
    let mut result = BTreeSet::new();
    for constraint in design.constraints.values() {
        if constraint.severity != ConstraintSeverity::Required
            && let ConstraintKind::ReservedArea { area, .. }
            | ConstraintKind::AssemblyAccess { area, .. } = &constraint.kind
        {
            add_rect(&mut result, area.min, area.max);
        }
    }
    for instance in design.components.values() {
        let definition = &design.component_definitions[&instance.definition_id];
        for access in &definition.access {
            if !access.required {
                add_local_rect(&mut result, instance, access.min, access.max);
            }
        }
    }
    result
}

fn component_occupied_holes(design: &Design) -> BTreeSet<RoutePos> {
    design
        .components
        .values()
        .flat_map(|instance| {
            let definition = &design.component_definitions[&instance.definition_id];
            definition.occupied_holes.iter().map(move |local| {
                let (x, y) = instance.rotation.rotate_local((local.x, local.y));
                RoutePos::new(
                    (i64::from(instance.position.x) + i64::from(x)) * 2,
                    (i64::from(instance.position.y) + i64::from(y)) * 2,
                )
            })
        })
        .collect()
}

fn add_local_rect(
    set: &mut BTreeSet<RoutePos>,
    instance: &perfboard_core::ComponentInstance,
    min: perfboard_core::LocalPos,
    max: perfboard_core::LocalPos,
) {
    for x2 in i64::from(min.x) * 2..=i64::from(max.x) * 2 {
        for y2 in i64::from(min.y) * 2..=i64::from(max.y) * 2 {
            let (x2, y2) = match instance.rotation {
                perfboard_core::Rotation::Deg0 => (x2, y2),
                perfboard_core::Rotation::Deg90 => (-y2, x2),
                perfboard_core::Rotation::Deg180 => (-x2, -y2),
                perfboard_core::Rotation::Deg270 => (y2, -x2),
            };
            set.insert(RoutePos::new(
                i64::from(instance.position.x) * 2 + x2,
                i64::from(instance.position.y) * 2 + y2,
            ));
        }
    }
}

fn add_rect(
    set: &mut BTreeSet<RoutePos>,
    min: perfboard_core::HolePos,
    max: perfboard_core::HolePos,
) {
    for x2 in i64::from(min.x) * 2..=i64::from(max.x) * 2 {
        for y2 in i64::from(min.y) * 2..=i64::from(max.y) * 2 {
            set.insert(RoutePos::new(x2, y2));
        }
    }
}

fn occupied_wire_positions(
    design: &Design,
    excluded: Option<WireId>,
) -> BTreeMap<RoutePos, BTreeSet<WireId>> {
    let mut result = BTreeMap::new();
    for wire in design
        .wires
        .values()
        .filter(|wire| Some(wire.id) != excluded)
    {
        for point in expand_path(&wire.vertices) {
            result
                .entry(point)
                .or_insert_with(BTreeSet::new)
                .insert(wire.id);
        }
    }
    result
}

/// Cost of a straight run of `dx` by `dy` half-hole units at `unit` per unit,
/// rounded to the nearest integer.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "board-sized, non-negative values are far inside f64's exact integer range"
)]
fn euclidean_cost(dx: u64, dy: u64, unit: u64) -> u64 {
    ((dx as f64).hypot(dy as f64) * unit as f64).round() as u64
}

fn expand_path(vertices: &[RoutePos]) -> Vec<RoutePos> {
    let mut result = Vec::new();
    for pair in vertices.windows(2) {
        let (dx, dy, steps) = crate::connectivity::segment_step(pair);
        for step in 0..steps {
            result.push(RoutePos::new(
                pair[0].x2 + dx * step,
                pair[0].y2 + dy * step,
            ));
        }
    }
    if let Some(last) = vertices.last() {
        result.push(*last);
    }
    result
}

fn compress_path(points: &[RoutePos]) -> Vec<RoutePos> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut result = vec![points[0]];
    for index in 1..points.len() - 1 {
        let before = (
            points[index].x2 - points[index - 1].x2,
            points[index].y2 - points[index - 1].y2,
        );
        let after = (
            points[index + 1].x2 - points[index].x2,
            points[index + 1].y2 - points[index].y2,
        );
        if before != after {
            result.push(points[index]);
        }
    }
    result.push(*points.last().expect("non-empty"));
    result
}

fn heuristic(from: RoutePos, to: RoutePos, config: &RoutingCostConfig) -> u64 {
    let dx = (to.x2 - from.x2).unsigned_abs();
    let dy = (to.y2 - from.y2).unsigned_abs();
    (dx + dy) * config.orthogonal_step
}

const fn is_board_edge(point: RoutePos, max_x: i64, max_y: i64) -> bool {
    point.x2 == 0 || point.y2 == 0 || point.x2 == max_x || point.y2 == max_y
}

fn is_parallel(
    point: RoutePos,
    direction: u8,
    existing: &BTreeMap<RoutePos, BTreeSet<WireId>>,
) -> bool {
    let (dx, dy) = DIRECTIONS[direction as usize];
    [
        RoutePos::new(point.x2 - dy, point.y2 + dx),
        RoutePos::new(point.x2 + dy, point.y2 - dx),
    ]
    .iter()
    .any(|value| existing.contains_key(value))
}

fn jumper_match(design: &Design, vertices: &[RoutePos]) -> Option<JumperMatch> {
    let first = *vertices.first()?;
    let last = *vertices.last()?;
    if !first.is_hole() || !last.is_hole() {
        return None;
    }
    let straight = vertices.len() == 2 && (first.x2 == last.x2 || first.y2 == last.y2);
    let requested = u32::try_from(
        (first.x2 - last.x2)
            .unsigned_abs()
            .max((first.y2 - last.y2).unsigned_abs())
            / 2,
    )
    .unwrap_or(u32::MAX);
    design
        .jumper_inventory
        .values()
        .filter_map(|entry| {
            let difference = entry.span_holes.abs_diff(requested);
            let kind = if straight && difference == 0 {
                JumperMatchKind::Exact
            } else if straight && difference <= entry.tolerance_holes {
                JumperMatchKind::Approximate
            } else if difference <= entry.tolerance_holes {
                JumperMatchKind::Reshaped
            } else {
                return None;
            };
            Some((kind, difference, entry.id.clone(), entry.span_holes))
        })
        .min_by_key(|(kind, difference, id, _)| (*kind as u8, *difference, id.clone()))
        .map(
            |(kind, _, inventory_id, inventory_span_holes)| JumperMatch {
                inventory_id,
                requested_span_holes: requested,
                inventory_span_holes,
                kind,
            },
        )
}
