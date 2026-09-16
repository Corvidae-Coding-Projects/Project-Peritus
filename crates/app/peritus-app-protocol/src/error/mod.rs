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

    /// Returns useful prose followed by the stable troubleshooting fields.
    #[must_use]
    pub fn actionable_message(&self) -> String {
        let explanation = self
            .diagnostic
            .as_ref()
            .map_or_else(|| default_explanation(self.code), |value| value.as_str());
        format!(
            "{explanation} [code: {}; subsystem: {}; retry: {}]",
            self.code.as_str(),
            self.subsystem.as_str(),
            self.retry.as_str()
        )
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "keeping the closed error vocabulary in one exhaustive mapping makes omissions visible"
)]
const fn default_explanation(code: AppErrorCode) -> &'static str {
    match code {
        AppErrorCode::UnsupportedFormat => {
            "The request uses an unsupported data format. Upgrade the older Peritus component and retry."
        }
        AppErrorCode::UnsupportedFamily => {
            "The request type is unsupported here. Upgrade the older Peritus component and retry."
        }
        AppErrorCode::UnsupportedSchema => {
            "The request uses an unsupported schema version. Upgrade the older Peritus component and retry."
        }
        AppErrorCode::UnknownTag => {
            "The request contains an unknown protocol value. Upgrade the older Peritus component and retry."
        }
        AppErrorCode::MalformedFrame => {
            "The request contains invalid data. Correct the request before retrying."
        }
        AppErrorCode::TruncatedFrame => {
            "The request ended before all expected data arrived. Reconnect and retry."
        }
        AppErrorCode::TrailingBytes => {
            "The request contains unexpected trailing data. Correct the request before retrying."
        }
        AppErrorCode::LimitExceeded => {
            "The request exceeds a configured limit. Reduce its size or free capacity before retrying."
        }
        AppErrorCode::InvalidIdentifier => {
            "The requested item does not exist or its identifier is invalid. Refresh current state before retrying."
        }
        AppErrorCode::InvalidVersion => {
            "The request contains an invalid version. Refresh current state before retrying."
        }
        AppErrorCode::IncompatibleVersion => {
            "The client and daemon versions are incompatible. Upgrade the older Peritus component."
        }
        AppErrorCode::MissingRequiredFeature => {
            "The connected component does not support a required feature. Change the request or upgrade it."
        }
        AppErrorCode::InvalidLimits => {
            "The requested resource limits are invalid. Correct them before retrying."
        }
        AppErrorCode::SessionMismatch => {
            "The request belongs to another daemon session. Reconnect and retry."
        }
        AppErrorCode::IdempotencyConflict => {
            "A request identifier was reused for different work. Submit a new request identifier."
        }
        AppErrorCode::IdempotencyCapacity => {
            "The daemon cannot retain another deduplication record. Wait for capacity or restart before retrying."
        }
        AppErrorCode::StaleRevision => {
            "The request used outdated state. Refresh current state and submit a new request."
        }
        AppErrorCode::InvalidCommandFrame => {
            "The command is not registered. Correct the command before retrying."
        }
        AppErrorCode::CommandBindingMismatch => {
            "The command does not match its request metadata. Rebuild the request before retrying."
        }
        AppErrorCode::InvalidEventRange => {
            "The requested event range is invalid. Refresh the event cursor before retrying."
        }
        AppErrorCode::SubscriptionState => {
            "The event subscription is in the wrong state for this operation. Reconnect before retrying."
        }
        AppErrorCode::SubscriptionGap => {
            "The event stream has a gap. Reconnect and resume from the latest durable cursor."
        }
        AppErrorCode::IllegalAcknowledgement => {
            "The event acknowledgement exceeds delivered events. Reconnect and resume from the latest durable cursor."
        }
        AppErrorCode::Backpressure => {
            "The receiver is temporarily at capacity. Wait for it to recover before retrying."
        }
        AppErrorCode::ArtifactState => {
            "The artifact transfer is in the wrong state for this operation. Restart the transfer."
        }
        AppErrorCode::ArtifactOrdering => {
            "Artifact chunks arrived out of order. Restart the transfer."
        }
        AppErrorCode::ArtifactSize => {
            "Artifact bytes do not match the declared size. Recreate and resend the artifact."
        }
        AppErrorCode::ArtifactDigest => {
            "Artifact bytes do not match the declared digest. Recreate and resend the artifact."
        }
        AppErrorCode::PromptMismatch => {
            "The answer does not match the active prompt. Refresh the prompt before answering."
        }
        AppErrorCode::PromptStale => {
            "The prompt was already resolved or replaced. Refresh current state before answering."
        }
        AppErrorCode::TerminalState => {
            "The terminal is in the wrong state for this operation. Reattach or start a new terminal."
        }
        AppErrorCode::TerminalOrdering => {
            "Terminal data arrived out of order. Reattach the terminal."
        }
        AppErrorCode::ReadOnly => {
            "This connection or workspace is read-only. Change permissions before retrying."
        }
        AppErrorCode::NotReady => {
            "The daemon is not ready. Wait for recovery, inspect daemon status, then retry."
        }
        AppErrorCode::Cancelled => "The operation was cancelled. Start a new request to continue.",
        AppErrorCode::Internal => {
            "Peritus hit an internal failure. Inspect the daemon log, restart Peritus, and retry."
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actionable_message_prefers_diagnostic_and_keeps_machine_fields() {
        let error = AppProtocolError::classified(
            AppErrorCode::Internal,
            RetryDisposition::AfterRecovery,
            ResponsibleSubsystem::Daemon,
            Some(
                AppDiagnostic::new("Restore state-directory write access.".to_owned(), 64).unwrap(),
            ),
        );
        assert_eq!(
            error.actionable_message(),
            "Restore state-directory write access. [code: internal; subsystem: daemon; retry: after-recovery]"
        );
    }

    #[test]
    fn actionable_message_explains_errors_without_diagnostics() {
        let message =
            AppProtocolError::new(AppErrorCode::SessionMismatch, None).actionable_message();
        assert!(message.contains("another daemon session"));
        assert!(message.contains("code: session-mismatch"));
    }
}
