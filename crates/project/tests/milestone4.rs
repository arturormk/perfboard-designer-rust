use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    Constraint, ConstraintKind, ConstraintOrigin, ConstraintSeverity, ElectricalNet, GridRect,
    HolePos, JumperInventoryEntry, JumperMatchKind, NetId, OperationId, PinRef, ProjectId,
    Rotation, RoutePos, Side, TerminalRef, Wire, WireColor, WireId,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument, RouteError,
    RouteRequest, RoutingCostConfig, apply_operation, evaluate_route, suggest_route,
    to_snapshot_json,
};

fn definition() -> ComponentDefinition {
    ComponentDefinition::from_toml(
        "schema_version=1\nid='test.point'\ndefinition_version='1'\nname='Point'\nallowed_rotations=['deg0']\n[body]\nx=-0.4\ny=-0.4\nwidth=0.8\nheight=0.8\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
    ).unwrap()
}

fn apply(document: &mut ProjectDocument, payload: OperationPayload) {
    let revision = document.revision;
    apply_operation(
        document,
        OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: revision,
            origin: OperationOrigin::User,
            rationale: None,
            allow_invalid_placement: true,
            payload,
        },
    )
    .unwrap();
}

fn fixture(
    points: &[(&str, u32, u32)],
) -> (ProjectDocument, BTreeMap<String, ComponentInstanceId>) {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "Milestone 4",
        Board::new("board", BoardSize::new(9, 9).unwrap(), 2.54),
    );
    let definition = definition();
    let mut ids = BTreeMap::new();
    for &(name, x, y) in points {
        let id = ComponentInstanceId::new();
        ids.insert(name.into(), id);
        apply(
            &mut document,
            OperationPayload::AddComponent {
                definition: definition.clone(),
                instance: ComponentInstance {
                    id,
                    definition_id: definition.id.clone(),
                    name: name.into(),
                    position: HolePos::new(x, y),
                    rotation: Rotation::Deg0,
                    properties: BTreeMap::new(),
                    notes: String::new(),
                },
            },
        );
    }
    (document, ids)
}

fn routed_net(
    document: &mut ProjectDocument,
    ids: &BTreeMap<String, ComponentInstanceId>,
    names: &[&str],
) -> (NetId, Vec<TerminalRef>) {
    let net_id = NetId::new();
    let pins: BTreeSet<_> = names
        .iter()
        .map(|name| PinRef {
            component_id: ids[*name],
            pin_id: "p".into(),
        })
        .collect();
    let terminals = pins.iter().cloned().map(TerminalRef::from).collect();
    apply(
        document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: names.join("-"),
                state: ConnectionState::Accepted,
                pins,
                notes: String::new(),
            },
        },
    );
    (net_id, terminals)
}

fn request(net_id: NetId, terminals: &[TerminalRef]) -> RouteRequest {
    RouteRequest {
        net_id,
        start: terminals[0].clone(),
        end: terminals[1].clone(),
        preferred_side: Some(Side::Solder),
        replace_wire_id: None,
    }
}

fn required_reserved(min: HolePos, max: HolePos) -> Constraint {
    Constraint {
        id: perfboard_core::ConstraintId::new(),
        severity: ConstraintSeverity::Required,
        origin: ConstraintOrigin::User,
        rationale: "Keep this area clear".into(),
        annotation: "natural-language note is preserved".into(),
        kind: ConstraintKind::ReservedArea {
            area: GridRect { min, max },
            label: "reserved".into(),
        },
    }
}

#[test]
fn identical_state_and_config_produce_byte_equivalent_obstacle_avoiding_routes() {
    let (mut document, ids) = fixture(&[("a", 1, 4), ("b", 7, 4)]);
    let (net_id, terminals) = routed_net(&mut document, &ids, &["a", "b"]);
    apply(
        &mut document,
        OperationPayload::AddConstraint {
            constraint: required_reserved(HolePos::new(3, 3), HolePos::new(5, 5)),
        },
    );
    apply(
        &mut document,
        OperationPayload::AddConstraint {
            constraint: Constraint {
                id: perfboard_core::ConstraintId::new(),
                severity: ConstraintSeverity::Required,
                origin: ConstraintOrigin::User,
                rationale: "leave room between bends".into(),
                annotation: String::new(),
                kind: ConstraintKind::BendSpacing {
                    min_half_grid_steps: 3,
                },
            },
        },
    );
    let request = request(net_id, &terminals);
    let first = suggest_route(&document.design, &request, &RoutingCostConfig::default()).unwrap();
    let second = suggest_route(&document.design, &request, &RoutingCostConfig::default()).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert!(first.vertices.len() >= 3);
    assert!(first.vertices.windows(2).all(|pair| {
        let dx = (pair[1].x2 - pair[0].x2).abs();
        let dy = (pair[1].y2 - pair[0].y2).abs();
        dx == 0 || dy == 0 || dx == dy
    }));
    assert!(first.vertices.windows(4).all(|points| {
        let between_bends = (points[2].x2 - points[1].x2)
            .abs()
            .max((points[2].y2 - points[1].y2).abs());
        between_bends >= 3
    }));
}

