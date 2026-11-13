//! Assembly planning (Milestone 6): dependency ordering, crossings, access,
//! cycles, wire list, and plans that go stale when the build changes.

use std::{collections::BTreeMap, fmt::Write as _};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, CrossingChoice,
    HolePos, NetId, ProjectId, Rotation, RoutePos, Saddle, SaddleId, Side, TerminalRef, Wire,
    WireColor, WireId,
};
use perfboard_project::{
    AssemblyPlan, Board, Design, DocumentRevision, ProjectDocument, StepKind, assembly_plan,
    load_project,
};

const fn p(x: i64, y: i64) -> RoutePos {
    RoutePos::new(x * 2, y * 2)
}

/// A component definition with pins along a row and a body of `w` × `h`
/// holes, centred on its pins.
fn definition(id: &str, kind: &str, pins: u32, w: f64, h: f64) -> ComponentDefinition {
    let pitch = 2.54;
    let mut toml = format!(
        "schema_version=1\n{kind}id='{id}'\ndefinition_version='1'\nname='{id}'\nallowed_rotations=['deg0']\n[body]\nx={}\ny={}\nwidth={}\nheight={}\n",
        -(w - f64::from(pins - 1)) / 2.0 * pitch,
        -h / 2.0 * pitch,
        w * pitch,
        h * pitch,
    );
    for pin in 0..pins {
        let _ = write!(
            toml,
            "[[pins]]\nid='p{pin}'\nnumber='{}'\nname='P{}'\nx={pin}\ny=0\n",
            pin + 1,
            pin + 1
        );
    }
    ComponentDefinition::from_toml(&toml).unwrap()
}

struct Fixture {
    document: ProjectDocument,
    net: NetId,
}

impl Fixture {
    fn new() -> Self {
        let mut document = ProjectDocument::new(
            ProjectId::new(),
            "assembly",
            Board::new("board", BoardSize::new(30, 20).unwrap(), 2.54),
        );
        let net = NetId::new();
        document.design.nets.insert(
            net,
            perfboard_core::ElectricalNet {
                id: net,
                name: "SIG".into(),
                state: perfboard_core::ConnectionState::Accepted,
                pins: std::collections::BTreeSet::default(),
                notes: String::new(),
            },
        );
        for definition in [
            definition("resistor", "", 2, 4.0, 1.0),
            definition("module", "", 4, 6.0, 6.0),
            definition("bridge", "kind='bridge'\n", 2, 1.0, 0.3),
        ] {
            document
                .design
                .component_definitions
                .insert(definition.id.clone(), definition);
        }
        Self { document, net }
    }

    fn design(&self) -> &Design {
        &self.document.design
    }

    fn plan(&self) -> AssemblyPlan {
        assembly_plan(self.design(), self.document.revision)
    }

    fn part(&mut self, definition: &str, name: &str, x: u32, y: u32) -> ComponentInstanceId {
        let id = ComponentInstanceId::new();
        self.document.design.components.insert(
            id,
            ComponentInstance {
                id,
                definition_id: definition.into(),
                name: name.into(),
                position: HolePos::new(x, y),
                rotation: Rotation::Deg0,
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
            },
        );
        id
    }

    fn wire(&mut self, side: Side, vertices: Vec<RoutePos>) -> WireId {
        let id = WireId::new();
        self.document.design.wires.insert(
            id,
            Wire {
                id,
                net_id: self.net,
                start: TerminalRef::Junction {
                    junction_id: perfboard_core::JunctionId::new(),
                },
                end: TerminalRef::Junction {
                    junction_id: perfboard_core::JunctionId::new(),
                },
                side,
                color: WireColor::Red,
                vertices,
                locked: false,
                notes: String::new(),
            },
        );
        id
    }
}

fn number(plan: &AssemblyPlan, id: impl std::fmt::Display) -> u32 {
    let id = id.to_string();
    plan.steps
        .iter()
        .find(|step| step.id.ends_with(&id))
        .unwrap_or_else(|| panic!("no step for {id}"))
        .number
}

/// Every step's prerequisites come earlier in the plan.
fn dependencies_respected(plan: &AssemblyPlan) -> bool {
    plan.steps.iter().all(|step| {
        step.after.iter().all(|dependency| {
            plan.steps
                .iter()
                .find(|other| other.id == dependency.step_id)
                .is_some_and(|other| other.number < step.number)
        })
    })
}

#[test]
fn stable_inputs_give_identical_plans() {
    let mut fixture = Fixture::new();
    fixture.part("resistor", "R1", 2, 2);
    fixture.part("module", "U1", 10, 10);
    fixture.wire(Side::Component, vec![p(1, 5), p(20, 5)]);
    let first = fixture.plan();
    let second = fixture.plan();
    assert_eq!(first, second);
    // A copy of the design plans identically too.
    let copy = fixture.design().clone();
    assert_eq!(
        assembly_plan(&copy, fixture.document.revision).fingerprint,
        first.fingerprint
    );
}

