//! Rules for saddles: net-free mechanical wire ties.
//!
//! A saddle's two end holes receive the legs of a strap that is soldered on the
//! opposite face, so both holes must stay electrically unused. Its body may
//! pass over wires and rails on its own face (that is its purpose) but not over
//! component pins.

use std::collections::BTreeSet;

use perfboard_core::{
    Diagnostic, DiagnosticCode, DiagnosticSeverity, HolePos, ObjectReference, RoutePos, Saddle,
};

use crate::Design;
use crate::connectivity::segment_step;

pub(crate) fn validate_structure(design: &Design) -> Result<(), String> {
    let size = design.board.size;
    for saddle in design.saddles.values() {
        if saddle.start == saddle.end {
            return Err("saddle end holes must be different".into());
        }
        for hole in [saddle.start, saddle.end] {
            if hole.x >= size.holes_x() || hole.y >= size.holes_y() {
                return Err("saddle hole lies outside the board".into());
            }
        }
    }
    Ok(())
}

/// Describes why one of `saddle`'s end holes is not free, if any is taken.
///
/// The saddle itself (matched by id) is ignored, so the check can run against
/// a design that already contains it.
pub(crate) fn hole_conflict(design: &Design, saddle: &Saddle) -> Option<String> {
    let ends = [saddle.start, saddle.end];
    let end_points: BTreeSet<RoutePos> = ends.iter().map(|hole| hole.as_route_pos()).collect();

    for instance in design.components.values() {
        let Some(definition) = design.component_definitions.get(&instance.definition_id) else {
            continue;
        };
        let place = |x: i32, y: i32| {
            let (x, y) = instance.rotation.rotate_local((x, y));
            RoutePos::new(
                (i64::from(instance.position.x) + i64::from(x)) * 2,
                (i64::from(instance.position.y) + i64::from(y)) * 2,
            )
        };
        if definition
            .pins
            .iter()
            .any(|pin| end_points.contains(&place(pin.x, pin.y)))
        {
            return Some(format!("a pin of {} occupies a saddle hole", instance.name));
        }
        if definition
            .occupied_holes
            .iter()
            .any(|local| end_points.contains(&place(local.x, local.y)))
        {
            return Some(format!("{} covers a saddle hole", instance.name));
        }
    }
    if design
        .junctions
        .values()
        .any(|junction| end_points.contains(&junction.position.as_route_pos()))
    {
        return Some("a junction occupies a saddle hole".into());
    }
    if design
        .wires
        .values()
        .any(|wire| path_touches(&wire.vertices, &end_points))
    {
        return Some("a wire passes through a saddle hole".into());
    }
    if design
        .rails
        .values()
        .any(|rail| path_touches(&rail.vertices, &end_points))
    {
        return Some("a rail passes through a saddle hole".into());
    }
    if design
        .saddles
        .values()
        .filter(|other| other.id != saddle.id)
        .any(|other| {
            [other.start, other.end]
                .iter()
                .any(|hole| ends.contains(hole))
        })
    {
        return Some("another saddle uses the same hole".into());
    }
    let features = &design.board.features;
    if features
        .mounting_holes
        .iter()
        .any(|mount| ends.contains(&mount.position))
    {
        return Some("a mounting hole occupies a saddle hole".into());
    }
    if features.forbidden_areas.iter().any(|area| {
        ends.iter().any(|hole| {
            (area.min.x..=area.max.x).contains(&hole.x)
                && (area.min.y..=area.max.y).contains(&hole.y)
        })
    }) {
        return Some("a saddle hole lies in a forbidden area".into());
    }
    None
}

/// Whether the saddle body passes over a component pin between its end holes.
pub(crate) fn crosses_pin(design: &Design, saddle: &Saddle) -> bool {
    let pins: BTreeSet<RoutePos> = all_pin_positions(design).collect();
    let vertices = saddle.vertices();
    let (dx, dy, steps) = segment_step(&vertices);
    (1..steps).any(|step| {
        pins.contains(&RoutePos::new(
            vertices[0].x2 + dx * step,
            vertices[0].y2 + dy * step,
        ))
    })
}

/// Whether any wire or rail on the saddle's face runs under its body.
pub(crate) fn holds_conductor(design: &Design, saddle: &Saddle) -> bool {
    let strap = saddle.vertices();
    let crosses = |vertices: &[RoutePos]| {
        vertices
            .windows(2)
            .any(|pair| segments_intersect(strap[0], strap[1], pair[0], pair[1]))
    };
    design
        .wires
        .values()
        .filter(|wire| wire.side == saddle.side)
        .any(|wire| crosses(&wire.vertices))
        || design
            .rails
            .values()
            .filter(|rail| rail.side == saddle.side)
            .any(|rail| crosses(&rail.vertices))
}

