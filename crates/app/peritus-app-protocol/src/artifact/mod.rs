//! Bounded, transport-neutral artifact transfer contracts.

mod error;
mod metadata;
mod state;
mod verified;

pub use error::{ArtifactTransferError, ArtifactTransferErrorKind};
pub use metadata::{ArtifactChunk, ArtifactMetadata, CanonicalMediaType};
pub use state::{
    ArtifactCancellation, ArtifactFailure, ArtifactFailureKind, ArtifactTerminalDisposition,
    ArtifactTransferPhase, ArtifactTransferState,
};
pub use verified::{chunk_is_contiguous, completion_is_conserved};
