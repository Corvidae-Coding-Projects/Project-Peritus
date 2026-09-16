use core::fmt;
use std::path::PathBuf;

use peritus_app_protocol::AppProtocolError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCategory {
    Usage,
    Connection,
    Negotiation,
    Rejected,
    LocalIo,
    Protocol,
    Interrupted,
    Internal,
}

impl ExitCategory {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Usage => 2,
            Self::Connection => 10,
            Self::Negotiation => 11,
            Self::Rejected => 12,
            Self::LocalIo => 13,
            Self::Protocol => 14,
            Self::Interrupted => 130,
            Self::Internal => 70,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Connection => "connection",
            Self::Negotiation => "negotiation",
            Self::Rejected => "rejected",
            Self::LocalIo => "local-io",
            Self::Protocol => "protocol",
            Self::Interrupted => "interrupted",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug)]
pub struct CliError {
    category: ExitCategory,
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl CliError {
    pub(crate) fn usage(detail: impl Into<String>) -> Self {
        Self::new(ExitCategory::Usage, "parse arguments", detail)
    }

    pub(crate) fn connection(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(ExitCategory::Connection, operation, detail)
    }

    pub(crate) fn negotiation(detail: impl Into<String>) -> Self {
        Self::new(ExitCategory::Negotiation, "negotiate daemon session", detail)
    }

    pub(crate) fn rejected(error: &AppProtocolError) -> Self {
        let diagnostic =
            error.diagnostic().map_or_else(String::new, |value| format!(": {}", value.as_str()));
        Self::new(
            ExitCategory::Rejected,
            "execute daemon request",
            format!(
                "{} (subsystem={}, retry={}){diagnostic}",
                error.code().as_str(),
                error.subsystem().as_str(),
                error.retry().as_str(),
            ),
        )
    }

    pub(crate) fn remote_failure(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(ExitCategory::Rejected, operation, detail)
    }

    pub(crate) fn protocol(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(ExitCategory::Protocol, operation, detail)
    }

    pub(crate) fn local_io(
        operation: &'static str,
        path: Option<PathBuf>,
        error: std::io::Error,
    ) -> Self {
        let detail =
            path.map_or_else(|| error.to_string(), |path| format!("{}: {error}", path.display()));
        Self::with_source(ExitCategory::LocalIo, operation, detail, error)
    }

    pub(crate) fn output(error: std::io::Error) -> Self {
        Self::local_io("write output", None, error)
    }

    pub(crate) fn interrupted() -> Self {
        Self::new(ExitCategory::Interrupted, "stream operation", "interrupted by user")
    }

    pub(crate) fn runtime(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(ExitCategory::Internal, operation, detail)
    }

    pub(crate) const fn category(&self) -> ExitCategory {
        self.category
    }

    fn new(category: ExitCategory, operation: &'static str, detail: impl Into<String>) -> Self {
        Self { category, operation, detail: detail.into(), source: None }
    }

    fn with_source(
        category: ExitCategory,
        operation: &'static str,
        detail: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self { category, operation, detail: detail.into(), source: Some(Box::new(source)) }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

impl From<AppProtocolError> for CliError {
    fn from(error: AppProtocolError) -> Self {
        let detail = error.to_string();
        Self::with_source(ExitCategory::Protocol, "construct protocol value", detail, error)
    }
}
