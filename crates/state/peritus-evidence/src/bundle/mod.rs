//! Deterministic portable bundle planning, streaming assembly, and offline verification.

mod assemble;
mod format;
mod plan;
mod verify;

pub use assemble::{BundleReceipt, assemble_bundle};
pub use plan::{BundleLimits, BundlePlan};
pub use verify::{VerifiedBundle, verify_bundle};
