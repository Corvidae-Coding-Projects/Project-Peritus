//! Product-state validation failures.

/// Stable validation failures for durable product state.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProductStateError {
    /// The persisted schema is newer or older than this executable understands.
    #[error("unsupported product-state schema version {0}")]
    UnsupportedSchema(u16),
    /// A stable installation identity is zero or has an invalid encoding.
    #[error("{0} must be 32 lowercase hexadecimal digits and must not be zero")]
    InvalidIdentity(&'static str),
    /// Durable bootstrap progress attempted to skip or reverse a phase.
    #[error("invalid durable bootstrap transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Phase currently persisted.
        from: crate::BootstrapPhase,
        /// Requested next phase.
        to: crate::BootstrapPhase,
    },
    /// The state payload is malformed or contains unsupported fields.
    #[error("product-state payload is invalid: {0}")]
    InvalidPayload(String),
}
