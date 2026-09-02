//! Independent final-audit reconstruction and signature admission.

use std::path::Path;

use peritus_release_artifacts::{
    BoundedId, Ed25519PublicKey, Ed25519Signature, ReleaseBinding, ReleasePath,
};

use crate::{
    AuditDraft, AuditFinding, EvidenceManifest, EvidenceManifestEntry, EvidenceManifestRole,
    EvidenceSignature, FinalAudit, FindingDisposition, FindingId, FindingSeverity, ParticipantId,
};

use super::{
    OperatorError,
    admission::EvidenceStore,
    files,
    plan::{AuditDispositionSpec, AuditSeverity, AuditSpec},
};

pub(super) struct AuditInputs {
    pub final_audit: FinalAudit,
    pub manifest: EvidenceManifest,
}

pub(super) fn assemble(
    binding: &ReleaseBinding,
    evidence_root: &Path,
    evidence: &EvidenceStore,
    spec: &AuditSpec,
) -> Result<AuditInputs, OperatorError> {
    let pre_audit = manifest(binding, evidence, None)?;
    let reviewed_digest = pre_audit.pre_audit_digest()?;
    let findings = spec
        .findings
        .iter()
        .map(|finding| {
            let disposition = match &finding.disposition {
                AuditDispositionSpec::Open => FindingDisposition::Open,
                AuditDispositionSpec::Closed { evidence: selector } => FindingDisposition::Closed {
                    closure_evidence: evidence.record(selector)?.evidence_reference().clone(),
                },
                AuditDispositionSpec::RiskAccepted { evidence: selector } => {
                    FindingDisposition::RiskAccepted {
                        approval_evidence: evidence.record(selector)?.evidence_reference().clone(),
                    }
                }
            };
            AuditFinding::new(
                FindingId::new(&finding.id)?,
                severity(finding.severity),
                &finding.summary,
                disposition,
            )
            .map_err(OperatorError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let draft = AuditDraft::new(
        binding.clone(),
        ParticipantId::new(&spec.auditor)?,
        spec.contributors.iter().map(ParticipantId::new).collect::<Result<Vec<_>, _>>()?,
        reviewed_digest,
        findings,
    )?;
    let retained_path = ReleasePath::new(&spec.path)?;
    let retained = files::read_rooted(evidence_root, &retained_path, "final audit")?;
    if retained != draft.canonical_json()? {
        return Err(OperatorError::integrity(
            "retained final audit differs from the canonical reconstructed draft",
        ));
    }
    let public_path = ReleasePath::new(&spec.public_key_path)?;
    let signature_path = ReleasePath::new(&spec.signature_path)?;
    let final_audit = FinalAudit::verify(
        draft,
        retained_path,
        EvidenceSignature::new(
            BoundedId::new(&spec.key_id)?,
            Ed25519PublicKey::from_bytes(files::read_rooted_material::<32>(
                evidence_root,
                &public_path,
                "final-audit public key",
            )?),
            Ed25519Signature::from_bytes(files::read_rooted_material::<64>(
                evidence_root,
                &signature_path,
                "final-audit signature",
            )?),
        ),
    )?;
    let manifest = manifest(binding, evidence, Some(&final_audit))?;
    Ok(AuditInputs { final_audit, manifest })
}

fn manifest(
    binding: &ReleaseBinding,
    evidence: &EvidenceStore,
    final_audit: Option<&FinalAudit>,
) -> Result<EvidenceManifest, OperatorError> {
    let mut entries = evidence
        .records()
        .map(|record| {
            let reference = record.evidence_reference();
            EvidenceManifestEntry::from_reference(role(reference.kind()), reference)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(audit) = final_audit {
        entries.push(EvidenceManifestEntry::from_reference(
            EvidenceManifestRole::FinalAudit,
            audit.evidence_reference(),
        )?);
    }
    EvidenceManifest::new(binding.clone(), entries).map_err(OperatorError::from)
}

const fn role(kind: crate::EvidenceKind) -> EvidenceManifestRole {
    use crate::{EvidenceKind as Kind, EvidenceManifestRole as Role};
    match kind {
        Kind::H0SecurityReport => Role::H0SecurityReport,
        Kind::H1ResilienceReport => Role::H1ResilienceReport,
        Kind::H2LinuxReport => Role::H2LinuxReport,
        Kind::H2MacosReport => Role::H2MacosReport,
        Kind::H2WindowsReport => Role::H2WindowsReport,
        Kind::H3PerformanceReport => Role::H3PerformanceReport,
        Kind::GateA => Role::GateA,
        Kind::Foundation => Role::Foundation,
        Kind::NativeLinux => Role::NativeLinux,
        Kind::NativeMacos => Role::NativeMacos,
        Kind::NativeWindows => Role::NativeWindows,
        Kind::Soak => Role::Soak,
        Kind::RepresentativeRust => Role::RepresentativeRust,
        Kind::RepresentativeTypeScript => Role::RepresentativeTypeScript,
        Kind::RepresentativePython => Role::RepresentativePython,
        Kind::RepresentativeJava => Role::RepresentativeJava,
        Kind::RepresentativeMixed => Role::RepresentativeMixed,
        Kind::ArtifactInventory => Role::ArtifactInventory,
        Kind::SpdxSbom => Role::SpdxSbom,
        Kind::Provenance => Role::Provenance,
        Kind::ArtifactSignatures => Role::ArtifactSignatures,
        Kind::Reproducibility => Role::Reproducibility,
        Kind::MigrationRecovery => Role::MigrationRecovery,
        Kind::LicenseNotices => Role::LicenseNotices,
        Kind::CriterionMap => Role::CriterionMap,
        Kind::FinalAudit => Role::FinalAudit,
    }
}

const fn severity(value: AuditSeverity) -> FindingSeverity {
    match value {
        AuditSeverity::Informational => FindingSeverity::Informational,
        AuditSeverity::Low => FindingSeverity::Low,
        AuditSeverity::Medium => FindingSeverity::Medium,
        AuditSeverity::High => FindingSeverity::High,
        AuditSeverity::Critical => FindingSeverity::Critical,
    }
}
