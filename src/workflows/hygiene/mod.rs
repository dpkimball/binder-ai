//! Hygiene orchestrates inspection findings. binder-ai remediates source.

mod config;
mod deterministic;
mod dispatcher;
mod engine;
mod finding;
mod remediation;
mod result;

pub use config::HygieneConfig;
pub use dispatcher::dispatch;
pub use engine::HygieneEngine;
pub use finding::{Finding, FindingKind, ProjectKind, Severity};
pub use remediation::{AutoRemediation, DeterministicAction, DispatchDecision, RemediationType};
pub use result::{FindingOutcome, HygieneReport};
