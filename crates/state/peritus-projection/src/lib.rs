//! Pure deterministic journal projections and durable shadow rebuilds for Peritus.
//!
//! Fold implementations receive only checked immutable journal records. Persistence is isolated
//! behind [`ProjectionStore`], so replay code cannot acquire a database connection or perform an
//! external effect.

mod agent;
mod artifacts;
mod authority;
mod budget;
mod catalog;
mod checkpoint;
mod encoding;
mod error;
mod evidence;
mod fold;
mod journal_catalog;
mod lifecycle;
mod rebuild;
mod replay;
pub mod sqlite;
pub mod verified;

pub use agent::{AgentEntry, AgentProjection, AgentState};
pub use artifacts::{
    ArtifactReferenceEntry, ArtifactReferenceProjection, ArtifactReferenceState,
    replay_artifact_references,
};
pub use authority::{AuthorityEntry, AuthorityProjection, AuthorityState};
pub use budget::{BudgetEntry, BudgetProjection, BudgetState};
pub use catalog::{ActiveGeneration, CatalogGeneration, RepairAction, RepairReason};
pub use checkpoint::{
    Checkpoint, ProjectionIdentity, ProjectionName, ProjectionSchema, ProjectionVersion,
};
pub use error::{ProjectionError, ProjectionErrorKind, RecoveryClass};
pub use evidence::{EvidenceCatalogProjection, EvidenceCatalogState, EvidenceEntry};
pub use fold::{FoldContext, Projection, ProjectionState};
pub use journal_catalog::{JournalCatalogEntry, JournalCatalogProjection, JournalCatalogState};
pub use lifecycle::{LifecycleEntry, LifecycleProjection, LifecycleState};
pub use rebuild::{RebuildCandidate, rebuild_from_genesis};
pub use replay::{ReplayOutput, replay_from_genesis};
pub use sqlite::{ProjectionStore, StoreOptions};
