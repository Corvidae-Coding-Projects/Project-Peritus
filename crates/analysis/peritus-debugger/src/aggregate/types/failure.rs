//! Redaction-safe terminal debugger job failures.

use peritus_types::Sha256Digest;

use crate::DebuggerError;

/// Stable terminal job failure code.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JobFailureCode {
    /// Required binding or selected dependency disagreed.
    Dependency,
    /// Selection could not complete atomically.
    Selection,
    /// Deterministic analysis exhausted a bound or failed validation.
    Analysis,
    /// Report validation failed.
    Report,
    /// Durable journal/artifact/evidence state is inconsistent.
    Durability,
    /// Recovery cannot safely choose resume or exact retry.
    Recovery,
}

impl JobFailureCode {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Dependency => 1,
            Self::Selection => 2,
            Self::Analysis => 3,
            Self::Report => 4,
            Self::Durability => 5,
            Self::Recovery => 6,
        }
    }
    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::Dependency),
            2 => Ok(Self::Selection),
            3 => Ok(Self::Analysis),
            4 => Ok(Self::Report),
            5 => Ok(Self::Durability),
            6 => Ok(Self::Recovery),
            _ => Err(super::invalid("unknown terminal job failure tag")),
        }
    }
}

/// Redaction-safe terminal job failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobFailure {
    code: JobFailureCode,
    diagnostic_digest: Sha256Digest,
}

impl JobFailure {
    /// Creates a terminal safe failure record.
    #[must_use]
    pub const fn new(code: JobFailureCode, diagnostic_digest: Sha256Digest) -> Self {
        Self { code, diagnostic_digest }
    }
    /// Stable code.
    #[must_use]
    pub const fn code(self) -> JobFailureCode {
        self.code
    }
    /// Digest of safe diagnostic metadata.
    #[must_use]
    pub const fn diagnostic_digest(self) -> Sha256Digest {
        self.diagnostic_digest
    }
}
