use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Structured process invocation. Never a shell string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    executable: String,
    arguments: Vec<String>,
    working_directory: Option<PathBuf>,
    environment: HashMap<String, String>,
}

impl Command {
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            working_directory: None,
            environment: HashMap::new(),
        }
    }

    pub fn arg(mut self, argument: impl Into<String>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    pub fn args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl AsRef<Path>) -> Self {
        self.working_directory = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    pub fn executable(&self) -> &str {
        &self.executable
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub fn working_directory(&self) -> Option<&Path> {
        self.working_directory.as_deref()
    }

    pub fn environment(&self) -> &HashMap<String, String> {
        &self.environment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_is_argv_not_a_shell_string() {
        let command = Command::new("cargo").args(["test", "--all-features"]);
        assert_eq!(command.executable(), "cargo");
        assert_eq!(command.arguments(), &["test", "--all-features"]);
        assert!(!command.executable().contains(' '));
    }
}
