use std::time::Duration;

/// Outcome of a structured [`super::Command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl CommandResult {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
            duration: Duration::ZERO,
        }
    }

    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}
