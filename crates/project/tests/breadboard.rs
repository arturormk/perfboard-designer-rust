//! Breadboard layouts (ADR 0018): placement, rails, jumpers, off-board parts,
//! and a connectivity check that the breadboard is the circuit.

use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, NetId, PinRef, Rotation, Side, TerminalRef, Wire, WireColor, WireId,
};
use perfboard_project::{
    Board, BreadboardLayout, Design, DocumentRevision, JumperKind, PartPlacement, RailPolarity,
    RailSlot, breadboard_layout, is_rail_row, load_project, verify_breadboard_layout,
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
}

impl Builder {
    fn new() -> Self {
        Self {
            design: Design {
                board: Board {
                    name: "Test".to_owned(),
                    size: BoardSize::new(40, 40).unwrap(),
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
        }
    }

    fn part(&mut self, name: &str, library_name: &str) -> &mut Self {
        let definition = library(library_name);
        let id = ComponentInstanceId::new();
        self.design.components.insert(
            id,
            ComponentInstance {
                id,
                definition_id: definition.id.clone(),
                name: name.to_owned(),
                position: HolePos::new(1, 1),
                rotation: Rotation::Deg0,
                value: String::new(),
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
            },
        );
        self.design
            .component_definitions
            .insert(definition.id.clone(), definition);
        self.names.insert(name.to_owned(), id);
        self
    }

    fn part_with(&mut self, name: &str, definition: ComponentDefinition) -> &mut Self {
        let id = ComponentInstanceId::new();
        self.design.components.insert(
            id,
            ComponentInstance {
                id,
                definition_id: definition.id.clone(),
                name: name.to_owned(),
                position: HolePos::new(1, 1),
                rotation: Rotation::Deg0,
                value: String::new(),
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
            },
        );
        self.design
            .component_definitions
            .insert(definition.id.clone(), definition);
        self.names.insert(name.to_owned(), id);
        self
    }

    /// Adds an accepted net over `Part.pin` names.
    fn net(&mut self, name: &str, pins: &[&str]) -> NetId {
        let id = NetId::new();
        let pins = pins
            .iter()
            .map(|spec| {
                let (part, pin) = spec.split_once('.').unwrap();
                PinRef {
                    component_id: self.names[part],
                    pin_id: pin.to_owned(),
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

    /// A perfboard wire of `color` on a net (only its colour matters here).
    fn wire(&mut self, net_id: NetId, color: WireColor) {
        let net = &self.design.nets[&net_id];
        let mut pins = net.pins.iter();
        let (a, b) = (pins.next().unwrap().clone(), pins.next().unwrap().clone());
        let id = WireId::new();
        self.design.wires.insert(
            id,
            Wire {
                id,
                net_id,
                start: TerminalRef::from(a),
                end: TerminalRef::from(b),
                side: Side::Component,
                color,
                vertices: Vec::new(),
                locked: false,
                notes: String::new(),
            },
        );
    }

    fn layout(&self) -> BreadboardLayout {
        breadboard_layout(&self.design, DocumentRevision(7))
    }
}

fn assert_sound(design: &Design, layout: &BreadboardLayout) {
    let problems = verify_breadboard_layout(design, layout);
    assert!(problems.is_empty(), "{problems:#?}");
    let failed: Vec<_> = layout
        .warnings
        .iter()
        .filter(|w| w.code == "breadboard.check_failed" || w.code == "breadboard.out_of_holes")
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

fn placement<'a>(
    layout: &'a BreadboardLayout,
    name: &str,
) -> &'a perfboard_project::BreadboardPart {
    layout.parts.iter().find(|p| p.name == name).unwrap()
}

/// A small 555-style circuit: DIP-8, resistors, capacitors, an LED, a power
/// header, and a 2×3 header that cannot sit on a breadboard.
fn timer() -> Builder {
    let mut b = Builder::new();
    b.part("U1", "dip-8")
        .part("R1", "resistor_4_holes")
        .part("R2", "resistor_4_holes")
        .part("R3", "resistor_4_holes")
        .part("C1", "capacitor-ceramic-2-54mm")
        .part("C2", "capacitor-electrolytic-10mm")
        .part("D1", "led-5mm")
        .part("J1", "header-1x4-male")
        .part("ISP", "header-2x3-male");
    let vcc = b.net(
        "VCC",
        &[
            "U1.pin_8",
            "U1.pin_4",
            "R1.terminal_1",
            "J1.pin_1",
            "C2.positive",
            "ISP.pin_2",
        ],
    );
    let gnd = b.net(
        "GND",
        &[
            "U1.pin_1",
            "C1.terminal_2",
            "D1.cathode",
            "J1.pin_4",
            "C2.negative",
            "ISP.pin_6",
        ],
    );
    b.net("DISCH", &["U1.pin_7", "R1.terminal_2", "R2.terminal_1"]);
    b.net(
        "THRESH",
        &["U1.pin_6", "U1.pin_2", "R2.terminal_2", "C1.terminal_1"],
    );
    b.net("OUT", &["U1.pin_3", "R3.terminal_1", "J1.pin_2"]);
    b.net("LED", &["R3.terminal_2", "D1.anode"]);
    b.net("RESET_SENSE", &["ISP.pin_5", "J1.pin_3"]);
    b.wire(vcc, WireColor::Orange);
    b.wire(gnd, WireColor::Black);
    b
}

#[test]
fn timer_circuit_is_laid_out_soundly() {
    let b = timer();
    let layout = b.layout();
    assert_sound(&b.design, &layout);
    assert_eq!(layout.board_count, 1);
    assert_eq!(layout.source_revision, DocumentRevision(7));

    // The DIP straddles the channel with pin 1 top left (columns e and f).
    let u1 = placement(&layout, "U1");
    assert_eq!(u1.placement, PartPlacement::Straddle);
    assert_eq!(u1.rotation, Rotation::Deg90);
    assert_eq!(u1.position.x % 19, 10);
    assert_eq!(u1.position.y, 2, "first row level with the rails");

    // Axial resistors bridge the channel; the LED and capacitors stand on
    // one side; the 2×3 header cannot sit on a board.
    assert_eq!(placement(&layout, "R1").placement, PartPlacement::Straddle);
    assert!(matches!(
        placement(&layout, "D1").placement,
        PartPlacement::Left | PartPlacement::Right
    ));
    let isp = placement(&layout, "ISP");
    assert_eq!(isp.placement, PartPlacement::OffBoard);
    assert!(isp.position.x >= layout.geometry.off_board_x);
    assert!(
        layout
            .warnings
            .iter()
            .any(|w| w.code == "breadboard.off_board")
    );
    assert!(
        layout
            .jumpers
            .iter()
            .filter(|j| j.kind == JumperKind::Lead)
            .count()
            >= 3,
        "ISP pins 2, 5 and 6 need leads"
    );
}

#[test]
fn ground_and_supply_take_the_rails_and_jumpers_keep_wire_colours() {
    let b = timer();
    let layout = b.layout();
    let rail_net = |slot: RailSlot| {
        layout
            .rails
            .iter()
            .find(|r| r.board == 0 && r.slot == slot)
            .map(|r| r.net_name.as_str())
    };
    // Two rail nets: each gets a rail on both sides, ground on −. Each pair
    // reads + then − from left to right.
    assert_eq!(RailSlot::LeftOuter.polarity(), RailPolarity::Plus);
    assert_eq!(RailSlot::LeftInner.polarity(), RailPolarity::Minus);
    assert_eq!(RailSlot::RightInner.polarity(), RailPolarity::Plus);
    assert_eq!(RailSlot::RightOuter.polarity(), RailPolarity::Minus);
    assert_eq!(rail_net(RailSlot::LeftOuter), Some("VCC"));
    assert_eq!(rail_net(RailSlot::LeftInner), Some("GND"));
    for slot in [RailSlot::RightInner, RailSlot::RightOuter] {
        assert!(matches!(rail_net(slot), Some("GND" | "VCC") | None));
    }
    if let Some(net) = rail_net(RailSlot::RightOuter) {
        assert_eq!(net, "GND");
    }
    for jumper in &layout.jumpers {
        match jumper.net_name.as_str() {
            "VCC" => assert_eq!(jumper.color, WireColor::Orange),
            "GND" => assert_eq!(jumper.color, WireColor::Black),
            _ => {}
        }
    }
    // Unrouted signal nets get distinct colours.
    let signal: BTreeSet<_> = layout
        .jumpers
        .iter()
        .filter(|j| !matches!(j.net_name.as_str(), "VCC" | "GND"))
        .map(|j| (j.net_name.clone(), format!("{:?}", j.color)))
        .collect();
    let colours: BTreeSet<_> = signal.iter().map(|(_, c)| c.clone()).collect();
    assert!(colours.len() >= 4, "{signal:?}");
}

#[test]
fn layout_is_deterministic() {
    let b = timer();
    assert_eq!(b.layout(), b.layout());
    let json = serde_json::to_string(&b.layout()).unwrap();
    assert!(json.contains("\"placement\":\"straddle\""));
}

#[test]
fn a_tall_design_continues_on_more_boards() {
    let mut b = Builder::new();
    let count = 8;
    for i in 1..=count {
        b.part(&format!("U{i}"), "dip-16");
    }
    // Chain the chips and give them all power.
    let vcc: Vec<String> = (1..=count).map(|i| format!("U{i}.pin_16")).collect();
    let gnd: Vec<String> = (1..=count).map(|i| format!("U{i}.pin_8")).collect();
    b.net("VCC", &vcc.iter().map(String::as_str).collect::<Vec<_>>());
    b.net("GND", &gnd.iter().map(String::as_str).collect::<Vec<_>>());
    for i in 1..count {
        for pin in 1..=7 {
            let a = format!("U{i}.pin_{}", pin + 8);
            let z = format!("U{}.pin_{pin}", i + 1);
            b.net(&format!("N{i}_{pin}"), &[&a, &z]);
        }
    }
    let layout = b.layout();
    assert_sound(&b.design, &layout);
    assert!(layout.board_count >= 2, "{}", layout.board_count);
    // Each board's rails used by a net are linked to the next board's.
    assert!(layout.rails.iter().any(|r| r.board == 1));
    assert!(
        layout
            .jumpers
            .iter()
            .any(|j| j.from_label.starts_with("B1 ") && j.to_label.starts_with("B2 "))
    );
}

#[test]
fn a_busy_signal_net_takes_free_rails() {
    // Six LEDs share one anode net: no ground or supply by name, but enough
    // pins to make a rail worth it.
    let mut b = Builder::new();
    for i in 1..=6 {
        b.part(&format!("D{i}"), "led-5mm");
    }
    let anodes: Vec<String> = (1..=6).map(|i| format!("D{i}.anode")).collect();
    b.net(
        "BUS",
        &anodes.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    for i in [1, 3, 5] {
        let a = format!("D{i}.cathode");
        let z = format!("D{}.cathode", i + 1);
        b.net(&format!("K{i}"), &[&a, &z]);
    }
    let layout = b.layout();
    assert_sound(&b.design, &layout);
    assert!(layout.rails.iter().any(|r| r.net_name == "BUS"));
}

#[test]
fn proposed_connections_are_left_out_with_a_warning() {
    let mut b = timer();
    let proposed = b.net("MAYBE", &["R1.terminal_1", "R3.terminal_2"]);
    b.design.nets.get_mut(&proposed).unwrap().state = ConnectionState::Proposed;
    let layout = b.layout();
    assert!(
        layout
            .warnings
            .iter()
            .any(|w| w.code == "breadboard.proposed_nets")
    );
    assert!(layout.jumpers.iter().all(|j| j.net_name != "MAYBE"));
}

#[test]
fn verification_catches_a_short_and_a_split_net() {
    let b = timer();
    let mut layout = b.layout();
    assert_eq!(
        verify_breadboard_layout(&b.design, &layout),
        Vec::<String>::new()
    );
    // Removing a jumper splits its net.
    let removed = layout
        .jumpers
        .iter()
        .position(|j| j.net_name == "DISCH" && j.kind == JumperKind::Jumper)
        .unwrap();
    layout.jumpers.remove(removed);
    let problems = verify_breadboard_layout(&b.design, &layout);
    assert!(
        problems.iter().any(|p| p.contains("DISCH is split")),
        "{problems:?}"
    );

    // Moving a jumper end into another net's rail shorts them.
    let mut layout = b.layout();
    let vcc_rail = layout
        .jumpers
        .iter()
        .find(|j| j.net_name == "VCC" && j.to_label.contains("rail"))
        .unwrap()
        .to;
    let out = layout
        .jumpers
        .iter_mut()
        .find(|j| j.net_name == "OUT" && j.kind == JumperKind::Jumper)
        .unwrap();
    out.to = HolePos::new(vcc_rail.x, if vcc_rail.y > 30 { 10 } else { 50 });
    let problems = verify_breadboard_layout(&b.design, &layout);
    assert!(
        problems.iter().any(|p| p.contains("shorted")),
        "{problems:?}"
    );
}

#[test]
fn example_project_straddles_both_modules() {
    let path = format!(
        "{}/../../examples/nano-rs485.perfboard.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let document = load_project(path).unwrap();
    let layout = breadboard_layout(&document.design, document.revision);
    assert_sound(&document.design, &layout);
    assert_eq!(layout.board_count, 1);
    // Both footprints have two pin columns six holes apart, like a 0.6" DIP.
    for name in ["Controller", "RS-485 transceiver"] {
        assert_eq!(placement(&layout, name).placement, PartPlacement::Straddle);
    }
    assert!(layout.jumpers.iter().all(|j| j.kind == JumperKind::Jumper));
    let colour = |net: &str| {
        layout
            .jumpers
            .iter()
            .find(|j| j.net_name == net)
            .unwrap()
            .color
            .clone()
    };
    assert_eq!(colour("VCC"), WireColor::Red);
    assert_eq!(colour("GND"), WireColor::Black);
    assert_eq!(colour("UART_TX"), WireColor::Green);
}

#[test]
fn off_board_parts_are_wired_in_with_leads() {
    let b = timer();
    let layout = b.layout();
    let leads: Vec<_> = layout
        .jumpers
        .iter()
        .filter(|j| j.kind == JumperKind::Lead)
        .collect();
    // ISP pins 2 (VCC), 5 (RESET_SENSE) and 6 (GND).
    assert_eq!(leads.len(), 3, "{leads:#?}");
    assert!(leads.iter().all(|j| j.from_label.starts_with("ISP · ")));
    // Supply and ground leads go straight into the rails.
    for lead in &leads {
        if matches!(lead.net_name.as_str(), "VCC" | "GND") {
            assert!(lead.to_label.contains("rail"), "{lead:?}");
        }
    }
}

#[test]
fn rail_holes_come_in_groups_of_five() {
    // Rows are 0-based: row 3 is 2.
    let rows: Vec<u32> = (0..63).filter(|row| is_rail_row(*row)).collect();
    assert_eq!(rows.len(), 50);
    assert_eq!(&rows[..6], &[2, 3, 4, 5, 6, 8]);
    assert!(!is_rail_row(7) && !is_rail_row(13) && !is_rail_row(61));
    assert_eq!(rows.last(), Some(&60));
    let b = timer();
    let layout = b.layout();
    assert_eq!(layout.geometry.rail_rows, rows);
    // No jumper is plugged into a row the rails leave out.
    for jumper in &layout.jumpers {
        for end in [jumper.from, jumper.to] {
            if [0, 1, 16, 17].contains(&(end.x % 19)) && end.x < 19 * layout.board_count {
                assert!(is_rail_row(end.y), "jumper {} at {end:?}", jumper.number);
            }
        }
    }
    // The checker refuses a jumper moved onto a missing rail hole.
    let mut broken = layout.clone();
    let rail_end = broken
        .jumpers
        .iter_mut()
        .find(|j| j.to_label.contains("rail"))
        .unwrap();
    rail_end.to.y = 7;
    let problems = verify_breadboard_layout(&b.design, &broken);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("not plugged into a hole")),
        "{problems:?}"
    );
}

fn example() -> perfboard_project::ProjectDocument {
    let path = format!(
        "{}/../../examples/nano-rs485.perfboard.json",
        env!("CARGO_MANIFEST_DIR")
    );
    load_project(path).unwrap()
}

/// Board holes under a laid-out part's required access regions.
fn access_of(design: &Design, layout: &BreadboardLayout, name: &str) -> Vec<HolePos> {
    let part = placement(layout, name);
    let definition = &design.component_definitions[&part.definition_id];
    let mut holes = Vec::new();
    for area in definition.access.iter().filter(|a| a.required) {
        for x in area.min.x..=area.max.x {
            for y in area.min.y..=area.max.y {
                let (rx, ry) = part.rotation.rotate_local((x, y));
                let gx = u32::try_from(i64::from(part.position.x) + i64::from(rx));
                let gy = u32::try_from(i64::from(part.position.y) + i64::from(ry));
                if let (Ok(gx), Ok(gy)) = (gx, gy) {
                    holes.push(HolePos::new(gx, gy));
                }
            }
        }
    }
    holes
}

#[test]
fn required_access_regions_stay_clear() {
    let document = example();
    let layout = breadboard_layout(&document.design, document.revision);
    assert_sound(&document.design, &layout);
    let usb = access_of(&document.design, &layout, "Controller");
    assert_ne!(usb, Vec::new());
    let ends: BTreeSet<HolePos> = layout.jumpers.iter().flat_map(|j| [j.from, j.to]).collect();
    assert!(usb.iter().all(|h| !ends.contains(h)), "{usb:?}");

    // The checker refuses a jumper moved into the USB connector's way. Pick
    // a hole of the area that lies in a strip (rows 0–62, strip columns).
    let target = *usb
        .iter()
        .find(|h| h.y < 63 && [3, 4, 5, 6, 7, 10, 11, 12, 13, 14].contains(&(h.x % 19)))
        .expect("the USB area lies over the board");
    let mut broken = layout.clone();
    broken.jumpers[0].to = target;
    let problems = verify_breadboard_layout(&document.design, &broken);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("Controller's access area")),
        "{problems:?}"
    );
}

#[test]
fn a_part_does_not_sit_in_another_part_access_area() {
    // Two Nanos with the USB connector north: the lower one's connector
    // needs the two rows above it, so they cannot stack one row apart.
    let document = example();
    let nano = document.design.component_definitions["local.nano-like"].clone();
    let mut b = Builder::new();
    b.part_with("N1", nano.clone()).part_with("N2", nano);
    b.net("GND", &["N1.gnd_left", "N2.gnd_left"]);
    b.net("D1", &["N1.d1", "N2.d0"]);
    let layout = b.layout();
    assert_sound(&b.design, &layout);
    for (name, other) in [("N1", "N2"), ("N2", "N1")] {
        let access: BTreeSet<HolePos> = access_of(&b.design, &layout, name).into_iter().collect();
        let part = placement(&layout, other);
        let definition = &b.design.component_definitions[&part.definition_id];
        for pin in &definition.pins {
            let (x, y) = part.rotation.rotate_local((pin.x, pin.y));
            let at = HolePos::new(
                u32::try_from(i64::from(part.position.x) + i64::from(x)).unwrap(),
                u32::try_from(i64::from(part.position.y) + i64::from(y)).unwrap(),
            );
            assert!(!access.contains(&at), "{other} pin in {name}'s access area");
        }
    }
}

#[test]
fn access_regions_face_away_from_the_channel() {
    // Two terminals land side by side, one each side of the channel: each
    // takes its wires from its own board edge, not from the other.
    let mut b = Builder::new();
    b.part("J1", "screw-terminal-3")
        .part("J2", "screw-terminal-3");
    b.net("A", &["J1.pin_1", "J2.pin_1"]);
    b.net("B", &["J1.pin_2", "J2.pin_2"]);
    b.net("C", &["J1.pin_3", "J2.pin_3"]);
    let layout = b.layout();
    assert_sound(&b.design, &layout);
    let mut sides = BTreeSet::new();
    for name in ["J1", "J2"] {
        let part = placement(&layout, name);
        sides.insert(format!("{:?}", part.placement));
        let definition = &b.design.component_definitions[&part.definition_id];
        let pin_x = {
            let pin = &definition.pins[0];
            i64::from(part.position.x) + i64::from(part.rotation.rotate_local((pin.x, pin.y)).0)
        };
        let access = access_of(&b.design, &layout, name);
        let access_x = access.iter().map(|h| i64::from(h.x)).sum::<i64>()
            / i64::try_from(access.len()).unwrap();
        match part.placement {
            PartPlacement::Left => assert!(access_x < pin_x, "{name} faces the channel"),
            PartPlacement::Right => assert!(access_x > pin_x, "{name} faces the channel"),
            other => panic!("{name} placed {other:?}"),
        }
    }
    assert_eq!(sides.len(), 2, "one terminal each side: {sides:?}");
}

#[test]
fn rail_links_sit_just_below_the_parts_on_rows_of_their_own() {
    let on_rail = |at: HolePos| matches!(at.x % 19, 0 | 1 | 16 | 17);
    let mut checked = 0;
    for (design, layout) in [
        {
            let b = timer();
            let layout = b.layout();
            (b.design, layout)
        },
        {
            let document = example();
            let layout = breadboard_layout(&document.design, DocumentRevision::default());
            (document.design, layout)
        },
    ] {
        assert_sound(&design, &layout);
        let (links, others): (Vec<_>, Vec<_>) = layout
            .jumpers
            .iter()
            .filter(|j| j.kind == JumperKind::Jumper)
            .partition(|j| on_rail(j.from) && on_rail(j.to));
        let lowest = others
            .iter()
            .flat_map(|j| [j.from.y, j.to.y])
            .max()
            .unwrap();
        let mut rows = BTreeSet::new();
        for link in &links {
            assert_eq!(link.from.y, link.to.y, "{link:?}");
            // Seen with the circuit, not dozens of rows away at the bottom.
            assert!(link.from.y <= lowest + 4, "{link:?} below row {lowest}");
            assert!(
                rows.insert(link.from.y),
                "two links share row {}",
                link.from.y
            );
            checked += 1;
        }
    }
    assert!(checked >= 2, "no rail links to check");
}
