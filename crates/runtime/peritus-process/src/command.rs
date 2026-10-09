//! Checked structured executable and argument values.

use crate::{ProcessError, error::invalid};

const MAX_EXECUTABLE_BYTES: usize = 4_096;
const MAX_ARGUMENT_BYTES: usize = 64 * 1_024;
const MAX_ARGUMENT_COUNT: usize = 4_096;
const MAX_ARGV_BYTES: usize = 2 * 1_024 * 1_024;

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
    /// Returns an error for an empty or over-limit executable, NUL bytes, excessive argument
    /// count, an over-limit argument, or an over-limit complete argv.
    pub fn new<I, S>(executable: impl Into<String>, arguments: I) -> Result<Self, ProcessError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let executable = executable.into();
        if executable.is_empty()
            || executable.len() > MAX_EXECUTABLE_BYTES
            || executable.as_bytes().contains(&0)
        {
            return Err(invalid("executable is empty, contains NUL, or exceeds its bound"));
        }
        let mut total = executable.len();
        let mut checked = Vec::new();
        for argument in arguments {
            if checked.len() == MAX_ARGUMENT_COUNT {
                return Err(invalid("argument count exceeds its bound"));
            }
            let argument = argument.into();
            if argument.len() > MAX_ARGUMENT_BYTES || argument.as_bytes().contains(&0) {
                return Err(invalid("argument contains NUL or exceeds its bound"));
            }
            total = total
                .checked_add(argument.len())
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| invalid("argv byte accounting overflowed"))?;
            if total > MAX_ARGV_BYTES {
                return Err(invalid("complete argv exceeds its bound"));
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

    /// Returns the complete argv byte count used by bounds checks.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.executable.len() + self.arguments.iter().map(|value| value.len() + 1).sum::<usize>()
    }
}