pub(crate) fn diagnostics(design: &Design) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for saddle in design.saddles.values() {
        if let Some(reason) = hole_conflict(design, saddle) {
            result.push(diag(
                "saddle.hole_conflict",
                DiagnosticSeverity::Warning,
                format!("Saddle holes must be free: {reason}"),
                saddle,
            ));
        }
        if crosses_pin(design, saddle) {
            result.push(diag(
                "saddle.crosses_pin",
                DiagnosticSeverity::Warning,
                "Saddle passes over a component pin".into(),
                saddle,
            ));
        }
        if !holds_conductor(design, saddle) {
            result.push(diag(
                "saddle.holds_nothing",
                DiagnosticSeverity::Advisory,
                "Saddle does not cross any wire or rail on its side".into(),
                saddle,
            ));
        }
    }
    result
}

/// Moves both end holes by a whole-hole offset, failing when either would
/// leave the non-negative coordinate range.
pub(crate) fn translate(saddle: &mut Saddle, offset_x: i64, offset_y: i64) -> Option<()> {
    let shift = |hole: HolePos| {
        Some(HolePos::new(
            u32::try_from(i64::from(hole.x).checked_add(offset_x)?).ok()?,
            u32::try_from(i64::from(hole.y).checked_add(offset_y)?).ok()?,
        ))
    };
    saddle.start = shift(saddle.start)?;
    saddle.end = shift(saddle.end)?;
    Some(())
}

fn all_pin_positions(design: &Design) -> impl Iterator<Item = RoutePos> + '_ {
    design.components.values().flat_map(move |instance| {
        design
            .component_definitions
            .get(&instance.definition_id)
            .into_iter()
            .flat_map(move |definition| {
                definition.pins.iter().map(move |pin| {
                    let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
                    RoutePos::new(
                        (i64::from(instance.position.x) + i64::from(x)) * 2,
                        (i64::from(instance.position.y) + i64::from(y)) * 2,
                    )
                })
            })
    })
}

fn path_touches(vertices: &[RoutePos], points: &BTreeSet<RoutePos>) -> bool {
    vertices.windows(2).any(|pair| {
        let (dx, dy, steps) = segment_step(pair);
        (0..=steps).any(|step| {
            points.contains(&RoutePos::new(
                pair[0].x2 + dx * step,
                pair[0].y2 + dy * step,
            ))
        })
    })
}

/// Closed-segment intersection on the integer half-grid, including touching
/// and collinear overlap.
fn segments_intersect(a: RoutePos, b: RoutePos, c: RoutePos, d: RoutePos) -> bool {
    let orient = |p: RoutePos, q: RoutePos, r: RoutePos| {
        ((q.x2 - p.x2) * (r.y2 - p.y2) - (q.y2 - p.y2) * (r.x2 - p.x2)).signum()
    };
    let on_segment = |p: RoutePos, q: RoutePos, r: RoutePos| {
        r.x2 >= p.x2.min(q.x2)
            && r.x2 <= p.x2.max(q.x2)
            && r.y2 >= p.y2.min(q.y2)
            && r.y2 <= p.y2.max(q.y2)
    };
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    if o1 != o2 && o3 != o4 {
        return true;
    }
    (o1 == 0 && on_segment(a, b, c))
        || (o2 == 0 && on_segment(a, b, d))
        || (o3 == 0 && on_segment(c, d, a))
        || (o4 == 0 && on_segment(c, d, b))
}

fn diag(code: &str, severity: DiagnosticSeverity, message: String, saddle: &Saddle) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode(code.into()),
        severity,
        message,
        objects: vec![ObjectReference {
            kind: "saddle".into(),
            id: saddle.id.to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_and_parallel_segments_are_told_apart() {
        let p = RoutePos::new;
        assert!(segments_intersect(p(2, 0), p(2, 4), p(0, 2), p(4, 2)));
        assert!(segments_intersect(p(0, 0), p(4, 4), p(0, 4), p(4, 0)));
        assert!(!segments_intersect(p(0, 0), p(0, 4), p(2, 0), p(2, 4)));
        assert!(!segments_intersect(p(0, 0), p(0, 4), p(1, 5), p(3, 5)));
    }
}
