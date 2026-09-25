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

    /// Constrains this diagnostic to a peer's negotiated UTF-8 byte ceiling.
    ///
    /// A ceiling too small to retain even one complete character removes the optional diagnostic;
    /// machine-readable error fields remain available to the peer.
    #[must_use]
    pub fn constrained(mut self, max_bytes: usize) -> Option<Self> {
        if self.0.len() <= max_bytes {
            return Some(self);
        }
        let suffix = if max_bytes >= 4 { "..." } else { "" };
        let mut end = max_bytes.saturating_sub(suffix.len());
        while end > 0 && !self.0.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 && suffix.is_empty() {
            return None;
        }
        self.0.truncate(end);
        self.0.push_str(suffix);
        Some(self)
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