#[test]
fn independent_parts_go_small_first_then_top_left() {
    let mut fixture = Fixture::new();
    let module = fixture.part("module", "U1", 2, 2);
    let lower = fixture.part("resistor", "R2", 2, 15);
    let upper = fixture.part("resistor", "R1", 20, 12);
    let plan = fixture.plan();
    assert!(plan.steps.iter().all(|step| step.after.is_empty()));
    assert_eq!(number(&plan, upper), 1);
    assert_eq!(number(&plan, lower), 2);
    assert_eq!(number(&plan, module), 3);
    assert_eq!(plan.warnings, []);
}

#[test]
fn a_wire_under_a_body_goes_in_before_the_part() {
    let mut fixture = Fixture::new();
    // The module's body spans x 9.5..15.5, y 7..13 around pins at row 10.
    let module = fixture.part("module", "U1", 11, 10);
    let under = fixture.wire(Side::Component, vec![p(8, 9), p(18, 9)]);
    let beside = fixture.wire(Side::Component, vec![p(8, 15), p(18, 15)]);
    let plan = fixture.plan();
    assert!(number(&plan, under) < number(&plan, module));
    let step = &plan.steps[usize::try_from(number(&plan, module) - 1).unwrap()];
    assert_eq!(step.after.len(), 1);
    assert!(step.after[0].reason.contains("runs under U1's body"));
    // A wire alongside, not under, adds no dependency.
    assert!(
        step.after
            .iter()
            .all(|item| !item.step_id.contains(&beside.to_string()))
    );
    assert!(dependencies_respected(&plan));
}

#[test]
fn a_body_over_another_parts_pins_goes_in_after_it() {
    let mut fixture = Fixture::new();
    // U1's body spans x 0.5..6.5, y 7..13, with pins at (2..5, 10).
    let covered = fixture.part("module", "U1", 2, 10);
    // C1 stands outside U1 at (8, 10) and (9, 10), but its body overhangs to
    // the left, over U1's pins at x 3, 4 and 5.
    let overhang = ComponentDefinition::from_toml(
        "schema_version=1\nid='overhang'\ndefinition_version='1'\nname='overhang'\nallowed_rotations=['deg0']\n[body]\nx=-13.97\ny=-1.27\nwidth=17.78\nheight=2.54\n[[pins]]\nid='a'\nnumber='1'\nname='A'\nx=0\ny=0\n[[pins]]\nid='b'\nnumber='2'\nname='B'\nx=1\ny=0\n",
    )
    .unwrap();
    fixture
        .document
        .design
        .component_definitions
        .insert(overhang.id.clone(), overhang);
    let cover = fixture.part("overhang", "C1", 8, 10);
    let plan = fixture.plan();
    // C1 is smaller, so it would normally go first; it must wait for U1.
    assert!(number(&plan, covered) < number(&plan, cover));
    let step = &plan.steps[usize::try_from(number(&plan, cover) - 1).unwrap()];
    assert!(step.after[0].reason.contains("C1's body covers U1's pins"));
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    assert!(dependencies_respected(&plan));
}

#[test]
fn a_bridge_comes_after_every_lead_it_solders() {
    let mut fixture = Fixture::new();
    let part = fixture.part("resistor", "R1", 2, 2);
    // A bridge from R1's second pin (3, 2) to the free hole (4, 2), where a
    // solder-side wire lands.
    let bridge = fixture.part("bridge", "B1", 3, 2);
    let wire = fixture.wire(Side::Solder, vec![p(4, 2), p(4, 8)]);
    let plan = fixture.plan();
    let bridge_step = number(&plan, bridge);
    assert!(number(&plan, part) < bridge_step);
    assert!(number(&plan, wire) < bridge_step);
    let wire_step = &plan.steps[usize::try_from(number(&plan, wire) - 1).unwrap()];
    assert!(
        wire_step
            .instructions
            .iter()
            .any(|line| line.contains("land on B1"))
    );
    assert_eq!(
        plan.steps[usize::try_from(bridge_step - 1).unwrap()].kind,
        StepKind::Bridge
    );
    assert!(dependencies_respected(&plan));
}

