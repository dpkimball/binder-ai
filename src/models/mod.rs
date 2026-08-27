//! Data-driven model catalog. Routing uses capabilities, not vendor names.

mod availability;
mod capability;
mod catalog;
mod config;
mod descriptor;
mod pricing;
mod quality;
mod requirements;

pub use availability::Availability;
pub use capability::{ModelCapabilities, ModelCapability};
pub use catalog::ModelCatalog;
pub use config::{CatalogFile, PrefixProfile, TaskDefaultConfig};
pub use descriptor::{ExecutionMode, ModelDescriptor, ModelId};
pub use pricing::{CostClass, Pricing};
pub use quality::QualityProfile;
pub use requirements::ModelRequirements;
