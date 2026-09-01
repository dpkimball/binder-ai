//! Binder AI orchestration.
//!
//! Applications construct [`AiClient`]. They do not call `binder-llm` or
//! providers directly.

mod builder;
mod cache;
mod client;
mod config;
mod context;
mod conversation;
mod error;
mod intel;
mod knowledge;
mod mock;
mod models;
mod planners;
mod policy;
mod prompt;
mod providers;
mod request;
mod response;
mod router;
mod routing;
mod runtime;
mod session;
mod task;
mod telemetry;
mod tools;
mod workflows;
mod workspace;

pub use builder::AiClientBuilder;
pub use cache::{AiCache, MemoryCache};
pub use client::AiClient;
pub use config::{AiConfig, BudgetLedger, ProviderSettings, TokenBudget};
pub use context::{ContextBuilder, ContextSource, IssueContext};
pub use conversation::{
    Conversation, ConversationId, ConversationStore, InMemoryConversationStore,
};
pub use error::{AiError, Result};
pub use intel::{
    assess_ask_context, build_ask_prompt, build_explain_prompt, classify_planning, correction_id,
    filter_guidance_for_repo, filter_linked_docs_for_repo, filter_paths_for_repo, lang_hint, plan,
    resolve_correction_id, safe_truncate, AgentPlan, AskRejectReason, FallbackReason, IntelError,
    PlanRequest, PlanResponse, PlanningDecision, PlanningMetrics, PlanningResult, UnresolvedTarget,
};
pub use knowledge::{
    decode_record_ref, encode_record_ref, engineering_question_uri, parse_scoped_ask, CodeintelCli,
    CodeintelKnowledge, DoctrineSnapshot, DomainError, ExecutionRecord, GuidanceSummary, Knowledge,
    KnowledgeDomain, KnowledgeError, KnowledgePlan, KnowledgeScope, LinkedDocSummary,
    LinkedIssueSummary, LinkedPrSummary, MemoryKnowledge, ProcessCodeintelCli, RecentChange,
    RecordRefError, ReindexStats, ScopedAskBody,
};
pub use mock::{MockAiClient, MockRouter, ScriptedProvider};
pub use models::{
    Availability, CatalogFile, CostClass, ExecutionMode, ModelCapabilities, ModelCapability,
    ModelCatalog, ModelDescriptor, ModelId, ModelRequirements, PrefixProfile, Pricing,
    QualityProfile, TaskDefaultConfig,
};
pub use planners::Plan;
pub use policy::{ExecutionPolicy, RoutingPolicy};
pub use prompt::manager::PromptManager;
pub use prompt::{PromptRef, PromptRegistry, PromptVersion};
pub use providers::{DefaultLlmFactory, LlmFactory, StaticLlmFactory};
pub use request::{AiRequest, AiRequestBuilder};
pub use response::{AiResponse, AiUsage, ExecutionTrace};
pub use router::Router;
pub use routing::{CatalogSelector, FallbackChain, ModelSelection, ModelSelector, ModelTarget};
pub use runtime::{
    Command, CommandResult, GitDiff, GitFileStatus, GitStatus, GitStatusEntry, LocalRuntime,
    MemoryRuntime, Runtime, RuntimeError, RuntimeResult, SearchKind, SearchRequest, SearchResult,
};
pub use session::{SessionId, UserId, WorkflowId};
pub use task::{Task, TaskKind};
pub use telemetry::{estimate_cost, estimate_cost_for, AiMetrics, CostEstimate};
pub use tools::{MockToolExecutor, ToolExecutor, ToolKind, ToolRegistry, UnboundToolExecutor};
pub use workflows::{
    dispatch, AutoRemediation, DeterministicAction, DispatchDecision, DocumentationGap,
    DocumentationReviewReport, DocumentationReviewRequest, DocumentationReviewWorkflow,
    EngineeringReport, EngineeringRequest, EngineeringWorkflow, FileEdit, Finding, FindingKind,
    FindingOutcome, HygieneConfig, HygieneEngine, HygieneReport, HygieneWorkflow,
    HygieneWorkflowReport, HygieneWorkflowRequest, ImplementationPlan, Issue,
    IssueImplementationRequest, IssueImplementationResult, IssueImplementationWorkflow,
    OperationalContext, ProjectKind, RemediationType, Severity, SkillApplyDocument,
    SkillApplyReport, SkillFindingJson, SkillOperationalJson, SkillOutcomeJson, WorkflowError,
};
pub use workspace::{
    resolve_inspection_set, resolve_inspection_set_best_effort, InspectionAttempt,
    InspectionResolution, ResolvedTarget, WorkspaceLayout, WorkspaceResolveError,
};

/// Spec alias: callers describe a task, not a vendor.
pub type AiTask = TaskKind;
/// Spec alias for [`ExecutionPolicy`].
pub type AiPolicy = ExecutionPolicy;
