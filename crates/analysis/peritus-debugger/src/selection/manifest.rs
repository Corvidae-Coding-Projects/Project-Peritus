//! Immutable manifest entries and canonical schema-v1 representation.

use crate::{
    AnalysisSubject, DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery,
    SelectionManifestId, TraceSelectionQuery,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactMetadata, FinalizationState, QuarantineState,
};
use peritus_trace::{
    CausalBinding, ObservationKind, ObservedTime, RedactedValue, SafeAttribute, SpanId, TraceId,
};
use peritus_types::{EventId, Sha256Digest};

const MANIFEST_ID_DOMAIN: &[u8] = b"peritus-e2-selection-manifest-id-v1\0";
const MANIFEST_DIGEST_DOMAIN: &[u8] = b"peritus-e2-selection-manifest-digest-v1\0";

/// Verified ordinary finalized artifact eligible for bounded report citations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedArtifact {
    digest: ArtifactDigest,
    size: u64,
    creating_event: EventId,
    source_event: Option<EventId>,
}

impl SelectedArtifact {
    /// Checks finalized, active, unencrypted ordinary artifact metadata.
    ///
    /// `source_event` narrows the artifact to a selected observation. `None` declares it as an
    /// explicit report-level inventory item; C0 provenance is still checked by the selector.
    ///
    /// # Errors
    ///
    /// Rejects partial, quarantined, encrypted, or empty artifacts.
    pub fn from_metadata(
        metadata: &ArtifactMetadata,
        source_event: Option<EventId>,
    ) -> Result<Self, DebuggerError> {
        if metadata.finalization() != FinalizationState::Finalized
            || metadata.quarantine() != QuarantineState::Active
            || metadata.encryption().is_encrypted()
            || metadata.size() == 0
        {
            return Err(DebuggerError::new(
                DebuggerErrorKind::Artifact,
                DebuggerOperation::SelectEvidence,
                DebuggerRecovery::RepairDependency,
                "selected ordinary artifact is not finalized, active, unencrypted, and nonempty",
            ));
        }
        Ok(Self {
            digest: metadata.digest(),
            size: metadata.size(),
            creating_event: metadata.creating_event(),
            source_event,
        })
    }

    /// Returns the finalized content digest.
    #[must_use]
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }
    /// Returns the exact verified durable size.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    /// Returns the journal event that created the artifact.
    #[must_use]
    pub const fn creating_event(&self) -> EventId {
        self.creating_event
    }
    /// Returns the selected observation that lists the artifact, when event-scoped.
    #[must_use]
    pub const fn source_event(&self) -> Option<EventId> {
        self.source_event
    }
}

/// One selected redacted C7 observation with exact C0 provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedEvidence {
    subject: AnalysisSubject,
    trace_id: TraceId,
    span_id: SpanId,
    span_sequence: u64,
    parent_span_id: Option<SpanId>,
    event_id: EventId,
    causal_events: Vec<EventId>,
    binding: CausalBinding,
    time: ObservedTime,
    kind: ObservationKind,
    attributes: Vec<SafeAttribute>,
    redactions: Vec<RedactedValue>,
    journal_position: u64,
    frame_digest: Sha256Digest,
    frame_length: u64,
}

impl SelectedEvidence {
    #[allow(clippy::too_many_arguments, reason = "the exact evidence envelope remains explicit")]
    pub(super) fn checked(
        subject: AnalysisSubject,
        observation: &peritus_trace::Observation,
        journal_position: u64,
        frame_digest: Sha256Digest,
        frame_length: u64,
    ) -> Self {
        Self {
            subject,
            trace_id: observation.trace_id(),
            span_id: observation.span_id(),
            span_sequence: observation.span_sequence(),
            parent_span_id: observation.parent_span_id(),
            event_id: observation.event_id(),
            causal_events: observation.causal_events().to_vec(),
            binding: observation.binding(),
            time: observation.time(),
            kind: observation.kind(),
            attributes: observation.attributes().to_vec(),
            redactions: observation.redactions().to_vec(),
            journal_position,
            frame_digest,
            frame_length,
        }
    }

    /// Borrows the complete checked subject binding.
    #[must_use]
    pub const fn subject(&self) -> &AnalysisSubject {
        &self.subject
    }
    /// Returns the trace identity.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }
    /// Returns the span identity.
    #[must_use]
    pub const fn span_id(&self) -> SpanId {
        self.span_id
    }
    /// Returns the one-based span sequence.
    #[must_use]
    pub const fn span_sequence(&self) -> u64 {
        self.span_sequence
    }
    /// Returns the structural parent span.
    #[must_use]
    pub const fn parent_span_id(&self) -> Option<SpanId> {
        self.parent_span_id
    }
    /// Returns the C7/C0 event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Borrows canonical causal predecessor identities.
    #[must_use]
    pub fn causal_events(&self) -> &[EventId] {
        &self.causal_events
    }
    /// Returns the complete C7 causal binding.
    #[must_use]
    pub const fn binding(&self) -> CausalBinding {
        self.binding
    }
    /// Returns source wall and monotonic time.
    #[must_use]
    pub const fn time(&self) -> ObservedTime {
        self.time
    }
    /// Returns the closed observation kind.
    #[must_use]
    pub const fn kind(&self) -> ObservationKind {
        self.kind
    }
    /// Borrows C7 safe scalar attributes.
    #[must_use]
    pub fn attributes(&self) -> &[SafeAttribute] {
        &self.attributes
    }
    /// Borrows omission markers and opaque encrypted vault metadata.
    #[must_use]
    pub fn redactions(&self) -> &[RedactedValue] {
        &self.redactions
    }
    /// Returns the exact one-based C0 position.
    #[must_use]
    pub const fn journal_position(&self) -> u64 {
        self.journal_position
    }
    /// Returns the exact C7 frame digest.
    #[must_use]
    pub const fn frame_digest(&self) -> Sha256Digest {
        self.frame_digest
    }
    /// Returns the exact C7 frame length.
    #[must_use]
    pub const fn frame_length(&self) -> u64 {
        self.frame_length
    }
}

