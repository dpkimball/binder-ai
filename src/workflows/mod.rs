//! Concrete Binder workflows. Not a generic engine.
//!
//! Hygiene and documentation-review workflows were extracted to
//! `binder-dashboard-ops` (see `#73` /
//! `docs/specs/scope-boundary-review.md`) — they were repo-dashboard-specific
//! automation, not general AI orchestration. That crate depends on this one.

pub mod engineering;
pub mod issue;

pub use engineering::{EngineeringReport, EngineeringRequest, EngineeringWorkflow};

pub use issue::{
    FileEdit, ImplementationPlan, Issue, IssueImplementationRequest, IssueImplementationResult,
    IssueImplementationWorkflow, WorkflowError,
};
