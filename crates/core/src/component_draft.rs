use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AccessRegion, ComponentDefinition, Keepout, LocalPos, PinDefinition, RectMm, Rotation,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComponentDraftOperation {
    SetMetadata {
        id: String,
        definition_version: String,
        name: String,
        description: String,
    },
    SetBody {
        body: RectMm,
    },
    SetOrigin {
        x: i32,
        y: i32,
    },
    SetRotations {
        rotations: Vec<Rotation>,
    },
    UpsertPin {
        #[serde(default)]
        previous_id: Option<String>,
        pin: PinDefinition,
    },
    RemovePin {
        id: String,
    },
    ReorderPin {
        id: String,
        index: usize,
    },
    SetOccupiedHoles {
        holes: Vec<LocalPos>,
    },
    SetKeepouts {
        keepouts: Vec<Keepout>,
    },
    SetWireKeepouts {
        keepouts: Vec<Keepout>,
    },
    SetAccessRegions {
        access: Vec<AccessRegion>,
    },
    SetDocumentation {
        documentation: String,
    },
}

#[allow(clippy::struct_excessive_bools)] // Explicit UI state avoids inference from nullable fields.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ComponentDraftSnapshot {
    pub definition: ComponentDefinition,
    pub documentation: String,
    pub valid: bool,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct DraftValue {
    definition: ComponentDefinition,
    documentation: String,
}

