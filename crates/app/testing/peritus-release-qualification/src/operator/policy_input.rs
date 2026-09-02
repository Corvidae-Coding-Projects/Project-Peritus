//! Deterministic projection from authenticated H4 inputs into verified release policy.

use peritus_release_artifacts::{ArtifactInventory, ReleaseBinding, Sha256Digest, digest_bytes};
use peritus_release_policy::{
    Architecture, CandidateId, EvidenceBinding, EvidenceObservation, GitCommitId, OperatingSystem,
    PlatformIdentity, PlatformMatrix, PrincipalId, QualificationObservation, QualificationSlice,
    QualificationVerdict as PolicyQualificationVerdict, REQUIRED_EVIDENCE, ReleaseCandidate,
    ReleaseEvidence, ReleaseVersion, ReviewId, ReviewObservation, ReviewOutcome, SchemaIdentity,
    ToolchainIdentity,
};
use peritus_types::Sha256Digest as PolicyDigest;

use crate::{
    CriterionEvidenceMap, EvidenceKind, EvidenceManifest, EvidenceReference, FinalAudit,
    VerifiedPolicyBinding, VerifiedReleasePolicyAdapter,
};

use super::{OperatorError, admission::EvidenceStore, binding::decode_hex};

pub(super) fn assemble(
    binding: &ReleaseBinding,
    inventory: &ArtifactInventory,
    manifest: &EvidenceManifest,
    criteria: &CriterionEvidenceMap,
    final_audit: &FinalAudit,
    evidence: &EvidenceStore,
    evaluated_at: u64,
) -> Result<VerifiedReleasePolicyAdapter, OperatorError> {
    if evaluated_at == 0 {
        return Err(OperatorError::integrity("policy evaluated_at must be positive"));
    }
    let candidate = candidate(binding)?;
    let source_revision = candidate.source_revision();
    let observations = REQUIRED_EVIDENCE
        .iter()
        .enumerate()
        .map(|(index, requirement)| {
            let reference = criterion_reference(criteria, requirement.criterion().stable_id())?;
            EvidenceObservation::new(
                *requirement,
                requirement.source_kind(),
                policy_binding(&candidate, evaluated_at, sequence(index, 1), source_revision)?,
                policy_digest(reference.payload_digest()),
                policy_digest(reference.signature_digest()),
                true,
                true,
            )
            .map_err(policy_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let qualifications = qualifications(&candidate, evaluated_at, source_revision, evidence)?;
    let reviews = reviews(&candidate, evaluated_at, source_revision, evidence, final_audit)?;
    let aggregate =
        ReleaseEvidence::new(observations, qualifications, reviews, Vec::new(), Vec::new())
            .map_err(policy_error)?;
    let verified_binding = VerifiedPolicyBinding::new(
        binding.clone(),
        inventory.digest()?,
        manifest.digest()?,
        criteria.digest()?,
        final_audit.digest(),
    );
    Ok(VerifiedReleasePolicyAdapter::new(verified_binding, candidate, evaluated_at, aggregate))
}

fn candidate(binding: &ReleaseBinding) -> Result<ReleaseCandidate, OperatorError> {
    let binding_digest = binding.digest()?;
    let mut candidate_id = [0_u8; 16];
    candidate_id.copy_from_slice(&binding_digest.as_bytes()[..16]);
    let commit = match binding.candidate_commit().as_str().len() {
        40 => GitCommitId::sha1(decode_hex::<20>(binding.candidate_commit().as_str().as_bytes())?),
        64 => {
            GitCommitId::sha256(decode_hex::<32>(binding.candidate_commit().as_str().as_bytes())?)
        }
        _ => return Err(OperatorError::integrity("unsupported Git object identity")),
    }
    .map_err(policy_error)?;
    let (major, minor, patch) = version_components(binding.version().as_str())?;
    let version = ReleaseVersion::new(
        major,
        minor,
        patch,
        policy_digest(digest_bytes(binding.version().as_str().as_bytes())),
    )
    .map_err(policy_error)?;
    let platform = binding.platform().as_str().as_bytes();
    let matrix = PlatformMatrix::new(
        PlatformIdentity::new(
            OperatingSystem::Linux,
            Architecture::X86_64,
            domain_digest(b"linux-x86_64", platform),
        )
        .map_err(policy_error)?,
        PlatformIdentity::new(
            OperatingSystem::MacOs,
            Architecture::Aarch64,
            domain_digest(b"macos-aarch64", platform),
        )
        .map_err(policy_error)?,
        PlatformIdentity::new(
            OperatingSystem::Windows,
            Architecture::X86_64,
            domain_digest(b"windows-x86_64", platform),
        )
        .map_err(policy_error)?,
    )
    .map_err(policy_error)?;
    let toolchain = binding.toolchain().as_str().as_bytes();
    let source_revision = source_revision(binding.source_tree_digest());
    ReleaseCandidate::new(
        CandidateId::new(candidate_id).map_err(policy_error)?,
        commit,
        version,
        matrix,
        ToolchainIdentity::new(
            domain_digest(b"rust", toolchain),
            domain_digest(b"verus", toolchain),
            domain_digest(b"vstd", toolchain),
            domain_digest(b"solver", toolchain),
        )
        .map_err(policy_error)?,
        peritus_release_policy::ProfileIdentity::new(
            source_revision,
            domain_digest(b"qualification-profile", binding_digest.as_bytes()),
        )
        .map_err(policy_error)?,
        SchemaIdentity::new(
            1,
            1,
            1,
            1,
            domain_digest(b"release-policy-catalog-v1", binding_digest.as_bytes()),
        )
        .map_err(policy_error)?,
        source_revision,
        policy_digest(binding_digest),
    )
    .map_err(policy_error)
}

fn qualifications(
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    source_revision: u64,
    evidence: &EvidenceStore,
) -> Result<Vec<QualificationObservation>, OperatorError> {
    let h0 = evidence.unique_kind(EvidenceKind::H0SecurityReport)?.evidence_reference();
    let h1 = evidence.unique_kind(EvidenceKind::H1ResilienceReport)?.evidence_reference();
    let h3 = evidence.unique_kind(EvidenceKind::H3PerformanceReport)?.evidence_reference();
    let h2_linux = evidence.unique_kind(EvidenceKind::H2LinuxReport)?.evidence_reference();
    let h2_macos = evidence.unique_kind(EvidenceKind::H2MacosReport)?.evidence_reference();
    let h2_windows = evidence.unique_kind(EvidenceKind::H2WindowsReport)?.evidence_reference();
    let h2_payload = aggregate_three(
        h2_linux.payload_digest(),
        h2_macos.payload_digest(),
        h2_windows.payload_digest(),
    );
    let h2_signatures = aggregate_three(
        h2_linux.signature_digest(),
        h2_macos.signature_digest(),
        h2_windows.signature_digest(),
    );
    let inputs = [
        (QualificationSlice::H0Security, h0.payload_digest(), h0.signature_digest(), h0),
        (QualificationSlice::H1Resilience, h1.payload_digest(), h1.signature_digest(), h1),
        (QualificationSlice::H2Platform, h2_payload, h2_signatures, h2_linux),
        (QualificationSlice::H3Performance, h3.payload_digest(), h3.signature_digest(), h3),
    ];
    inputs
        .into_iter()
        .enumerate()
        .map(|(index, (slice, report, signature, signer))| {
            QualificationObservation::new(
                slice,
                policy_binding(candidate, evaluated_at, sequence(index, 100), source_revision)?,
                PolicyQualificationVerdict::Ready,
                policy_digest(report),
                policy_digest(signature),
                principal(signer.signature_digest()),
                true,
            )
            .map_err(policy_error)
        })
        .collect()
}

fn reviews(
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    source_revision: u64,
    evidence: &EvidenceStore,
    audit: &FinalAudit,
) -> Result<Vec<ReviewObservation>, OperatorError> {
    let producer = principal(
        evidence
            .unique_kind(EvidenceKind::ArtifactInventory)?
            .evidence_reference()
            .signature_digest(),
    );
    let references = [
        evidence.unique_kind(EvidenceKind::H0SecurityReport)?.evidence_reference(),
        audit.evidence_reference(),
    ];
    references
        .into_iter()
        .enumerate()
        .map(|(index, reference)| {
            let reviewer = principal(reference.signature_digest());
            ReviewObservation::new(
                review_id(reference.envelope_digest(), index)?,
                policy_binding(candidate, evaluated_at, sequence(index, 200), source_revision)?,
                reviewer,
                producer,
                policy_digest(reference.envelope_digest()),
                policy_digest(reference.payload_digest()),
                ReviewOutcome::Approved,
                reviewer != producer,
            )
            .map_err(policy_error)
        })
        .collect()
}

fn criterion_reference(
    criteria: &CriterionEvidenceMap,
    number: u8,
) -> Result<&EvidenceReference, OperatorError> {
    criteria
        .mappings()
        .iter()
        .find(|mapping| mapping.criterion().number() == number)
        .and_then(|mapping| mapping.evidence().first())
        .ok_or_else(|| OperatorError::integrity("policy requirement lacks mapped H4 evidence"))
}

fn policy_binding(
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    sequence: u64,
    source_revision: u64,
) -> Result<EvidenceBinding, OperatorError> {
    EvidenceBinding::new(*candidate, evaluated_at, evaluated_at, sequence, source_revision)
        .map_err(policy_error)
}

fn version_components(value: &str) -> Result<(u16, u16, u16), OperatorError> {
    let without_build = value.split_once('+').map_or(value, |(left, _)| left);
    let core = without_build.split_once('-').map_or(without_build, |(left, _)| left);
    let mut values = core.split('.').map(str::parse::<u16>);
    let major = values.next().transpose().map_err(|_| OperatorError::integrity("version"))?;
    let minor = values.next().transpose().map_err(|_| OperatorError::integrity("version"))?;
    let patch = values.next().transpose().map_err(|_| OperatorError::integrity("version"))?;
    match (major, minor, patch, values.next()) {
        (Some(major), Some(minor), Some(patch), None) => Ok((major, minor, patch)),
        _ => Err(OperatorError::integrity("release version must have three numeric components")),
    }
}

fn domain_digest(domain: &[u8], value: &[u8]) -> PolicyDigest {
    let mut bytes = Vec::with_capacity(domain.len() + value.len() + 1);
    bytes.extend_from_slice(domain);
    bytes.push(0);
    bytes.extend_from_slice(value);
    policy_digest(digest_bytes(&bytes))
}

fn source_revision(digest: Sha256Digest) -> u64 {
    u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("fixed digest prefix")).max(1)
}

fn principal(digest: Sha256Digest) -> PrincipalId {
    let bytes = digest.as_bytes()[..16].try_into().expect("fixed digest prefix");
    PrincipalId::new(bytes).expect("nonzero admitted signature digest")
}

fn review_id(digest: Sha256Digest, index: usize) -> Result<ReviewId, OperatorError> {
    let derived =
        domain_digest(if index == 0 { b"h0-review" } else { b"final-audit" }, digest.as_bytes());
    ReviewId::new(derived.as_bytes()[..16].try_into().expect("fixed digest prefix"))
        .map_err(policy_error)
}

fn aggregate_three(left: Sha256Digest, middle: Sha256Digest, right: Sha256Digest) -> Sha256Digest {
    let mut bytes = [0_u8; 96];
    bytes[..32].copy_from_slice(left.as_bytes());
    bytes[32..64].copy_from_slice(middle.as_bytes());
    bytes[64..].copy_from_slice(right.as_bytes());
    digest_bytes(&bytes)
}

fn sequence(index: usize, base: u64) -> u64 {
    base + u64::try_from(index).expect("bounded catalog index")
}

const fn policy_digest(digest: Sha256Digest) -> PolicyDigest {
    PolicyDigest::new(*digest.as_bytes())
}

fn policy_error(error: peritus_release_policy::ConstructionError) -> OperatorError {
    OperatorError::integrity(format!("release policy input rejected: {}", error.code()))
}
