//! Bounded human-readable application diagnostics.

use core::fmt;

/// Bounded UTF-8 diagnostic prose that carries no machine semantics.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AppDiagnostic(String);

impl AppDiagnostic {
    /// Creates diagnostic prose under an explicit byte ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticError::Empty`] for empty prose and [`DiagnosticError::TooLong`] when
    /// the UTF-8 byte length exceeds `max_bytes`.
    pub fn new(value: String, max_bytes: usize) -> Result<Self, DiagnosticError> {
        if value.is_empty() {
            Err(DiagnosticError::Empty)
        } else if value.len() > max_bytes {
            Err(DiagnosticError::TooLong)
        } else {
            Ok(Self(value))
        }
    }

    /// Borrows the exact diagnostic prose.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Consumes the diagnostic and returns its prose.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

/// Failure to construct bounded diagnostic prose.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticError {
    /// Empty prose is represented by absence instead.
    Empty,
    /// The UTF-8 byte length exceeded the configured ceiling.
    TooLong,
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "diagnostic text must not be empty",
            Self::TooLong => "diagnostic text exceeds the negotiated limit",
        })
    }
}

impl std::error::Error for DiagnosticError {}
