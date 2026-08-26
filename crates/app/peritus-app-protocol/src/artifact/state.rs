//! Pure artifact-transfer state machine.

use crate::{CorrelationId, TransferId};
use peritus_types::{ArtifactId, Sha256Digest};

use super::{
    ArtifactChunk, ArtifactMetadata, ArtifactTransferError, ArtifactTransferErrorKind,
    chunk_is_contiguous, completion_is_conserved, error::reject,
};

/// Stable reason category for terminal transfer failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactFailureKind {
    /// The sender could not read the source bytes.
    SourceUnavailable,
    /// The receiver rejected the transfer representation.
    InvalidRepresentation,
    /// An external operation failed without a stronger public category.
    ExternalFailure,
}

/// Bounded terminal failure fact.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactFailure {
    kind: ArtifactFailureKind,
    detail: String,
}

impl ArtifactFailure {
    /// Creates a bounded nonempty failure fact.
    ///
    /// # Errors
    ///
    /// Rejects a zero bound or empty/oversized diagnostic text.
    pub fn new(
        kind: ArtifactFailureKind,
        detail: String,
        maximum_detail_bytes: usize,
    ) -> Result<Self, ArtifactTransferError> {
        if maximum_detail_bytes == 0 {
            return Err(reject(ArtifactTransferErrorKind::InvalidLimit, "detail limit is zero"));
        }
        if detail.is_empty() || detail.len() > maximum_detail_bytes {
            return Err(reject(
                ArtifactTransferErrorKind::InvalidInput,
                "failure detail is empty or exceeds its negotiated bound",
            ));
        }
        Ok(Self { kind, detail })
    }
    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ArtifactFailureKind {
        self.kind
    }
    /// Borrows inert diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Correlated terminal artifact cancellation fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactCancellation {
    transfer: TransferId,
    artifact: ArtifactId,
    correlation: CorrelationId,
}

impl ArtifactCancellation {
    /// Creates an exact cancellation fact.
    #[must_use]
    pub const fn new(
        transfer_id: TransferId,
        artifact_id: ArtifactId,
        correlation_id: CorrelationId,
    ) -> Self {
        Self { transfer: transfer_id, artifact: artifact_id, correlation: correlation_id }
    }
    /// Returns the transfer identity.
    #[must_use]
    pub const fn transfer_id(self) -> TransferId {
        self.transfer
    }
    /// Returns the artifact identity.
    #[must_use]
    pub const fn artifact_id(self) -> ArtifactId {
        self.artifact
    }
    /// Returns the request/response correlation identity.
    #[must_use]
    pub const fn correlation_id(self) -> CorrelationId {
        self.correlation
    }
}

/// Observable artifact-transfer phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactTransferPhase {
    /// Chunks may be admitted.
    Receiving,
    /// Exact size and observed digest were accepted; persistence is not implied.
    Completed(Sha256Digest),
    /// A correlated cancellation ended the transfer.
    Cancelled(ArtifactCancellation),
    /// A reported external failure ended the transfer.
    Failed(ArtifactFailure),
}

/// Result of an idempotent terminal cancellation/failure observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactTerminalDisposition {
    /// The terminal fact caused the transition.
    Applied,
    /// The exact retained terminal fact was repeated.
    Repeated,
}

/// Pure transfer state tracking only conserved length and expected ordinal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactTransferState {
    metadata: ArtifactMetadata,
    maximum_chunk_bytes: usize,
    conserved_bytes: u64,
    next_ordinal: u64,
    phase: ArtifactTransferPhase,
}

