//! Final H0 report and explicit non-authorizing readiness verdict.

use peritus_security_policy::{
    IndependentSecurityReview, SecurityDecision, SecurityVerdict, UnmetSecurityCondition,
    evaluate_security_readiness,
};
use peritus_types::Sha256Digest;

use crate::{
    CaseOutcome, EvidenceManifest, ProbeId, QualificationError, QualificationRun,
    policy_bridge::build_policy_evidence,
};

/// Stable reason H0 readiness was withheld.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NotReadyReason {
    /// A native probe did not execute, failed, was unsupported, exceeded bounds, or failed cleanup.
    Probe(ProbeId),
    /// The verified security policy reported one unmet obligation.
    Policy(UnmetSecurityCondition),
}

/// Compact evidence carried by an H0-ready result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReadinessEvidence {
    candidate_source_digest: Sha256Digest,
    evidence_manifest_digest: Sha256Digest,
    probe_count: usize,
}

impl ReadinessEvidence {
    /// Returns the exact qualified source-tree digest.
    #[must_use]
    pub const fn candidate_source_digest(self) -> Sha256Digest {
        self.candidate_source_digest
    }

    /// Returns the digest of canonical manifest JSON.
    #[must_use]
    pub const fn evidence_manifest_digest(self) -> Sha256Digest {
        self.evidence_manifest_digest
    }

    /// Returns the complete production probe count.
    #[must_use]
    pub const fn probe_count(self) -> usize {
        self.probe_count
    }
}

/// Final H0-only qualification disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadinessVerdict {
    /// Complete native campaign, cleanup, external review, findings, and policy all passed.
    Ready(ReadinessEvidence),
    /// One or more deterministic reasons withheld readiness.
    NotReady(Vec<NotReadyReason>),
}

/// Complete native run, canonical manifest, verified decision, and H0 verdict.
#[derive(Debug, Eq, PartialEq)]
pub struct QualificationReport {
    run: QualificationRun,
    manifest: EvidenceManifest,
    policy_decision: SecurityDecision,
    verdict: ReadinessVerdict,
}

impl QualificationReport {
    /// Reduces a native run and separately supplied external review.
    ///
    /// `None` is retained as missing review and is always not ready. This function never creates,
    /// substitutes, broadens the declared scope of, or completes an external security review.
    ///
    /// # Errors
    ///
    /// Returns canonical manifest or checked policy-evidence construction failures.
    pub fn evaluate(
        run: QualificationRun,
        review: Option<IndependentSecurityReview>,
    ) -> Result<Self, QualificationError> {
        let manifest = EvidenceManifest::new(&run, review.as_ref())?;
        let evidence = build_policy_evidence(&run, review)?;
        let policy_decision = evaluate_security_readiness(run.candidate(), &evidence);
        let mut reasons = run
            .cases()
            .iter()
            .filter(|case| case.outcome() != CaseOutcome::Passed)
            .map(|case| NotReadyReason::Probe(case.spec().id()))
            .collect::<Vec<_>>();
        reasons
            .extend(policy_decision.unmet_conditions().iter().copied().map(NotReadyReason::Policy));
        reasons.sort();
        reasons.dedup();
        let verdict = if reasons.is_empty()
            && policy_decision.verdict() == SecurityVerdict::Ready
            && run.all_passed()
        {
            ReadinessVerdict::Ready(ReadinessEvidence {
                candidate_source_digest: run.candidate().source_digest(),
                evidence_manifest_digest: manifest.digest(),
                probe_count: run.cases().len(),
            })
        } else {
            ReadinessVerdict::NotReady(reasons)
        };
        Ok(Self { run, manifest, policy_decision, verdict })
    }

    /// Borrows the complete native campaign.
    #[must_use]
    pub const fn run(&self) -> &QualificationRun {
        &self.run
    }

    /// Borrows canonical manifest bytes and digest.
    #[must_use]
    pub const fn manifest(&self) -> &EvidenceManifest {
        &self.manifest
    }

    /// Borrows the V-class deterministic policy decision.
    #[must_use]
    pub const fn policy_decision(&self) -> &SecurityDecision {
        &self.policy_decision
    }

    /// Borrows the H0-only final disposition.
    #[must_use]
    pub const fn verdict(&self) -> &ReadinessVerdict {
        &self.verdict
    }

    /// Reports H0 readiness without conferring H4 release authority.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self.verdict, ReadinessVerdict::Ready(_))
    }
}
