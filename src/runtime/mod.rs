//! Development-environment operations. Independent of LLM providers.
//!
//! AI reasons; [`Runtime`] performs file, search, command, and git work.
//! Workflows depend on the trait, not `std::fs` / `Command` / `git2`.

mod command;
mod command_result;
mod errors;
mod local_runtime;
mod memory_runtime;
#[allow(clippy::module_inception)]
mod runtime;
mod search;
mod search_result;

pub use command::Command;
pub use command_result::CommandResult;
pub use errors::{Result as RuntimeResult, RuntimeError};
pub use local_runtime::LocalRuntime;
pub use memory_runtime::MemoryRuntime;
pub use runtime::{GitDiff, GitFileStatus, GitStatus, GitStatusEntry, Runtime};
pub use search::{SearchKind, SearchRequest};
pub use search_result::SearchResult;
