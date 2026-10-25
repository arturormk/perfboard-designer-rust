use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::HolePos;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MarginsMm {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub right: f64,
    #[serde(default)]
    pub bottom: f64,
    #[serde(default)]
    pub left: f64,
}

impl MarginsMm {
    #[must_use]
    pub fn is_valid(self) -> bool {
        [self.top, self.right, self.bottom, self.left]
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MountingHole {
    pub position: HolePos,
    pub diameter_mm: f64,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ForbiddenArea {
    pub min: HolePos,
    pub max: HolePos,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoardFeatures {
    #[serde(default)]
    pub margins_mm: MarginsMm,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounting_holes: Vec<MountingHole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbidden_areas: Vec<ForbiddenArea>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl BoardFeatures {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.margins_mm == MarginsMm::default()
            && self.mounting_holes.is_empty()
            && self.forbidden_areas.is_empty()
            && self.notes.is_empty()
    }
}
