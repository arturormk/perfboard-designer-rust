use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ComponentInstanceId, HolePos, Rotation};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentInstance {
    pub id: ComponentInstanceId,
    pub definition_id: String,
    pub name: String,
    pub position: HolePos,
    pub rotation: Rotation,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Whether a shunt is fitted on a jumper. Only meaningful when the
    /// definition's kind is `jumper`; a shunted jumper joins its two pins.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shunted: bool,
}
