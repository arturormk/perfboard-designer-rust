//! Test points (ADR 0022): what a multimeter should read on a built board,
//! worked out from the design alone, so the assembly plan and the breadboard
//! layout give the same expected values.
//!
//! Only what the design proves is predicted. With the power off the circuit
//! is a network of nets (nodes) joined by parts:
//!
//! - a resistor with a value and two pins is a resistance, with its colour
//!   code's tolerance; a zero-ohm link, a bridge and a jumper with its shunt
//!   fitted merge their nets;
//! - a capacitor, and a jumper without its shunt, is open;
//! - anything else with pins on two or more nets (diodes, transistors, ICs, a
//!   resistor with no value or other than two pins) is an unknown element: a
//!   meter's test voltage may or may not make it conduct.
//!
//! The reading between two nets depends only on the biconnected blocks along
//! the path between them in the block–cut tree. When none of those holds an
//! unknown element, the effective resistance over them is exact; otherwise
//! the reading is not predicted at all.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use perfboard_core::{
    BandCount, ComponentKind, ConnectionState, NetId, PinRef, ValueKind, read_value,
    resistance_ohms,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Design, connectivity::copper_joined_pins};

/// What a measurement should read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Measure {
    /// The points should beep on the continuity setting.
    Continuity,
    /// No connection: the meter should read open.
    Open,
    /// A resistance in ohms, power off, with the range the parts'
    /// tolerances allow (the meter's own accuracy comes on top).
    Resistance { nominal: f64, min: f64, max: f64 },
}

/// Whether a net name reads as ground.
pub(crate) fn is_ground(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["GND", "VSS", "0V"].iter().any(|word| {
        upper == *word
            || upper.starts_with(&format!("{word}_"))
            || upper.ends_with(&format!("_{word}"))
    })
}

/// Whether a net name reads as a supply.
pub(crate) fn is_power(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.starts_with('+')
        || ["VCC", "VDD", "VIN", "VBUS", "5V", "3V3", "12V"]
            .iter()
            .any(|word| upper.contains(word))
}

// ── Readings ─────────────────────────────────────────────────────────────────

/// A predicted resistance between two nets.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Reading {
    pub nominal: f64,
    pub min: f64,
    pub max: f64,
    /// Other resistors that share the measured path, by name, so the
    /// builder knows why a reading is not the part's own value.
    pub alongside: Vec<String>,
}

impl Reading {
    pub(crate) fn measure(&self) -> Measure {
        Measure::Resistance {
            nominal: self.nominal,
            min: self.min,
            max: self.max,
        }
    }

    /// `4.7 kΩ (4.65–4.75 kΩ)`.
    pub(crate) fn text(&self) -> String {
        format!(
            "{} ({})",
            format_ohms(self.nominal),
            ohms_range(self.min, self.max)
        )
    }
}

/// What the network says about two nets.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Prediction {
    Reading(Reading),
    /// No part at all joins them.
    Open,
    /// Parts whose behaviour the design does not know lie on the path.
    Unknown {
        blockers: Vec<String>,
    },
}

/// One resistor measured in place, across its own leads.
#[derive(Clone, Debug)]
pub(crate) struct ResistorCheck {
    pub name: String,
    /// The value as shown, such as `4.7 kΩ`; empty when there is none.
    pub value: String,
    pub pins: [PinRef; 2],
    /// The nets its leads are on, where they have one.
    pub nets: Vec<NetId>,
    /// The expected reading, or why there is none.
    pub outcome: Result<Reading, String>,
}

impl ResistorCheck {
    /// `R3 (4.7 kΩ)`, or the bare name without a value.
    pub(crate) fn label(&self) -> String {
        if self.value.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.value)
        }
    }
}

impl ResistorCheck {
    /// The measurement's text, or the note saying why there is none:
    /// `R3 (4.7 kΩ): expect 4.7 kΩ (4.65–4.75 kΩ).`
    pub(crate) fn line(&self) -> String {
        match &self.outcome {
            Ok(reading) => {
                let through = if reading.alongside.is_empty() {
                    String::new()
                } else {
                    format!(", through {} as well", and_list(&reading.alongside))
                };
                format!("{}: expect {}{through}.", self.label(), reading.text())
            }
            Err(reason) => format!("{}: not predicted, {reason}.", self.label()),
        }
    }
}

