//! Exact revision freshness and explicit invalidation decisions.

use crate::{EvidenceInvalidation, EvidenceRecord};
use peritus_types::RevisionTuple;

/// First exact revision component that differs in stable tuple order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RevisionDrift {
    /// Acceptance specification identity changed.
    AcceptanceSpec,
    /// Harness identity changed.
    Harness,
    /// Workspace lineage identity changed.
    Workspace,
    /// Workspace fencing generation changed.
    WorkspaceGeneration,
    /// Immutable workspace revision changed.
    WorkspaceRevision,
    /// Policy identity changed.
    Policy,
    /// Provider-profile identity changed.
    ProviderProfile,
}

/// Currentness observation for immutable evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    /// Every revision component matches and no invalidation exists.
    Current,
    /// The exact named revision component drifted.
    RevisionStale(RevisionDrift),
    /// A later durable invalidation explicitly retired the evidence.
    Invalidated(EvidenceInvalidation),
}

/// Evaluates exact currentness without effects.
#[must_use]
pub fn evaluate_freshness(
    record: &EvidenceRecord,
    current: &RevisionTuple,
    invalidation: Option<EvidenceInvalidation>,
) -> Freshness {
    if let Some(invalidation) = invalidation.filter(|value| value.target() == record.id()) {
        return Freshness::Invalidated(invalidation);
    }
    if crate::verified::revisions_equal(record.revision(), current) {
        return Freshness::Current;
    }
    Freshness::RevisionStale(first_drift(record.revision(), current))
}

fn first_drift(stored: &RevisionTuple, current: &RevisionTuple) -> RevisionDrift {
    if stored.acceptance_spec_id() != current.acceptance_spec_id() {
        RevisionDrift::AcceptanceSpec
    } else if stored.harness_id() != current.harness_id() {
        RevisionDrift::Harness
    } else if stored.workspace_id() != current.workspace_id() {
        RevisionDrift::Workspace
    } else if stored.workspace_generation() != current.workspace_generation() {
        RevisionDrift::WorkspaceGeneration
    } else if stored.workspace_revision() != current.workspace_revision() {
        RevisionDrift::WorkspaceRevision
    } else if stored.policy_id() != current.policy_id() {
        RevisionDrift::Policy
    } else {
        RevisionDrift::ProviderProfile
    }
}

/// Computes the canonical digest journal events use to bind an exact evidence revision tuple.
#[must_use]
pub fn revision_digest(revision: &RevisionTuple) -> peritus_types::Sha256Digest {
    let mut bytes = Vec::with_capacity(112);
    crate::canonical::put_revision(&mut bytes, revision);
    peritus_codec::sha256(&bytes)
}
