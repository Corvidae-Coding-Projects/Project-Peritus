//! Real F0 publication adapter for the exact claimed durable directive.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_evidence::EvidenceStore;
use peritus_evolution::{EvolutionError, EvolutionPublication, EvolutionPublicationClaim};
use peritus_journal::SqliteJournal;
use peritus_types::EventId;

use super::{identity, incomplete, owner_error};

/// Publishes an existing actual F0 evidence bundle and acknowledges only its exact claim fence.
///
/// # Errors
/// Rejects absent, oversized, corrupt, or unreferenced artifacts and stale claim fences.
pub fn publish_evolution_claim(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    evidence: &mut EvidenceStore,
    claim: &EvolutionPublicationClaim,
    maximum_bytes: u64,
) -> Result<EvolutionPublication, EvolutionError> {
    let directive = claim.directive();
    let digest = ArtifactDigest::from_sha256(directive.artifact_digest());
    let bytes = artifacts.read(digest, maximum_bytes).map_err(owner_error)?;
    let event = EventId::new(identity(&directive.canonical_bytes())).map_err(owner_error)?;
    let finalized = peritus_evolution::finalize_evolution_artifact(
        artifacts,
        &bytes,
        directive.evidence_digest(),
        event,
    )?;
    if finalized.artifact_digest() != digest {
        return Err(incomplete("F0 publication artifact changed during verification"));
    }
    peritus_evolution::publish_claimed_evolution(journal, evidence, artifacts, claim, finalized)
}
