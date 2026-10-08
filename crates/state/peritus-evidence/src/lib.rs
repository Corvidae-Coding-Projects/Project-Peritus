//! Durable evidence provenance and portable bundle assembly for Peritus.
//!
//! Admission proves immutable records against an integrity-checked journal export, a matching row
//! in the caller-selected shared `SQLite` database, exact revision binding, valid causal ancestry,
//! and finalized artifact bytes. Portable bundles are inert deterministic bytes and never grant an
//! effect capability.

mod admission;
mod bundle;
mod canonical;
mod causality;
mod error;
mod freshness;
mod invalidation;
mod manifest;
mod provenance;
mod record;
pub mod sqlite;
pub mod verified;

pub use bundle::{
    BundleExportCursor, BundleExportOperation, BundleExportPhase, BundleLimits, BundlePlan,
    BundleReceipt, BundleVerificationCursor, BundleVerificationOperation,
    BundleVerificationPhase, VerifiedBundle, assemble_bundle, publish_bundle, resume_bundle,
    verify_bundle,
};
pub use causality::CausalLink;
pub use error::{EvidenceError, EvidenceErrorKind, RecoveryAction};
pub use freshness::{
    Freshness, FreshnessRequirement, RevisionDrift, evaluate_freshness, revision_digest,
};
pub use invalidation::EvidenceInvalidation;
pub use manifest::{
    ArtifactManifestEntry, EvidenceManifest, JournalManifestEntry, RecordManifestEntry,
};
pub use peritus_types::EvidenceId;
/// Explicit retained-owner cancellation used by long-running evidence operations.
pub use peritus_journal::JournalCancellation as EvidenceCancellation;
pub use provenance::JournalProvenance;
pub use record::{EvidenceDraft, EvidenceKind, EvidenceRecord, EvidenceSource};
pub use sqlite::{EvidenceQuarantine, EvidenceStore, EvidenceStoreOptions};
