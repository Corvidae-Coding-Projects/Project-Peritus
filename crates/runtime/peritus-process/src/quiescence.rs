//! Exact holder-quiescence inspection over the complete durable registry.

use core::fmt;

use peritus_leases::{HolderQuiescenceEvidence, LeaseClaim, ReconciliationCorrelation};
use peritus_types::{EvidenceId, ProcessId, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{LifecyclePhase, ProcessStore, verified::holder_quiescence_exact};

/// Precise reason the durable registry cannot establish holder quiescence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuiescenceBlocker {
    /// The requested correlation does not exactly match the supplied claim.
    CorrelationMismatch,
    /// A matching process has not reached a terminal phase.
    LiveProcess(ProcessId),
    /// A matching terminal record lacks complete tree or support-task cleanup.
    UnresolvedProcess(ProcessId),
    /// A record for the correlated holder contains a different exact lease claim.
    ClaimMismatch(ProcessId),
    /// The complete durable registry frontier cannot currently be inspected.
    RegistryUnavailable,
}

impl fmt::Display for QuiescenceBlocker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CorrelationMismatch => {
                formatter.write_str("lease correlation differs from claim")
            }
            Self::LiveProcess(_) => formatter.write_str("a correlated process remains live"),
            Self::UnresolvedProcess(_) => formatter.write_str("a correlated process is unresolved"),
            Self::ClaimMismatch(_) => {
                formatter.write_str("a correlated process has a different claim")
            }
            Self::RegistryUnavailable => formatter.write_str("durable process registry is unavailable"),
        }
    }
}

impl std::error::Error for QuiescenceBlocker {}

/// Successful complete-registry holder-quiescence observation with C2 provenance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HolderQuiescenceObservation {
    correlation: ReconciliationCorrelation,
    claim: LeaseClaim,
    evidence_id: EvidenceId,
    inspected_records: u64,
    provenance_digest: Sha256Digest,
}

impl HolderQuiescenceObservation {
    /// Returns the exact fenced-holder correlation inspected by C2.
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation {
        self.correlation
    }

    /// Returns the exact claim whose durable ownership records were scanned.
    #[must_use]
    pub const fn claim(self) -> LeaseClaim {
        self.claim
    }

    /// Returns the caller-selected evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }

    /// Returns the number of durable manifests examined in the complete scan.
    #[must_use]
    pub const fn inspected_records(self) -> u64 {
        self.inspected_records
    }

    /// Returns the digest binding the correlation, claim, evidence, and scan count.
    #[must_use]
    pub const fn provenance_digest(self) -> Sha256Digest {
        self.provenance_digest
    }

    /// Projects the exact B1 evidence only after successful C2 inspection.
    #[must_use]
    pub const fn evidence(self) -> HolderQuiescenceEvidence {
        HolderQuiescenceEvidence::new(self.claim, self.evidence_id)
    }
}

impl ProcessStore {
    /// Inspects the complete decoded registry for one exact fenced lease holder.
    ///
    /// A successful observation proves every matching process has a durably observed quiescent
    /// tree and joined support tasks. The command outcome may remain explicitly unknown after
    /// restart; holder quiescence concerns resource ownership rather than result knowledge. The
    /// method does not infer quiescence from a phase label, process absence, or a partial index.
    ///
    /// # Errors
    ///
    /// Returns the exact first correlation, live-process, claim, or cleanup blocker.
    pub fn inspect_holder_quiescence(
        &self,
        correlation: ReconciliationCorrelation,
        claim: LeaseClaim,
        evidence_id: EvidenceId,
    ) -> Result<HolderQuiescenceObservation, QuiescenceBlocker> {
        let claim_generation = claim.generation();
        if correlation.scope() != claim.scope()
            || correlation.fenced_generation() != claim_generation
            || correlation.prior_holder() != claim.holder()
        {
            return Err(QuiescenceBlocker::CorrelationMismatch);
        }

        if !self.quarantined_records().is_empty() {
            return Err(QuiescenceBlocker::RegistryUnavailable);
        }

        let mut inspected_records = 0_u64;
        let mut blocker = None;
        self.visit_registry(|record| {
            let Some(manifest) = record.manifest else {
                blocker.get_or_insert(QuiescenceBlocker::UnresolvedProcess(record.process_id));
                return Ok(());
            };
            inspected_records = inspected_records.checked_add(1).ok_or_else(|| {
                crate::ProcessError::new(crate::ErrorCode::Persistence, crate::ProcessOperation::Reconcile,
                    crate::RecoveryClass::ReopenAndReconcile, "quiescence record count overflow")
            })?;
            let Some(ownership) = manifest.lease else { return Ok(()); };
            let same_correlation = ownership.workspace_id() == correlation.scope().workspace_id()
                && ownership.resource_id() == correlation.scope().resource_id()
                && ownership.environment_id() == correlation.scope().environment_id()
                && ownership.actor_id() == correlation.prior_holder().actor_id()
                && ownership.session_id() == correlation.prior_holder().session_id()
                && ownership.generation() == correlation.fenced_generation();
            if !same_correlation {
                return Ok(());
            }
            let process_id = manifest.identity.process_id();
            if !ownership.matches_claim(claim)
                || !record.claim.is_some_and(|claim| claim.matches_manifest(&manifest))
            {
                blocker.get_or_insert(QuiescenceBlocker::ClaimMismatch(process_id));
                return Ok(());
            }
            if matches!(
                manifest.phase,
                LifecyclePhase::Authorized
                    | LifecyclePhase::Starting
                    | LifecyclePhase::Running
                    | LifecyclePhase::Stopping
            ) {
                blocker.get_or_insert(QuiescenceBlocker::LiveProcess(process_id));
                return Ok(());
            }
            if !manifest.ownership_settled() {
                blocker.get_or_insert(QuiescenceBlocker::UnresolvedProcess(process_id));
            }
            Ok(())
        }).map_err(|_| QuiescenceBlocker::RegistryUnavailable)?;
        if let Some(blocker) = blocker { return Err(blocker); }
        if !holder_quiescence_exact(true, true, 0, 0, true, true) {
            return Err(QuiescenceBlocker::CorrelationMismatch);
        }
        Ok(HolderQuiescenceObservation {
            correlation,
            claim,
            evidence_id,
            inspected_records,
            provenance_digest: quiescence_digest(
                correlation,
                claim,
                evidence_id,
                inspected_records,
            ),
        })
    }
}

fn quiescence_digest(
    correlation: ReconciliationCorrelation,
    claim: LeaseClaim,
    evidence_id: EvidenceId,
    inspected_records: u64,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.process-holder-quiescence.v1\0");
    hasher.update(correlation.scope().workspace_id().as_bytes());
    hasher.update(correlation.scope().resource_id().as_bytes());
    hasher.update(correlation.scope().environment_id().as_bytes());
    hasher.update(correlation.fenced_generation().get().to_be_bytes());
    hasher.update(correlation.prior_holder().actor_id().as_bytes());
    hasher.update(correlation.prior_holder().session_id().as_bytes());
    hasher.update(claim.claim_version().get().to_be_bytes());
    hasher.update(claim.issued_at().epoch().get().to_be_bytes());
    hasher.update(claim.issued_at().tick_millis().to_be_bytes());
    hasher.update(claim.expires_at().epoch().get().to_be_bytes());
    hasher.update(claim.expires_at().tick_millis().to_be_bytes());
    hasher.update(evidence_id.as_bytes());
    hasher.update(inspected_records.to_be_bytes());
    Sha256Digest::new(hasher.finalize().into())
}
