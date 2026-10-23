//! Deterministic, UI-independent domain contracts for Perfboard Designer.

pub mod board;
pub mod circuit;
pub mod component;
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
pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity, ObjectReference};
pub use geometry::{BoardSize, GeometryError, HolePos, Rotation, RoutePos, Side};
pub use id::{
    CommitId, ComponentDefinitionId, ComponentInstanceId, ConstraintId, JunctionId, NetId,
    OperationId, PinId, ProjectId, WireId,
};
pub use layout::ComponentInstance;
pub use routing::{RouteMeasurements, Wire, WireColor, measure_route};
pub use validation::{PlacementPolicy, validate_design_placements};
