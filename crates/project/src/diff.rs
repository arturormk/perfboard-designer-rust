use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::Design;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
    Moved,
    Rotated,
    RouteChanged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffEntry {
    pub object_type: String,
    pub object_id: String,
    pub kind: ChangeKind,
    pub summary: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub components: Vec<DiffEntry>,
    pub nets: Vec<DiffEntry>,
    pub wires: Vec<DiffEntry>,
    pub rails: Vec<DiffEntry>,
    #[serde(default)]
    pub saddles: Vec<DiffEntry>,
    pub constraints: Vec<DiffEntry>,
    pub routes: Vec<DiffEntry>,
}

impl SemanticDiff {
    #[must_use]
    #[allow(clippy::too_many_lines)] // Explicit object categories keep the serialized diff readable.
    pub fn between(before: &Design, after: &Design) -> Self {
        let mut result = Self::default();
        let component_ids: BTreeSet<_> = before
            .components
            .keys()
            .chain(after.components.keys())
            .copied()
            .collect();
        for id in component_ids {
            let old = before.components.get(&id);
            let new = after.components.get(&id);
            let entry = match (old, new) {
                (None, Some(value)) => Some((ChangeKind::Added, format!("Added {}", value.name))),
                (Some(value), None) => {
                    Some((ChangeKind::Removed, format!("Removed {}", value.name)))
                }
                (Some(old), Some(new)) if old.position != new.position => Some((
                    ChangeKind::Moved,
                    format!(
                        "Moved {} from {},{} to {},{}",
                        new.name, old.position.x, old.position.y, new.position.x, new.position.y
                    ),
                )),
                (Some(old), Some(new)) if old.rotation != new.rotation => Some((
                    ChangeKind::Rotated,
                    format!(
                        "Rotated {} from {:?} to {:?}",
                        new.name, old.rotation, new.rotation
                    ),
                )),
                (Some(old), Some(new)) if old != new => {
                    Some((ChangeKind::Modified, format!("Modified {}", new.name)))
                }
                _ => None,
            };
            if let Some((kind, summary)) = entry {
                result.components.push(DiffEntry {
                    object_type: "component".into(),
                    object_id: id.to_string(),
                    kind,
                    summary,
                });
            }
        }
        let net_ids: BTreeSet<_> = before
            .nets
            .keys()
            .chain(after.nets.keys())
            .copied()
            .collect();
        for id in net_ids {
            let old = before.nets.get(&id);
            let new = after.nets.get(&id);
            let entry = match (old, new) {
                (None, Some(value)) => {
                    Some((ChangeKind::Added, format!("Added net {}", value.name)))
                }
                (Some(value), None) => {
                    Some((ChangeKind::Removed, format!("Removed net {}", value.name)))
                }
                (Some(_), Some(value)) if old != new => {
                    Some((ChangeKind::Modified, format!("Modified net {}", value.name)))
                }
                _ => None,
            };
            if let Some((kind, summary)) = entry {
                result.nets.push(DiffEntry {
                    object_type: "net".into(),
                    object_id: id.to_string(),
                    kind,
                    summary,
                });
            }
        }
        let wire_ids: BTreeSet<_> = before
            .wires
            .keys()
            .chain(after.wires.keys())
            .copied()
            .collect();
        for id in wire_ids {
            let old = before.wires.get(&id);
            let new = after.wires.get(&id);
            let entry = match (old, new) {
                (None, Some(_)) => Some((ChangeKind::Added, format!("Added wire {id}"))),
                (Some(_), None) => Some((ChangeKind::Removed, format!("Removed wire {id}"))),
                (Some(old), Some(new)) if old.vertices != new.vertices || old.side != new.side => {
                    let route = DiffEntry {
                        object_type: "route".into(),
                        object_id: id.to_string(),
                        kind: ChangeKind::RouteChanged,
                        summary: format!("Changed route for wire {id}"),
                    };
                    result.routes.push(route.clone());
                    Some((route.kind, route.summary))
                }
                (Some(_), Some(_)) if old != new => {
                    Some((ChangeKind::Modified, format!("Modified wire {id}")))
                }
                _ => None,
            };
            if let Some((kind, summary)) = entry {
                result.wires.push(DiffEntry {
                    object_type: "wire".into(),
                    object_id: id.to_string(),
                    kind,
                    summary,
                });
            }
        }
        let rail_ids: BTreeSet<_> = before
            .rails
            .keys()
            .chain(after.rails.keys())
            .copied()
            .collect();
        for id in rail_ids {
            let old = before.rails.get(&id);
            let new = after.rails.get(&id);
            let entry = match (old, new) {
                (None, Some(_)) => Some((ChangeKind::Added, format!("Added rail {id}"))),
                (Some(_), None) => Some((ChangeKind::Removed, format!("Removed rail {id}"))),
                (Some(old), Some(new)) if old.vertices != new.vertices || old.side != new.side => {
                    Some((
                        ChangeKind::RouteChanged,
                        format!("Changed route for rail {id}"),
                    ))
                }
                (Some(_), Some(_)) if old != new => {
                    Some((ChangeKind::Modified, format!("Modified rail {id}")))
                }
                _ => None,
            };
            if let Some((kind, summary)) = entry {
                result.rails.push(DiffEntry {
                    object_type: "rail".into(),
                    object_id: id.to_string(),
                    kind,
                    summary,
                });
            }
        }
        let saddle_ids: BTreeSet<_> = before
            .saddles
            .keys()
            .chain(after.saddles.keys())
            .copied()
            .collect();
        for id in saddle_ids {
            let entry = match (before.saddles.get(&id), after.saddles.get(&id)) {
                (None, Some(_)) => Some((ChangeKind::Added, format!("Added saddle {id}"))),
                (Some(_), None) => Some((ChangeKind::Removed, format!("Removed saddle {id}"))),
                (Some(old), Some(new))
                    if old.start != new.start || old.end != new.end || old.side != new.side =>
                {
                    Some((ChangeKind::Moved, format!("Moved saddle {id}")))
                }
                (Some(old), Some(new)) if old != new => {
                    Some((ChangeKind::Modified, format!("Modified saddle {id}")))
                }
                _ => None,
            };
            if let Some((kind, summary)) = entry {
                result.saddles.push(DiffEntry {
                    object_type: "saddle".into(),
                    object_id: id.to_string(),
                    kind,
                    summary,
                });
            }
        }
        let constraint_ids: BTreeSet<_> = before
            .constraints
            .keys()
            .chain(after.constraints.keys())
            .copied()
            .collect();
        for id in constraint_ids {
            let old = before.constraints.get(&id);
            let new = after.constraints.get(&id);
            let entry = match (old, new) {
                (None, Some(_)) => Some((ChangeKind::Added, format!("Added constraint {id}"))),
                (Some(_), None) => Some((ChangeKind::Removed, format!("Removed constraint {id}"))),
                (Some(_), Some(_)) if old != new => {
                    Some((ChangeKind::Modified, format!("Modified constraint {id}")))
                }
                _ => None,
            };
            if let Some((kind, summary)) = entry {
                result.constraints.push(DiffEntry {
                    object_type: "constraint".into(),
                    object_id: id.to_string(),
                    kind,
                    summary,
                });
            }
        }
        result
    }
}