#[test]
fn a_recorded_crossing_puts_the_lower_wire_first() {
    let mut fixture = Fixture::new();
    // The heuristics would lay `top` first (it starts higher up).
    let top = fixture.wire(Side::Component, vec![p(5, 1), p(5, 10)]);
    let across = fixture.wire(Side::Component, vec![p(1, 5), p(10, 5)]);
    let (first, second) = if top < across {
        (top, across)
    } else {
        (across, top)
    };
    fixture.document.design.crossing_choices.insert(
        "choice".into(),
        CrossingChoice {
            first_wire_id: first,
            second_wire_id: second,
            over_wire_id: top,
            position: p(5, 5),
        },
    );
    let plan = fixture.plan();
    assert!(number(&plan, across) < number(&plan, top));
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    let over = &plan.steps[usize::try_from(number(&plan, top) - 1).unwrap()];
    assert!(over.notes.iter().any(|note| note.contains("lay it over")));
}

#[test]
fn an_unrecorded_crossing_adds_no_dependency_but_explains_the_order() {
    let mut fixture = Fixture::new();
    let top = fixture.wire(Side::Component, vec![p(5, 1), p(5, 10)]);
    let across = fixture.wire(Side::Component, vec![p(1, 5), p(10, 5)]);
    // On opposite faces they do not cross at all.
    fixture.wire(Side::Solder, vec![p(1, 6), p(10, 6)]);
    let plan = fixture.plan();
    assert!(plan.steps.iter().all(|step| step.after.is_empty()));
    assert!(number(&plan, top) < number(&plan, across));
    let crossings: Vec<_> = plan
        .warnings
        .iter()
        .filter(|warning| warning.code == "assembly.unordered_crossing")
        .collect();
    assert_eq!(crossings.len(), 1);
    assert!(crossings[0].message.contains("either order works"));
}

#[test]
fn a_crossing_cycle_is_reported_and_still_planned() {
    let mut fixture = Fixture::new();
    // Two wires crossing twice, with contradictory recorded choices.
    let a = fixture.wire(Side::Component, vec![p(1, 3), p(12, 3)]);
    let b = fixture.wire(Side::Component, vec![p(3, 1), p(3, 6), p(9, 6), p(9, 1)]);
    let (first, second) = if a < b { (a, b) } else { (b, a) };
    for (key, over, x) in [("one", a, 3), ("two", b, 9)] {
        fixture.document.design.crossing_choices.insert(
            key.into(),
            CrossingChoice {
                first_wire_id: first,
                second_wire_id: second,
                over_wire_id: over,
                position: p(x, 3),
            },
        );
    }
    let plan = fixture.plan();
    let cycle = plan
        .warnings
        .iter()
        .find(|warning| warning.code == "assembly.cycle")
        .expect("cycle warning");
    assert_eq!(cycle.objects.len(), 2);
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| step.kind == StepKind::Wire)
            .count(),
        2
    );
}

#[test]
fn a_saddle_comes_after_the_wires_it_holds() {
    let mut fixture = Fixture::new();
    let wire = fixture.wire(Side::Solder, vec![p(1, 5), p(10, 5)]);
    let saddle = SaddleId::new();
    fixture.document.design.saddles.insert(
        saddle,
        Saddle {
            id: saddle,
            side: Side::Solder,
            start: HolePos::new(4, 4),
            end: HolePos::new(4, 6),
            color: WireColor::Black,
            notes: String::new(),
        },
    );
    let plan = fixture.plan();
    assert!(number(&plan, wire) < number(&plan, saddle));
    let step = &plan.steps[usize::try_from(number(&plan, saddle) - 1).unwrap()];
    assert!(step.instructions[0].contains("over W1"));
}

#[test]
fn the_wire_list_covers_every_wire_with_estimates_and_steps() {
    let mut fixture = Fixture::new();
    let bent = fixture.wire(Side::Component, vec![p(1, 1), p(1, 5), p(6, 5)]);
    fixture.wire(Side::Solder, vec![p(2, 2), p(8, 2)]);
    let plan = fixture.plan();
    assert_eq!(plan.wire_list.len(), 2);
    let labels: Vec<_> = plan
        .wire_list
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, ["W1", "W2"]);
    let first = plan
        .wire_list
        .iter()
        .find(|entry| entry.id == bent.to_string())
        .unwrap();
    assert!((first.path_mm - 9.0 * 2.54).abs() < 1e-9);
    assert!((first.cut_mm - (9.0 * 2.54 + 10.0)).abs() < 1e-9);
    assert_eq!(first.bend_count, 1);
    for entry in &plan.wire_list {
        assert_eq!(
            plan.steps[usize::try_from(entry.step - 1).unwrap()].label,
            entry.label
        );
    }
}

