use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, NetId, OperationId, PinRef, ProjectId, Rotation, RoutePos, Saddle,
    SaddleId, Side, TerminalRef, Wire, WireColor, WireId,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument, RouteRequest,
    RoutingCostConfig, apply_operation, suggest_route, to_snapshot_json,
};

fn apply(document: &mut ProjectDocument, payload: OperationPayload) -> Result<(), String> {
    apply_operation(
        document,
        OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: document.revision,
            origin: OperationOrigin::User,
            rationale: None,
            allow_invalid_placement: false,
            payload,
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn saddle(side: Side, start: (u32, u32), end: (u32, u32)) -> Saddle {
    Saddle {
        id: SaddleId::new(),
        side,
        start: HolePos::new(start.0, start.1),
        end: HolePos::new(end.0, end.1),
        color: WireColor::Black,
        notes: String::new(),
    }
}

fn codes(document: &ProjectDocument) -> BTreeSet<String> {
    document
        .validation
        .iter()
        .filter(|diagnostic| diagnostic.code.0.starts_with("saddle."))
        .map(|diagnostic| diagnostic.code.0.clone())
        .collect()
}

/// Two single-pin parts at (1,2) and (7,2) joined by a straight solder-side
/// wire along row 2.
fn fixture() -> (ProjectDocument, [ComponentInstanceId; 2], NetId) {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "saddles",
        Board::new("board", BoardSize::new(10, 6).unwrap(), 2.54),
    );
    let definition = ComponentDefinition::from_toml(
        "schema_version=1\nid='pin'\ndefinition_version='1'\nname='Pin'\nallowed_rotations=['deg0']\n[body]\nx=-0.2\ny=-0.2\nwidth=0.4\nheight=0.4\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
    )
    .unwrap();
    let ids = [ComponentInstanceId::new(), ComponentInstanceId::new()];
    for (id, x) in ids.into_iter().zip([1, 7]) {
        apply(
            &mut document,
            OperationPayload::AddComponent {
                definition: definition.clone(),
                instance: ComponentInstance {
                    id,
                    definition_id: definition.id.clone(),
                    name: format!("pin{x}"),
                    position: HolePos::new(x, 2),
                    rotation: Rotation::Deg0,
                    properties: BTreeMap::new(),
                    notes: String::new(),
                },
            },
        )
        .unwrap();
    }
    let pins: Vec<PinRef> = ids
        .into_iter()
        .map(|component_id| PinRef {
            component_id,
            pin_id: "p".into(),
        })
        .collect();
    let net_id = NetId::new();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "signal".into(),
                state: ConnectionState::Accepted,
                pins: pins.iter().cloned().collect(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    apply(
        &mut document,
        OperationPayload::AddWire {
            wire: Wire {
                id: WireId::new(),
                net_id,
                start: TerminalRef::from(pins[0].clone()),
                end: TerminalRef::from(pins[1].clone()),
                side: Side::Solder,
                color: WireColor::Green,
                vertices: vec![RoutePos::new(2, 4), RoutePos::new(14, 4)],
                locked: false,
                notes: String::new(),
            },
        },
    )
    .unwrap();
    (document, ids, net_id)
}

#[test]
fn a_saddle_across_a_wire_is_clean_net_free_and_survives_a_round_trip() {
    let (mut document, _, _) = fixture();
    let tie = saddle(Side::Solder, (4, 1), (4, 3));
    apply(
        &mut document,
        OperationPayload::AddSaddle {
            saddle: tie.clone(),
        },
    )
    .unwrap();
    assert!(codes(&document).is_empty(), "{:?}", document.validation);
    assert!(
        !document
            .validation
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "circuit.net_incomplete")
    );

    let json = to_snapshot_json(&document).unwrap();
    assert!(json.contains("\"saddles\""));
    let reloaded = ProjectDocument::from_json(&json).unwrap();
    assert_eq!(reloaded.design.saddles.get(&tie.id), Some(&tie));
}

#[test]
fn saddle_holes_must_be_free_and_distinct() {
    let (mut document, _, _) = fixture();
    let rejected = [
        (saddle(Side::Solder, (1, 2), (1, 4)), "pin"),
        (saddle(Side::Solder, (4, 2), (4, 4)), "wire"),
        (saddle(Side::Solder, (4, 1), (4, 1)), "two different holes"),
        (saddle(Side::Solder, (4, 1), (4, 6)), "two different holes"),
        (saddle(Side::Solder, (1, 1), (1, 3)), "component pin"),
    ];
    for (candidate, expected) in rejected {
        let error = apply(
            &mut document,
            OperationPayload::AddSaddle { saddle: candidate },
        )
        .unwrap_err();
        assert!(
            error.contains(expected),
            "{error} should mention {expected}"
        );
    }

    let first = saddle(Side::Solder, (4, 1), (4, 3));
    apply(&mut document, OperationPayload::AddSaddle { saddle: first }).unwrap();
    let error = apply(
        &mut document,
        OperationPayload::AddSaddle {
            saddle: saddle(Side::Solder, (4, 3), (4, 5)),
        },
    )
    .unwrap_err();
    assert!(error.contains("another saddle"), "{error}");
}

#[test]
fn a_saddle_on_the_wrong_side_holds_nothing() {
    let (mut document, _, _) = fixture();
    let tie = saddle(Side::Component, (4, 1), (4, 3));
    apply(
        &mut document,
        OperationPayload::AddSaddle {
            saddle: tie.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        codes(&document),
        BTreeSet::from(["saddle.holds_nothing".to_string()])
    );

    apply(
        &mut document,
        OperationPayload::UpdateSaddle {
            saddle: Saddle {
                side: Side::Solder,
                ..tie
            },
        },
    )
    .unwrap();
    assert!(codes(&document).is_empty());
}

#[test]
fn later_edits_that_take_a_saddle_hole_are_reported() {
    let (mut document, ids, _) = fixture();
    apply(
        &mut document,
        OperationPayload::AddSaddle {
            saddle: saddle(Side::Solder, (4, 1), (4, 3)),
        },
    )
    .unwrap();
    apply(
        &mut document,
        OperationPayload::MoveComponent {
            instance_id: ids[1],
            position: HolePos::new(4, 3),
        },
    )
    .unwrap();
    assert!(codes(&document).contains("saddle.hole_conflict"));
}

#[test]
fn the_router_treats_saddle_holes_as_obstacles() {
    let (mut document, ids, net_id) = fixture();
    // Leave the wire out of the way: route a fresh connection instead.
    let wire_id = *document.design.wires.keys().next().unwrap();
    apply(&mut document, OperationPayload::RemoveWire { wire_id }).unwrap();
    apply(
        &mut document,
        OperationPayload::AddSaddle {
            saddle: saddle(Side::Component, (4, 2), (4, 0)),
        },
    )
    .unwrap();
    let request = RouteRequest {
        net_id,
        start: TerminalRef::Pin {
            component_id: ids[0],
            pin_id: "p".into(),
        },
        end: TerminalRef::Pin {
            component_id: ids[1],
            pin_id: "p".into(),
        },
        preferred_side: Some(Side::Solder),
        replace_wire_id: None,
    };
    let proposal = suggest_route(&document.design, &request, &RoutingCostConfig::default())
        .expect("a detour exists");
    let blocked = RoutePos::new(8, 4);
    let passes = proposal.vertices.windows(2).any(|pair| {
        let (low_x, high_x) = (pair[0].x2.min(pair[1].x2), pair[0].x2.max(pair[1].x2));
        let (low_y, high_y) = (pair[0].y2.min(pair[1].y2), pair[0].y2.max(pair[1].y2));
        (low_x..=high_x).contains(&blocked.x2) && (low_y..=high_y).contains(&blocked.y2)
    });
    assert!(
        !passes,
        "route {:?} crosses a saddle hole",
        proposal.vertices
    );
}

#[test]
fn resizing_the_board_moves_saddles_with_the_design() {
    let (mut document, _, _) = fixture();
    let tie = saddle(Side::Solder, (4, 1), (4, 3));
    apply(
        &mut document,
        OperationPayload::AddSaddle {
            saddle: tie.clone(),
        },
    )
    .unwrap();
    apply(
        &mut document,
        OperationPayload::ResizeBoard {
            holes_x: 12,
            holes_y: 8,
            offset_x: 2,
            offset_y: 2,
        },
    )
    .unwrap();
    let moved = &document.design.saddles[&tie.id];
    assert_eq!(
        (moved.start, moved.end),
        (HolePos::new(6, 3), HolePos::new(6, 5))
    );
    apply(
        &mut document,
        OperationPayload::RemoveSaddle { saddle_id: tie.id },
    )
    .unwrap();
    assert!(document.design.saddles.is_empty());
}