#[test]
fn router_uses_diagonal_moves_when_they_are_clear() {
    let (mut document, ids) = fixture(&[("a", 1, 1), ("b", 7, 7)]);
    let (net_id, terminals) = routed_net(&mut document, &ids, &["a", "b"]);
    let proposal = suggest_route(
        &document.design,
        &request(net_id, &terminals),
        &RoutingCostConfig::default(),
    )
    .unwrap();
    assert_eq!(proposal.vertices.len(), 2);
    let endpoints: BTreeSet<_> = proposal.vertices.into_iter().collect();
    assert_eq!(
        endpoints,
        BTreeSet::from([RoutePos::new(2, 2), RoutePos::new(14, 14)])
    );
}

#[test]
fn required_bend_spacing_and_no_path_explanations_are_enforced() {
    let (mut document, ids) = fixture(&[("a", 1, 4), ("b", 7, 4)]);
    let (net_id, terminals) = routed_net(&mut document, &ids, &["a", "b"]);
    apply(
        &mut document,
        OperationPayload::AddConstraint {
            constraint: Constraint {
                id: perfboard_core::ConstraintId::new(),
                severity: ConstraintSeverity::Required,
                origin: ConstraintOrigin::User,
                rationale: "space bends".into(),
                annotation: String::new(),
                kind: ConstraintKind::BendSpacing {
                    min_half_grid_steps: 3,
                },
            },
        },
    );
    apply(
        &mut document,
        OperationPayload::AddConstraint {
            constraint: required_reserved(HolePos::new(4, 0), HolePos::new(4, 8)),
        },
    );
    let error = suggest_route(
        &document.design,
        &request(net_id, &terminals),
        &RoutingCostConfig::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error, RouteError::NoPath { reasons } if reasons.iter().any(|reason| reason.contains("bend spacing")))
    );
}

#[test]
fn crossings_are_explicit_and_do_not_create_connectivity() {
    let (mut document, ids) = fixture(&[
        ("left", 1, 4),
        ("right", 7, 4),
        ("top", 4, 1),
        ("bottom", 4, 7),
    ]);
    let (vertical_net, vertical) = routed_net(&mut document, &ids, &["top", "bottom"]);
    let crossing_wire = WireId::new();
    apply(
        &mut document,
        OperationPayload::AddWire {
            wire: Wire {
                id: crossing_wire,
                net_id: vertical_net,
                start: vertical[0].clone(),
                end: vertical[1].clone(),
                side: Side::Solder,
                color: WireColor::Blue,
                vertices: vec![RoutePos::new(8, 2), RoutePos::new(8, 14)],
                locked: true,
                notes: String::new(),
            },
        },
    );
    let (horizontal_net, horizontal) = routed_net(&mut document, &ids, &["left", "right"]);
    let proposal = evaluate_route(
        &document.design,
        &request(horizontal_net, &horizontal),
        Side::Solder,
        vec![RoutePos::new(2, 8), RoutePos::new(14, 8)],
        &RoutingCostConfig::default(),
    );
    assert_eq!(proposal.crossings.len(), 1);
    assert_eq!(proposal.crossings[0].other_wire_id, crossing_wire);
    assert!(
        proposal
            .warnings
            .iter()
            .any(|warning| warning.contains("over/under"))
    );
    let horizontal_wire = WireId::new();
    apply(
        &mut document,
        OperationPayload::AcceptRouteProposal {
            proposal,
            wire_id: horizontal_wire,
            color: WireColor::Red,
            notes: String::new(),
            replace_wire_id: None,
            new_route_over: Some(false),
        },
    );
    let choice = document.design.crossing_choices.values().next().unwrap();
    assert_eq!(choice.over_wire_id, crossing_wire);
}

#[test]
fn exact_and_tolerance_boundary_jumper_matches_are_deterministic() {
    let (mut document, ids) = fixture(&[("a", 1, 2), ("b", 5, 2)]);
    let (net_id, terminals) = routed_net(&mut document, &ids, &["a", "b"]);
    for entry in [
        JumperInventoryEntry {
            id: "exact-4".into(),
            span_holes: 4,
            quantity: Some(1),
            color: "red".into(),
            wire_diameter_mm: Some(0.6),
            body_height_mm: Some(3.0),
            tolerance_holes: 0,
        },
        JumperInventoryEntry {
            id: "near-3".into(),
            span_holes: 3,
            quantity: None,
            color: "blue".into(),
            wire_diameter_mm: None,
            body_height_mm: None,
            tolerance_holes: 1,
        },
    ] {
        apply(
            &mut document,
            OperationPayload::UpsertJumperInventory { entry },
        );
    }
    let proposal = suggest_route(
        &document.design,
        &request(net_id, &terminals),
        &RoutingCostConfig::default(),
    )
    .unwrap();
    assert_eq!(
        proposal.jumper_match.as_ref().unwrap().inventory_id,
        "exact-4"
    );
    assert_eq!(
        proposal.jumper_match.as_ref().unwrap().kind,
        JumperMatchKind::Exact
    );
    apply(
        &mut document,
        OperationPayload::RemoveJumperInventory {
            inventory_id: "exact-4".into(),
        },
    );
    let approximate = suggest_route(
        &document.design,
        &request(net_id, &terminals),
        &RoutingCostConfig::default(),
    )
    .unwrap();
    assert_eq!(
        approximate.jumper_match.unwrap().kind,
        JumperMatchKind::Approximate
    );
}

