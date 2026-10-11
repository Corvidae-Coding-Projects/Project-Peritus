//! Checked structured executable and argument values.

use crate::{ProcessError, error::invalid};

/// One checked direct-execution command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    executable: String,
    arguments: Vec<String>,
}

impl CommandSpec {
    /// Creates a structured command without parsing or invoking a shell.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty executable or any NUL byte. Native argv limits are checked
    /// by the operating system at process creation.
    pub fn new<I, S>(executable: impl Into<String>, arguments: I) -> Result<Self, ProcessError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let executable = executable.into();
        if executable.is_empty() || executable.as_bytes().contains(&0) {
            return Err(invalid("executable is empty or contains NUL"));
        }
        let mut checked = Vec::new();
        for argument in arguments {
            let argument = argument.into();
            if argument.as_bytes().contains(&0) {
                return Err(invalid("argument contains NUL"));
            }
            checked.push(argument);
        }
        Ok(Self { executable, arguments: checked })
    }

    /// Returns the literal executable text.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Returns literal arguments in execution order.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Returns the complete UTF-8 argv byte count, saturating on overflow.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.arguments.iter().fold(self.executable.len(), |total, value| {
            total.saturating_add(value.len()).saturating_add(1)
        })
    }
}
