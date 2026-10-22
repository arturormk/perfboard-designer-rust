//! Deterministic, UI-independent domain contracts for Perfboard Designer.

pub mod diagnostic;
pub mod geometry;
pub mod id;

pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity, ObjectReference};
pub use geometry::{BoardSize, GeometryError, HolePos, Rotation, RoutePos, Side};
pub use id::{
    CommitId, ComponentDefinitionId, ComponentInstanceId, ConstraintId, NetId, OperationId, PinId,
    ProjectId, WireId,
};