#[test]
fn accepting_a_proposal_preserves_unrelated_and_locked_routes_and_round_trips() {
    let (mut document, ids) = fixture(&[("a", 1, 2), ("b", 7, 2), ("c", 1, 6), ("d", 7, 6)]);
    let (first_net, first_terms) = routed_net(&mut document, &ids, &["a", "b"]);
    let locked_id = WireId::new();
    apply(
        &mut document,
        OperationPayload::AddWire {
            wire: Wire {
                id: locked_id,
                net_id: first_net,
                start: first_terms[0].clone(),
                end: first_terms[1].clone(),
                side: Side::Solder,
                color: WireColor::Black,
                vertices: vec![RoutePos::new(2, 4), RoutePos::new(14, 4)],
                locked: true,
                notes: "accepted".into(),
            },
        },
    );
    let locked_before = document.design.wires[&locked_id].clone();
    let (second_net, second_terms) = routed_net(&mut document, &ids, &["c", "d"]);
    let proposal = suggest_route(
        &document.design,
        &request(second_net, &second_terms),
        &RoutingCostConfig::default(),
    )
    .unwrap();
    let new_id = WireId::new();
    apply(
        &mut document,
        OperationPayload::AcceptRouteProposal {
            proposal,
            wire_id: new_id,
            color: WireColor::Green,
            notes: "automatic".into(),
            replace_wire_id: None,
            new_route_over: Some(true),
        },
    );
    assert_eq!(document.design.wires[&locked_id], locked_before);
    assert!(document.design.wires.contains_key(&new_id));
    let reopened = ProjectDocument::from_json(&to_snapshot_json(&document).unwrap()).unwrap();
    assert_eq!(reopened, document);
}

#[test]
fn constraint_crud_is_revisioned_and_required_rules_survive_manual_override() {
    let (mut document, ids) = fixture(&[("a", 1, 1)]);
    let constraint = Constraint {
        id: perfboard_core::ConstraintId::new(),
        severity: ConstraintSeverity::Required,
        origin: ConstraintOrigin::Mcp,
        rationale: "fixture must stay put".into(),
        annotation: "user prose".into(),
        kind: ConstraintKind::FixedPlacement {
            component_id: ids["a"],
            position: HolePos::new(1, 1),
        },
    };
    apply(
        &mut document,
        OperationPayload::AddConstraint {
            constraint: constraint.clone(),
        },
    );
    let before = document.clone();
    let revision = document.revision;
    let result = apply_operation(
        &mut document,
        OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: revision,
            origin: OperationOrigin::User,
            rationale: Some("try override".into()),
            allow_invalid_placement: true,
            payload: OperationPayload::MoveComponent {
                instance_id: ids["a"],
                position: HolePos::new(2, 1),
            },
        },
    );
    assert!(result.is_err());
    assert_eq!(document, before);
    apply(
        &mut document,
        OperationPayload::RemoveConstraint {
            constraint_id: constraint.id,
        },
    );
    assert!(document.design.constraints.is_empty());
}

#[test]
fn sample_project_auto_route_manual_adjust_and_reopen_has_no_route_drift() {
    let mut document =
        ProjectDocument::from_json(include_str!("../../../examples/nano-rs485.perfboard.json"))
            .unwrap();
    let original = document.design.wires.values().next().unwrap().clone();
    let request = RouteRequest {
        net_id: original.net_id,
        start: original.start.clone(),
        end: original.end.clone(),
        preferred_side: Some(original.side),
        replace_wire_id: Some(original.id),
    };
    let proposal =
        suggest_route(&document.design, &request, &RoutingCostConfig::default()).unwrap();
    let replacement_id = WireId::new();
    apply(
        &mut document,
        OperationPayload::AcceptRouteProposal {
            proposal,
            wire_id: replacement_id,
            color: original.color,
            notes: "Automatically routed, then reviewed manually".into(),
            replace_wire_id: Some(original.id),
            new_route_over: Some(true),
        },
    );
    let mut adjusted = document.design.wires[&replacement_id].vertices.clone();
    let first = adjusted[0];
    let second = adjusted[1];
    let inserted = RoutePos::new(
        first.x2 + (second.x2 - first.x2).signum(),
        first.y2 + (second.y2 - first.y2).signum(),
    );
    adjusted.insert(1, inserted);
    apply(
        &mut document,
        OperationPayload::ModifyWirePath {
            wire_id: replacement_id,
            vertices: adjusted.clone(),
        },
    );
    let reopened = ProjectDocument::from_json(&to_snapshot_json(&document).unwrap()).unwrap();
    assert_eq!(reopened.design.wires[&replacement_id].vertices, adjusted);
    assert!(!reopened.design.wires.contains_key(&original.id));
}
