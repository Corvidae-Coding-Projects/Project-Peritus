//! Product-state validation failures.

/// Stable validation failures for durable product state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductStateError {
    /// The persisted schema is newer or older than this executable understands.
    UnsupportedSchema(u16),
    /// A stable installation identity is zero or has an invalid encoding.
    InvalidIdentity(&'static str),
    /// Durable bootstrap progress attempted to skip or reverse a phase.
    InvalidTransition {
        /// Phase currently persisted.
        from: crate::BootstrapPhase,
        /// Requested next phase.
        to: crate::BootstrapPhase,
    },
    /// The state payload is malformed or contains unsupported fields.
    InvalidPayload(String),
}

impl core::fmt::Display for ProductStateError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported product-state schema version {version}")
            }
            Self::InvalidIdentity(field) => write!(
                formatter,
                "{field} must be 32 lowercase hexadecimal digits and must not be zero"
            ),
            Self::InvalidTransition { from, to } => {
                write!(formatter, "invalid durable bootstrap transition from {from:?} to {to:?}")
            }
            Self::InvalidPayload(message) => {
                write!(formatter, "product-state payload is invalid: {message}")
            }
        }
    }
}

impl std::error::Error for ProductStateError {}
