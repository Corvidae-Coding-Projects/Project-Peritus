//! Deterministic portable bundle planning, streaming assembly, and offline verification.

mod assemble;
mod format;
mod plan;
mod preparation;
mod verify;

pub use assemble::{
    BundleExportCursor, BundleExportOperation, BundleExportPhase, BundleReceipt, assemble_bundle,
    publish_bundle, resume_bundle,
};
pub use plan::{BundleLimits, BundlePlan};
pub use preparation::BundlePreparation;
pub use verify::{
    BundleVerificationCursor, BundleVerificationOperation, VerifiedBundle, verify_bundle,
};
