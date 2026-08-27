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
mod planners;
mod policy;
mod prompt;
mod providers;
mod request;
mod response;
mod router;
mod routing;
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
pub use planners::Plan;
pub use policy::RoutingPolicy;
pub use prompt::manager::PromptManager;
pub use prompt::{PromptRef, PromptRegistry, PromptVersion};
pub use providers::{DefaultLlmFactory, LlmFactory, StaticLlmFactory};
pub use request::AiRequest;
pub use response::{AiResponse, ExecutionTrace};
pub use router::Router;
pub use routing::{FallbackChain, ModelCatalog, ModelTarget};
pub use session::{SessionId, UserId, WorkflowId};
pub use task::{Task, TaskKind};
pub use telemetry::{estimate_cost, AiMetrics, CostEstimate};
pub use tools::{MockToolExecutor, ToolExecutor, ToolKind, ToolRegistry, UnboundToolExecutor};
