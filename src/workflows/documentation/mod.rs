//! Documentation review. Inspect README/docs; do not edit; do not call an LLM.

mod request;
mod result;
mod workflow;

pub use request::DocumentationReviewRequest;
pub use result::{DocumentationGap, DocumentationReviewReport};
pub use workflow::DocumentationReviewWorkflow;