#[test]
fn relevant_edits_make_a_plan_stale_and_others_do_not() {
    let mut fixture = Fixture::new();
    let part = fixture.part("resistor", "R1", 2, 2);
    let wire = fixture.wire(Side::Component, vec![p(1, 5), p(9, 5)]);
    let plan = fixture.plan();
    assert!(plan.is_current(fixture.design()));

    // Wire notes are not part of the build.
    fixture.document.design.wires.get_mut(&wire).unwrap().notes = "tidy".into();
    assert!(plan.is_current(fixture.design()));

    // Moving a part changes where it goes.
    fixture
        .document
        .design
        .components
        .get_mut(&part)
        .unwrap()
        .position = HolePos::new(3, 2);
    assert!(!plan.is_current(fixture.design()));

    // Rerouting a wire changes it too.
    let plan = fixture.plan();
    fixture
        .document
        .design
        .wires
        .get_mut(&wire)
        .unwrap()
        .vertices = vec![p(1, 6), p(9, 6)];
    assert!(!plan.is_current(fixture.design()));
}

#[test]
fn step_fingerprints_survive_renumbering() {
    let mut fixture = Fixture::new();
    let part = fixture.part("resistor", "R1", 10, 10);
    let before = fixture.plan();
    // A new part placed earlier in the order shifts R1 to step 2.
    fixture.part("resistor", "R0", 1, 1);
    let after = fixture.plan();
    let find = |plan: &AssemblyPlan| {
        plan.steps
            .iter()
            .find(|step| step.id.ends_with(&part.to_string()))
            .cloned()
            .unwrap()
    };
    assert_eq!(find(&before).number, 1);
    assert_eq!(find(&after).number, 2);
    assert_eq!(find(&before).fingerprint, find(&after).fingerprint);
    assert_ne!(before.fingerprint, after.fingerprint);
}

#[test]
fn the_sample_project_gives_a_usable_sequence() {
    let document = load_project("../../examples/nano-rs485.perfboard.json").unwrap();
    let plan = assembly_plan(&document.design, DocumentRevision(7));
    assert_eq!(plan.source_revision, DocumentRevision(7));
    let design = &document.design;
    let objects =
        design.components.len() + design.wires.len() + design.rails.len() + design.saddles.len();
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| step.kind != StepKind::Check)
            .count(),
        objects
    );
    assert!(dependencies_respected(&plan));
    assert!(
        plan.warnings
            .iter()
            .all(|warning| warning.code != "assembly.cycle"),
        "{:?}",
        plan.warnings
    );
    assert_eq!(plan.wire_list.len(), design.wires.len());
    assert_eq!(plan.steps.last().unwrap().kind, StepKind::Check);
    // Steps are numbered 1..=n in order.
    for (index, step) in plan.steps.iter().enumerate() {
        assert_eq!(step.number as usize, index + 1);
    }
}

#[test]
fn check_steps_say_where_to_probe() {
    use perfboard_core::{ConnectionState, ElectricalNet, PinRef};
    let mut fixture = Fixture::new();
    let r1 = fixture.part("resistor", "R1", 2, 2);
    let r2 = fixture.part("resistor", "R2", 10, 2);
    let r3 = fixture.part("resistor", "R3", 2, 8);
    let pin = |component_id, pin: &str| PinRef {
        component_id,
        pin_id: pin.into(),
    };
    let sig = fixture.net;
    let mut add_net = |name: &str, pins: Vec<PinRef>| {
        let id = NetId::new();
        fixture.document.design.nets.insert(
            id,
            ElectricalNet {
                id,
                name: name.into(),
                state: ConnectionState::Accepted,
                pins: pins.into_iter().collect(),
                notes: String::new(),
            },
        );
        id
    };
    let ground = add_net("GND", vec![pin(r2, "p1"), pin(r3, "p1")]);
    let supply = add_net("+5V", vec![pin(r3, "p0")]);
    fixture
        .document
        .design
        .nets
        .get_mut(&sig)
        .unwrap()
        .pins
        .extend([pin(r1, "p0"), pin(r2, "p0")]);
    let plan = fixture.plan();
    let step = |id: &str| plan.steps.iter().find(|step| step.id == id).unwrap();

    let continuity = step("check:continuity");
    assert_eq!(continuity.probes.len(), 2, "SIG and GND; +5V has one pin");
    let sig_probe = continuity
        .probes
        .iter()
        .find(|probe| probe.nets == [sig.to_string()])
        .unwrap();
    assert!(sig_probe.text.starts_with("SIG: R1"));
    assert_eq!(sig_probe.holes.len(), 2);
    assert!(sig_probe.holes.iter().all(|hole| hole.is_hole()));

    let power = step("check:power");
    assert_eq!(power.probes.len(), 1);
    assert_eq!(
        power.probes[0].nets,
        [supply.to_string(), ground.to_string()]
    );
    assert_eq!(power.probes[0].holes.len(), 2);
    assert!(
        power.probes[0]
            .text
            .starts_with("+5V and GND, for example R3")
    );
}
