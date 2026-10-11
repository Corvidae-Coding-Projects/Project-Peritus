//! Verified, time-independent primitive domain types for Peritus.
//!
//! Constructors reject invalid boundary values in ordinary Rust. The same executable bodies carry
//! Verus postconditions and type invariants, so verified consumers can rely on those constraints
//! without preconditions.

mod capability;
mod digest;
mod errors;
mod ids;
mod numbers;
mod resource;
mod revision;
pub use capability::CapabilityName;
pub use digest::Sha256Digest;
pub use errors::{
    CapabilityNameError, IdentifierError, OneBasedNumberError, ResourceQuantityError,
};
pub use ids::*;
pub use numbers::{EventSequence, Generation, RevisionNumber};
pub use resource::{ResourceKind, ResourceQuantity};
pub use revision::RevisionTuple;
