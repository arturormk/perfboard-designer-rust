//! Test points (ADR 0022): expected multimeter readings in the assembly plan
//! and on the breadboard, predicted only where the design proves them.

use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BandCount, BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId,
    ConnectionState, ElectricalNet, HolePos, NetId, PinRef, Rotation,
};
use perfboard_project::{
    AssemblyPlan, AssemblyStep, Board, BreadboardCheck, BreadboardLayout, Design, DocumentRevision,
    Measure, assembly_plan, breadboard_layout, verify_breadboard_layout,
};

fn library(name: &str) -> ComponentDefinition {
    let path = format!(
        "{}/../../components/{name}/component.toml",
        env!("CARGO_MANIFEST_DIR")
    );
    ComponentDefinition::from_toml(&std::fs::read_to_string(path).unwrap()).unwrap()
}

struct Builder {
    design: Design,
    names: BTreeMap<String, ComponentInstanceId>,
    next_x: i32,
}

impl Builder {
    fn new() -> Self {
        Self {
            design: Design {
                board: Board {
                    name: "Test".to_owned(),
                    size: BoardSize::new(60, 30).unwrap(),
                    pitch_mm: 2.54,
                    features: perfboard_core::BoardFeatures::default(),
                },
                component_definitions: BTreeMap::new(),
                components: BTreeMap::new(),
                nets: BTreeMap::new(),
                junctions: BTreeMap::new(),
                wires: BTreeMap::new(),
                rails: BTreeMap::new(),
                saddles: BTreeMap::new(),
                constraints: BTreeMap::new(),
                jumper_inventory: BTreeMap::new(),
                crossing_choices: BTreeMap::new(),
            },
            names: BTreeMap::new(),
            next_x: 2,
        }
    }

    /// Places a library part with a value, each one further along the board.
    fn part(&mut self, name: &str, library_name: &str, value: &str) -> &mut Self {
        let definition = library(library_name);
        let id = ComponentInstanceId::new();
        self.design.components.insert(
            id,
            ComponentInstance {
                id,
                definition_id: definition.id.clone(),
                name: name.to_owned(),
                position: HolePos::new(self.next_x, 4),
                rotation: Rotation::Deg0,
                value: value.to_owned(),
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
                component_side: false,
                band_count: BandCount::Five,
                pin_labels: BTreeMap::new(),
            },
        );
        self.next_x += 6;
        self.design
            .component_definitions
            .insert(definition.id.clone(), definition);
        self.names.insert(name.to_owned(), id);
        self
    }

    fn resistor(&mut self, name: &str, value: &str) -> &mut Self {
        self.part(name, "resistor-standing", value)
    }

    fn four_bands(&mut self, name: &str) -> &mut Self {
        let id = self.names[name];
        self.design.components.get_mut(&id).unwrap().band_count = BandCount::Four;
        self
    }

    /// Adds an accepted net over `Part.pin` names; `1` and `2` stand for a
    /// two-pin part's first and second pin.
    fn net(&mut self, name: &str, pins: &[&str]) -> NetId {
        let id = NetId::new();
        let pins = pins
            .iter()
            .map(|spec| {
                let (part, pin) = spec.split_once('.').unwrap();
                let component_id = self.names[part];
                let instance = &self.design.components[&component_id];
                let definition = &self.design.component_definitions[&instance.definition_id];
                let pin_id = match pin {
                    "1" => definition.pins[0].id.clone(),
                    "2" => definition.pins[1].id.clone(),
                    other => other.to_owned(),
                };
                PinRef {
                    component_id,
                    pin_id,
                }
            })
            .collect::<BTreeSet<_>>();
        self.design.nets.insert(
            id,
            ElectricalNet {
                id,
                name: name.to_owned(),
                state: ConnectionState::Accepted,
                pins,
                notes: String::new(),
            },
        );
        id
    }

    fn plan(&self) -> AssemblyPlan {
        assembly_plan(&self.design, DocumentRevision(3))
    }

    fn layout(&self) -> BreadboardLayout {
        breadboard_layout(&self.design, DocumentRevision(3))
    }
}

fn step<'a>(plan: &'a AssemblyPlan, id: &str) -> &'a AssemblyStep {
    plan.steps
        .iter()
        .find(|step| step.id == id)
        .unwrap_or_else(|| panic!("no {id} step"))
}

fn check<'a>(layout: &'a BreadboardLayout, id: &str) -> &'a BreadboardCheck {
    layout
        .checks
        .iter()
        .find(|check| check.id == id)
        .unwrap_or_else(|| panic!("no {id} check: {:#?}", layout.checks))
}

fn resistance(measure: Option<&Measure>) -> (f64, f64, f64) {
    match measure {
        Some(Measure::Resistance { nominal, min, max }) => (*nominal, *min, *max),
        other => panic!("not a resistance: {other:?}"),
    }
}

