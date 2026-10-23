use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{ComponentInstanceId, HolePos, JunctionId, NetId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Proposed,
    Accepted,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PinRef {
    pub component_id: ComponentInstanceId,
    pub pin_id: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalRef {
    Pin {
        component_id: ComponentInstanceId,
        pin_id: String,
    },
    Junction {
        junction_id: JunctionId,
    },
}

impl From<PinRef> for TerminalRef {
    fn from(value: PinRef) -> Self {
        Self::Pin {
            component_id: value.component_id,
            pin_id: value.pin_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElectricalNet {
    pub id: NetId,
    pub name: String,
    pub state: ConnectionState,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pins: BTreeSet<PinRef>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Junction {
    pub id: JunctionId,
    pub net_id: NetId,
    pub position: HolePos,
    /// When true, wires on opposite faces may join electrically at this hole.
    #[serde(default)]
    pub through_hole: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}