/// What the resistance step asks the builder to do.
pub(crate) fn resistance_instructions(design: &Design) -> Vec<String> {
    let mut instructions = vec![
        "With the power off, measure each resistor in place, across its own leads.".to_owned(),
        "The range allows for the resistors' tolerance; your meter's accuracy comes on top."
            .to_owned(),
    ];
    if has_capacitors(design) {
        instructions.push(
            "Capacitors read open once charged: let the reading settle before you judge it."
                .to_owned(),
        );
    }
    instructions
}

/// A supply net measured against ground before power goes on.
#[derive(Clone, Debug)]
pub(crate) struct SupplyCheck {
    pub supply: NetId,
    pub supply_name: String,
    pub ground: NetId,
    pub ground_name: String,
    /// `: expect 10 kΩ (9.9–10.1 kΩ)`, `: expect no connection`, or empty.
    pub expect: String,
    pub measure: Option<Measure>,
}

/// Each supply net against the ground net, when both can be recognised by
/// name, in name order.
pub(crate) fn supply_checks(design: &Design) -> Vec<SupplyCheck> {
    let mut accepted: Vec<&perfboard_core::ElectricalNet> = design
        .nets
        .values()
        .filter(|net| net.state == ConnectionState::Accepted)
        .collect();
    accepted.sort_by(|a, b| crate::assembly::natural_cmp(&a.name, &b.name).then(a.id.cmp(&b.id)));
    let Some(ground) = accepted.iter().find(|net| is_ground(&net.name)) else {
        return Vec::new();
    };
    accepted
        .iter()
        .filter(|net| is_power(&net.name) && !is_ground(&net.name))
        .map(|net| {
            let (expect, measure) = match between_nets(design, net.id, ground.id) {
                Prediction::Reading(reading) => (
                    format!(": expect {}", reading.text()),
                    Some(reading.measure()),
                ),
                Prediction::Open => (": expect no connection".to_owned(), Some(Measure::Open)),
                Prediction::Unknown { .. } => (String::new(), None),
            };
            SupplyCheck {
                supply: net.id,
                supply_name: net.name.clone(),
                ground: ground.id,
                ground_name: ground.name.clone(),
                expect,
                measure,
            }
        })
        .collect()
}

/// The power step's instructions.
pub(crate) const SUPPLY_INSTRUCTIONS: [&str; 2] = [
    "Measure the resistance between each pair of nets below.",
    "A reading near 0 Ω means a short: find it before connecting power.",
];

/// Every two-pin resistor's in-circuit reading, in designator order.
pub(crate) fn resistor_checks(design: &Design) -> Vec<ResistorCheck> {
    let model = Model::build(design);
    let mut checks: Vec<ResistorCheck> = design
        .components
        .values()
        .filter_map(|instance| {
            let definition = design.component_definitions.get(&instance.definition_id)?;
            if definition.kind != ComponentKind::Standard
                || definition.value_kind != ValueKind::Resistance
            {
                return None;
            }
            let [first, second] = &definition.pins[..] else {
                return None;
            };
            let pin = |id: &str| PinRef {
                component_id: instance.id,
                pin_id: id.to_owned(),
            };
            let pins = [pin(&first.id), pin(&second.id)];
            let value = read_value(ValueKind::Resistance, &instance.value, instance.band_count)
                .map(|reading| reading.display)
                .unwrap_or_default();
            let nodes = pins.clone().map(|pin| model.pin_node[&pin]);
            let nets = nodes
                .iter()
                .filter_map(|node| model.node_net[*node])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let outcome = match resistance_ohms(&instance.value) {
                None => Err("it has no value".to_owned()),
                Some(0.0) => Err("it is a zero-ohm link".to_owned()),
                Some(_) if nodes[0] == nodes[1] => {
                    Err("both its leads are on the same net".to_owned())
                }
                Some(_) => {
                    let element = model
                        .elements
                        .iter()
                        .position(|element| element.component_id == Some(instance.id));
                    match model.between(nodes[0], nodes[1], element) {
                        Prediction::Reading(reading) => Ok(reading),
                        Prediction::Unknown { blockers } => Err(format!(
                            "{} also {} its nets",
                            and_list(&blockers),
                            if blockers.len() == 1 { "joins" } else { "join" }
                        )),
                        // Its own element always joins its two nodes.
                        Prediction::Open => Err("it is not in the circuit".to_owned()),
                    }
                }
            };
            Some(ResistorCheck {
                name: instance.name.clone(),
                value,
                pins,
                nets,
                outcome,
            })
        })
        .collect();
    checks.sort_by(|a, b| crate::assembly::natural_cmp(&a.name, &b.name));
    checks
}