fn close(value: f64, expected: f64) -> bool {
    (value - expected).abs() <= expected.abs() * 1e-9
}

/// +5V –R1 10k– MID –R2 10k– GND.
fn divider() -> Builder {
    let mut builder = Builder::new();
    builder.resistor("R1", "10k").resistor("R2", "10k");
    builder.net("+5V", &["R1.1"]);
    builder.net("MID", &["R1.2", "R2.1"]);
    builder.net("GND", &["R2.2"]);
    builder
}

#[test]
fn a_divider_gives_each_resistor_its_own_value_and_the_supply_their_sum() {
    let plan = divider().plan();
    let resistances = step(&plan, "check:resistance");
    assert_eq!(resistances.probes.len(), 2, "{resistances:#?}");
    assert!(resistances.notes.is_empty(), "{:?}", resistances.notes);
    let r1 = &resistances.probes[0];
    assert_eq!(r1.text, "R1 (10 kΩ): expect 10 kΩ (9.9–10.1 kΩ).");
    assert_eq!(r1.holes.len(), 2);
    let (nominal, min, max) = resistance(r1.measure.as_ref());
    assert!(close(nominal, 10_000.0) && close(min, 9_900.0) && close(max, 10_100.0));

    let power = step(&plan, "check:power");
    let (nominal, ..) = resistance(power.probes[0].measure.as_ref());
    assert!(close(nominal, 20_000.0));
    assert!(
        power.probes[0]
            .text
            .starts_with("+5V and GND: expect 20 kΩ (19.8–20.2 kΩ), for example R1"),
        "{}",
        power.probes[0].text
    );
    // Power off comes before the short check.
    let order: Vec<&str> = plan
        .steps
        .iter()
        .filter(|step| step.id.starts_with("check:"))
        .map(|step| step.id.as_str())
        .collect();
    assert_eq!(
        order,
        [
            "check:inspect",
            "check:continuity",
            "check:resistance",
            "check:power"
        ]
    );
    let continuity = step(&plan, "check:continuity");
    assert!(
        continuity
            .probes
            .iter()
            .all(|probe| probe.measure == Some(Measure::Continuity))
    );
}

#[test]
fn a_resistor_in_parallel_lowers_the_reading_and_is_named() {
    let mut builder = Builder::new();
    builder
        .resistor("R1", "10k")
        .resistor("R2", "10k")
        .resistor("R3", "4k7")
        .four_bands("R3");
    builder.net("A", &["R1.1", "R3.1"]);
    builder.net("B", &["R1.2", "R3.2", "R2.1"]);
    builder.net("C", &["R2.2"]);
    let plan = builder.plan();
    let resistances = step(&plan, "check:resistance");
    let r1 = &resistances.probes[0];
    let (nominal, min, max) = resistance(r1.measure.as_ref());
    let parallel = |a: f64, b: f64| a * b / (a + b);
    assert!(close(nominal, parallel(10_000.0, 4_700.0)));
    // Each end of its own tolerance: 1% for R1, 5% for four-band R3.
    assert!(close(min, parallel(9_900.0, 4_465.0)));
    assert!(close(max, parallel(10_100.0, 4_935.0)));
    assert!(r1.text.ends_with(", through R3 as well."), "{}", r1.text);
    // R2 hangs off the pair as a branch of its own: its reading is exact.
    let r2 = &resistances.probes[1];
    assert_eq!(r2.text, "R2 (10 kΩ): expect 10 kΩ (9.9–10.1 kΩ).");
}

#[test]
fn a_part_the_design_cannot_model_stops_only_the_readings_it_can_change() {
    // An LED across the supply closes a loop through both resistors; R4,
    // hanging off MID on its own, is still exact.
    let mut builder = Builder::new();
    builder
        .resistor("R1", "10k")
        .resistor("R2", "10k")
        .part("D1", "led-5mm", "red")
        .resistor("R4", "1k")
        .resistor("R5", "")
        .part("C1", "capacitor-ceramic-2-54mm", "100n");
    builder.net("+5V", &["R1.1", "D1.anode"]);
    builder.net("GND", &["R2.2", "D1.cathode", "C1.2"]);
    builder.net("MID", &["R1.2", "R2.1", "R4.1", "C1.1"]);
    builder.net("TAP", &["R4.2", "R5.1"]);

    let plan = builder.plan();
    let resistances = step(&plan, "check:resistance");
    assert_eq!(
        resistances
            .probes
            .iter()
            .map(|probe| probe.text.as_str())
            .collect::<Vec<_>>(),
        ["R4 (1 kΩ): expect 1 kΩ (990 Ω–1.01 kΩ)."]
    );
    assert_eq!(
        resistances.notes,
        [
            "R1 (10 kΩ): not predicted, D1 also joins its nets.",
            "R2 (10 kΩ): not predicted, D1 also joins its nets.",
            "R5: not predicted, it has no value.",
        ]
    );
    assert!(
        resistances
            .instructions
            .iter()
            .any(|line| line.starts_with("Capacitors read open"))
    );
    let power = step(&plan, "check:power");
    assert_eq!(power.probes[0].measure, None);
    assert!(power.probes[0].text.starts_with("+5V and GND, for example"));
}

