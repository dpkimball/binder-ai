use std::path::Path;

use crate::runtime::{Command, CommandResult, Runtime};
use crate::workflows::issue::result::WorkflowError;

pub async fn cargo_fmt<R: Runtime>(
    runtime: &R,
    cwd: &Path,
) -> Result<CommandResult, WorkflowError> {
    runtime
        .execute(Command::new("cargo").arg("fmt").cwd(cwd))
        .await
        .map_err(|err| WorkflowError::from_runtime_fmt(&err))
}

pub async fn cargo_test<R: Runtime>(
    runtime: &R,
    cwd: &Path,
) -> Result<CommandResult, WorkflowError> {
    runtime
        .execute(Command::new("cargo").arg("test").cwd(cwd))
        .await
        .map_err(|err| WorkflowError::from_runtime_test(&err))
}
