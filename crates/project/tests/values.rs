use std::collections::BTreeMap;

use perfboard_core::{
    Band::{Black, Brown, Red, Violet, Yellow},
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, HolePos, OperationId,
    ProjectId, Rotation, ValueKind,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument, apply_operation,
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
            defer_checks: false,
            payload,
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

const PART: &str = "schema_version=1\nid='part'\ndefinition_version='1'\nname='Part'\ndesignator='R'\nallowed_rotations=['deg0']\n[body]\nx=-1.27\ny=-1.27\nwidth=5.08\nheight=2.54\n[[pins]]\nid='pin_1'\nnumber='1'\nname='Pin 1'\nx=0\ny=0\n[[pins]]\nid='pin_2'\nnumber='2'\nname='Pin 2'\nx=1\ny=0";

fn definition(kind: Option<&str>) -> ComponentDefinition {
    let toml = kind.map_or_else(
        || PART.to_owned(),
        |kind| {
            PART.replacen(
                "name='Part'\n",
                &format!("name='Part'\nvalue_kind='{kind}'\n"),
                1,
            )
        },
    );
    ComponentDefinition::from_toml(&toml).unwrap()
}

fn instance(definition: &ComponentDefinition, value: &str, x: i32) -> ComponentInstance {
    ComponentInstance {
        id: ComponentInstanceId::new(),
        definition_id: definition.id.clone(),
        name: String::new(),
        position: HolePos::new(x, 1),
        rotation: Rotation::Deg0,
        value: value.into(),
        properties: BTreeMap::new(),
        notes: String::new(),
        shunted: false,
        component_side: false,
        pin_labels: BTreeMap::new(),
    }
}

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "values",
        Board::new("board", BoardSize::new(16, 4).unwrap(), 2.54),
    )
}

fn add(
    document: &mut ProjectDocument,
    definition: &ComponentDefinition,
    value: &str,
) -> Result<ComponentInstanceId, String> {
    let x = 1 + 3 * i32::try_from(document.design.components.len()).unwrap();
    let instance = instance(definition, value, x);
    let id = instance.id;
    apply(
        document,
        OperationPayload::AddComponent {
            definition: definition.clone(),
            instance,
        },
    )?;
    Ok(id)
}

fn set(document: &mut ProjectDocument, id: ComponentInstanceId, value: &str) -> Result<(), String> {
    apply(
        document,
        OperationPayload::SetComponentValue {
            instance_id: id,
            value: value.into(),
        },
    )
}

#[test]
fn the_definition_declares_its_value_kind() {
    assert_eq!(definition(None).value_kind, ValueKind::Text);
    assert_eq!(
        definition(Some("resistance")).value_kind,
        ValueKind::Resistance
    );
    assert!(
        ComponentDefinition::from_toml(&PART.replacen(
            "name='Part'\n",
            "name='Part'\nvalue_kind='voltage'\n",
            1
        ))
        .is_err()
    );
}

#[test]
fn resistor_values_are_stored_normalised() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let id = add(&mut document, &resistor, "4k7").unwrap();
    assert_eq!(document.design.components[&id].value, "4.7 kΩ");

    set(&mut document, id, "10000").unwrap();
    assert_eq!(document.design.components[&id].value, "10 kΩ");

    set(&mut document, id, "").unwrap();
    assert_eq!(document.design.components[&id].value, "");
}

#[test]
fn an_invalid_value_is_refused_and_names_the_part() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let id = add(&mut document, &resistor, "").unwrap();
    let revision = document.revision;

    let error = set(&mut document, id, "2m2").unwrap_err();
    assert!(error.starts_with("R1: "), "{error}");
    assert!(error.contains("megohms"), "{error}");
    assert_eq!(document.revision, revision, "nothing changed");

    let capacitor = definition(Some("capacitance"));
    let mut other = self::document();
    let error = add(&mut other, &capacitor, "47").unwrap_err();
    assert!(error.contains("needs a unit"), "{error}");
}

#[test]
fn text_values_are_kept_as_typed() {
    let mut document = document();
    let diode = definition(None);
    let id = add(&mut document, &diode, " 1N4148 ").unwrap();
    assert_eq!(document.design.components[&id].value, "1N4148");
}

#[test]
fn a_value_kind_is_metadata_that_a_newer_library_part_brings_in() {
    let mut document = document();
    let old = definition(None);
    let first = add(&mut document, &old, "10k").unwrap();
    assert_eq!(document.design.components[&first].value, "10k");

    let new = definition(Some("resistance"));
    assert!(old.same_except_metadata(&new));
    add(&mut document, &new, "220").unwrap();
    assert_eq!(
        document.design.component_definitions["part"].value_kind,
        ValueKind::Resistance
    );
}

#[test]
fn values_survive_a_save_and_load() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let id = add(&mut document, &resistor, "4k7").unwrap();
    let json = perfboard_project::to_snapshot_json(&document).unwrap();
    assert!(json.contains("\"value\": \"4.7 kΩ\""), "{json}");
    assert!(json.contains("\"value_kind\": \"resistance\""), "{json}");
    let loaded = ProjectDocument::from_json(&json).unwrap();
    assert_eq!(loaded.design.components[&id].value, "4.7 kΩ");
}