#[derive(Clone, Debug)]
pub struct ComponentDraft {
    value: DraftValue,
    saved: Option<DraftValue>,
    undo: Vec<DraftValue>,
    redo: Vec<DraftValue>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ComponentDraftError {
    #[error("pin `{0}` was not found")]
    PinNotFound(String),
    #[error("pin id `{0}` is already used")]
    DuplicatePinId(String),
    #[error("pin number `{0}` is already used")]
    DuplicatePinNumber(String),
    #[error("could not serialize component TOML: {0}")]
    Serialize(String),
}

impl ComponentDraft {
    #[must_use]
    pub fn new(definition: ComponentDefinition, documentation: impl Into<String>) -> Self {
        let value = DraftValue {
            definition,
            documentation: documentation.into(),
        };
        Self {
            saved: Some(value.clone()),
            value,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    #[must_use]
    pub fn new_unsaved(definition: ComponentDefinition, documentation: impl Into<String>) -> Self {
        let mut draft = Self::new(definition, documentation);
        draft.saved = None;
        draft
    }

    /// Applies an authoring operation and returns the normalized draft state.
    /// Invalid intermediate definitions are retained and reported in the snapshot.
    ///
    /// # Errors
    /// Returns an error when an operation references a missing pin or would create a
    /// duplicate stable pin identity or physical pin number.
    pub fn apply(
        &mut self,
        operation: ComponentDraftOperation,
    ) -> Result<ComponentDraftSnapshot, ComponentDraftError> {
        let mut next = self.value.clone();
        match operation {
            ComponentDraftOperation::SetMetadata {
                id,
                definition_version,
                name,
                description,
            } => {
                next.definition.id = normalized_string(&id);
                next.definition.definition_version = normalized_string(&definition_version);
                next.definition.name = normalized_string(&name);
                next.definition.description = normalized_string(&description);
            }
            ComponentDraftOperation::SetBody { body } => next.definition.body = body,
            ComponentDraftOperation::SetOrigin { x, y } => {
                next.definition.origin_x = x;
                next.definition.origin_y = y;
            }
            ComponentDraftOperation::SetRotations { rotations } => {
                let mut normalized = rotations;
                normalized.sort_unstable();
                normalized.dedup();
                next.definition.allowed_rotations = normalized;
            }
            ComponentDraftOperation::UpsertPin {
                previous_id,
                mut pin,
            } => {
                pin.id = normalized_string(&pin.id);
                pin.number = normalized_string(&pin.number);
                pin.name = normalized_string(&pin.name);
                pin.aliases = normalized_strings(pin.aliases);
                pin.functions = normalized_strings(pin.functions);
                let existing = if let Some(previous_id) = previous_id {
                    Some(
                        next.definition
                            .pins
                            .iter()
                            .position(|item| item.id == previous_id)
                            .ok_or(ComponentDraftError::PinNotFound(previous_id))?,
                    )
                } else {
                    next.definition
                        .pins
                        .iter()
                        .position(|item| item.id == pin.id)
                };
                if next
                    .definition
                    .pins
                    .iter()
                    .enumerate()
                    .any(|(index, item)| Some(index) != existing && item.id == pin.id)
                {
                    return Err(ComponentDraftError::DuplicatePinId(pin.id));
                }
                if next
                    .definition
                    .pins
                    .iter()
                    .enumerate()
                    .any(|(index, item)| Some(index) != existing && item.number == pin.number)
                {
                    return Err(ComponentDraftError::DuplicatePinNumber(pin.number));
                }
                if let Some(index) = existing {
                    next.definition.pins[index] = pin;
                } else {
                    next.definition.pins.push(pin);
                }
            }
            ComponentDraftOperation::RemovePin { id } => {
                let Some(index) = next.definition.pins.iter().position(|pin| pin.id == id) else {
                    return Err(ComponentDraftError::PinNotFound(id));
                };
                next.definition.pins.remove(index);
            }
            ComponentDraftOperation::ReorderPin { id, index } => {
                reorder_pin(&mut next.definition.pins, id, index)?;
            }
            ComponentDraftOperation::SetOccupiedHoles { mut holes } => {
                holes.sort_unstable();
                holes.dedup();
                next.definition.occupied_holes = holes;
            }
            ComponentDraftOperation::SetKeepouts { keepouts } => {
                next.definition.keepouts = keepouts;
            }
            ComponentDraftOperation::SetWireKeepouts { keepouts } => {
                next.definition.wire_keepouts = keepouts;
            }
            ComponentDraftOperation::SetAccessRegions { access } => next.definition.access = access,
            ComponentDraftOperation::SetDocumentation { documentation } => {
                next.documentation = documentation;
            }
        }
        if next == self.value {
            return Ok(self.snapshot());
        }
        self.undo.push(self.value.clone());
        self.value = next;
        self.redo.clear();
        Ok(self.snapshot())
    }

    #[must_use]
    pub fn undo(&mut self) -> ComponentDraftSnapshot {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(self.value.clone());
            self.value = previous;
        }
        self.snapshot()
    }

    #[must_use]
    pub fn redo(&mut self) -> ComponentDraftSnapshot {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.value.clone());
            self.value = next;
        }
        self.snapshot()
    }

    #[must_use]
    pub fn snapshot(&self) -> ComponentDraftSnapshot {
        let validation = self.value.definition.validate();
        ComponentDraftSnapshot {
            definition: self.value.definition.clone(),
            documentation: self.value.documentation.clone(),
            valid: validation.is_ok(),
            dirty: self.saved.as_ref() != Some(&self.value),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            diagnostics: validation
                .err()
                .map_or_else(Vec::new, |error| vec![error.to_string()]),
        }
    }

    /// Produces stable, reviewable TOML in authoritative struct field order.
    ///
    /// # Errors
    /// Returns an error if the authoritative component model cannot be serialized.
    pub fn to_toml(&self) -> Result<String, ComponentDraftError> {
        toml::to_string_pretty(&self.value.definition)
            .map_err(|error| ComponentDraftError::Serialize(error.to_string()))
    }

    #[must_use]
    pub fn documentation(&self) -> &str {
        &self.value.documentation
    }

    pub fn mark_saved(&mut self) {
        self.saved = Some(self.value.clone());
        self.undo.clear();
        self.redo.clear();
    }
}

