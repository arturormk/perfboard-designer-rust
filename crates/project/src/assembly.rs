//! Assembly planning: a deterministic, step-by-step build order derived from
//! the design (ADR 0012).
//!
//! The plan is derived data. It is never stored in the project or its history;
//! it is recomputed from a design and carries the revision it came from plus a
//! fingerprint of its own content, so a caller can tell when an edit has
//! changed how the board is built ([`AssemblyPlan::is_current`]).
//!
//! Ordering has two layers. Physical dependencies ("W1 must go in before the
//! Nano covers it") form a directed graph that is always respected unless it
//! contains a cycle. Everything else is a heuristic preference (flat wires,
//! then small parts, then large ones; then the solder side), used only to
//! choose among steps that are ready at the same time.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fmt::Write as _;

use perfboard_core::{
    ComponentDefinition, ComponentInstance, ComponentKind, ConnectionState, JumperMatchKind,
    ObjectReference, PinRef, Rail, RoutePos, Saddle, Side, TerminalRef, Wire, WireColor,
    measure_route,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::connectivity::{pin_position, segment_step};
use crate::{Design, DocumentRevision};

/// Bare lead allowed at each wire end: board thickness plus a short tail to
/// solder. The same estimate as the wire templates.
pub const LEAD_MM: f64 = 5.0;
/// Bare tip of a rail end that lies between holes (half a rail window).
pub const RAIL_TIP_MM: f64 = 1.5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssemblyPlan {
    /// The working-copy revision the plan was derived from.
    pub source_revision: DocumentRevision,
    /// Hash of everything below. Equal fingerprints mean the board is built
    /// the same way, whatever else changed in the design.
    pub fingerprint: String,
    pub steps: Vec<AssemblyStep>,
    pub wire_list: Vec<WireListEntry>,
    pub warnings: Vec<AssemblyWarning>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Component,
    Wire,
    Rail,
    Bridge,
    Saddle,
    Check,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssemblyStep {
    /// Stable across plans: `component:<id>`, `wire:<id>`, `check:<name>`…
    pub id: String,
    /// 1-based position in the plan.
    pub number: u32,
    pub kind: StepKind,
    /// Short name as shown on the board: R1, W3, Nano.
    pub label: String,
    pub title: String,
    /// The face to look at while doing the step.
    pub side: Side,
    pub objects: Vec<ObjectReference>,
    /// Holes and points to mark: pins, wire ends, rail nodes, saddle legs.
    pub holes: Vec<RoutePos>,
    pub instructions: Vec<String>,
    /// Practical consequences worth knowing while doing the step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Steps that must be done first, and why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<StepDependency>,
    /// Check steps: the individual measurements, each with where to probe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<Probe>,
    /// Hash of what the step asks for, independent of its number. A completion
    /// checkmark still applies while the fingerprint is unchanged.
    pub fingerprint: String,
}

/// One measurement in a check step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Probe {
    /// What to measure, e.g. "GND: Nano · 4 GND, J1 · 1."
    pub text: String,
    /// IDs of the nets being measured, so their wiring can be shown.
    pub nets: Vec<String>,
    /// The test points: pin holes to touch with the meter's probes.
    pub holes: Vec<RoutePos>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepDependency {
    pub step_id: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WireListKind {
    Wire,
    Rail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WireListEntry {
    pub kind: WireListKind,
    pub id: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub net: String,
    pub color: WireColor,
    pub side: Side,
    /// Routed path length.
    pub path_mm: f64,
    /// Estimated length to cut: path plus bare leads. An estimate: real bends
    /// are not sharp.
    pub cut_mm: f64,
    pub bend_count: u32,
    /// A preformed jumper from the inventory that fits, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jumper: Option<String>,
    /// The step that installs it.
    pub step: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssemblyWarning {
    pub code: String,
    pub message: String,
    pub objects: Vec<ObjectReference>,
}

impl AssemblyPlan {
    /// Whether this plan still describes how to build `design`.
    #[must_use]
    pub fn is_current(&self, design: &Design) -> bool {
        assembly_plan(design, self.source_revision).fingerprint == self.fingerprint
    }
}

// ── Nodes ─────────────────────────────────────────────────────────────────────

/// Heuristic preference among steps that are ready together: lower first.
type Priority = (u8, u64, i64, i64, String, String);

struct Node {
    key: String,
    kind: StepKind,
    label: String,
    side: Side,
    objects: Vec<ObjectReference>,
    priority: Priority,
    /// Top-left point, for plain-language positions.
    holes: Vec<RoutePos>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Component(usize),
    Wire(usize),
    Rail(usize),
    Saddle(usize),
}

struct Context<'a> {
    design: &'a Design,
    components: Vec<(&'a ComponentInstance, &'a ComponentDefinition)>,
    wires: Vec<&'a Wire>,
    rails: Vec<&'a Rail>,
    saddles: Vec<&'a Saddle>,
    wire_labels: BTreeMap<String, String>,
    rail_labels: BTreeMap<String, String>,
    saddle_labels: BTreeMap<String, String>,
}

impl<'a> Context<'a> {
    fn new(design: &'a Design) -> Self {
        let components = design
            .components
            .values()
            .filter_map(|instance| {
                design
                    .component_definitions
                    .get(&instance.definition_id)
                    .map(|definition| (instance, definition))
            })
            .collect();
        // Labels follow document (map) order, like the editor and drawings.
        let labels = |prefix: &str, ids: Vec<String>| {
            ids.into_iter()
                .enumerate()
                .map(|(index, id)| (id, format!("{prefix}{}", index + 1)))
                .collect()
        };
        Self {
            design,
            components,
            wires: design.wires.values().collect(),
            rails: design.rails.values().collect(),
            saddles: design.saddles.values().collect(),
            wire_labels: labels("W", design.wires.keys().map(ToString::to_string).collect()),
            rail_labels: labels("R", design.rails.keys().map(ToString::to_string).collect()),
            saddle_labels: labels(
                "S",
                design.saddles.keys().map(ToString::to_string).collect(),
            ),
        }
    }

    fn wire_label(&self, wire: &Wire) -> String {
        self.wire_labels[&wire.id.to_string()].clone()
    }

    fn rail_label(&self, rail: &Rail) -> String {
        self.rail_labels[&rail.id.to_string()].clone()
    }

    fn component_index(&self, id: perfboard_core::ComponentInstanceId) -> Option<usize> {
        self.components
            .iter()
            .position(|(instance, _)| instance.id == id)
    }

    fn pin_display(&self, pin: &PinRef) -> String {
        let Some(index) = self.component_index(pin.component_id) else {
            return "a missing part".into();
        };
        let (instance, definition) = self.components[index];
        let Some(definition_pin) = definition.pins.iter().find(|item| item.id == pin.pin_id) else {
            return format!("{} · {}", instance.name, pin.pin_id);
        };
        if definition_pin.number == definition_pin.name {
            format!("{} · {}", instance.name, definition_pin.number)
        } else {
            format!(
                "{} · {} {}",
                instance.name, definition_pin.number, definition_pin.name
            )
        }
    }

    fn terminal_display(&self, terminal: &TerminalRef) -> String {
        match terminal {
            TerminalRef::Pin {
                component_id,
                pin_id,
            } => self.pin_display(&PinRef {
                component_id: *component_id,
                pin_id: pin_id.clone(),
            }),
            TerminalRef::RailNode { rail_id, position } => format!(
                "{} node {}",
                self.rail_labels
                    .get(&rail_id.to_string())
                    .map_or("rail", String::as_str),
                hole_text(*position)
            ),
            TerminalRef::Junction { .. } => "a junction".into(),
        }
    }

    fn net_name(&self, net_id: perfboard_core::NetId) -> String {
        self.design
            .nets
            .get(&net_id)
            .map_or_else(|| "an unknown net".into(), |net| net.name.clone())
    }
}

fn is_bridge(definition: &ComponentDefinition) -> bool {
    definition.kind == ComponentKind::Bridge
}

/// `(x, y)` in holes, with `.5` for points between holes.
fn hole_text(point: RoutePos) -> String {
    let coordinate = |value: i64| {
        if value % 2 == 0 {
            (value / 2).to_string()
        } else {
            format!("{}.5", value.div_euclid(2))
        }
    };
    format!("({}, {})", coordinate(point.x2), coordinate(point.y2))
}

fn color_name(color: &WireColor) -> String {
    match color {
        WireColor::Red => "red".into(),
        WireColor::Black => "black".into(),
        WireColor::Yellow => "yellow".into(),
        WireColor::White => "white".into(),
        WireColor::Green => "green".into(),
        WireColor::Blue => "blue".into(),
        WireColor::Orange => "orange".into(),
        WireColor::Other(name) => name.clone(),
    }
}

const fn side_name(side: Side) -> &'static str {
    match side {
        Side::Component => "component side",
        Side::Solder => "solder side",
    }
}

const fn opposite(side: Side) -> Side {
    match side {
        Side::Component => Side::Solder,
        Side::Solder => Side::Component,
    }
}

/// A component's body as `(min_x, min_y, max_x, max_y)` in holes.
fn body_rect(
    instance: &ComponentInstance,
    definition: &ComponentDefinition,
    pitch: f64,
) -> [f64; 4] {
    let body = &definition.body;
    let corners = [
        (body.x, body.y),
        (body.x + body.width, body.y),
        (body.x, body.y + body.height),
        (body.x + body.width, body.y + body.height),
    ]
    .map(|(x, y)| {
        let (x, y) = (x / pitch, y / pitch);
        match instance.rotation {
            perfboard_core::Rotation::Deg0 => (x, y),
            perfboard_core::Rotation::Deg90 => (-y, x),
            perfboard_core::Rotation::Deg180 => (-x, -y),
            perfboard_core::Rotation::Deg270 => (y, -x),
        }
    });
    let origin = (
        f64::from(instance.position.x),
        f64::from(instance.position.y),
    );
    let xs = corners.map(|corner| corner.0);
    let ys = corners.map(|corner| corner.1);
    [
        origin.0 + xs.iter().copied().fold(f64::INFINITY, f64::min),
        origin.1 + ys.iter().copied().fold(f64::INFINITY, f64::min),
        origin.0 + xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        origin.1 + ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    ]
}

/// Strictly inside, so a wire alongside a body or a pin on its edge is clear.
fn strictly_inside(rect: [f64; 4], point: RoutePos) -> bool {
    #[allow(clippy::cast_precision_loss)] // Board coordinates are small.
    let (x, y) = (point.x2 as f64 / 2.0, point.y2 as f64 / 2.0);
    let margin = 1e-6;
    x > rect[0] + margin && x < rect[2] - margin && y > rect[1] + margin && y < rect[3] - margin
}

fn body_area(rect: [f64; 4]) -> f64 {
    (rect[2] - rect[0]) * (rect[3] - rect[1])
}

/// Every half-grid point along a path, ends included.
fn expand(vertices: &[RoutePos]) -> Vec<RoutePos> {
    let mut result = Vec::new();
    for pair in vertices.windows(2) {
        let (dx, dy, steps) = segment_step(pair);
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

fn top_left(points: &[RoutePos]) -> (i64, i64) {
    points
        .iter()
        .map(|point| (point.y2, point.x2))
        .min()
        .unwrap_or((0, 0))
}

/// Where two paths cross or lie along each other, if they do. Touching at a
/// point (a wire ending on another, two wires meeting at a terminal) is not a
/// crossing; a proper crossing or a shared run of positive length is.
fn paths_cross(first: &[RoutePos], second: &[RoutePos]) -> Option<RoutePos> {
    let orient = |p: RoutePos, q: RoutePos, r: RoutePos| {
        ((q.x2 - p.x2) * (r.y2 - p.y2) - (q.y2 - p.y2) * (r.x2 - p.x2)).signum()
    };
    for a in first.windows(2) {
        for b in second.windows(2) {
            let (o1, o2, o3, o4) = (
                orient(a[0], a[1], b[0]),
                orient(a[0], a[1], b[1]),
                orient(b[0], b[1], a[0]),
                orient(b[0], b[1], a[1]),
            );
            if o1 * o2 < 0 && o3 * o4 < 0 {
                // A proper crossing: report the nearest half-grid point.
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let point = {
                    let (x1, y1, x2, y2) = (
                        a[0].x2 as f64,
                        a[0].y2 as f64,
                        a[1].x2 as f64,
                        a[1].y2 as f64,
                    );
                    let (x3, y3, x4, y4) = (
                        b[0].x2 as f64,
                        b[0].y2 as f64,
                        b[1].x2 as f64,
                        b[1].y2 as f64,
                    );
                    let denominator = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
                    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denominator;
                    RoutePos::new(
                        (x1 + t * (x2 - x1)).round() as i64,
                        (y1 + t * (y2 - y1)).round() as i64,
                    )
                };
                return Some(point);
            }
            if o1 == 0 && o2 == 0 {
                // Collinear: a shared run longer than a point is an overlap.
                let along = |p: RoutePos| {
                    (p.x2 - a[0].x2) * (a[1].x2 - a[0].x2) + (p.y2 - a[0].y2) * (a[1].y2 - a[0].y2)
                };
                let length = along(a[1]);
                let (low, high) = {
                    let (s, e) = (along(b[0]), along(b[1]));
                    (s.min(e).max(0), s.max(e).min(length))
                };
                if high > low && length > 0 {
                    // The start of the shared run, on the half-grid.
                    let at = |from: i64, to: i64| from + (to - from) * low / length;
                    return Some(RoutePos::new(at(a[0].x2, a[1].x2), at(a[0].y2, a[1].y2)));
                }
            }
        }
    }
    None
}

// ── Planning ──────────────────────────────────────────────────────────────────

/// Derives the build plan for `design`, recording `revision` as its source.
///
/// # Panics
/// Only on an internal inconsistency: every step is built from an object that
/// was just read from `design`.
#[must_use]
#[allow(clippy::too_many_lines)] // One readable pass from graph to steps.
pub fn assembly_plan(design: &Design, revision: DocumentRevision) -> AssemblyPlan {
    let context = Context::new(design);
    let pitch = design.board.pitch_mm;
    let mut nodes: Vec<Node> = Vec::new();
    let mut index_of: BTreeMap<Target, usize> = BTreeMap::new();

    let reference = |kind: &str, id: String| ObjectReference {
        kind: kind.into(),
        id,
    };

    for (position, (instance, definition)) in context.components.iter().enumerate() {
        let holes: Vec<RoutePos> = definition
            .pins
            .iter()
            .filter_map(|pin| {
                pin_position(
                    design,
                    &PinRef {
                        component_id: instance.id,
                        pin_id: pin.id.clone(),
                    },
                )
            })
            .collect();
        let (y, x) = top_left(&holes);
        let rect = body_rect(instance, definition, pitch);
        let (kind, side, priority) = if is_bridge(definition) {
            (StepKind::Bridge, Side::Solder, (5, 0, y, x))
        } else {
            // Smaller footprints first: low parts are soldered while the board
            // can still lie flat on its face.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let area = (body_area(rect) * 100.0).round().max(0.0) as u64;
            let phase = if body_area(rect) <= 12.0 { 1 } else { 2 };
            (StepKind::Component, Side::Component, (phase, area, y, x))
        };
        index_of.insert(Target::Component(position), nodes.len());
        nodes.push(Node {
            key: format!("component:{}", instance.id),
            kind,
            label: instance.name.clone(),
            side,
            objects: vec![reference("component", instance.id.to_string())],
            priority: (
                priority.0,
                priority.1,
                priority.2,
                priority.3,
                instance.name.clone(),
                instance.id.to_string(),
            ),
            holes,
        });
    }
    for (position, wire) in context.wires.iter().enumerate() {
        let (y, x) = top_left(&wire.vertices);
        let phase = match wire.side {
            Side::Component => 0,
            Side::Solder => 4,
        };
        index_of.insert(Target::Wire(position), nodes.len());
        let label = context.wire_label(wire);
        nodes.push(Node {
            key: format!("wire:{}", wire.id),
            kind: StepKind::Wire,
            label: label.clone(),
            side: wire.side,
            objects: vec![reference("wire", wire.id.to_string())],
            priority: (phase, 1, y, x, label, wire.id.to_string()),
            holes: [wire.vertices.first(), wire.vertices.last()]
                .into_iter()
                .flatten()
                .copied()
                .collect(),
        });
    }
    for (position, rail) in context.rails.iter().enumerate() {
        let (y, x) = top_left(&rail.vertices);
        let phase = match rail.side {
            Side::Component => 0,
            Side::Solder => 3,
        };
        index_of.insert(Target::Rail(position), nodes.len());
        let label = context.rail_label(rail);
        nodes.push(Node {
            key: format!("rail:{}", rail.id),
            kind: StepKind::Rail,
            label: label.clone(),
            side: rail.side,
            objects: vec![reference("rail", rail.id.to_string())],
            priority: (phase, 0, y, x, label, rail.id.to_string()),
            holes: rail.vertices.clone(),
        });
    }
    for (position, saddle) in context.saddles.iter().enumerate() {
        let holes = vec![saddle.start.as_route_pos(), saddle.end.as_route_pos()];
        let (y, x) = top_left(&holes);
        let phase = match saddle.side {
            Side::Component => 0,
            Side::Solder => 6,
        };
        index_of.insert(Target::Saddle(position), nodes.len());
        let label = context.saddle_labels[&saddle.id.to_string()].clone();
        nodes.push(Node {
            key: format!("saddle:{}", saddle.id),
            kind: StepKind::Saddle,
            label: label.clone(),
            side: saddle.side,
            objects: vec![reference("saddle", saddle.id.to_string())],
            priority: (phase, 2, y, x, label, saddle.id.to_string()),
            holes,
        });
    }

    // ── Physical dependencies: before → after, with the first reason found.
    let mut edges: BTreeMap<usize, BTreeMap<usize, String>> = BTreeMap::new();
    let mut add = |before: usize, after: usize, reason: String| {
        if before != after {
            edges
                .entry(before)
                .or_default()
                .entry(after)
                .or_insert(reason);
        }
    };
    let label_of = |nodes: &[Node], index: usize| nodes[index].label.clone();

    let rects: Vec<Option<[f64; 4]>> = context
        .components
        .iter()
        .map(|(instance, definition)| {
            (!is_bridge(definition)).then(|| body_rect(instance, definition, pitch))
        })
        .collect();

    // Conductors on the component side that run under a body go in first.
    let flat: Vec<(usize, &[RoutePos])> = context
        .wires
        .iter()
        .enumerate()
        .filter(|(_, wire)| wire.side == Side::Component)
        .map(|(position, wire)| (index_of[&Target::Wire(position)], wire.vertices.as_slice()))
        .chain(
            context
                .rails
                .iter()
                .enumerate()
                .filter(|(_, rail)| rail.side == Side::Component)
                .map(|(position, rail)| {
                    (index_of[&Target::Rail(position)], rail.vertices.as_slice())
                }),
        )
        .collect();
    for (conductor, vertices) in &flat {
        let points = expand(vertices);
        let interior = &points[1..points.len().saturating_sub(1)];
        for (position, rect) in rects.iter().enumerate() {
            let Some(rect) = rect else { continue };
            if interior.iter().any(|point| strictly_inside(*rect, *point)) {
                let component = index_of[&Target::Component(position)];
                add(
                    *conductor,
                    component,
                    format!(
                        "{} runs under {}'s body",
                        label_of(&nodes, *conductor),
                        label_of(&nodes, component)
                    ),
                );
            }
        }
    }

    // A body that covers another part's pins goes in after that part.
    for (cover, rect) in rects.iter().enumerate() {
        let Some(rect) = rect else { continue };
        for (covered, _) in rects
            .iter()
            .enumerate()
            .filter(|(_, other)| other.is_some())
        {
            if covered == cover {
                continue;
            }
            let pins = &nodes[index_of[&Target::Component(covered)]].holes;
            if pins.iter().any(|pin| strictly_inside(*rect, *pin)) {
                let (before, after) = (
                    index_of[&Target::Component(covered)],
                    index_of[&Target::Component(cover)],
                );
                add(
                    before,
                    after,
                    format!(
                        "{}'s body covers {}'s pins",
                        label_of(&nodes, after),
                        label_of(&nodes, before)
                    ),
                );
            }
        }
    }

    // A bridge is the solder joint for everything that ends in its holes.
    for (position, (_, definition)) in context.components.iter().enumerate() {
        if !is_bridge(definition) {
            continue;
        }
        let bridge = index_of[&Target::Component(position)];
        let holes: BTreeSet<RoutePos> = nodes[bridge].holes.iter().copied().collect();
        for (other, (_, other_definition)) in context.components.iter().enumerate() {
            if other == position || is_bridge(other_definition) {
                continue;
            }
            let part = index_of[&Target::Component(other)];
            if nodes[part].holes.iter().any(|pin| holes.contains(pin)) {
                add(
                    part,
                    bridge,
                    format!(
                        "{} joins a pin of {}",
                        label_of(&nodes, bridge),
                        label_of(&nodes, part)
                    ),
                );
            }
        }
        for (wire_position, wire) in context.wires.iter().enumerate() {
            let ends_here = [&wire.start, &wire.end].into_iter().any(|terminal| {
                matches!(terminal, TerminalRef::Pin { component_id, .. }
                    if *component_id == context.components[position].0.id)
            }) || [wire.vertices.first(), wire.vertices.last()]
                .into_iter()
                .flatten()
                .any(|point| holes.contains(point));
            if ends_here {
                let wire_node = index_of[&Target::Wire(wire_position)];
                add(
                    wire_node,
                    bridge,
                    format!(
                        "{} ends in a hole that {} solders",
                        label_of(&nodes, wire_node),
                        label_of(&nodes, bridge)
                    ),
                );
            }
        }
    }

    // A solder-side wire soldered to a part's pin, or any wire soldered to a
    // rail node, needs it in place.
    let mut shared_holes: Vec<(usize, String)> = Vec::new();
    for (wire_position, wire) in context.wires.iter().enumerate() {
        let wire_node = index_of[&Target::Wire(wire_position)];
        for terminal in [&wire.start, &wire.end] {
            match terminal {
                TerminalRef::Pin { component_id, .. } => {
                    let Some(part) = context.component_index(*component_id) else {
                        continue;
                    };
                    if is_bridge(context.components[part].1) {
                        continue;
                    }
                    let part_node = index_of[&Target::Component(part)];
                    if wire.side == Side::Component {
                        // The lead shares the pin's hole: either it goes in
                        // first or it is soldered to the pin's lead below, so
                        // neither order is forced.
                        shared_holes.push((
                            wire_node,
                            format!(
                                "Its end at {} shares that pin's hole: insert it before {}, \
                                 or solder it to the pin's lead on the solder side.",
                                context.terminal_display(terminal),
                                label_of(&nodes, part_node)
                            ),
                        ));
                        continue;
                    }
                    add(
                        part_node,
                        wire_node,
                        format!(
                            "{} is soldered to a pin of {}",
                            label_of(&nodes, wire_node),
                            label_of(&nodes, part_node)
                        ),
                    );
                }
                TerminalRef::RailNode { rail_id, .. } => {
                    if let Some(rail) = context.rails.iter().position(|rail| rail.id == *rail_id) {
                        let rail_node = index_of[&Target::Rail(rail)];
                        add(
                            rail_node,
                            wire_node,
                            format!(
                                "{} is soldered to {}",
                                label_of(&nodes, wire_node),
                                label_of(&nodes, rail_node)
                            ),
                        );
                    }
                }
                TerminalRef::Junction { .. } => {}
            }
        }
        // A rail lies flat on the board, so a wire crossing it goes over it.
        for (rail_position, rail) in context.rails.iter().enumerate() {
            if rail.side == wire.side && paths_cross(&wire.vertices, &rail.vertices).is_some() {
                let rail_node = index_of[&Target::Rail(rail_position)];
                add(
                    rail_node,
                    wire_node,
                    format!(
                        "{} passes over {}",
                        label_of(&nodes, wire_node),
                        label_of(&nodes, rail_node)
                    ),
                );
            }
        }
    }

    // Recorded crossing choices: the wire underneath goes in first.
    let wire_node_of = |id: perfboard_core::WireId| {
        context
            .wires
            .iter()
            .position(|wire| wire.id == id)
            .map(|position| index_of[&Target::Wire(position)])
    };
    let mut recorded: BTreeSet<(usize, usize)> = BTreeSet::new();
    for choice in design.crossing_choices.values() {
        let under_id = if choice.over_wire_id == choice.first_wire_id {
            choice.second_wire_id
        } else {
            choice.first_wire_id
        };
        if let (Some(under), Some(over)) =
            (wire_node_of(under_id), wire_node_of(choice.over_wire_id))
        {
            recorded.insert((under.min(over), under.max(over)));
            add(
                under,
                over,
                format!(
                    "{} passes over {}",
                    label_of(&nodes, over),
                    label_of(&nodes, under)
                ),
            );
        }
    }

    // A saddle holds down the conductors it crosses, so it comes after them.
    for (saddle_position, saddle) in context.saddles.iter().enumerate() {
        let saddle_node = index_of[&Target::Saddle(saddle_position)];
        let strap = [saddle.start.as_route_pos(), saddle.end.as_route_pos()];
        let held = context
            .wires
            .iter()
            .enumerate()
            .filter(|(_, wire)| wire.side == saddle.side)
            .filter(|(_, wire)| paths_cross(&strap, &wire.vertices).is_some())
            .map(|(position, _)| index_of[&Target::Wire(position)])
            .chain(
                context
                    .rails
                    .iter()
                    .enumerate()
                    .filter(|(_, rail)| rail.side == saddle.side)
                    .filter(|(_, rail)| paths_cross(&strap, &rail.vertices).is_some())
                    .map(|(position, _)| index_of[&Target::Rail(position)]),
            )
            .collect::<Vec<_>>();
        for conductor in held {
            add(
                conductor,
                saddle_node,
                format!(
                    "{} holds {} down",
                    label_of(&nodes, saddle_node),
                    label_of(&nodes, conductor)
                ),
            );
        }
    }

    // ── Order: Kahn's algorithm, ready steps taken in priority order.
    let count = nodes.len();
    let mut incoming = vec![0usize; count];
    for targets in edges.values() {
        for &after in targets.keys() {
            incoming[after] += 1;
        }
    }
    let mut ready: BinaryHeap<Reverse<(Priority, usize)>> = (0..count)
        .filter(|&index| incoming[index] == 0)
        .map(|index| Reverse((nodes[index].priority.clone(), index)))
        .collect();
    let mut placed = vec![false; count];
    let mut order: Vec<usize> = Vec::with_capacity(count);
    let mut warnings: Vec<AssemblyWarning> = Vec::new();
    while order.len() < count {
        let next = if let Some(Reverse((_, index))) = ready.pop() {
            index
        } else {
            // Every remaining step waits on another: a cycle. Report it and
            // break it at the step the heuristics would do first.
            let cycle = find_cycle(&edges, &placed);
            let members: Vec<String> = cycle.iter().map(|&index| label_of(&nodes, index)).collect();
            warnings.push(AssemblyWarning {
                code: "assembly.cycle".into(),
                message: format!(
                    "{} each need another to go in first, so no order satisfies them all. \
                     The plan starts with {}; check that step by hand, or reroute to break the loop.",
                    members.join(", "),
                    label_of(
                        &nodes,
                        *cycle
                            .iter()
                            .min_by_key(|&&index| nodes[index].priority.clone())
                            .unwrap_or(&cycle[0])
                    )
                ),
                objects: cycle
                    .iter()
                    .flat_map(|&index| nodes[index].objects.clone())
                    .collect(),
            });
            (0..count)
                .filter(|&index| !placed[index])
                .min_by_key(|&index| nodes[index].priority.clone())
                .expect("an unplaced step remains")
        };
        if placed[next] {
            continue;
        }
        placed[next] = true;
        order.push(next);
        if let Some(targets) = edges.get(&next) {
            for &after in targets.keys() {
                incoming[after] = incoming[after].saturating_sub(1);
                if incoming[after] == 0 && !placed[after] {
                    ready.push(Reverse((nodes[after].priority.clone(), after)));
                }
            }
        }
    }
    let rank: BTreeMap<usize, usize> = order
        .iter()
        .enumerate()
        .map(|(position, &index)| (index, position))
        .collect();

    // ── Notes that depend on the chosen order.
    let mut step_notes: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (node, note) in shared_holes {
        step_notes.entry(node).or_default().push(note);
    }
    for (first, wire) in context.wires.iter().enumerate() {
        for (second, other) in context.wires.iter().enumerate().skip(first + 1) {
            if wire.side != other.side {
                continue;
            }
            let Some(point) = paths_cross(&wire.vertices, &other.vertices) else {
                continue;
            };
            let (a, b) = (
                index_of[&Target::Wire(first)],
                index_of[&Target::Wire(second)],
            );
            let (under, over) = if rank[&a] < rank[&b] { (a, b) } else { (b, a) };
            step_notes.entry(over).or_default().push(format!(
                "Crosses {} at {}: lay it over {}, which is already in place.",
                label_of(&nodes, under),
                hole_text(point),
                label_of(&nodes, under)
            ));
            if !recorded.contains(&(a.min(b), a.max(b))) {
                warnings.push(AssemblyWarning {
                    code: "assembly.unordered_crossing".into(),
                    message: format!(
                        "{} and {} cross at {} with no recorded order. The plan lays {} first, \
                         so {} goes over it; either order works.",
                        label_of(&nodes, under),
                        label_of(&nodes, over),
                        hole_text(point),
                        label_of(&nodes, under),
                        label_of(&nodes, over)
                    ),
                    objects: [under, over]
                        .iter()
                        .flat_map(|&index| nodes[index].objects.clone())
                        .collect(),
                });
            }
        }
    }
    for (position, wire) in context.wires.iter().enumerate() {
        if has_sharp_turn(&wire.vertices) {
            let node = index_of[&Target::Wire(position)];
            let message = format!(
                "{} turns back by 135°: form that bend with pliers before fitting it.",
                label_of(&nodes, node)
            );
            step_notes.entry(node).or_default().push(message.clone());
            warnings.push(AssemblyWarning {
                code: "assembly.sharp_bend".into(),
                message,
                objects: nodes[node].objects.clone(),
            });
        }
    }

    // ── Steps.
    let mut steps: Vec<AssemblyStep> = Vec::with_capacity(count + 3);
    let mut wire_list: Vec<WireListEntry> = Vec::new();
    for (position, &index) in order.iter().enumerate() {
        let node = &nodes[index];
        let number = u32::try_from(position + 1).unwrap_or(u32::MAX);
        let mut after: Vec<StepDependency> = edges
            .iter()
            .filter_map(|(&before, targets)| {
                targets.get(&index).map(|reason| (before, reason.clone()))
            })
            .map(|(before, reason)| StepDependency {
                step_id: nodes[before].key.clone(),
                reason,
            })
            .collect();
        after.sort_by(|a, b| a.step_id.cmp(&b.step_id));
        let (title, instructions) = match node.kind {
            StepKind::Component | StepKind::Bridge => {
                let target = node.objects[0].id.clone();
                let (instance, definition) = context
                    .components
                    .iter()
                    .find(|(instance, _)| instance.id.to_string() == target)
                    .copied()
                    .expect("component node");
                if node.kind == StepKind::Bridge {
                    bridge_text(node, instance)
                } else {
                    component_text(node, instance, definition)
                }
            }
            StepKind::Wire => {
                let wire = context
                    .wires
                    .iter()
                    .find(|wire| wire.id.to_string() == node.objects[0].id)
                    .copied()
                    .expect("wire node");
                let entry = wire_entry(&context, wire, number);
                // Bridges whose holes the ends land in solder them later.
                let landings: Vec<String> = context
                    .components
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, definition))| is_bridge(definition))
                    .filter(|(position, _)| {
                        let holes = &nodes[index_of[&Target::Component(*position)]].holes;
                        [wire.vertices.first(), wire.vertices.last()]
                            .into_iter()
                            .flatten()
                            .any(|end| holes.contains(end))
                    })
                    .map(|(_, (instance, _))| instance.name.clone())
                    .collect();
                let text = wire_text(node, wire, &entry, &landings);
                wire_list.push(entry);
                text
            }
            StepKind::Rail => {
                let rail = context
                    .rails
                    .iter()
                    .find(|rail| rail.id.to_string() == node.objects[0].id)
                    .copied()
                    .expect("rail node");
                let entry = rail_entry(&context, rail, number);
                let text = rail_text(node, rail, &entry);
                wire_list.push(entry);
                text
            }
            StepKind::Saddle => {
                let saddle = context
                    .saddles
                    .iter()
                    .find(|saddle| saddle.id.to_string() == node.objects[0].id)
                    .copied()
                    .expect("saddle node");
                let held: Vec<String> = after
                    .iter()
                    .filter(|dependency| dependency.reason.contains("holds"))
                    .filter_map(|dependency| {
                        nodes
                            .iter()
                            .find(|candidate| candidate.key == dependency.step_id)
                            .map(|candidate| candidate.label.clone())
                    })
                    .collect();
                saddle_text(node, saddle, &held)
            }
            StepKind::Check => unreachable!("checks are added below"),
        };
        steps.push(finish_step(
            node.key.clone(),
            number,
            node.kind,
            node.label.clone(),
            title,
            node.side,
            node.objects.clone(),
            node.holes.clone(),
            instructions,
            step_notes.remove(&index).unwrap_or_default(),
            after,
            Vec::new(),
        ));
    }
    for check in check_steps(&context) {
        let number = u32::try_from(steps.len() + 1).unwrap_or(u32::MAX);
        steps.push(finish_step(
            check.key,
            number,
            StepKind::Check,
            check.label,
            check.title,
            Side::Solder,
            Vec::new(),
            Vec::new(),
            check.instructions,
            Vec::new(),
            Vec::new(),
            check.probes,
        ));
    }
    wire_list.sort_by(|a, b| {
        let key = |entry: &WireListEntry| {
            (
                entry.kind == WireListKind::Rail,
                entry.label[1..].parse::<u32>().unwrap_or(u32::MAX),
            )
        };
        key(a).cmp(&key(b))
    });

    let fingerprint = hash(&(&steps, &wire_list, &warnings));
    AssemblyPlan {
        source_revision: revision,
        fingerprint,
        steps,
        wire_list,
        warnings,
    }
}

/// Some step that is part of a cycle among the unplaced steps.
fn find_cycle(edges: &BTreeMap<usize, BTreeMap<usize, String>>, placed: &[bool]) -> Vec<usize> {
    // Walk "before" links from any unplaced step: in a graph where every
    // unplaced step has an unplaced predecessor, the walk must revisit a step.
    let predecessor = |index: usize| {
        edges
            .iter()
            .find(|&(&before, targets)| !placed[before] && targets.contains_key(&index))
            .map(|(&before, _)| before)
    };
    let Some(start) = (0..placed.len()).find(|&index| !placed[index]) else {
        return Vec::new();
    };
    let mut seen: Vec<usize> = Vec::new();
    let mut current = start;
    loop {
        if let Some(position) = seen.iter().position(|&index| index == current) {
            let mut cycle = seen[position..].to_vec();
            cycle.reverse();
            return cycle;
        }
        seen.push(current);
        match predecessor(current) {
            Some(previous) => current = previous,
            None => return vec![current],
        }
    }
}

fn has_sharp_turn(vertices: &[RoutePos]) -> bool {
    vertices.windows(3).any(|points| {
        let first = (points[1].x2 - points[0].x2, points[1].y2 - points[0].y2);
        let second = (points[2].x2 - points[1].x2, points[2].y2 - points[1].y2);
        // A turn of more than 90° has a negative dot product.
        first.0 * second.0 + first.1 * second.1 < 0
    })
}

fn hash<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("plan data serializes");
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(12)
        .fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[allow(clippy::too_many_arguments)] // A plain constructor; every field is named at the call.
fn finish_step(
    id: String,
    number: u32,
    kind: StepKind,
    label: String,
    title: String,
    side: Side,
    objects: Vec<ObjectReference>,
    holes: Vec<RoutePos>,
    instructions: Vec<String>,
    notes: Vec<String>,
    after: Vec<StepDependency>,
    probes: Vec<Probe>,
) -> AssemblyStep {
    let fingerprint = hash(&(
        &id,
        kind,
        &title,
        side,
        &objects,
        &holes,
        &instructions,
        &probes,
    ));
    AssemblyStep {
        id,
        number,
        kind,
        label,
        title,
        side,
        objects,
        holes,
        instructions,
        notes,
        after,
        probes,
        fingerprint,
    }
}

// ── Step text ─────────────────────────────────────────────────────────────────

fn rotation_text(rotation: perfboard_core::Rotation) -> &'static str {
    match rotation {
        perfboard_core::Rotation::Deg0 => "",
        perfboard_core::Rotation::Deg90 => ", turned 90°",
        perfboard_core::Rotation::Deg180 => ", turned 180°",
        perfboard_core::Rotation::Deg270 => ", turned 270°",
    }
}

fn component_text(
    node: &Node,
    instance: &ComponentInstance,
    definition: &ComponentDefinition,
) -> (String, Vec<String>) {
    let first = definition.pins.first();
    let mut instructions = vec![match (first, node.holes.first()) {
        (Some(pin), Some(hole)) => format!(
            "Insert {} on the component side{}, with pin {} in hole {}.",
            instance.name,
            rotation_text(instance.rotation),
            pin.number,
            hole_text(*hole)
        ),
        _ => format!("Insert {} on the component side.", instance.name),
    }];
    instructions.push(match definition.pins.len() {
        1 => "Solder its pin on the solder side.".into(),
        count => format!("Solder its {count} pins on the solder side."),
    });
    if definition.kind == ComponentKind::Jumper {
        instructions.push(if instance.shunted {
            "Fit the shunt: it joins the two pins.".into()
        } else {
            "Leave the shunt off (open).".into()
        });
    }
    (
        format!("Fit {} ({})", instance.name, definition.name),
        instructions,
    )
}

fn bridge_text(node: &Node, instance: &ComponentInstance) -> (String, Vec<String>) {
    let holes: Vec<String> = node.holes.iter().map(|hole| hole_text(*hole)).collect();
    (
        format!("Bridge {}", instance.name),
        vec![
            format!(
                "On the solder side, join the pads at {} with a short bare link or a solder bridge.",
                holes.join(" and ")
            ),
            "Make sure every lead in those holes is caught in the joint.".into(),
        ],
    )
}

fn wire_entry(context: &Context<'_>, wire: &Wire, step: u32) -> WireListEntry {
    let measured = measure_route(&wire.vertices, context.design.board.pitch_mm);
    let jumper = crate::routing::jumper_match(context.design, &wire.vertices).map(|matched| {
        let fit = match matched.kind {
            JumperMatchKind::Exact => "exact",
            JumperMatchKind::Approximate => "close",
            JumperMatchKind::Reshaped => "reshaped",
        };
        format!(
            "{} ({} holes, {fit})",
            matched.inventory_id, matched.inventory_span_holes
        )
    });
    WireListEntry {
        kind: WireListKind::Wire,
        id: wire.id.to_string(),
        label: context.wire_label(wire),
        from: context.terminal_display(&wire.start),
        to: context.terminal_display(&wire.end),
        net: context.net_name(wire.net_id),
        color: wire.color.clone(),
        side: wire.side,
        path_mm: measured.length_mm,
        cut_mm: measured.length_mm + 2.0 * LEAD_MM,
        bend_count: measured.bend_count,
        jumper,
        step,
    }
}

fn wire_text(
    node: &Node,
    wire: &Wire,
    entry: &WireListEntry,
    landings: &[String],
) -> (String, Vec<String>) {
    let start = wire
        .vertices
        .first()
        .copied()
        .unwrap_or(RoutePos::new(0, 0));
    let end = wire.vertices.last().copied().unwrap_or(start);
    let shape = match entry.bend_count {
        0 => "straight".to_string(),
        1 => "1 bend".to_string(),
        count => format!("{count} bends"),
    };
    let mut instructions = vec![match &entry.jumper {
        Some(jumper) => format!(
            "Use a preformed jumper: {jumper}. Otherwise cut ≈ {:.0} mm of {} wire.",
            entry.cut_mm,
            color_name(&wire.color)
        ),
        None => format!(
            "Cut ≈ {:.0} mm of {} wire and strip {LEAD_MM:.0} mm at each end; bend it to its template ({shape}).",
            entry.cut_mm,
            color_name(&wire.color)
        ),
    }];
    instructions.push(format!(
        "Lay it on the {} from {} at {} to {} at {}.",
        side_name(wire.side),
        entry.from,
        hole_text(start),
        entry.to,
        hole_text(end)
    ));
    instructions.push(match (wire.side, landings.is_empty()) {
        (Side::Component, true) => {
            "Push the leads through and solder them on the solder side.".into()
        }
        (Side::Solder, true) => "Solder both ends.".into(),
        (side, false) => format!(
            "{} the ends land on {}, which solder{} them in a later step.",
            match side {
                Side::Component => "Push the leads through:",
                Side::Solder => "Put the leads through their holes:",
            },
            landings.join(" and "),
            if landings.len() == 1 { "s" } else { "" }
        ),
    });
    (
        format!(
            "Wire {} · {} · {}",
            node.label,
            color_name(&wire.color),
            entry.net
        ),
        instructions,
    )
}

fn rail_ends(rail: &Rail) -> [f64; 2] {
    let lead = |point: Option<&RoutePos>| {
        if point.is_some_and(|point| point.is_hole()) {
            LEAD_MM
        } else {
            RAIL_TIP_MM
        }
    };
    [lead(rail.vertices.first()), lead(rail.vertices.last())]
}

fn rail_entry(context: &Context<'_>, rail: &Rail, step: u32) -> WireListEntry {
    let measured = measure_route(&rail.vertices, context.design.board.pitch_mm);
    let [start, end] = rail_ends(rail);
    let first = rail
        .vertices
        .first()
        .copied()
        .unwrap_or(RoutePos::new(0, 0));
    let last = rail.vertices.last().copied().unwrap_or(first);
    WireListEntry {
        kind: WireListKind::Rail,
        id: rail.id.to_string(),
        label: context.rail_label(rail),
        from: format!("end {}", hole_text(first)),
        to: format!("end {}", hole_text(last)),
        net: context.net_name(rail.net_id),
        color: rail.color.clone(),
        side: rail.side,
        path_mm: measured.length_mm,
        cut_mm: measured.length_mm + start + end,
        bend_count: measured.bend_count,
        jumper: None,
        step,
    }
}

fn rail_text(node: &Node, rail: &Rail, entry: &WireListEntry) -> (String, Vec<String>) {
    let taps = rail.vertices.len().saturating_sub(2);
    (
        format!(
            "Rail {} · {} · {}",
            node.label,
            color_name(&rail.color),
            entry.net
        ),
        vec![
            format!(
                "Cut ≈ {:.0} mm of {} wire, bend it to its template and slice the insulation at the marked windows.",
                entry.cut_mm,
                color_name(&rail.color)
            ),
            format!(
                "Lay it on the {} from {} to {}{}.",
                side_name(rail.side),
                entry.from.trim_start_matches("end "),
                entry.to.trim_start_matches("end "),
                match taps {
                    0 => String::new(),
                    1 => ", with 1 node between".into(),
                    count => format!(", with {count} nodes between"),
                }
            ),
            "Solder the ends into their holes (clip any you do not need) and solder it at its nodes."
                .into(),
        ],
    )
}

fn saddle_text(node: &Node, saddle: &Saddle, held: &[String]) -> (String, Vec<String>) {
    (
        format!("Saddle {}", node.label),
        vec![
            format!(
                "On the {}, fit a staple of bare wire{} with its legs through holes {} and {}.",
                side_name(saddle.side),
                if held.is_empty() {
                    String::new()
                } else {
                    format!(" over {}", held.join(", "))
                },
                hole_text(saddle.start.as_route_pos()),
                hole_text(saddle.end.as_route_pos()),
            ),
            format!(
                "Solder its legs on the {}.",
                side_name(opposite(saddle.side))
            ),
        ],
    )
}

fn is_ground(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["GND", "VSS", "0V"].iter().any(|word| {
        upper == *word
            || upper.starts_with(&format!("{word}_"))
            || upper.ends_with(&format!("_{word}"))
    })
}

fn is_power(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.starts_with('+')
        || ["VCC", "VDD", "VIN", "VBUS", "5V", "3V3", "12V"]
            .iter()
            .any(|word| upper.contains(word))
}

struct CheckStep {
    key: String,
    label: String,
    title: String,
    instructions: Vec<String>,
    probes: Vec<Probe>,
}

/// A net's component pins (bridges excluded), as display names and holes, in
/// designator order: pins are keyed by random component IDs, so their set
/// order means nothing to the reader.
fn net_test_points(
    context: &Context<'_>,
    net: &perfboard_core::ElectricalNet,
) -> Vec<(String, RoutePos)> {
    let mut points: Vec<(String, RoutePos)> = net
        .pins
        .iter()
        .filter(|pin| {
            context
                .component_index(pin.component_id)
                .is_some_and(|index| !is_bridge(context.components[index].1))
        })
        .filter_map(|pin| {
            pin_position(context.design, pin).map(|hole| (context.pin_display(pin), hole))
        })
        .collect();
    points.sort_by(|left, right| natural_cmp(&left.0, &right.0).then(left.1.cmp(&right.1)));
    points
}

/// Orders text with embedded numbers by value, so `R2` comes before `R10`.
fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    fn runs(text: &str) -> impl Iterator<Item = &str> {
        let mut rest = text;
        std::iter::from_fn(move || {
            let first = rest.chars().next()?;
            let digit = first.is_ascii_digit();
            let end = rest
                .find(|c: char| c.is_ascii_digit() != digit)
                .unwrap_or(rest.len());
            let (run, tail) = rest.split_at(end);
            rest = tail;
            Some(run)
        })
    }
    let mut left_runs = runs(left);
    let mut right_runs = runs(right);
    loop {
        match (left_runs.next(), right_runs.next()) {
            (None, None) => return left.cmp(right),
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => {
                let numeric = a.starts_with(|c: char| c.is_ascii_digit())
                    && b.starts_with(|c: char| c.is_ascii_digit());
                let order = if numeric {
                    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
                    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
                } else {
                    a.cmp(b)
                };
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

fn check_steps(context: &Context<'_>) -> Vec<CheckStep> {
    let mut steps = vec![CheckStep {
        key: "check:inspect".into(),
        label: "Inspect".into(),
        title: "Inspect the solder side".into(),
        instructions: vec![
            "Look for solder bridges between neighbouring pads that the design does not have."
                .into(),
            "Reflow any joint that looks dull, cracked or balled up.".into(),
        ],
        probes: Vec::new(),
    }];
    // Nets are keyed by random IDs; list them by name.
    let mut accepted: Vec<&perfboard_core::ElectricalNet> = context
        .design
        .nets
        .values()
        .filter(|net| net.state == ConnectionState::Accepted)
        .collect();
    accepted
        .sort_by(|left, right| natural_cmp(&left.name, &right.name).then(left.id.cmp(&right.id)));
    let continuity: Vec<Probe> = accepted
        .iter()
        .filter_map(|net| {
            let points = net_test_points(context, net);
            if points.len() < 2 {
                return None;
            }
            let shown = points
                .iter()
                .take(6)
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let text = if points.len() > 6 {
                format!("{}: {shown} and {} more.", net.name, points.len() - 6)
            } else {
                format!("{}: {shown}.", net.name)
            };
            Some(Probe {
                text,
                nets: vec![net.id.to_string()],
                holes: points.into_iter().map(|(_, hole)| hole).collect(),
            })
        })
        .collect();
    if !continuity.is_empty() {
        steps.push(CheckStep {
            key: "check:continuity".into(),
            label: "Continuity".into(),
            title: "Check continuity".into(),
            instructions: vec![
                "With a multimeter on continuity, check that each net below connects all its pins."
                    .into(),
            ],
            probes: continuity,
        });
    }
    let ground = accepted.iter().find(|net| is_ground(&net.name));
    let power: Vec<&&perfboard_core::ElectricalNet> = accepted
        .iter()
        .filter(|net| is_power(&net.name) && !is_ground(&net.name))
        .collect();
    if let Some(ground) = ground
        && !power.is_empty()
    {
        let ground_point = net_test_points(context, ground).into_iter().next();
        let probes = power
            .iter()
            .map(|net| {
                let point = net_test_points(context, net).into_iter().next();
                let at = match (&point, &ground_point) {
                    (Some((from, _)), Some((to, _))) => format!(", for example {from} to {to}"),
                    _ => String::new(),
                };
                Probe {
                    text: format!("{} and {}{at}.", net.name, ground.name),
                    nets: vec![net.id.to_string(), ground.id.to_string()],
                    holes: point
                        .iter()
                        .chain(ground_point.iter())
                        .map(|(_, hole)| *hole)
                        .collect(),
                }
            })
            .collect();
        steps.push(CheckStep {
            key: "check:power".into(),
            label: "Power".into(),
            title: "Check for shorts before powering".into(),
            instructions: vec![
                "Measure the resistance between each pair of nets below.".into(),
                "A reading near 0 Ω means a short: find it before connecting power.".into(),
            ],
            probes,
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_compares_numbers_by_value() {
        let mut names = vec![
            "R10 · 1", "R2 · 10", "R2 · 2", "C1", "R1 · 1", "R01 · 1", "r1",
        ];
        names.sort_by(|left, right| super::natural_cmp(left, right));
        assert_eq!(
            names,
            [
                "C1", "R01 · 1", "R1 · 1", "R2 · 2", "R2 · 10", "R10 · 1", "r1"
            ]
        );
    }

    const fn p(x2: i64, y2: i64) -> RoutePos {
        RoutePos::new(x2, y2)
    }

    #[test]
    fn crossing_paths_are_told_from_touching_ones() {
        // An X crossing.
        assert!(paths_cross(&[p(0, 0), p(4, 4)], &[p(0, 4), p(4, 0)]).is_some());
        // A T: one wire ends on the other.
        assert!(paths_cross(&[p(0, 0), p(4, 0)], &[p(2, 0), p(2, 4)]).is_none());
        // Meeting at a shared end.
        assert!(paths_cross(&[p(0, 0), p(4, 0)], &[p(4, 0), p(4, 4)]).is_none());
        // Lying along each other.
        assert!(paths_cross(&[p(0, 0), p(6, 0)], &[p(2, 0), p(8, 0)]).is_some());
        // Parallel neighbours.
        assert!(paths_cross(&[p(0, 0), p(6, 0)], &[p(0, 2), p(6, 2)]).is_none());
    }

    #[test]
    fn hole_text_marks_half_positions() {
        assert_eq!(hole_text(p(4, 7)), "(2, 3.5)");
        assert_eq!(hole_text(p(-1, 0)), "(-1.5, 0)");
    }

    #[test]
    fn a_turn_back_is_sharp() {
        assert!(has_sharp_turn(&[p(0, 0), p(4, 0), p(2, 2)]));
        assert!(!has_sharp_turn(&[p(0, 0), p(4, 0), p(4, 4)]));
        assert!(!has_sharp_turn(&[p(0, 0), p(4, 0), p(6, 2)]));
    }
}
