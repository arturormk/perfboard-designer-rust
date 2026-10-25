use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ComponentInstanceId, ConstraintId, HolePos, NetId, Rotation, Side, WireId};

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSeverity {
    Required,
    Preferred,
    Advisory,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOrigin {
    User,
    Mcp,
    Import,
    SystemDerived,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GridRect {
    pub min: HolePos,
    pub max: HolePos,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstraintKind {
    FixedPlacement {
        component_id: ComponentInstanceId,
        position: HolePos,
    },
    Orientation {
        component_id: ComponentInstanceId,
        rotation: Rotation,
    },
    Proximity {
        first_component_id: ComponentInstanceId,
        second_component_id: ComponentInstanceId,
        max_distance_holes: f64,
    },
    PreferredMaximumLength {
        net_id: NetId,
        max_length_mm: f64,
    },
    BendSpacing {
        min_half_grid_steps: u32,
    },
    Access {
        component_id: ComponentInstanceId,
    },
    EdgePlacement {
        component_id: ComponentInstanceId,
        max_edge_distance_holes: u32,
    },
    ReservedArea {
        area: GridRect,
        #[serde(default)]
        label: String,
    },
    WireSide {
        net_id: NetId,
        side: Side,
    },
    LockedRoute {
        wire_id: WireId,
    },
    AssemblyAccess {
        component_id: ComponentInstanceId,
        area: GridRect,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Constraint {
    pub id: ConstraintId,
    pub severity: ConstraintSeverity,
    pub origin: ConstraintOrigin,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rationale: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub annotation: String,
    #[serde(flatten)]
    pub kind: ConstraintKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JumperInventoryEntry {
    pub id: String,
    /// Number of board intervals bridged, not number of occupied holes.
    pub span_holes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u32>,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_diameter_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_height_mm: Option<f64>,
    #[serde(default)]
    pub tolerance_holes: u32,
}