/// The reading between two nets, power off.
pub(crate) fn between_nets(design: &Design, first: NetId, second: NetId) -> Prediction {
    let model = Model::build(design);
    match (model.net_node.get(&first), model.net_node.get(&second)) {
        (Some(&a), Some(&b)) => model.between(a, b, None),
        _ => Prediction::Unknown {
            blockers: Vec::new(),
        },
    }
}

/// Whether any capacitor joins two different nets: its reading climbs while
/// it charges, so the builder should wait for it to settle.
pub(crate) fn has_capacitors(design: &Design) -> bool {
    Model::build(design).capacitors
}

// ── The network ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum ElementKind {
    Resistor { ohms: f64, tolerance: f64 },
    Unknown,
}

#[derive(Clone, Debug)]
struct Element {
    component_id: Option<perfboard_core::ComponentInstanceId>,
    name: String,
    /// Distinct nodes, at least two.
    nodes: Vec<usize>,
    kind: ElementKind,
}

struct Model {
    node_count: usize,
    pin_node: BTreeMap<PinRef, usize>,
    net_node: BTreeMap<NetId, usize>,
    /// The accepted net a node stands for, if any (a pin on no net gets a
    /// node of its own).
    node_net: Vec<Option<NetId>>,
    elements: Vec<Element>,
    capacitors: bool,
}

fn root(parent: &mut [usize], mut item: usize) -> usize {
    while parent[item] != item {
        parent[item] = parent[parent[item]];
        item = parent[item];
    }
    item
}

fn join(parent: &mut [usize], a: usize, b: usize) {
    let (a, b) = (root(parent, a), root(parent, b));
    if a != b {
        parent[a.max(b)] = a.min(b);
    }
}

/// What a part is in the power-off network.
enum Role {
    /// Joins its pins: a bridge, a fitted shunt, a zero-ohm link.
    Copper,
    /// Joins nothing: a jumper header without its shunt.
    Open,
    /// Open once charged.
    Capacitor,
    Element(ElementKind),
}

fn role(
    instance: &perfboard_core::ComponentInstance,
    definition: &perfboard_core::ComponentDefinition,
) -> Role {
    match definition.kind {
        ComponentKind::Bridge => Role::Copper,
        ComponentKind::Jumper if instance.shunted => Role::Copper,
        ComponentKind::Jumper => Role::Open,
        ComponentKind::Standard => match definition.value_kind {
            ValueKind::Capacitance => Role::Capacitor,
            ValueKind::Resistance if definition.pins.len() == 2 => {
                match resistance_ohms(&instance.value) {
                    Some(0.0) => Role::Copper,
                    Some(ohms) if ohms > 0.0 => Role::Element(ElementKind::Resistor {
                        ohms,
                        tolerance: match instance.band_count {
                            BandCount::Four => 0.05,
                            BandCount::Five => 0.01,
                        },
                    }),
                    _ => Role::Element(ElementKind::Unknown),
                }
            }
            _ => Role::Element(ElementKind::Unknown),
        },
    }
}

/// A part with its definition and pins.
type Part<'a> = (
    &'a perfboard_core::ComponentInstance,
    &'a perfboard_core::ComponentDefinition,
    Vec<PinRef>,
);

