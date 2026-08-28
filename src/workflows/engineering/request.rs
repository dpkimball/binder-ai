use std::path::PathBuf;

/// Engineering request. Inspection set comes from bindb, not from this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineeringRequest {
    pub request: String,
    pub repository_root: PathBuf,
}
