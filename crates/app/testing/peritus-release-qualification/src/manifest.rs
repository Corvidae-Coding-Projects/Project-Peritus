//! Content-addressed final evidence manifest.

use serde::Serialize;

use peritus_release_artifacts::{ReleaseBinding, ReleasePath, Sha256Digest, digest_bytes};

use crate::{EvidenceKind, EvidenceReference, QualificationError, QualificationErrorCode};

/// Maximum evidence entries in one H4 manifest.
pub const MAX_MANIFEST_ENTRIES: usize = 4_096;

/// Closed role for each required H4 evidence-manifest entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceManifestRole {
    /// Signed H0 report.
    H0SecurityReport,
    /// Signed H1 report.
    H1ResilienceReport,
    /// Signed native Linux H2 report.
    H2LinuxReport,
    /// Signed native macOS H2 report.
    H2MacosReport,
    /// Signed native Windows H2 report.
    H2WindowsReport,
    /// Signed H3 report.
    H3PerformanceReport,
    /// Gate A evidence.
    GateA,
    /// Foundation evidence.
    Foundation,
    /// Native Linux matrix.
    NativeLinux,
    /// Native macOS matrix.
    NativeMacos,
    /// Native Windows matrix.
    NativeWindows,
    /// Long-duration soak.
    Soak,
    /// Representative Rust campaign.
    RepresentativeRust,
    /// Representative TypeScript campaign.
    RepresentativeTypeScript,
    /// Representative Python campaign.
    RepresentativePython,
    /// Representative Java campaign.
    RepresentativeJava,
    /// Representative mixed-repository campaign.
    RepresentativeMixed,
    /// Artifact inventory.
    ArtifactInventory,
    /// SPDX SBOM.
    SpdxSbom,
    /// Provenance statement.
    Provenance,
    /// Detached signature bundle.
    ArtifactSignatures,
    /// Independent-builder comparison.
    Reproducibility,
    /// Migration and recovery documentation evidence.
    MigrationRecovery,
    /// License notice evidence.
    LicenseNotices,
    /// Complete AC-01 through AC-25 evidence map.
    CriterionMap,
    /// Independent final audit.
    FinalAudit,
}

impl EvidenceManifestRole {
    /// Returns every required role in stable order.
    #[must_use]
    pub const fn required() -> [Self; 26] {
        [
            Self::H0SecurityReport,
            Self::H1ResilienceReport,
            Self::H2LinuxReport,
            Self::H2MacosReport,
            Self::H2WindowsReport,
            Self::H3PerformanceReport,
            Self::GateA,
            Self::Foundation,
            Self::NativeLinux,
            Self::NativeMacos,
            Self::NativeWindows,
            Self::Soak,
            Self::RepresentativeRust,
            Self::RepresentativeTypeScript,
            Self::RepresentativePython,
            Self::RepresentativeJava,
            Self::RepresentativeMixed,
            Self::ArtifactInventory,
            Self::SpdxSbom,
            Self::Provenance,
            Self::ArtifactSignatures,
            Self::Reproducibility,
            Self::MigrationRecovery,
            Self::LicenseNotices,
            Self::CriterionMap,
            Self::FinalAudit,
        ]
    }

    /// Returns the signed evidence kind required for this role.
    #[must_use]
    pub const fn evidence_kind(self) -> EvidenceKind {
        match self {
            Self::H0SecurityReport => EvidenceKind::H0SecurityReport,
            Self::H1ResilienceReport => EvidenceKind::H1ResilienceReport,
            Self::H2LinuxReport => EvidenceKind::H2LinuxReport,
            Self::H2MacosReport => EvidenceKind::H2MacosReport,
            Self::H2WindowsReport => EvidenceKind::H2WindowsReport,
            Self::H3PerformanceReport => EvidenceKind::H3PerformanceReport,
            Self::GateA => EvidenceKind::GateA,
            Self::Foundation => EvidenceKind::Foundation,
            Self::NativeLinux => EvidenceKind::NativeLinux,
            Self::NativeMacos => EvidenceKind::NativeMacos,
            Self::NativeWindows => EvidenceKind::NativeWindows,
            Self::Soak => EvidenceKind::Soak,
            Self::RepresentativeRust => EvidenceKind::RepresentativeRust,
            Self::RepresentativeTypeScript => EvidenceKind::RepresentativeTypeScript,
            Self::RepresentativePython => EvidenceKind::RepresentativePython,
            Self::RepresentativeJava => EvidenceKind::RepresentativeJava,
            Self::RepresentativeMixed => EvidenceKind::RepresentativeMixed,
            Self::ArtifactInventory => EvidenceKind::ArtifactInventory,
            Self::SpdxSbom => EvidenceKind::SpdxSbom,
            Self::Provenance => EvidenceKind::Provenance,
            Self::ArtifactSignatures => EvidenceKind::ArtifactSignatures,
            Self::Reproducibility => EvidenceKind::Reproducibility,
            Self::MigrationRecovery => EvidenceKind::MigrationRecovery,
            Self::LicenseNotices => EvidenceKind::LicenseNotices,
            Self::CriterionMap => EvidenceKind::CriterionMap,
            Self::FinalAudit => EvidenceKind::FinalAudit,
        }
    }
}

