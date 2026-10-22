//! Deterministic, UI-independent domain contracts for Perfboard Designer.

pub mod board;
pub mod component;
pub mod diagnostic;
pub mod geometry;
pub mod id;
pub mod layout;
pub mod validation;

pub use board::{BoardFeatures, ForbiddenArea, MarginsMm, MountingHole};
pub use component::{
    AccessDirection, AccessRegion, ComponentDefinition, ComponentDefinitionError, Keepout,
    LocalPos, PinDefinition, RectMm, parse_component_library,
};
pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity, ObjectReference};
pub use geometry::{BoardSize, GeometryError, HolePos, Rotation, RoutePos, Side};
pub use id::{
    CommitId, ComponentDefinitionId, ComponentInstanceId, ConstraintId, NetId, OperationId, PinId,
    ProjectId, WireId,
};
pub use layout::ComponentInstance;
pub use validation::{PlacementPolicy, validate_design_placements};
