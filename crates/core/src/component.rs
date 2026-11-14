use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Rotation;

pub const COMPONENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    #[default]
    Standard,
    Bridge,
    /// A two-pin header that a removable shunt can close. Each placed jumper
    /// records whether its shunt is fitted (`ComponentInstance::shunted`); a
    /// shunted jumper joins its two pins electrically.
    Jumper,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub struct LocalPos {
    pub x: i32,
    pub y: i32,
}

impl LocalPos {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RectMm {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PinDefinition {
    pub id: String,
    /// Author-provided physical pin marking; not inferred from array order.
    pub number: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Keepout {
    pub min: LocalPos,
    pub max: LocalPos,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessDirection {
    North,
    East,
    South,
    West,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AccessRegion {
    pub min: LocalPos,
    pub max: LocalPos,
    pub direction: AccessDirection,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub allow_board_overhang: bool,
}

const fn default_required() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentDefinition {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "is_standard_component")]
    pub kind: ComponentKind,
    pub id: String,
    pub definition_version: String,
    pub name: String,
    /// Reference designator prefix for placed parts, such as `R` for R1, R2.
    /// One to eight ASCII letters; empty names placed parts after `name`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designator: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub origin_x: i32,
    #[serde(default)]
    pub origin_y: i32,
    pub body: RectMm,
    pub allowed_rotations: Vec<Rotation>,
    pub pins: Vec<PinDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occupied_holes: Vec<LocalPos>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keepouts: Vec<Keepout>,
    /// Areas that component-side wires must not cross. Unlike `keepouts`,
    /// these do not constrain placement of other components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wire_keepouts: Vec<Keepout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access: Vec<AccessRegion>,
    #[serde(default)]
    pub documentation: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum ComponentDefinitionError {
    #[error("invalid component TOML: {0}")]
    InvalidToml(String),
    #[error("unsupported component schema version {0}")]
    UnsupportedSchema(u32),
    #[error("component {field} must not be blank")]
    BlankField { field: &'static str },
    #[error("component body dimensions must be positive finite numbers")]
    InvalidBody,
    #[error("component must allow at least one unique rotation")]
    InvalidRotations,
    #[error("duplicate pin id `{0}`")]
    DuplicatePinId(String),
    #[error("duplicate pin number `{0}`")]
    DuplicatePinNumber(String),
    #[error("pin `{0}` has a blank number")]
    BlankPinNumber(String),
    #[error("duplicate component definition id `{0}`")]
    DuplicateDefinitionId(String),
    #[error("pin `{0}` has a blank name")]
    BlankPinName(String),
    #[error("invalid rectangle in {kind} `{label}`")]
    InvalidRectangle { kind: &'static str, label: String },
    #[error("a bridge must have exactly two pins in contiguous holes")]
    InvalidBridgePins,
    #[error("a bridge cannot define occupied holes, keepouts, wire keepouts, or access regions")]
    InvalidBridgeExclusions,
    #[error("a jumper must have exactly two pins")]
    InvalidJumperPins,
    #[error("designator `{0}` must be one to eight ASCII letters, such as `R` or `LED`")]
    InvalidDesignator(String),
}

/// The longest designator prefix a definition may declare.
pub const MAX_DESIGNATOR_LEN: usize = 8;

#[allow(clippy::trivially_copy_pass_by_ref)] // Serde's skip predicate receives a reference.
const fn is_standard_component(kind: &ComponentKind) -> bool {
    matches!(kind, ComponentKind::Standard)
}

/// Parses a component library and rejects duplicate stable definition IDs.
///
/// # Errors
/// Returns a component parse/validation error or a duplicate-ID error.
pub fn parse_component_library<'a>(
    sources: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<ComponentDefinition>, ComponentDefinitionError> {
    let mut ids = BTreeSet::new();
    let mut definitions = Vec::new();
    for source in sources {
        let definition = ComponentDefinition::from_toml(source)?;
        if !ids.insert(definition.id.clone()) {
            return Err(ComponentDefinitionError::DuplicateDefinitionId(
                definition.id,
            ));
        }
        definitions.push(definition);
    }
    Ok(definitions)
}

impl ComponentDefinition {
    /// The first free name for another placed part of this definition: the
    /// designator with the lowest unused number (`R3`), or, without one, the
    /// name and a number (`Bridge 3`).
    #[must_use]
    pub fn next_instance_name<'a>(&self, used: impl IntoIterator<Item = &'a str>) -> String {
        let used: BTreeSet<&str> = used.into_iter().collect();
        let mut index = 1_usize;
        loop {
            let candidate = if self.designator.is_empty() {
                format!("{} {index}", self.name.trim())
            } else {
                format!("{}{index}", self.designator)
            };
            if !used.contains(candidate.as_str()) {
                return candidate;
            }
            index += 1;
        }
    }

    /// Whether `other` differs from this definition at most in what "Update
    /// metadata" refreshes: name, designator, description, documentation and
    /// version. Pins, geometry and kind must match.
    #[must_use]
    pub fn same_except_metadata(&self, other: &Self) -> bool {
        let mut other = other.clone();
        other
            .definition_version
            .clone_from(&self.definition_version);
        other.name.clone_from(&self.name);
        other.designator.clone_from(&self.designator);
        other.description.clone_from(&self.description);
        other.documentation.clone_from(&self.documentation);
        *self == other
    }

    /// Parses and validates a component library TOML document.
    ///
    /// # Errors
    /// Returns a typed format or invariant error.
    pub fn from_toml(source: &str) -> Result<Self, ComponentDefinitionError> {
        let value: Self = toml::from_str(source)
            .map_err(|error| ComponentDefinitionError::InvalidToml(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates component identity, rotations, pins, and rectangles.
    ///
    /// # Errors
    /// Returns the first deterministic invariant violation.
    pub fn validate(&self) -> Result<(), ComponentDefinitionError> {
        if self.schema_version != COMPONENT_SCHEMA_VERSION {
            return Err(ComponentDefinitionError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        for (field, value) in [
            ("id", self.id.as_str()),
            ("definition_version", self.definition_version.as_str()),
            ("name", self.name.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ComponentDefinitionError::BlankField { field });
            }
        }
        if !self.body.width.is_finite()
            || !self.body.height.is_finite()
            || self.body.width <= 0.0
            || self.body.height <= 0.0
            || !self.body.x.is_finite()
            || !self.body.y.is_finite()
        {
            return Err(ComponentDefinitionError::InvalidBody);
        }
        if !self.designator.is_empty()
            && (self.designator.len() > MAX_DESIGNATOR_LEN
                || !self
                    .designator
                    .bytes()
                    .all(|byte| byte.is_ascii_alphabetic()))
        {
            return Err(ComponentDefinitionError::InvalidDesignator(
                self.designator.clone(),
            ));
        }
        let rotations: BTreeSet<_> = self.allowed_rotations.iter().copied().collect();
        if rotations.is_empty() || rotations.len() != self.allowed_rotations.len() {
            return Err(ComponentDefinitionError::InvalidRotations);
        }
        let mut pin_ids = BTreeSet::new();
        let mut pin_numbers = BTreeSet::new();
        for pin in &self.pins {
            if pin.id.trim().is_empty() || !pin_ids.insert(pin.id.as_str()) {
                return Err(ComponentDefinitionError::DuplicatePinId(pin.id.clone()));
            }
            if pin.name.trim().is_empty() {
                return Err(ComponentDefinitionError::BlankPinName(pin.id.clone()));
            }
            if pin.number.trim().is_empty() {
                return Err(ComponentDefinitionError::BlankPinNumber(pin.id.clone()));
            }
            if !pin_numbers.insert(pin.number.as_str()) {
                return Err(ComponentDefinitionError::DuplicatePinNumber(
                    pin.number.clone(),
                ));
            }
        }
        for keepout in &self.keepouts {
            validate_rect(keepout.min, keepout.max, "keepout", &keepout.label)?;
        }
        for keepout in &self.wire_keepouts {
            validate_rect(keepout.min, keepout.max, "wire keepout", &keepout.label)?;
        }
        for access in &self.access {
            validate_rect(access.min, access.max, "access", &access.label)?;
        }
        if self.kind == ComponentKind::Bridge {
            if !self.occupied_holes.is_empty()
                || !self.keepouts.is_empty()
                || !self.wire_keepouts.is_empty()
                || !self.access.is_empty()
            {
                return Err(ComponentDefinitionError::InvalidBridgeExclusions);
            }
            if self.pins.len() != 2 {
                return Err(ComponentDefinitionError::InvalidBridgePins);
            }
            let dx = u64::from(self.pins[0].x.abs_diff(self.pins[1].x));
            let dy = u64::from(self.pins[0].y.abs_diff(self.pins[1].y));
            if dx + dy != 1 {
                return Err(ComponentDefinitionError::InvalidBridgePins);
            }
        }
        if self.kind == ComponentKind::Jumper && self.pins.len() != 2 {
            return Err(ComponentDefinitionError::InvalidJumperPins);
        }
        Ok(())
    }

    #[must_use]
    pub fn occupied_positions(&self) -> Vec<LocalPos> {
        let mut result = self.occupied_holes.clone();
        result.extend(self.pins.iter().map(|pin| LocalPos::new(pin.x, pin.y)));
        result.sort_unstable();
        result.dedup();
        result
    }
}

fn validate_rect(
    min: LocalPos,
    max: LocalPos,
    kind: &'static str,
    label: &str,
) -> Result<(), ComponentDefinitionError> {
    if min.x > max.x || min.y > max.y {
        return Err(ComponentDefinitionError::InvalidRectangle {
            kind,
            label: label.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> ComponentDefinition {
        ComponentDefinition {
            schema_version: 1,
            kind: ComponentKind::Standard,
            id: "dev.example.part".into(),
            definition_version: "1.0.0".into(),
            name: "Part".into(),
            designator: String::new(),
            description: String::new(),
            origin_x: 0,
            origin_y: 0,
            body: RectMm {
                x: -1.0,
                y: -1.0,
                width: 4.0,
                height: 3.0,
            },
            allowed_rotations: vec![Rotation::Deg0, Rotation::Deg90],
            pins: vec![PinDefinition {
                id: "1".into(),
                number: "1".into(),
                name: "Pin 1".into(),
                x: 0,
                y: 0,
                aliases: vec![],
                functions: vec![],
            }],
            occupied_holes: vec![],
            keepouts: vec![],
            wire_keepouts: vec![],
            access: vec![],
            documentation: String::new(),
        }
    }

    #[test]
    fn duplicate_pin_ids_are_rejected() {
        let mut value = definition();
        value.pins.push(value.pins[0].clone());
        assert_eq!(
            value.validate(),
            Err(ComponentDefinitionError::DuplicatePinId("1".into()))
        );
    }

    #[test]
    fn duplicate_physical_pin_numbers_are_rejected() {
        let mut value = definition();
        let mut duplicate = value.pins[0].clone();
        duplicate.id = "other".into();
        value.pins.push(duplicate);
        assert_eq!(
            value.validate(),
            Err(ComponentDefinitionError::DuplicatePinNumber("1".into()))
        );
    }

    #[test]
    fn malformed_pin_locations_fail_toml_parsing() {
        let source = "schema_version=1\nid='x'\ndefinition_version='1'\nname='x'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=0\nwidth=1\nheight=1\n[[pins]]\nid='1'\nnumber='1'\nname='one'\nx='left'\ny=0";
        assert!(matches!(
            ComponentDefinition::from_toml(source),
            Err(ComponentDefinitionError::InvalidToml(_))
        ));
    }

    #[test]
    fn duplicate_definition_ids_are_rejected() {
        let source = toml::to_string(&definition()).unwrap();
        assert!(matches!(
            parse_component_library([source.as_str(), source.as_str()]),
            Err(ComponentDefinitionError::DuplicateDefinitionId(_))
        ));
    }

    #[test]
    fn bridge_requires_exactly_two_adjacent_pins() {
        let mut value = definition();
        value.kind = ComponentKind::Bridge;
        assert_eq!(
            value.validate(),
            Err(ComponentDefinitionError::InvalidBridgePins)
        );

        let mut second = value.pins[0].clone();
        second.id = "2".into();
        second.number = "2".into();
        second.x = 2;
        value.pins.push(second);
        assert_eq!(
            value.validate(),
            Err(ComponentDefinitionError::InvalidBridgePins)
        );

        value.pins[1].x = 1;
        assert_eq!(value.validate(), Ok(()));
    }

    #[test]
    fn jumper_requires_exactly_two_pins() {
        let mut value = definition();
        value.kind = ComponentKind::Jumper;
        assert_eq!(
            value.validate(),
            Err(ComponentDefinitionError::InvalidJumperPins)
        );
        let mut second = value.pins[0].clone();
        second.id = "2".into();
        second.number = "2".into();
        second.x = 1;
        value.pins.push(second);
        assert_eq!(value.validate(), Ok(()));
    }

    #[test]
    fn designators_are_short_runs_of_letters() {
        let mut part = definition();
        for valid in ["", "R", "LED", "ABCDEFGH"] {
            part.designator = valid.into();
            assert_eq!(part.validate(), Ok(()), "{valid}");
        }
        for invalid in ["R1", "R-", "ABCDEFGHI", "Ω", " R"] {
            part.designator = invalid.into();
            assert_eq!(
                part.validate(),
                Err(ComponentDefinitionError::InvalidDesignator(invalid.into()))
            );
        }
    }

    #[test]
    fn next_instance_names_fill_the_lowest_gap() {
        let mut part = definition();
        assert_eq!(part.next_instance_name(["Part 1", "R1"]), "Part 2");
        part.designator = "R".into();
        assert_eq!(part.next_instance_name(["R1", "R3", "Part 2"]), "R2");
        assert_eq!(part.next_instance_name([]), "R1");
    }

    #[test]
    fn metadata_differences_are_told_apart_from_shape_changes() {
        let part = definition();
        let mut renamed = part.clone();
        renamed.name = "Other".into();
        renamed.designator = "U".into();
        renamed.definition_version = "2.0.0".into();
        renamed.documentation = "docs".into();
        assert!(part.same_except_metadata(&renamed));
        let mut moved = part.clone();
        moved.origin_x = 3;
        assert!(!part.same_except_metadata(&moved));
    }

    #[test]
    fn bridge_rejects_exclusion_geometry() {
        let mut value = definition();
        value.kind = ComponentKind::Bridge;
        let mut second = value.pins[0].clone();
        second.id = "2".into();
        second.number = "2".into();
        second.x = 1;
        value.pins.push(second);
        value.occupied_holes.push(LocalPos::new(0, 0));
        assert_eq!(
            value.validate(),
            Err(ComponentDefinitionError::InvalidBridgeExclusions)
        );
    }
}