/// One content-addressed, signature-authenticated manifest entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceManifestEntry {
    role: EvidenceManifestRole,
    path: ReleasePath,
    byte_length: u64,
    payload_digest: Sha256Digest,
    envelope_digest: Sha256Digest,
}

impl EvidenceManifestEntry {
    /// Creates a manifest entry from a verified evidence reference.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the declared role does not match the signed kind.
    pub fn from_reference(
        role: EvidenceManifestRole,
        reference: &EvidenceReference,
    ) -> Result<Self, QualificationError> {
        if role.evidence_kind() != reference.kind() {
            return Err(QualificationError::new(
                QualificationErrorCode::BindingMismatch,
                "create evidence manifest entry",
                "manifest role does not match signed evidence kind",
            ));
        }
        Ok(Self {
            role,
            path: reference.path().clone(),
            byte_length: reference.byte_length(),
            payload_digest: reference.payload_digest(),
            envelope_digest: reference.envelope_digest(),
        })
    }

    /// Returns the manifest role.
    #[must_use]
    pub const fn role(&self) -> EvidenceManifestRole {
        self.role
    }

    /// Returns the retained evidence path.
    #[must_use]
    pub const fn path(&self) -> &ReleasePath {
        &self.path
    }

    /// Returns the exact payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
}

/// Canonically ordered, content-addressed H4 evidence manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceManifest {
    schema_version: u32,
    binding: ReleaseBinding,
    entries: Vec<EvidenceManifestEntry>,
}

impl EvidenceManifest {
    /// Creates a path-stable manifest without treating missing roles as success.
    ///
    /// Completeness is checked by [`Self::is_complete`] and final report reduction.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for no entries, excessive entries, duplicate roles, or
    /// duplicate paths.
    pub fn new(
        binding: ReleaseBinding,
        mut entries: Vec<EvidenceManifestEntry>,
    ) -> Result<Self, QualificationError> {
        if entries.is_empty() || entries.len() > MAX_MANIFEST_ENTRIES {
            return Err(QualificationError::new(
                QualificationErrorCode::BoundExceeded,
                "create H4 evidence manifest",
                "manifest must contain 1 through 4096 entries",
            ));
        }
        entries.sort_by(|left, right| left.role.cmp(&right.role).then(left.path.cmp(&right.path)));
        if entries.windows(2).any(|pair| pair[0].role == pair[1].role) {
            return Err(QualificationError::new(
                QualificationErrorCode::Duplicate,
                "create H4 evidence manifest",
                "manifest repeats a required evidence role",
            ));
        }
        let mut paths = entries.iter().map(|entry| &entry.path).collect::<Vec<_>>();
        paths.sort();
        if paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(QualificationError::new(
                QualificationErrorCode::Duplicate,
                "create H4 evidence manifest",
                "manifest repeats a retained evidence path",
            ));
        }
        Ok(Self { schema_version: 1, binding, entries })
    }

    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns entries in canonical role order.
    #[must_use]
    pub fn entries(&self) -> &[EvidenceManifestEntry] {
        &self.entries
    }

    /// Returns whether each required evidence role occurs exactly once.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        EvidenceManifestRole::required()
            .into_iter()
            .all(|role| self.entries.iter().filter(|entry| entry.role == role).count() == 1)
    }

    /// Returns whether the manifest contains an exact verified evidence reference.
    #[must_use]
    pub fn contains_reference(&self, reference: &EvidenceReference) -> bool {
        self.entries.iter().any(|entry| {
            entry.role.evidence_kind() == reference.kind()
                && entry.path == *reference.path()
                && entry.byte_length == reference.byte_length()
                && entry.payload_digest == reference.payload_digest()
                && entry.envelope_digest == reference.envelope_digest()
        })
    }

    /// Returns the digest reviewed by the independent auditor before its own entry is attached.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn pre_audit_digest(&self) -> Result<Sha256Digest, QualificationError> {
        #[derive(Serialize)]
        struct PreAudit<'a> {
            schema_version: u32,
            binding: &'a ReleaseBinding,
            entries: Vec<&'a EvidenceManifestEntry>,
        }
        let entries = self
            .entries
            .iter()
            .filter(|entry| entry.role != EvidenceManifestRole::FinalAudit)
            .collect();
        serde_json::to_vec(&PreAudit {
            schema_version: self.schema_version,
            binding: &self.binding,
            entries,
        })
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|source| {
            QualificationError::serialization("serialize pre-audit evidence set", source)
        })
    }

    /// Serializes deterministic compact manifest JSON.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        serde_json::to_vec(self)
            .map_err(|source| QualificationError::serialization("serialize H4 manifest", source))
    }

    /// Returns the content identity of the final manifest, including final audit evidence.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, QualificationError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }
}
