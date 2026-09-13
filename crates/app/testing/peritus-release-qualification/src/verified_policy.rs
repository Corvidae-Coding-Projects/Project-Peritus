//! Composition adapter from authenticated H4 evidence into the verified release policy.

use peritus_release_artifacts::{ReleaseBinding, Sha256Digest, digest_bytes};
use peritus_release_policy::{
    QualificationSlice, ReleaseCandidate, ReleaseEvidence, ReleaseVerdict, evaluate_release,
};

use crate::{
    DeterministicReleasePolicy, EvidenceDisposition, EvidenceKind, PolicyDecision, PolicyFailure,
    ReleasePolicyInput,
};

/// Exact H4 digests admitted before the verified policy may run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPolicyBinding {
    release: ReleaseBinding,
    artifact_inventory_digest: Sha256Digest,
    evidence_manifest_digest: Sha256Digest,
    criterion_map_digest: Sha256Digest,
    final_audit_digest: Sha256Digest,
}

impl VerifiedPolicyBinding {
    /// Creates the complete binding checked at the C-class/V-class seam.
    #[must_use]
    pub const fn new(
        release: ReleaseBinding,
        artifact_inventory_digest: Sha256Digest,
        evidence_manifest_digest: Sha256Digest,
        criterion_map_digest: Sha256Digest,
        final_audit_digest: Sha256Digest,
    ) -> Self {
        Self {
            release,
            artifact_inventory_digest,
            evidence_manifest_digest,
            criterion_map_digest,
            final_audit_digest,
        }
    }

    /// Returns the exact candidate binding expected from H4 collection.
    #[must_use]
    pub const fn release(&self) -> &ReleaseBinding {
        &self.release
    }
}

/// Fail-closed bridge into [`peritus_release_policy`].
///
/// The bridge owns already-admitted verified-policy observations. It does not manufacture
/// evidence. Before evaluation, every owned artifact and qualification observation must link back
/// to signature-bound H4 references, and the C-class release binding must digest to the exact
/// manifest identity carried by the V-class candidate.
pub struct VerifiedReleasePolicyAdapter {
    binding: VerifiedPolicyBinding,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: ReleaseEvidence,
}

impl VerifiedReleasePolicyAdapter {
    /// Creates a composition adapter over previously authenticated policy evidence.
    ///
    /// The constructor deliberately accepts no signer, publisher, tag, upload, or deployment
    /// authority. Candidate correspondence and observation linkage are checked again on every
    /// evaluation because H4 policy input is supplied only after the final audit succeeds.
    #[must_use]
    pub const fn new(
        binding: VerifiedPolicyBinding,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
        evidence: ReleaseEvidence,
    ) -> Self {
        Self { binding, candidate, evaluated_at, evidence }
    }

    fn validate_input(&self, input: &ReleasePolicyInput) -> Result<(), &'static str> {
        if input.binding() != &self.binding.release
            || input.artifact_inventory_digest() != self.binding.artifact_inventory_digest
            || input.evidence_manifest_digest() != self.binding.evidence_manifest_digest
            || input.criterion_map_digest() != self.binding.criterion_map_digest
            || input.final_audit_digest() != self.binding.final_audit_digest
        {
            return Err("release-policy.input-binding-mismatch");
        }
        if !self.candidate_corresponds() {
            return Err("release-policy.candidate-correspondence-failed");
        }
        if !self.artifact_observations_are_linked(input) {
            return Err("release-policy.artifact-link-failed");
        }
        if !self.qualification_observations_are_linked(input) {
            return Err("release-policy.qualification-link-failed");
        }
        Ok(())
    }

    fn candidate_corresponds(&self) -> bool {
        let Ok(binding_digest) = self.binding.release.digest() else {
            return false;
        };
        if binding_digest.as_bytes() != self.candidate.manifest_digest().as_bytes() {
            return false;
        }
        if !commit_matches(&self.binding.release, &self.candidate) {
            return false;
        }
        version_matches(&self.binding.release, &self.candidate)
    }

    fn artifact_observations_are_linked(&self, input: &ReleasePolicyInput) -> bool {
        self.evidence.observations().iter().all(|observation| {
            let requirement = observation.requirement();
            input
                .criteria()
                .iter()
                .find(|criterion| {
                    criterion.criterion().number() == requirement.criterion().stable_id()
                })
                .is_some_and(|criterion| {
                    criterion.evidence().iter().any(|reference| {
                        reference.disposition() == EvidenceDisposition::Satisfied
                            && reference.payload_digest().as_bytes()
                                == observation.artifact_digest().as_bytes()
                            && reference.signature_digest().as_bytes()
                                == observation.attestation_digest().as_bytes()
                    })
                })
        })
    }

    fn qualification_observations_are_linked(&self, input: &ReleasePolicyInput) -> bool {
        self.evidence.qualifications().iter().all(|observation| {
            let references =
                input.criteria().iter().flat_map(crate::PolicyCriterionInput::evidence);
            match observation.slice() {
                QualificationSlice::H0Security => references
                    .filter(|reference| reference.kind() == EvidenceKind::H0SecurityReport)
                    .any(|reference| qualification_digest_matches(reference, observation)),
                QualificationSlice::H1Resilience => references
                    .filter(|reference| reference.kind() == EvidenceKind::H1ResilienceReport)
                    .any(|reference| qualification_digest_matches(reference, observation)),
                QualificationSlice::H2Platform => h2_aggregate_matches(input, observation),
                QualificationSlice::H3Performance => references
                    .filter(|reference| reference.kind() == EvidenceKind::H3PerformanceReport)
                    .any(|reference| qualification_digest_matches(reference, observation)),
            }
        })
    }
}

