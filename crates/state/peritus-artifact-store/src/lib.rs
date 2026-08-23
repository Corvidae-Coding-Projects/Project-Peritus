//! Streaming content-addressed artifact storage for Peritus.
//!
//! The crate separates checked, deterministic planning from narrow filesystem operations. Object
//! and quarantine paths are derived only from [`ArtifactDigest`] values; callers never supply a
//! path below the configured store root.

mod catalog;
mod config;
mod digest;
mod error;
mod finalize;
mod gc_plan;
mod metadata;
mod path;
mod quota;
mod recovery;
mod references;
/// Narrow transaction helpers for sharing the artifact catalog with the journal.
pub mod sqlite_interop;
mod store;
mod verified;
mod writer;

pub use config::StoreConfig;
pub use digest::ArtifactDigest;
pub use error::{ArtifactStoreError, ErrorCode, RecoveryClass, StoreOperation};
pub use gc_plan::{CollectionGeneration, GcAction, GcApplication, GcInventoryEntry, GcPlan};
pub use metadata::{
    ArtifactMetadata, EncryptionMetadata, FinalizationState, MediaType, QuarantineState,
};
pub use quota::{QuotaPlan, QuotaSnapshot};
pub use recovery::{QuarantinedArtifact, RecoveryReport};
pub use references::{ArtifactReferenceSet, ReferenceRoots};
pub use references::{ReferenceOwner, ReferenceOwnerKind};
pub use store::{ArtifactStore, SpaceObservation};
pub use writer::{ArtifactWriter, FinalizedArtifact, Publication, WriteRequest};