impl ArtifactTransferState {
    /// Starts a transfer from checked immutable metadata.
    ///
    /// # Errors
    ///
    /// Rejects a zero chunk bound or metadata whose preferred chunk size exceeds it.
    pub fn new(
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, ArtifactTransferError> {
        let preferred_fits = usize::try_from(metadata.preferred_chunk_size())
            .is_ok_and(|preferred| preferred <= maximum_chunk_bytes);
        if maximum_chunk_bytes == 0 || !preferred_fits {
            return Err(reject(
                ArtifactTransferErrorKind::InvalidLimit,
                "transfer chunk bound is zero or below the metadata preference",
            ));
        }
        Ok(Self {
            metadata,
            maximum_chunk_bytes,
            conserved_bytes: 0,
            next_ordinal: 0,
            phase: ArtifactTransferPhase::Receiving,
        })
    }

    /// Borrows immutable transfer metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }
    /// Returns the conserved accepted byte count.
    #[must_use]
    pub const fn conserved_bytes(&self) -> u64 {
        self.conserved_bytes
    }
    /// Returns the exact next zero-based chunk ordinal.
    #[must_use]
    pub const fn next_ordinal(&self) -> u64 {
        self.next_ordinal
    }
    /// Borrows the current phase.
    #[must_use]
    pub const fn phase(&self) -> &ArtifactTransferPhase {
        &self.phase
    }

    /// Accepts one matching, ordered, contiguous, non-overlapping chunk.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, identity mismatch, wrong ordinal/offset, oversized bytes, or size
    /// overflow beyond declared metadata.
    pub fn accept_chunk(&mut self, chunk: &ArtifactChunk) -> Result<u64, ArtifactTransferError> {
        if self.phase != ArtifactTransferPhase::Receiving {
            return Err(reject(
                ArtifactTransferErrorKind::AlreadyTerminal,
                "terminal artifact transfer cannot accept chunks",
            ));
        }
        if chunk.transfer_id() != self.metadata.transfer_id()
            || chunk.artifact_id() != self.metadata.artifact_id()
        {
            return Err(reject(
                ArtifactTransferErrorKind::BindingMismatch,
                "chunk names another transfer or artifact",
            ));
        }
        if chunk.ordinal() != self.next_ordinal {
            return Err(reject(
                ArtifactTransferErrorKind::UnexpectedOrdinal,
                "chunk ordinal is not the exact expected ordinal",
            ));
        }
        if chunk.offset() != self.conserved_bytes {
            return Err(reject(
                ArtifactTransferErrorKind::UnexpectedOffset,
                "chunk offset is not the conserved byte count",
            ));
        }
        if chunk.bytes().len() > self.maximum_chunk_bytes
            || !chunk_is_contiguous(
                self.conserved_bytes,
                chunk.offset(),
                chunk.bytes().len(),
                self.metadata.byte_size(),
            )
        {
            return Err(reject(
                ArtifactTransferErrorKind::SizeOverflow,
                "chunk is oversized or exceeds declared artifact size",
            ));
        }
        let length = u64::try_from(chunk.bytes().len()).map_err(|_| {
            reject(ArtifactTransferErrorKind::SizeOverflow, "chunk length does not fit u64")
        })?;
        let conserved_bytes = self.conserved_bytes.checked_add(length).ok_or_else(|| {
            reject(ArtifactTransferErrorKind::SizeOverflow, "conserved byte count overflow")
        })?;
        let next_ordinal = self.next_ordinal.checked_add(1).ok_or_else(|| {
            reject(ArtifactTransferErrorKind::SizeOverflow, "chunk ordinal overflow")
        })?;
        self.conserved_bytes = conserved_bytes;
        self.next_ordinal = next_ordinal;
        Ok(self.conserved_bytes)
    }

    /// Completes only an exact-size transfer whose observed SHA-256 matches metadata.
    ///
    /// Completion records protocol observation only; it does not claim persistence or C0
    /// finalization.
    ///
    /// # Errors
    ///
    /// Rejects terminal state, incomplete byte conservation, or digest mismatch.
    pub fn complete(&mut self, observed: Sha256Digest) -> Result<(), ArtifactTransferError> {
        if self.phase != ArtifactTransferPhase::Receiving {
            return Err(reject(
                ArtifactTransferErrorKind::AlreadyTerminal,
                "artifact transfer is already terminal",
            ));
        }
        if !completion_is_conserved(self.conserved_bytes, self.metadata.byte_size()) {
            return Err(reject(
                ArtifactTransferErrorKind::Incomplete,
                "conserved bytes do not equal declared artifact size",
            ));
        }
        if observed != self.metadata.digest() {
            self.phase = ArtifactTransferPhase::Failed(ArtifactFailure {
                kind: ArtifactFailureKind::InvalidRepresentation,
                detail: "observed digest differs from declared metadata".to_owned(),
            });
            return Err(reject(
                ArtifactTransferErrorKind::DigestMismatch,
                "observed artifact digest differs from metadata",
            ));
        }
        self.phase = ArtifactTransferPhase::Completed(observed);
        Ok(())
    }

    /// Applies a matching cancellation idempotently.
    ///
    /// # Errors
    ///
    /// Rejects an identity mismatch, completion/failure, or a conflicting cancellation fact.
    pub fn cancel(
        &mut self,
        cancellation: ArtifactCancellation,
    ) -> Result<ArtifactTerminalDisposition, ArtifactTransferError> {
        if cancellation.transfer != self.metadata.transfer_id()
            || cancellation.artifact != self.metadata.artifact_id()
        {
            return Err(reject(
                ArtifactTransferErrorKind::BindingMismatch,
                "cancellation names another transfer or artifact",
            ));
        }
        match &self.phase {
            ArtifactTransferPhase::Receiving => {
                self.phase = ArtifactTransferPhase::Cancelled(cancellation);
                Ok(ArtifactTerminalDisposition::Applied)
            }
            ArtifactTransferPhase::Cancelled(retained) if *retained == cancellation => {
                Ok(ArtifactTerminalDisposition::Repeated)
            }
            ArtifactTransferPhase::Cancelled(_) => Err(reject(
                ArtifactTransferErrorKind::TerminalConflict,
                "cancellation conflicts with the retained terminal fact",
            )),
            ArtifactTransferPhase::Completed(_) | ArtifactTransferPhase::Failed(_) => Err(reject(
                ArtifactTransferErrorKind::AlreadyTerminal,
                "completed or failed transfer cannot be cancelled",
            )),
        }
    }

    /// Records an external failure idempotently when its exact fact repeats.
    ///
    /// # Errors
    ///
    /// Rejects completion/cancellation or a conflicting failure fact.
    pub fn fail(
        &mut self,
        failure: ArtifactFailure,
    ) -> Result<ArtifactTerminalDisposition, ArtifactTransferError> {
        match &self.phase {
            ArtifactTransferPhase::Receiving => {
                self.phase = ArtifactTransferPhase::Failed(failure);
                Ok(ArtifactTerminalDisposition::Applied)
            }
            ArtifactTransferPhase::Failed(retained) if retained == &failure => {
                Ok(ArtifactTerminalDisposition::Repeated)
            }
            ArtifactTransferPhase::Failed(_) => Err(reject(
                ArtifactTransferErrorKind::TerminalConflict,
                "failure conflicts with the retained terminal fact",
            )),
            ArtifactTransferPhase::Completed(_) | ArtifactTransferPhase::Cancelled(_) => Err(
                reject(ArtifactTransferErrorKind::AlreadyTerminal, "transfer is already terminal"),
            ),
        }
    }
}
