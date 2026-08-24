//! Stable, redacted fake-server failures.

use std::error::Error;
use std::fmt;

/// Stable category for a fake HTTP server failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FakeHttpErrorKind {
    /// A limit, request expectation, or response script is invalid.
    InvalidConfiguration,
    /// The isolated loopback listener could not be created.
    Bind,
    /// The owned worker thread could not be created.
    Spawn,
    /// A socket operation failed before an exchange could be observed.
    Io,
    /// The incoming request was malformed.
    MalformedRequest,
    /// The incoming request exceeded a configured bound.
    RequestLimit,
    /// A release operation was requested in the wrong state.
    ReleaseState,
    /// The requested wait expired.
    Timeout,
    /// The owned worker panicked.
    WorkerPanic,
    /// The worker ended without reporting its result.
    MissingResult,
}

impl FakeHttpErrorKind {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "PERITUS-TEST-HTTP-001",
            Self::Bind => "PERITUS-TEST-HTTP-002",
            Self::Spawn => "PERITUS-TEST-HTTP-003",
            Self::Io => "PERITUS-TEST-HTTP-004",
            Self::MalformedRequest => "PERITUS-TEST-HTTP-005",
            Self::RequestLimit => "PERITUS-TEST-HTTP-006",
            Self::ReleaseState => "PERITUS-TEST-HTTP-007",
            Self::Timeout => "PERITUS-TEST-HTTP-008",
            Self::WorkerPanic => "PERITUS-TEST-HTTP-009",
            Self::MissingResult => "PERITUS-TEST-HTTP-010",
        }
    }
}

/// A bounded failure that never includes request, header, or body bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeHttpError {
    kind: FakeHttpErrorKind,
    detail: &'static str,
}

impl FakeHttpError {
    pub(crate) const fn new(kind: FakeHttpErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> FakeHttpErrorKind {
        self.kind
    }

    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns bounded context that contains no peer-controlled bytes.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for FakeHttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)
    }
}

impl Error for FakeHttpError {}