#[test]
fn the_assembly_plan_names_the_value_and_its_colour_code() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let first = add(&mut document, &resistor, "10k").unwrap();
    let plan = perfboard_project::assembly_plan(&document.design, document.revision);
    let step = plan
        .steps
        .iter()
        .find(|step| step.id == format!("component:{first}"))
        .unwrap();
    assert_eq!(step.title, "Fit R1 (Part), 10 kΩ");
    assert_eq!(
        step.instructions[0],
        "Check the colour bands: brown, black, black, red, brown (10 kΩ, 1%)."
    );
    let value = step.value.as_ref().unwrap();
    assert_eq!(value.display, "10 kΩ");
    assert_eq!(value.bands, [Brown, Black, Black, Red, Brown]);
}

#[test]
fn capacitors_show_their_code() {
    let mut document = document();
    let capacitor = definition(Some("capacitance"));
    add(&mut document, &capacitor, "0.1u").unwrap();
    let plan = perfboard_project::assembly_plan(&document.design, document.revision);
    let step = plan.steps.iter().find(|step| step.label == "R1").unwrap();
    assert_eq!(step.instructions[0], "Check the value: 100 nF (code 104).");
    assert_eq!(step.value.as_ref().unwrap().code.as_deref(), Some("104"));
}

#[test]
fn the_parts_list_groups_parts_by_value() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    add(&mut document, &resistor, "10k").unwrap();
    add(&mut document, &resistor, "4k7").unwrap();
    let third = add(&mut document, &resistor, "").unwrap();
    add(&mut document, &resistor, "10000").unwrap();

    let plan = perfboard_project::assembly_plan(&document.design, document.revision);
    let groups: Vec<(Option<&str>, Vec<&str>)> = plan
        .parts
        .iter()
        .map(|entry| {
            (
                entry.value.as_ref().map(|value| value.display.as_str()),
                entry.names.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            (Some("4.7 kΩ"), vec!["R2"]),
            (Some("10 kΩ"), vec!["R1", "R4"]),
            (None, vec!["R3"]),
        ]
    );

    let missing: Vec<&str> = plan
        .warnings
        .iter()
        .filter(|warning| warning.code == "assembly.value_missing")
        .map(|warning| warning.objects[0].id.as_str())
        .collect();
    assert_eq!(missing, [third.to_string()]);
}

#[test]
fn changing_a_value_changes_the_step_fingerprint() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let id = add(&mut document, &resistor, "10k").unwrap();
    let step = |document: &ProjectDocument| {
        perfboard_project::assembly_plan(&document.design, document.revision)
            .steps
            .into_iter()
            .find(|step| step.id == format!("component:{id}"))
            .unwrap()
            .fingerprint
    };
    let before = step(&document);
    set(&mut document, id, "4k7").unwrap();
    assert_ne!(
        step(&document),
        before,
        "the fitted part is now the wrong one"
    );
}

#[test]
fn text_parts_never_miss_a_value() {
    let mut document = document();
    add(&mut document, &definition(None), "").unwrap();
    let plan = perfboard_project::assembly_plan(&document.design, document.revision);
    assert!(
        plan.warnings
            .iter()
            .all(|warning| warning.code != "assembly.value_missing")
    );
}

#[test]
fn breadboard_parts_carry_their_value() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let id = add(&mut document, &resistor, "220").unwrap();
    let layout = perfboard_project::breadboard_layout(&document.design, document.revision);
    let part = layout
        .parts
        .iter()
        .find(|part| part.component_id == id)
        .unwrap();
    let value = part.value.as_ref().unwrap();
    assert_eq!(value.display, "220 Ω");
    assert_eq!(value.bands, [Red, Red, Black, Black, Brown]);
}

#[test]
fn the_project_view_carries_each_parts_reading() {
    let mut document = document();
    let resistor = definition(Some("resistance"));
    let valued = add(&mut document, &resistor, "4k7").unwrap();
    let bare = add(&mut document, &resistor, "").unwrap();
    let view = perfboard_project::ProjectView::from_document(&document);
    let reading = &view.value_readings[&valued];
    assert_eq!(reading.display, "4.7 kΩ");
    assert_eq!(reading.bands, [Yellow, Violet, Black, Brown, Brown]);
    assert!(!view.value_readings.contains_key(&bare));
}

#[test]
fn update_metadata_brings_in_the_value_kind_and_tidies_values() {
    let mut document = document();
    let old = definition(None);
    let typed = add(&mut document, &old, "220").unwrap();
    let odd = add(&mut document, &old, "about 5k").unwrap();
    assert_eq!(document.design.components[&typed].value, "220");

    // The library's newer copy: same version, now a resistance.
    let new = definition(Some("resistance"));
    apply(
        &mut document,
        OperationPayload::UpdateComponentMetadata {
            instance_id: typed,
            definition: new,
        },
    )
    .unwrap();
    assert_eq!(
        document.design.component_definitions["part"].value_kind,
        ValueKind::Resistance
    );
    assert_eq!(document.design.components[&typed].value, "220 Ω");
    assert_eq!(document.design.components[&odd].value, "about 5k");
    let view = perfboard_project::ProjectView::from_document(&document);
    assert_eq!(
        view.value_readings[&typed].bands,
        [Red, Red, Black, Black, Brown]
    );
}