/// Union-find keys before merging: one per accepted net, then one per pin on
/// none of them. Returns the nets' keys, every pin's key, the key count, and
/// the parts.
fn keys(
    design: &Design,
) -> (
    BTreeMap<NetId, usize>,
    BTreeMap<PinRef, usize>,
    usize,
    Vec<Part<'_>>,
) {
    let accepted: Vec<NetId> = design
        .nets
        .values()
        .filter(|net| net.state == ConnectionState::Accepted)
        .map(|net| net.id)
        .collect();
    let net_key: BTreeMap<NetId, usize> = accepted
        .iter()
        .enumerate()
        .map(|(key, id)| (*id, key))
        .collect();
    let mut pin_key: BTreeMap<PinRef, usize> = BTreeMap::new();
    for net in design.nets.values() {
        if let Some(&key) = net_key.get(&net.id) {
            for pin in &net.pins {
                pin_key.insert(pin.clone(), key);
            }
        }
    }
    // Copper can join a lead to a net it is not listed in (ADR 0018).
    for (pin, net_id) in copper_joined_pins(design) {
        if let Some(&key) = net_key.get(&net_id) {
            pin_key.entry(pin).or_insert(key);
        }
    }
    let mut keys = accepted.len();
    let mut parts = Vec::new();
    for instance in design.components.values() {
        let Some(definition) = design.component_definitions.get(&instance.definition_id) else {
            continue;
        };
        let pins: Vec<PinRef> = definition
            .pins
            .iter()
            .map(|pin| PinRef {
                component_id: instance.id,
                pin_id: pin.id.clone(),
            })
            .collect();
        for pin in &pins {
            pin_key.entry(pin.clone()).or_insert_with(|| {
                keys += 1;
                keys - 1
            });
        }
        parts.push((instance, definition, pins));
    }
    (net_key, pin_key, keys, parts)
}

impl Model {
    fn build(design: &Design) -> Self {
        let (net_key, pin_key, keys, parts) = keys(design);

        // Parts that are plain copper merge their pins' keys.
        let mut parent: Vec<usize> = (0..keys).collect();
        for (instance, definition, pins) in &parts {
            if matches!(role(instance, definition), Role::Copper) {
                for pair in pins.windows(2) {
                    join(&mut parent, pin_key[&pair[0]], pin_key[&pair[1]]);
                }
            }
        }

        // Compact the merged keys into nodes.
        let mut node_of_root: BTreeMap<usize, usize> = BTreeMap::new();
        let node_of_key: Vec<usize> = (0..keys)
            .map(|key| {
                let next = node_of_root.len();
                *node_of_root.entry(root(&mut parent, key)).or_insert(next)
            })
            .collect();
        let node_count = node_of_root.len();
        let mut node_net = vec![None; node_count];
        let mut net_node = BTreeMap::new();
        for (net_id, key) in &net_key {
            let node = node_of_key[*key];
            net_node.insert(*net_id, node);
            node_net[node].get_or_insert(*net_id);
        }
        let pin_node: BTreeMap<PinRef, usize> = pin_key
            .into_iter()
            .map(|(pin, key)| (pin, node_of_key[key]))
            .collect();

        let mut elements = Vec::new();
        let mut capacitors = false;
        for (instance, definition, pins) in &parts {
            let nodes: Vec<usize> = pins
                .iter()
                .map(|pin| pin_node[pin])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if nodes.len() < 2 {
                continue;
            }
            let kind = match role(instance, definition) {
                Role::Copper | Role::Open => continue,
                Role::Capacitor => {
                    capacitors = true;
                    continue;
                }
                Role::Element(kind) => kind,
            };
            elements.push(Element {
                component_id: Some(instance.id),
                name: instance.name.clone(),
                nodes,
                kind,
            });
        }
        // Designator order, so lists in step text read naturally.
        elements.sort_by(|a, b| crate::assembly::natural_cmp(&a.name, &b.name));
        Self {
            node_count,
            pin_node,
            net_node,
            node_net,
            elements,
            capacitors,
        }
    }