impl DeterministicReleasePolicy for VerifiedReleasePolicyAdapter {
    fn evaluate(&self, input: &ReleasePolicyInput) -> PolicyDecision {
        if let Err(code) = self.validate_input(input) {
            return PolicyDecision::Unavailable { failure: PolicyFailure::known(code) };
        }
        let decision = evaluate_release(self.candidate, self.evaluated_at, &self.evidence);
        if decision.verdict() == ReleaseVerdict::Ready {
            PolicyDecision::Ready
        } else {
            PolicyDecision::NotReady {
                failures: vec![PolicyFailure::known("release-policy.not-ready")],
            }
        }
    }
}

fn qualification_digest_matches(
    reference: &crate::EvidenceReference,
    observation: &peritus_release_policy::QualificationObservation,
) -> bool {
    reference.disposition() == EvidenceDisposition::Satisfied
        && reference.payload_digest().as_bytes() == observation.report_digest().as_bytes()
        && reference.signature_digest().as_bytes() == observation.signature_digest().as_bytes()
}

fn h2_aggregate_matches(
    input: &ReleasePolicyInput,
    observation: &peritus_release_policy::QualificationObservation,
) -> bool {
    let linux = unique_reference(input, EvidenceKind::H2LinuxReport);
    let macos = unique_reference(input, EvidenceKind::H2MacosReport);
    let windows = unique_reference(input, EvidenceKind::H2WindowsReport);
    let (Some(linux), Some(macos), Some(windows)) = (linux, macos, windows) else {
        return false;
    };
    if [linux, macos, windows]
        .iter()
        .any(|reference| reference.disposition() != EvidenceDisposition::Satisfied)
    {
        return false;
    }
    let payload =
        aggregate_three(linux.payload_digest(), macos.payload_digest(), windows.payload_digest());
    let signatures = aggregate_three(
        linux.signature_digest(),
        macos.signature_digest(),
        windows.signature_digest(),
    );
    payload.as_bytes() == observation.report_digest().as_bytes()
        && signatures.as_bytes() == observation.signature_digest().as_bytes()
}

fn unique_reference(
    input: &ReleasePolicyInput,
    kind: EvidenceKind,
) -> Option<&crate::EvidenceReference> {
    let mut matches = input
        .criteria()
        .iter()
        .flat_map(crate::PolicyCriterionInput::evidence)
        .filter(|reference| reference.kind() == kind);
    let first = matches.next()?;
    if matches.any(|reference| reference != first) { None } else { Some(first) }
}

fn aggregate_three(left: Sha256Digest, middle: Sha256Digest, right: Sha256Digest) -> Sha256Digest {
    let mut bytes = [0_u8; 96];
    bytes[..32].copy_from_slice(left.as_bytes());
    bytes[32..64].copy_from_slice(middle.as_bytes());
    bytes[64..].copy_from_slice(right.as_bytes());
    digest_bytes(&bytes)
}

fn commit_matches(binding: &ReleaseBinding, candidate: &ReleaseCandidate) -> bool {
    let expected = binding.candidate_commit().as_str().as_bytes();
    match candidate.commit().format() {
        peritus_release_policy::GitObjectFormat::Sha1 => {
            candidate.commit().sha1_bytes().is_some_and(|bytes| hex_matches(expected, &bytes))
        }
        peritus_release_policy::GitObjectFormat::Sha256 => {
            candidate.commit().sha256_bytes().is_some_and(|bytes| hex_matches(expected, &bytes))
        }
    }
}

fn hex_matches(expected: &[u8], bytes: &[u8]) -> bool {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    expected.len() == bytes.len() * 2
        && bytes.iter().enumerate().all(|(index, byte)| {
            expected[index * 2] == HEX[usize::from(byte >> 4)]
                && expected[index * 2 + 1] == HEX[usize::from(byte & 0x0f)]
        })
}

fn version_matches(binding: &ReleaseBinding, candidate: &ReleaseCandidate) -> bool {
    let canonical = binding.version().as_str();
    let without_build = canonical.split_once('+').map_or(canonical, |(left, _)| left);
    let core = without_build.split_once('-').map_or(without_build, |(left, _)| left);
    let mut components = core.split('.');
    let Some(major) = components.next().and_then(|value| value.parse::<u16>().ok()) else {
        return false;
    };
    let Some(minor) = components.next().and_then(|value| value.parse::<u16>().ok()) else {
        return false;
    };
    let Some(patch) = components.next().and_then(|value| value.parse::<u16>().ok()) else {
        return false;
    };
    if components.next().is_some() {
        return false;
    }
    let policy_version = candidate.version();
    let descriptor = digest_bytes(canonical.as_bytes());
    major == policy_version.major()
        && minor == policy_version.minor()
        && patch == policy_version.patch()
        && descriptor.as_bytes() == policy_version.descriptor_digest().as_bytes()
}

#[cfg(test)]
mod tests;
