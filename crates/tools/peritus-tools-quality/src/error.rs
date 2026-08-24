//! Stable quality-tool errors.

use core::fmt;

/// Stable quality adapter failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QualityErrorKind {
    /// A definition, input, or bound was invalid.
    InvalidInput,
    /// Descriptor or envelope construction failed.
    Protocol,
    /// Immutable workspace inspection failed.
    Workspace,
    /// C2 rejected a command or execution plan.
    Process,
    /// Output could not be parsed completely.
    Parser,
    /// The selected check is absent from the bound catalog.
    UnknownCheck,
    /// Invocation and plan identities disagreed.
    InvocationMismatch,
    /// One-use execution state was already consumed.
    AlreadyConsumed,
}

/// Typed quality-tool error with bounded diagnostic context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityError {
    kind: QualityErrorKind,
    detail: String,
}

impl QualityError {
    pub(crate) fn new(kind: QualityErrorKind, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        truncate_utf8(&mut detail, 4_096);
        Self { kind, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> QualityErrorKind {
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

impl fmt::Display for QualityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for QualityError {}

impl From<peritus_process::ProcessError> for QualityError {
    fn from(error: peritus_process::ProcessError) -> Self {
        Self::new(QualityErrorKind::Process, error.to_string())
    }
}

impl From<peritus_tool_protocol::ProtocolError> for QualityError {
    fn from(error: peritus_tool_protocol::ProtocolError) -> Self {
        Self::new(QualityErrorKind::Protocol, error.to_string())
    }
}

impl From<peritus_workspace::WorkspaceError> for QualityError {
    fn from(error: peritus_workspace::WorkspaceError) -> Self {
        Self::new(QualityErrorKind::Workspace, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_cap_preserves_utf8_boundaries() {
        let error = QualityError::new(QualityErrorKind::Parser, format!("{}é", "a".repeat(4095)));
        assert_eq!(error.detail().len(), 4095);
        assert!(error.detail().is_char_boundary(error.detail().len()));
    }
}
