use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Rotation;

pub const COMPONENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RectMm {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Keepout {
    pub min: LocalPos,
    pub max: LocalPos,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessDirection {
    North,
    East,
    South,
    West,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentDefinition {
    pub schema_version: u32,
    pub id: String,
    pub definition_version: String,
    pub name: String,
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
        for access in &self.access {
            validate_rect(access.min, access.max, "access", &access.label)?;
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
            id: "dev.example.part".into(),
            definition_version: "1.0.0".into(),
            name: "Part".into(),
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
}
