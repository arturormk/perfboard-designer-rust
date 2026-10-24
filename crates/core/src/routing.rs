use serde::{Deserialize, Serialize};

use crate::{NetId, RoutePos, Side, TerminalRef, WireId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct RouteCrossing {
    pub other_wire_id: WireId,
    pub position: RoutePos,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CrossingChoice {
    pub first_wire_id: WireId,
    pub second_wire_id: WireId,
    pub over_wire_id: WireId,
    pub position: RoutePos,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JumperMatch {
    pub inventory_id: String,
    pub requested_span_holes: u32,
    pub inventory_span_holes: u32,
    pub kind: JumperMatchKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JumperMatchKind {
    Exact,
    Approximate,
    Reshaped,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteCostBreakdown {
    pub length: u64,
    pub bends: u64,
    pub cramped_bends: u64,
    pub crossings: u64,
    pub access: u64,
    pub side_transitions: u64,
    pub parallel_runs: u64,
    pub jumper_adjustment: i64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteProposal {
    pub net_id: NetId,
    pub start: TerminalRef,
    pub end: TerminalRef,
    pub side: Side,
    pub vertices: Vec<RoutePos>,
    pub length_mm: f64,
    pub bend_count: u32,
    pub crossings: Vec<RouteCrossing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jumper_match: Option<JumperMatch>,
    pub cost: RouteCostBreakdown,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireColor {
    Red,
    Black,
    Yellow,
    White,
    Green,
    Blue,
    Orange,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub id: WireId,
    pub net_id: NetId,
    pub start: TerminalRef,
    pub end: TerminalRef,
    pub side: Side,
    pub color: WireColor,
    pub vertices: Vec<RoutePos>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteMeasurements {
    pub length_mm: f64,
    pub bend_count: u32,
}

#[must_use]
#[allow(clippy::cast_precision_loss)] // Board coordinates are small and length is intentionally physical f64 data.
pub fn measure_route(vertices: &[RoutePos], pitch_mm: f64) -> RouteMeasurements {
    let length_half_grid = vertices
        .windows(2)
        .map(|pair| {
            let dx = (pair[1].x2 - pair[0].x2).abs() as f64;
            let dy = (pair[1].y2 - pair[0].y2).abs() as f64;
            if dx == 0.0 {
                dy
            } else if dy == 0.0 {
                dx
            } else {
                dx * 2.0_f64.sqrt()
            }
        })
        .sum::<f64>();
    let bend_count = u32::try_from(
        vertices
            .windows(3)
            .filter(|points| {
                let first = (points[1].x2 - points[0].x2, points[1].y2 - points[0].y2);
                let second = (points[2].x2 - points[1].x2, points[2].y2 - points[1].y2);
                first.0 * second.1 != first.1 * second.0
            })
            .count(),
    )
    .unwrap_or(u32::MAX);
    RouteMeasurements {
        length_mm: length_half_grid * pitch_mm / 2.0,
        bend_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_orthogonal_and_diagonal_segments_at_custom_pitch() {
        let horizontal = measure_route(&[RoutePos::new(0, 0), RoutePos::new(4, 0)], 2.0);
        assert_eq!(horizontal.length_mm, 4.0);
        let vertical = measure_route(&[RoutePos::new(0, 0), RoutePos::new(0, 6)], 2.0);
        assert_eq!(vertical.length_mm, 6.0);
        let diagonal = measure_route(&[RoutePos::new(0, 0), RoutePos::new(4, 4)], 2.0);
        assert!((diagonal.length_mm - 4.0 * 2.0_f64.sqrt()).abs() < 1e-10);
    }

    #[test]
    fn counts_direction_changes_not_collinear_vertices() {
        let route = [
            RoutePos::new(0, 0),
            RoutePos::new(2, 0),
            RoutePos::new(4, 0),
            RoutePos::new(4, 2),
        ];
        assert_eq!(measure_route(&route, 2.54).bend_count, 1);
    }
}
