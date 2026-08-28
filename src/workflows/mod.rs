//! Concrete Binder workflows. Not a generic engine.

pub mod engineering;
pub mod hygiene;
pub mod issue;

pub use engineering::{EngineeringReport, EngineeringRequest, EngineeringWorkflow};

pub use hygiene::{
    dispatch, AutoRemediation, DeterministicAction, DispatchDecision, Finding, FindingKind,
    FindingOutcome, HygieneConfig, HygieneEngine, HygieneReport, HygieneWorkflow,
    HygieneWorkflowReport, HygieneWorkflowRequest, OperationalContext, ProjectKind,
    RemediationType, Severity,
};
pub use issue::{
    FileEdit, ImplementationPlan, Issue, IssueImplementationRequest, IssueImplementationResult,
    IssueImplementationWorkflow, WorkflowError,
};
