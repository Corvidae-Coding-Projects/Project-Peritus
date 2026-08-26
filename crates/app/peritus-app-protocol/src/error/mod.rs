//! Stable application-protocol error vocabulary.

mod code;
mod diagnostic;

pub use code::{AppErrorCode, ResponsibleSubsystem, RetryDisposition};
pub use diagnostic::{AppDiagnostic, DiagnosticError};

use core::fmt;
use peritus_codec::{CodecError, CodecErrorKind};

/// Complete machine-actionable application error plus optional bounded prose.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppProtocolError {
    code: AppErrorCode,
    retry: RetryDisposition,
    subsystem: ResponsibleSubsystem,
    diagnostic: Option<AppDiagnostic>,
    codec_source: Option<CodecError>,
}

impl AppProtocolError {
    /// Creates an error using the code's stable default classification.
    #[must_use]
    pub const fn new(code: AppErrorCode, diagnostic: Option<AppDiagnostic>) -> Self {
        Self {
            code,
            retry: code.default_retry(),
            subsystem: code.default_subsystem(),
            diagnostic,
            codec_source: None,
        }
    }

    /// Creates an error with an explicit retry and subsystem classification.
    #[must_use]
    pub const fn classified(
        code: AppErrorCode,
        retry: RetryDisposition,
        subsystem: ResponsibleSubsystem,
        diagnostic: Option<AppDiagnostic>,
    ) -> Self {
        Self { code, retry, subsystem, diagnostic, codec_source: None }
    }

    /// Lifts a codec failure without discarding its typed source category or byte offset.
    #[must_use]
    pub const fn from_codec(source: CodecError) -> Self {
        let code = match source.kind() {
            CodecErrorKind::LimitExceeded | CodecErrorKind::LengthOverflow => {
                AppErrorCode::LimitExceeded
            }
            CodecErrorKind::Truncated => AppErrorCode::TruncatedFrame,
            CodecErrorKind::TrailingBytes => AppErrorCode::TrailingBytes,
            CodecErrorKind::UnsupportedFormatVersion => AppErrorCode::UnsupportedFormat,
            CodecErrorKind::WrongFamily | CodecErrorKind::InvalidFamily => {
                AppErrorCode::UnsupportedFamily
            }
            CodecErrorKind::WrongSchemaVersion | CodecErrorKind::InvalidSchemaVersion => {
                AppErrorCode::UnsupportedSchema
            }
            CodecErrorKind::UnknownTag => AppErrorCode::UnknownTag,
            CodecErrorKind::InvalidBoolean
            | CodecErrorKind::InvalidOption
            | CodecErrorKind::InvalidUtf8
            | CodecErrorKind::InvalidMagic
            | CodecErrorKind::NonzeroFlags
            | CodecErrorKind::InvalidDomainValue => AppErrorCode::MalformedFrame,
        };
        Self {
            code,
            retry: code.default_retry(),
            subsystem: ResponsibleSubsystem::Codec,
            diagnostic: None,
            codec_source: Some(source),
        }
    }

    /// Returns the stable machine code.
    #[must_use]
    pub const fn code(&self) -> AppErrorCode {
        self.code
    }
    /// Returns the retry classification.
    #[must_use]
    pub const fn retry(&self) -> RetryDisposition {
        self.retry
    }
    /// Returns the responsible subsystem.
    #[must_use]
    pub const fn subsystem(&self) -> ResponsibleSubsystem {
        self.subsystem
    }
    /// Returns optional bounded diagnostic prose.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&AppDiagnostic> {
        self.diagnostic.as_ref()
    }
    /// Returns the retained typed codec source, when decoding produced this error.
    #[must_use]
    pub const fn codec_source(&self) -> Option<CodecError> {
        self.codec_source
    }
}

impl From<CodecError> for AppProtocolError {
    fn from(error: CodecError) -> Self {
        Self::from_codec(error)
    }
}

impl fmt::Display for AppProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())?;
        if let Some(diagnostic) = &self.diagnostic {
            formatter.write_str(": ")?;
            formatter.write_str(diagnostic.as_str())?;
        }
        Ok(())
    }
}

impl std::error::Error for AppProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.codec_source.as_ref().map(|error| error as &(dyn std::error::Error + 'static))
    }
}
