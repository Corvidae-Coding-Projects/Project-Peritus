//! Redaction-safe developer-loop failures.

use core::fmt;

use crate::ModelDriveError;

/// Failure while driving a production inspect/edit/run/test model loop.
#[derive(Debug)]
#[non_exhaustive]
pub enum DeveloperLoopError {
    /// A checked C5 protocol value could not be constructed.
    Protocol(peritus_model_protocol::ProtocolError),
    /// The provider session failed or produced an invalid event stream.
    Model(ModelDriveError),
    /// A durable trace boundary rejected an event.
    Trace(String),
    /// The model requested an undeclared or invalid tool operation.
    Tool(String),
    /// The provider refused the developer role.
    Refused,
    /// The configured turn or tool-call limit was exhausted.
    LimitExceeded,
    /// The request was cancelled.
    Cancelled,
    /// A terminal provider response contained no usable result.
    EmptyResponse,
}

impl fmt::Display for DeveloperLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(error) => fmt::Display::fmt(error, formatter),
            Self::Model(error) => fmt::Display::fmt(error, formatter),
            Self::Trace(detail) => write!(formatter, "persist developer trace: {detail}"),
            Self::Tool(detail) => write!(formatter, "execute developer tool: {detail}"),
            Self::Refused => formatter.write_str("provider refused the developer request"),
            Self::LimitExceeded => formatter.write_str("developer loop limit was exhausted"),
            Self::Cancelled => formatter.write_str("developer loop was cancelled"),
            Self::EmptyResponse => {
                formatter.write_str("provider returned no tool calls or usable final response")
            }
        }
    }
}

impl std::error::Error for DeveloperLoopError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            Self::Model(error) => Some(error),
            Self::Trace(_)
            | Self::Tool(_)
            | Self::Refused
            | Self::LimitExceeded
            | Self::Cancelled
            | Self::EmptyResponse => None,
        }
    }
}

impl From<peritus_model_protocol::ProtocolError> for DeveloperLoopError {
    fn from(value: peritus_model_protocol::ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<ModelDriveError> for DeveloperLoopError {
    fn from(value: ModelDriveError) -> Self {
        Self::Model(value)
    }
}