fn reorder_pin(
    pins: &mut Vec<PinDefinition>,
    id: String,
    index: usize,
) -> Result<(), ComponentDraftError> {
    let Some(current_index) = pins.iter().position(|pin| pin.id == id) else {
        return Err(ComponentDraftError::PinNotFound(id));
    };
    let pin = pins.remove(current_index);
    let index = index.min(pins.len());
    pins.insert(index, pin);
    Ok(())
}

fn normalized_strings(values: Vec<String>) -> Vec<String> {
    let mut result: Vec<_> = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect();
    result.sort();
    result.dedup();
    result
}

fn normalized_string(value: &str) -> String {
    value.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> ComponentDefinition {
        ComponentDefinition::from_toml(
            r#"schema_version = 1
id = "dev.example.part"
definition_version = "1.0.0"
name = "Part"
allowed_rotations = ["deg0"]
pins = []

[body]
x = 0.0
y = 0.0
width = 2.54
height = 2.54
"#,
        )
        .unwrap()
    }

    #[test]
    fn operations_allow_invalid_intermediate_state_and_undo_redo() {
        let mut draft = ComponentDraft::new(definition(), "notes");
        let invalid = draft
            .apply(ComponentDraftOperation::SetBody {
                body: RectMm {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 2.54,
                },
            })
            .unwrap();
        assert!(!invalid.valid);
        assert!(invalid.dirty);
        assert!(draft.undo().valid);
        assert!(!draft.redo().valid);
    }

    #[test]
    fn pins_are_snapped_integer_positions_and_duplicate_numbers_are_rejected() {
        let mut draft = ComponentDraft::new(definition(), "");
        let pin = |id: &str, number: &str, x| PinDefinition {
            id: id.into(),
            number: number.into(),
            name: id.into(),
            x,
            y: 0,
            aliases: vec![],
            functions: vec![],
        };
        draft
            .apply(ComponentDraftOperation::UpsertPin {
                previous_id: None,
                pin: pin("a", "1", 3),
            })
            .unwrap();
        assert_eq!(draft.snapshot().definition.pins[0].x, 3);
        assert_eq!(
            draft.apply(ComponentDraftOperation::UpsertPin {
                previous_id: None,
                pin: pin("b", "1", 4)
            }),
            Err(ComponentDraftError::DuplicatePinNumber("1".into()))
        );
    }

    #[test]
    fn pins_can_be_reordered_for_serialization_and_undo() {
        let mut draft = ComponentDraft::new(definition(), "");
        for (id, number) in [("a", "1"), ("b", "2"), ("c", "3")] {
            draft
                .apply(ComponentDraftOperation::UpsertPin {
                    previous_id: None,
                    pin: PinDefinition {
                        id: id.into(),
                        number: number.into(),
                        name: id.into(),
                        x: 0,
                        y: 0,
                        aliases: vec![],
                        functions: vec![],
                    },
                })
                .unwrap();
        }

        draft
            .apply(ComponentDraftOperation::ReorderPin {
                id: "c".into(),
                index: 0,
            })
            .unwrap();
        assert_eq!(
            draft
                .snapshot()
                .definition
                .pins
                .iter()
                .map(|pin| pin.id.as_str())
                .collect::<Vec<_>>(),
            ["c", "a", "b"]
        );
        let serialized = draft.to_toml().unwrap();
        assert!(serialized.find("id = \"c\"").unwrap() < serialized.find("id = \"a\"").unwrap());

        assert_eq!(
            draft
                .undo()
                .definition
                .pins
                .iter()
                .map(|pin| pin.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn serialization_is_deterministic_and_round_trips() {
        let draft = ComponentDraft::new(definition(), "");
        let first = draft.to_toml().unwrap();
        assert_eq!(first, draft.to_toml().unwrap());
        assert_eq!(
            ComponentDefinition::from_toml(&first).unwrap(),
            definition()
        );
    }

    #[test]
    fn a_new_unsaved_draft_starts_dirty() {
        assert!(
            ComponentDraft::new_unsaved(definition(), "")
                .snapshot()
                .dirty
        );
    }
}
