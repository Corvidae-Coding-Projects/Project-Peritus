//! Stable shell-tool errors.

use core::fmt;

/// Stable shell adapter failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShellErrorKind {
    /// Structured tool input was invalid.
    InvalidInput,
    /// Descriptor or envelope construction failed.
    Protocol,
    /// C2 rejected a command or execution plan.
    Process,
    /// The authorized invocation and bound plan disagreed.
    InvocationMismatch,
    /// A one-use bound execution resource was already consumed.
    AlreadyConsumed,
}

/// Typed shell-tool error with bounded diagnostic context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellError {
    kind: ShellErrorKind,
    detail: String,
}

impl ShellError {
    pub(crate) fn new(kind: ShellErrorKind, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        truncate_utf8(&mut detail, 4_096);
        Self { kind, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ShellErrorKind {
        self.kind
    }

    /// Returns bounded diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

pub fn truncate_utf8(value: &mut String, maximum: usize) {
    if value.len() <= maximum {
        return;
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

impl fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for ShellError {}

impl From<peritus_process::ProcessError> for ShellError {
    fn from(error: peritus_process::ProcessError) -> Self {
        Self::new(ShellErrorKind::Process, error.to_string())
    }
}

impl From<peritus_tool_protocol::ProtocolError> for ShellError {
    fn from(error: peritus_tool_protocol::ProtocolError) -> Self {
        Self::new(ShellErrorKind::Protocol, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_cap_preserves_utf8_boundaries() {
        let error = ShellError::new(ShellErrorKind::InvalidInput, format!("{}é", "a".repeat(4095)));
        assert_eq!(error.detail().len(), 4095);
        assert!(error.detail().is_char_boundary(error.detail().len()));
    }
}