    /// The reading between nodes `a` and `b`. `measured` is the element being
    /// checked, left out of the list of parts alongside it.
    fn between(&self, a: usize, b: usize, measured: Option<usize>) -> Prediction {
        if a == b {
            return Prediction::Reading(Reading {
                nominal: 0.0,
                min: 0.0,
                max: 0.0,
                alongside: Vec::new(),
            });
        }
        // Graph edges: a resistor or a two-node unknown is one edge; an
        // unknown on three or more nodes is a star around a vertex of its own.
        let mut vertex_count = self.node_count;
        let mut edges: Vec<(usize, usize, usize)> = Vec::new();
        for (index, element) in self.elements.iter().enumerate() {
            match &element.nodes[..] {
                [u, v] => edges.push((*u, *v, index)),
                nodes => {
                    let hub = vertex_count;
                    vertex_count += 1;
                    edges.extend(nodes.iter().map(|node| (hub, *node, index)));
                }
            }
        }
        let block_of = biconnected_blocks(vertex_count, &edges);
        let Some(path) = block_path(vertex_count, &edges, &block_of, a, b) else {
            return Prediction::Open;
        };
        let on_path = |edge: usize| path.contains(&block_of[edge]);
        let mut blockers: BTreeSet<usize> = BTreeSet::new();
        let mut resistors: Vec<(usize, usize, f64, f64)> = Vec::new();
        let mut alongside: BTreeSet<usize> = BTreeSet::new();
        for (edge, (u, v, index)) in edges.iter().enumerate() {
            if !on_path(edge) {
                continue;
            }
            match self.elements[*index].kind {
                ElementKind::Unknown => {
                    blockers.insert(*index);
                }
                ElementKind::Resistor { ohms, tolerance } => {
                    resistors.push((*u, *v, ohms, tolerance));
                    if Some(*index) != measured {
                        alongside.insert(*index);
                    }
                }
            }
        }
        if !blockers.is_empty() {
            return Prediction::Unknown {
                blockers: blockers
                    .into_iter()
                    .map(|index| self.elements[index].name.clone())
                    .collect(),
            };
        }
        // Effective resistance only grows when a resistor does, so the
        // extremes come from every part at the same end of its tolerance.
        let solve = |scale: f64| {
            let edges: Vec<(usize, usize, f64)> = resistors
                .iter()
                .map(|&(u, v, ohms, tolerance)| (u, v, ohms * scale.mul_add(tolerance, 1.0)))
                .collect();
            effective_resistance(&edges, a, b)
        };
        match (solve(0.0), solve(-1.0), solve(1.0)) {
            (Some(nominal), Some(min), Some(max)) => Prediction::Reading(Reading {
                nominal,
                min,
                max,
                alongside: alongside
                    .into_iter()
                    .map(|index| self.elements[index].name.clone())
                    .collect(),
            }),
            _ => Prediction::Unknown {
                blockers: Vec::new(),
            },
        }
    }
}

/// Assigns each edge of a multigraph to its biconnected block (Hopcroft–
/// Tarjan). Edges are `(u, v, payload)`; parallel edges share a block.
fn biconnected_blocks(vertex_count: usize, edges: &[(usize, usize, usize)]) -> Vec<usize> {
    struct Search {
        adjacent: Vec<Vec<(usize, usize)>>,
        discovered: Vec<Option<usize>>,
        low: Vec<usize>,
        time: usize,
        stack: Vec<usize>,
        block_of: Vec<usize>,
        blocks: usize,
    }
    impl Search {
        fn visit(&mut self, u: usize, via: Option<usize>) {
            self.discovered[u] = Some(self.time);
            self.low[u] = self.time;
            self.time += 1;
            for (v, edge) in self.adjacent[u].clone() {
                if Some(edge) == via {
                    continue;
                }
                match self.discovered[v] {
                    None => {
                        self.stack.push(edge);
                        self.visit(v, Some(edge));
                        self.low[u] = self.low[u].min(self.low[v]);
                        if self.low[v] >= self.discovered[u].unwrap_or(0) {
                            while let Some(top) = self.stack.pop() {
                                self.block_of[top] = self.blocks;
                                if top == edge {
                                    break;
                                }
                            }
                            self.blocks += 1;
                        }
                    }
                    Some(seen) if seen < self.discovered[u].unwrap_or(0) => {
                        self.stack.push(edge);
                        self.low[u] = self.low[u].min(seen);
                    }
                    Some(_) => {}
                }
            }
        }
    }
    let mut adjacent = vec![Vec::new(); vertex_count];
    for (index, (u, v, _)) in edges.iter().enumerate() {
        adjacent[*u].push((*v, index));
        adjacent[*v].push((*u, index));
    }
    let mut search = Search {
        adjacent,
        discovered: vec![None; vertex_count],
        low: vec![0; vertex_count],
        time: 0,
        stack: Vec::new(),
        block_of: vec![usize::MAX; edges.len()],
        blocks: 0,
    };
    for vertex in 0..vertex_count {
        if search.discovered[vertex].is_none() {
            search.visit(vertex, None);
        }
    }
    search.block_of
}