#[test]
fn nets_no_part_joins_read_open() {
    let mut builder = Builder::new();
    builder.resistor("R1", "1k").resistor("R2", "1k");
    builder.net("+5V", &["R1.1", "R1.2"]);
    builder.net("GND", &["R2.1", "R2.2"]);
    let plan = builder.plan();
    let power = step(&plan, "check:power");
    assert_eq!(power.probes[0].measure, Some(Measure::Open));
    assert!(
        power.probes[0]
            .text
            .starts_with("+5V and GND: expect no connection")
    );
    let resistances = step(&plan, "check:resistance");
    assert_eq!(resistances.probes, Vec::new());
    assert_eq!(
        resistances.notes[0],
        "R1 (1 kΩ): not predicted, both its leads are on the same net."
    );
}

#[test]
fn the_breadboard_gets_the_same_readings_at_its_own_holes() {
    let builder = divider();
    let layout = builder.layout();
    assert_eq!(
        verify_breadboard_layout(&builder.design, &layout),
        Vec::<String>::new()
    );
    let ids: Vec<&str> = layout
        .checks
        .iter()
        .map(|check| check.id.as_str())
        .collect();
    assert!(ids.contains(&"check:leads"), "{ids:?}");
    assert!(ids.contains(&"check:resistance"), "{ids:?}");
    assert!(ids.contains(&"check:power"), "{ids:?}");

    // The same text as the assembly plan, probed at the parts' leads.
    let plan = builder.plan();
    let bench = check(&layout, "check:resistance");
    let assembly = step(&plan, "check:resistance");
    assert_eq!(
        bench.probes.iter().map(|p| &p.text).collect::<Vec<_>>(),
        assembly.probes.iter().map(|p| &p.text).collect::<Vec<_>>()
    );
    let r1 = layout.parts.iter().find(|part| part.name == "R1").unwrap();
    for hole in &bench.probes[0].holes {
        assert!(
            hole.y >= r1.position.y - 4 && hole.y <= r1.position.y + 4,
            "{hole:?} is not at R1's leads ({:?})",
            r1.position
        );
    }

    // MID joins two parts: continuity is measured at both their leads.
    let leads = check(&layout, "check:leads");
    let mid = leads
        .probes
        .iter()
        .find(|probe| probe.text.starts_with("MID:"))
        .unwrap();
    assert_eq!(mid.holes.len(), 2);
    assert_eq!(mid.measure, Some(Measure::Continuity));
}

#[test]
fn neighbouring_strips_and_rails_with_different_nets_are_checked_apart() {
    // A DIP-8 with each pin on its own net: neighbouring pins sit in
    // neighbouring rows, which must read open.
    let mut builder = Builder::new();
    builder.part("U1", "dip-8", "NE555");
    let definition = library("dip-8");
    for pin in &definition.pins[..4] {
        let other = format!("R{}", pin.number);
        builder.resistor(&other, "1k");
        let net = if pin.number == "1" {
            "GND".to_owned()
        } else if pin.number == "4" {
            "+5V".to_owned()
        } else {
            format!("N{}", pin.number)
        };
        builder.net(&net, &[&format!("U1.{}", pin.id), &format!("{other}.1")]);
    }
    let layout = builder.layout();
    assert_eq!(
        verify_breadboard_layout(&builder.design, &layout),
        Vec::<String>::new()
    );
    let neighbours = check(&layout, "check:neighbours");
    assert!(
        neighbours
            .probes
            .iter()
            .all(|probe| probe.measure == Some(Measure::Open)
                && probe.holes.len() == 2
                && probe.nets[0] != probe.nets[1])
    );
    // U1's pins 1–4 run down one side, row after row: three pairs at least.
    let u1_pairs = neighbours
        .probes
        .iter()
        .filter(|probe| probe.text.matches("U1 ·").count() == 2)
        .count();
    assert!(u1_pairs >= 3, "{neighbours:#?}");

    // Ground and supply take rails; a used rail is checked end to end when
    // its jumpers span more than one row.
    if let Some(rails) = layout.checks.iter().find(|check| check.id == "check:rails") {
        for probe in &rails.probes {
            assert_eq!(probe.measure, Some(Measure::Continuity));
            assert_eq!(probe.holes[0].x, probe.holes[1].x);
            assert!(probe.holes[0].y < probe.holes[1].y);
        }
    }
}
