//! Deterministic, UI-independent domain contracts for Perfboard Designer.

pub mod board;
pub mod circuit;
pub mod component;
pub mod component_draft;
pub mod constraints;
pub mod diagnostic;
pub mod geometry;
pub mod id;
pub mod layout;
pub mod routing;
pub mod validation;

pub use board::{BoardFeatures, ForbiddenArea, MarginsMm, MountingHole};
pub use circuit::{ConnectionState, ElectricalNet, Junction, PinRef, TerminalRef};
pub use component::{
    AccessDirection, AccessRegion, ComponentDefinition, ComponentDefinitionError, Keepout,
    LocalPos, PinDefinition, RectMm, parse_component_library,
};
pub use component_draft::{
    ComponentDraft, ComponentDraftError, ComponentDraftOperation, ComponentDraftSnapshot,
};
pub use constraints::{
    Constraint, ConstraintKind, ConstraintOrigin, ConstraintSeverity, GridRect,
    JumperInventoryEntry,
};
pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity, ObjectReference};
pub use geometry::{BoardSize, GeometryError, HolePos, Rotation, RoutePos, Side};
pub use id::{
    CommitId, ComponentDefinitionId, ComponentInstanceId, ConstraintId, JunctionId, NetId,
    OperationId, PinId, ProjectId, WireId,
};
pub use layout::ComponentInstance;
pub use routing::{
    CrossingChoice, JumperMatch, JumperMatchKind, RouteCostBreakdown, RouteCrossing,
    RouteMeasurements, RouteProposal, Wire, WireColor, measure_route,
};
pub use validation::{PlacementPolicy, validate_design_placements};