/// The blocks on the path from `a` to `b` in the block–cut tree, or `None`
/// when no edges join them.
fn block_path(
    vertex_count: usize,
    edges: &[(usize, usize, usize)],
    block_of: &[usize],
    a: usize,
    b: usize,
) -> Option<BTreeSet<usize>> {
    // Tree nodes: vertices, then blocks after them.
    let block_count = block_of.iter().copied().max().map_or(0, |max| max + 1);
    let mut adjacent: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); vertex_count + block_count];
    for (edge, (u, v, _)) in edges.iter().enumerate() {
        let block = vertex_count + block_of[edge];
        for vertex in [*u, *v] {
            adjacent[vertex].insert(block);
            adjacent[block].insert(vertex);
        }
    }
    let mut came_from: Vec<Option<usize>> = vec![None; adjacent.len()];
    let mut queue = VecDeque::from([a]);
    came_from[a] = Some(a);
    while let Some(node) = queue.pop_front() {
        if node == b {
            break;
        }
        for &next in &adjacent[node] {
            if came_from[next].is_none() {
                came_from[next] = Some(node);
                queue.push_back(next);
            }
        }
    }
    came_from[b]?;
    let mut blocks = BTreeSet::new();
    let mut node = b;
    while node != a {
        if node >= vertex_count {
            blocks.insert(node - vertex_count);
        }
        node = came_from[node]?;
    }
    Some(blocks)
}

/// The resistance between `a` and `b` over resistors `(u, v, ohms)`: inject
/// one ampere at `a`, hold `b` at zero, and read the voltage at `a`.
fn effective_resistance(edges: &[(usize, usize, f64)], a: usize, b: usize) -> Option<f64> {
    let mut index: BTreeMap<usize, usize> = BTreeMap::new();
    for &(u, v, _) in edges {
        for node in [u, v] {
            if node != b {
                let next = index.len();
                index.entry(node).or_insert(next);
            }
        }
    }
    let size = index.len();
    let row_a = *index.get(&a)?;
    let mut matrix = vec![vec![0.0_f64; size + 1]; size];
    for &(u, v, ohms) in edges {
        let conductance = 1.0 / ohms;
        let (iu, iv) = (index.get(&u).copied(), index.get(&v).copied());
        if let Some(iu) = iu {
            matrix[iu][iu] += conductance;
        }
        if let Some(iv) = iv {
            matrix[iv][iv] += conductance;
        }
        if let (Some(iu), Some(iv)) = (iu, iv) {
            matrix[iu][iv] -= conductance;
            matrix[iv][iu] -= conductance;
        }
    }
    matrix[row_a][size] = 1.0;
    // Gaussian elimination with partial pivoting.
    for column in 0..size {
        let pivot = (column..size)
            .max_by(|&x, &y| matrix[x][column].abs().total_cmp(&matrix[y][column].abs()))?;
        if matrix[pivot][column].abs() < 1e-18 {
            return None;
        }
        matrix.swap(column, pivot);
        for row in 0..size {
            if row != column {
                let factor = matrix[row][column] / matrix[column][column];
                if factor != 0.0 {
                    let pivot_row = matrix[column].clone();
                    for (cell, pivot) in matrix[row].iter_mut().zip(&pivot_row).skip(column) {
                        *cell -= factor * pivot;
                    }
                }
            }
        }
    }
    let value = matrix[row_a][size] / matrix[row_a][row_a];
    value.is_finite().then_some(value)
}

// ── Text ─────────────────────────────────────────────────────────────────────

/// Three significant figures, trailing zeros dropped.
fn figures(value: f64) -> String {
    let text = if value >= 100.0 {
        format!("{value:.0}")
    } else if value >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    };
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// The prefix and scale for a resistance.
fn prefix(ohms: f64) -> (&'static str, f64) {
    // Rounding to three figures can carry 999.6 up to 1000.
    let ohms = ohms * (1.0 + 5e-4);
    if ohms >= 1e9 {
        ("G", 1e9)
    } else if ohms >= 1e6 {
        ("M", 1e6)
    } else if ohms >= 1e3 {
        ("k", 1e3)
    } else if ohms >= 1.0 || ohms == 0.0 {
        ("", 1.0)
    } else {
        ("m", 1e-3)
    }
}

