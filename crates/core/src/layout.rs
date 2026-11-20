use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ComponentInstanceId, HolePos, PinDefinition, Rotation};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentInstance {
    pub id: ComponentInstanceId,
    pub definition_id: String,
    /// The part's name on the board, such as `R3`. Leave it empty when
    /// adding a part to get the next free name from the definition's
    /// designator.
    pub name: String,
    pub position: HolePos,
    pub rotation: Rotation,
    /// The part's value as printed or coded on it: `10 kΩ`, `100 nF`,
    /// `1N4148`. Resistances and capacitances are stored normalised (see
    /// [`crate::normalize_value`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Whether a shunt is fitted on a jumper. Only meaningful when the
    /// definition's kind is `jumper`; a shunted jumper joins its two pins.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shunted: bool,
    /// Whether a bridge is made on the component side, by laying a part's
    /// lead across the holes, rather than on the solder side. Only
    /// meaningful when the definition's kind is `bridge`. Such a bridge is
    /// still a conductor and takes up the holes it passes through; the
    /// assembly plan folds it into the step of the part whose lead it is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub component_side: bool,
    /// This part's own names for some of its pins, keyed by pin id, such as
    /// `TRIG` for pin 2 of one 555 timer. They replace the definition's pin
    /// names for this part only; pins left out keep the definition's name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pin_labels: BTreeMap<String, String>,
}

impl ComponentInstance {
    /// The name this part gives a pin: its own label when it has one,
    /// otherwise the definition's pin name.
    #[must_use]
    pub fn pin_name<'a>(&'a self, pin: &'a PinDefinition) -> &'a str {
        self.pin_labels
            .get(&pin.id)
            .map_or(&pin.name, String::as_str)
    }
}
