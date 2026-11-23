use std::collections::BTreeMap;

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, HolePos, OperationId,
    ProjectId, Rotation, Side,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument, apply_operation,
    render_design_svg,
};

const PART: &str = "schema_version=1\nid='timer'\ndefinition_version='1'\nname='Timer'\ndesignator='U'\nallowed_rotations=['deg0']\n[body]\nx=-1.27\ny=-1.27\nwidth=5.08\nheight=2.54\n[[pins]]\nid='pin_1'\nnumber='1'\nname='GND'\nx=0\ny=0\n[[pins]]\nid='pin_2'\nnumber='2'\nname='TRIG'\nx=1\ny=0";

fn apply(document: &mut ProjectDocument, payload: OperationPayload) -> Result<String, String> {
    apply_operation(
        document,
        OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: document.revision,
            origin: OperationOrigin::User,
            rationale: None,
            allow_invalid_placement: false,
            defer_checks: false,
            payload,
        },
    )
    .map(|result| result.change_summary)
    .map_err(|error| error.to_string())
}

fn setup() -> (ProjectDocument, ComponentInstanceId, ComponentInstanceId) {
    let definition = ComponentDefinition::from_toml(PART).unwrap();
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "pin labels",
        Board::new("board", BoardSize::new(16, 4).unwrap(), 2.54),
    );
    let mut ids = Vec::new();
    for x in [1, 5] {
        let instance = ComponentInstance {
            id: ComponentInstanceId::new(),
            definition_id: definition.id.clone(),
            name: String::new(),
            position: HolePos::new(x, 1),
            rotation: Rotation::Deg0,
            value: String::new(),
            properties: BTreeMap::new(),
            notes: String::new(),
            shunted: false,
            component_side: false,
            band_count: perfboard_core::BandCount::Five,
            pin_labels: BTreeMap::new(),
        };
        ids.push(instance.id);
        apply(
            &mut document,
            OperationPayload::AddComponent {
                definition: definition.clone(),
                instance,
            },
        )
        .unwrap();
    }
    (document, ids[0], ids[1])
}

fn label(
    document: &mut ProjectDocument,
    id: ComponentInstanceId,
    pin: &str,
    label: &str,
) -> Result<String, String> {
    apply(
        document,
        OperationPayload::SetPinLabel {
            instance_id: id,
            pin_id: pin.into(),
            label: label.into(),
        },
    )
}

fn pin_name(document: &ProjectDocument, id: ComponentInstanceId, pin: &str) -> String {
    let instance = &document.design.components[&id];
    let definition = &document.design.component_definitions[&instance.definition_id];
    let pin = definition.pins.iter().find(|item| item.id == pin).unwrap();
    instance.pin_name(pin).to_owned()
}

#[test]
fn a_label_renames_the_pin_on_one_part_only() {
    let (mut document, first, second) = setup();
    let summary = label(&mut document, first, "pin_2", "  START ").unwrap();
    assert_eq!(summary, "Named pin 2 of U1 START");
    assert_eq!(pin_name(&document, first, "pin_2"), "START");
    assert_eq!(pin_name(&document, second, "pin_2"), "TRIG");
    assert_eq!(pin_name(&document, first, "pin_1"), "GND");
    let definition = &document.design.component_definitions["timer"];
    assert_eq!(definition.pins[1].name, "TRIG");
}

#[test]
fn an_empty_label_or_the_default_name_restores_the_definition_name() {
    let (mut document, first, _) = setup();
    label(&mut document, first, "pin_2", "START").unwrap();
    let summary = label(&mut document, first, "pin_2", "  ").unwrap();
    assert_eq!(summary, "Pin 2 of U1 is named TRIG again");
    assert!(document.design.components[&first].pin_labels.is_empty());

    label(&mut document, first, "pin_2", "START").unwrap();
    label(&mut document, first, "pin_2", "TRIG").unwrap();
    assert!(document.design.components[&first].pin_labels.is_empty());
}

#[test]
fn a_label_needs_a_pin_the_part_has() {
    let (mut document, first, _) = setup();
    let revision = document.revision;
    let error = label(&mut document, first, "pin_9", "X").unwrap_err();
    assert_eq!(error, "component has no pin pin_9");
    assert_eq!(document.revision, revision);
}

#[test]
fn labels_are_saved_only_when_a_part_has_some() {
    let (mut document, first, second) = setup();
    label(&mut document, first, "pin_2", "START").unwrap();
    let json = serde_json::to_value(&document.design.components).unwrap();
    assert_eq!(
        json[first.to_string()]["pin_labels"],
        serde_json::json!({ "pin_2": "START" })
    );
    assert!(json[second.to_string()].get("pin_labels").is_none());
    let back: BTreeMap<ComponentInstanceId, ComponentInstance> =
        serde_json::from_value(json).unwrap();
    assert_eq!(back, document.design.components);
}

#[test]
fn the_drawing_names_pins_by_their_label() {
    let (mut document, first, _) = setup();
    label(&mut document, first, "pin_2", "START").unwrap();
    let svg = render_design_svg(&document, Side::Component);
    assert!(svg.contains("<title>Pin 2: START</title>"));
    assert!(svg.contains("<title>Pin 2: TRIG</title>"));
}
