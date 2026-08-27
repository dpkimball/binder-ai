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

pub use builder::AiClientBuilder;
pub use cache::{AiCache, MemoryCache};
pub use client::AiClient;
pub use config::{AiConfig, BudgetLedger, ProviderSettings, TokenBudget};
pub use context::{ContextBuilder, ContextSource, IssueContext};
pub use conversation::{
    Conversation, ConversationId, ConversationStore, InMemoryConversationStore,
};
pub use error::{AiError, Result};
pub use mock::{MockRouter, ScriptedProvider};
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

/// Spec alias: callers describe a task, not a vendor.
pub type AiTask = TaskKind;
/// Spec alias for [`ExecutionPolicy`].
pub type AiPolicy = ExecutionPolicy;