/// Exact selected resource counts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SelectionCounts {
    subjects: u64,
    traces: u64,
    events: u64,
    causal_edges: u64,
    artifacts: u64,
    artifact_bytes: u64,
}

impl SelectionCounts {
    pub(super) const fn new(
        subjects: u64,
        traces: u64,
        events: u64,
        causal_edges: u64,
        artifacts: u64,
        artifact_bytes: u64,
    ) -> Self {
        Self { subjects, traces, events, causal_edges, artifacts, artifact_bytes }
    }
    /// Returns the subject count.
    #[must_use]
    pub const fn subjects(self) -> u64 {
        self.subjects
    }
    /// Returns the trace count.
    #[must_use]
    pub const fn traces(self) -> u64 {
        self.traces
    }
    /// Returns the event count.
    #[must_use]
    pub const fn events(self) -> u64 {
        self.events
    }
    /// Returns the causal-edge count.
    #[must_use]
    pub const fn causal_edges(self) -> u64 {
        self.causal_edges
    }
    /// Returns the selected ordinary artifact count.
    #[must_use]
    pub const fn artifacts(self) -> u64 {
        self.artifacts
    }
    /// Returns the selected ordinary artifact bytes.
    #[must_use]
    pub const fn artifact_bytes(self) -> u64 {
        self.artifact_bytes
    }
}

/// Complete immutable, content-addressed evidence selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceSelectionManifest {
    id: SelectionManifestId,
    digest: Sha256Digest,
    query_digest: Sha256Digest,
    subjects: Vec<AnalysisSubject>,
    entries: Vec<SelectedEvidence>,
    artifacts: Vec<SelectedArtifact>,
    counts: SelectionCounts,
    canonical_bytes: Vec<u8>,
}

impl TraceSelectionManifest {
    pub(super) fn checked(
        query: &TraceSelectionQuery,
        entries: Vec<SelectedEvidence>,
        artifacts: Vec<SelectedArtifact>,
        counts: SelectionCounts,
    ) -> Result<Self, DebuggerError> {
        let mut manifest = Self {
            id: SelectionManifestId::new([1; 16])?,
            digest: Sha256Digest::new([0; 32]),
            query_digest: query.digest(),
            subjects: query.subjects().to_vec(),
            entries,
            artifacts,
            counts,
            canonical_bytes: Vec::new(),
        };
        manifest.canonical_bytes = super::canonical::encode_manifest(&manifest);
        manifest.digest =
            crate::identity::domain_digest(MANIFEST_DIGEST_DOMAIN, &manifest.canonical_bytes);
        manifest.id = SelectionManifestId::derive(MANIFEST_ID_DOMAIN, manifest.digest.as_bytes())?;
        Ok(manifest)
    }

    /// Returns the content-derived manifest identity.
    #[must_use]
    pub const fn id(&self) -> SelectionManifestId {
        self.id
    }
    /// Returns the complete manifest digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the frozen query digest.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Borrows canonical complete subject bindings.
    #[must_use]
    pub fn subjects(&self) -> &[AnalysisSubject] {
        &self.subjects
    }
    /// Borrows entries in `(subject, journal position, event)` order.
    #[must_use]
    pub fn entries(&self) -> &[SelectedEvidence] {
        &self.entries
    }
    /// Borrows selected ordinary artifacts in digest order.
    #[must_use]
    pub fn artifacts(&self) -> &[SelectedArtifact] {
        &self.artifacts
    }
    /// Returns exact selection counts.
    #[must_use]
    pub const fn counts(&self) -> SelectionCounts {
        self.counts
    }
    /// Borrows canonical schema-v1 manifest bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Looks up exactly one selected event.
    #[must_use]
    pub fn event(&self, event_id: EventId) -> Option<&SelectedEvidence> {
        self.entries.iter().find(|entry| entry.event_id == event_id)
    }
    /// Looks up one selected ordinary artifact.
    #[must_use]
    pub fn artifact(&self, digest: ArtifactDigest) -> Option<&SelectedArtifact> {
        self.artifacts
            .binary_search_by_key(&digest, SelectedArtifact::digest)
            .ok()
            .map(|index| &self.artifacts[index])
    }

    #[cfg(test)]
    pub(crate) fn testing_empty(query_digest: Sha256Digest) -> Self {
        let mut manifest = Self {
            id: SelectionManifestId::new([1; 16]).expect("nonzero test manifest identity"),
            digest: Sha256Digest::new([0; 32]),
            query_digest,
            subjects: Vec::new(),
            entries: Vec::new(),
            artifacts: Vec::new(),
            counts: SelectionCounts::new(0, 0, 0, 0, 0, 0),
            canonical_bytes: Vec::new(),
        };
        manifest.canonical_bytes = super::canonical::encode_manifest(&manifest);
        manifest.digest =
            crate::identity::domain_digest(MANIFEST_DIGEST_DOMAIN, &manifest.canonical_bytes);
        manifest.id = SelectionManifestId::derive(MANIFEST_ID_DOMAIN, manifest.digest.as_bytes())
            .expect("digest-derived test manifest identity");
        manifest
    }
}