/// `4.7 kΩ`, `220 Ω`, `2.2 MΩ`.
pub(crate) fn format_ohms(ohms: f64) -> String {
    let (prefix, scale) = prefix(ohms);
    format!("{} {prefix}Ω", figures(ohms / scale))
}

/// `4.65–4.75 kΩ`, or both ends in full when their prefixes differ.
pub(crate) fn ohms_range(min: f64, max: f64) -> String {
    let (low, low_scale) = prefix(min);
    let (high, high_scale) = prefix(max);
    if low == high {
        format!(
            "{}–{} {high}Ω",
            figures(min / low_scale),
            figures(max / high_scale)
        )
    } else {
        format!("{}–{}", format_ohms(min), format_ohms(max))
    }
}

/// `A`, `A and B`, `A, B and C`.
pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(value: f64, expected: f64) -> bool {
        (value - expected).abs() <= expected.abs() * 1e-9
    }

    #[test]
    fn series_and_parallel_resistances() {
        // 0 -1k- 1 -2k- 2, and 0 -3k- 2: (1k + 2k) ‖ 3k = 1.5k.
        let edges = [(0, 1, 1000.0), (1, 2, 2000.0), (0, 2, 3000.0)];
        assert!(close(effective_resistance(&edges, 0, 2).unwrap(), 1500.0));
        // Between 0 and 1: 1k ‖ (2k + 3k) = 833.3.
        assert!(close(
            effective_resistance(&edges, 0, 1).unwrap(),
            1000.0 * 5000.0 / 6000.0
        ));
    }

    #[test]
    fn a_balanced_bridge_is_solved_exactly() {
        // A balanced Wheatstone bridge: by symmetry its middle resistor
        // carries no current, so the reading is (1k + 1k) ‖ (1k + 1k).
        let edges = [
            (0, 1, 1000.0),
            (0, 2, 1000.0),
            (1, 3, 1000.0),
            (2, 3, 1000.0),
            (1, 2, 470.0),
        ];
        assert!(close(effective_resistance(&edges, 0, 3).unwrap(), 1000.0));
    }

    #[test]
    fn blocks_split_at_cut_vertices() {
        // A triangle 0-1-2 and a tail 2-3: two blocks.
        let edges = [(0, 1, 0), (1, 2, 0), (2, 0, 0), (2, 3, 0)];
        let blocks = biconnected_blocks(4, &edges);
        assert_eq!(blocks[0], blocks[1]);
        assert_eq!(blocks[1], blocks[2]);
        assert_ne!(blocks[2], blocks[3]);
        // The path from 0 to 3 crosses both; from 0 to 1 only the triangle.
        assert_eq!(block_path(4, &edges, &blocks, 0, 3).unwrap().len(), 2);
        assert_eq!(
            block_path(4, &edges, &blocks, 0, 1).unwrap(),
            BTreeSet::from([blocks[0]])
        );
        // Parallel edges form a block of their own.
        let parallel = [(0, 1, 0), (0, 1, 0), (1, 2, 0)];
        let blocks = biconnected_blocks(3, &parallel);
        assert_eq!(blocks[0], blocks[1]);
        assert_ne!(blocks[0], blocks[2]);
        // Nothing joins an isolated vertex.
        assert!(block_path(4, &parallel, &blocks, 0, 3).is_none());
    }

    #[test]
    fn ohms_read_naturally() {
        assert_eq!(format_ohms(4700.0), "4.7 kΩ");
        assert_eq!(format_ohms(220.0), "220 Ω");
        assert_eq!(format_ohms(2_200_000.0), "2.2 MΩ");
        assert_eq!(format_ohms(1500.0), "1.5 kΩ");
        assert_eq!(format_ohms(833.333), "833 Ω");
        assert_eq!(format_ohms(999.9), "1 kΩ");
        assert_eq!(ohms_range(4653.0, 4747.0), "4.65–4.75 kΩ");
        assert_eq!(ohms_range(990.0, 1010.0), "990 Ω–1.01 kΩ");
    }
}
