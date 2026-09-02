//! Signed evidence envelopes bound to an exact H4 candidate.

use serde::{Deserialize, Serialize};

use peritus_release_artifacts::{
    ArtifactError, BoundedId, Ed25519PublicKey, Ed25519Signature, ReleaseBinding, ReleasePath,
    Sha256Digest, VerifiedSignature, digest_bytes, verify_detached_ed25519,
};

use crate::{QualificationError, QualificationErrorCode};

/// Signed result asserted by the producer of an evidence record.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceDisposition {
    /// The recorded check or evidence contract was satisfied.
    Satisfied,
    /// The recorded check failed, was incomplete, or did not satisfy its contract.
    NotSatisfied,
}

/// Public key and detached signature supplied by an external evidence signer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSignature {
    key_id: BoundedId,
    public_key: Ed25519PublicKey,
    signature: Ed25519Signature,
}

impl EvidenceSignature {
    /// Groups public verification material without accepting a private key.
    #[must_use]
    pub const fn new(
        key_id: BoundedId,
        public_key: Ed25519PublicKey,
        signature: Ed25519Signature,
    ) -> Self {
        Self { key_id, public_key, signature }
    }
}

/// Closed H4 evidence category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    /// Signed H0 security qualification report.
    H0SecurityReport,
    /// Signed H1 resilience qualification report.
    H1ResilienceReport,
    /// Signed H2 native Linux qualification report.
    H2LinuxReport,
    /// Signed H2 native macOS qualification report.
    H2MacosReport,
    /// Signed H2 native Windows qualification report.
    H2WindowsReport,
    /// Signed H3 performance qualification report.
    H3PerformanceReport,
    /// Complete candidate-bound Gate A result.
    GateA,
    /// Complete locked Foundation result.
    Foundation,
    /// Native Linux matrix result.
    NativeLinux,
    /// Native macOS matrix result.
    NativeMacos,
    /// Native Windows matrix result.
    NativeWindows,
    /// Required long-duration soak result.
    Soak,
    /// Representative Rust writer/reviewer/fixer campaign.
    RepresentativeRust,
    /// Representative TypeScript writer/reviewer/fixer campaign.
    RepresentativeTypeScript,
    /// Representative Python writer/reviewer/fixer campaign.
    RepresentativePython,
    /// Representative Java writer/reviewer/fixer campaign.
    RepresentativeJava,
    /// Representative mixed-repository writer/reviewer/fixer campaign.
    RepresentativeMixed,
    /// Canonical artifact inventory.
    ArtifactInventory,
    /// SPDX 2.3 software bill of materials.
    SpdxSbom,
    /// SLSA-style provenance statement.
    Provenance,
    /// Detached artifact signature bundle.
    ArtifactSignatures,
    /// Independent-builder reproducibility comparison.
    Reproducibility,
    /// Migration, backup, restore, and rollback evidence inventory.
    MigrationRecovery,
    /// Complete project and third-party license notices.
    LicenseNotices,
    /// Complete AC-01 through AC-25 evidence map.
    CriterionMap,
    /// Independent final audit.
    FinalAudit,
}

impl EvidenceKind {
    /// Returns the closed fresh-subject campaign catalog in execution order.
    #[must_use]
    pub const fn fresh_subject_campaigns() -> [Self; 11] {
        [
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
        ]
    }

    /// Returns every signed input required before policy evaluation.
    #[must_use]
    pub const fn required_signed_inputs() -> [Self; 25] {
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
        ]
    }
}

/// Verified reference to externally retained signed evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceReference {
    kind: EvidenceKind,
    disposition: EvidenceDisposition,
    path: ReleasePath,
    byte_length: u64,
    payload_digest: Sha256Digest,
    envelope_digest: Sha256Digest,
    signer_key_id: BoundedId,
    signature_digest: Sha256Digest,
}

impl EvidenceReference {
    /// Returns the evidence category.
    #[must_use]
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }

    /// Returns the signature-bound producer disposition.
    #[must_use]
    pub const fn disposition(&self) -> EvidenceDisposition {
        self.disposition
    }

    /// Returns the retained release-relative path.
    #[must_use]
    pub const fn path(&self) -> &ReleasePath {
        &self.path
    }

    /// Returns the exact payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }

    /// Returns the digest of the candidate-bound signature envelope.
    #[must_use]
    pub const fn envelope_digest(&self) -> Sha256Digest {
        self.envelope_digest
    }

    /// Returns the external evidence byte length.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the stable signer key identity.
    #[must_use]
    pub const fn signer_key_id(&self) -> &BoundedId {
        &self.signer_key_id
    }

    /// Returns the detached signature digest.
    #[must_use]
    pub const fn signature_digest(&self) -> Sha256Digest {
        self.signature_digest
    }
}

/// Signed, candidate-bound observation of externally retained evidence bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SignedEvidenceRecord {
    binding: ReleaseBinding,
    reference: EvidenceReference,
    signature: VerifiedSignature,
}

impl SignedEvidenceRecord {
    /// Verifies a detached signature over the canonical binding envelope for exact payload bytes.
    ///
    /// The detached signature authenticates the envelope, which includes the complete release
    /// binding, evidence category, retained path, byte length, and payload SHA-256.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the length cannot be represented, envelope
    /// serialization fails, or the detached signature is invalid.
    pub fn verify(
        binding: ReleaseBinding,
        kind: EvidenceKind,
        disposition: EvidenceDisposition,
        path: ReleasePath,
        payload: &[u8],
        signature: EvidenceSignature,
    ) -> Result<Self, QualificationError> {
        let byte_length = payload_length(payload)?;
        let payload_digest = digest_bytes(payload);
        let envelope_bytes =
            canonical_evidence_signature_envelope(&binding, kind, disposition, &path, payload)?;
        let verified = verify_detached_ed25519(
            signature.key_id.clone(),
            signature.public_key,
            signature.signature,
            &envelope_bytes,
        )
        .map_err(|error| signature_error(&error))?;
        let reference = EvidenceReference {
            kind,
            disposition,
            path,
            byte_length,
            payload_digest,
            envelope_digest: digest_bytes(&envelope_bytes),
            signer_key_id: signature.key_id,
            signature_digest: verified.signature_digest(),
        };
        Ok(Self { binding, reference, signature: verified })
    }

    /// Returns the exact candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns the verified evidence reference.
    #[must_use]
    pub const fn evidence_reference(&self) -> &EvidenceReference {
        &self.reference
    }

    /// Returns the successful public-signature observation.
    #[must_use]
    pub const fn signature(&self) -> &VerifiedSignature {
        &self.signature
    }
}

/// Produces the exact domain-separated envelope bytes an external signer must authenticate.
///
/// This function performs no signing and accepts no private key material.
///
/// # Errors
///
/// Returns [`QualificationError`] when payload length cannot be represented or canonical JSON
/// serialization fails.
pub fn canonical_evidence_signature_envelope(
    binding: &ReleaseBinding,
    kind: EvidenceKind,
    disposition: EvidenceDisposition,
    path: &ReleasePath,
    payload: &[u8],
) -> Result<Vec<u8>, QualificationError> {
    let envelope = EvidenceEnvelope {
        domain: "peritus/h4-signed-evidence/v1",
        binding,
        kind,
        disposition,
        path,
        byte_length: payload_length(payload)?,
        payload_digest: digest_bytes(payload),
    };
    serde_json::to_vec(&envelope).map_err(|source| {
        QualificationError::serialization("serialize signed evidence envelope", source)
    })
}

#[derive(Serialize)]
struct EvidenceEnvelope<'a> {
    domain: &'static str,
    binding: &'a ReleaseBinding,
    kind: EvidenceKind,
    disposition: EvidenceDisposition,
    path: &'a ReleasePath,
    byte_length: u64,
    payload_digest: Sha256Digest,
}

fn payload_length(payload: &[u8]) -> Result<u64, QualificationError> {
    u64::try_from(payload.len()).map_err(|_| {
        QualificationError::new(
            QualificationErrorCode::BoundExceeded,
            "verify signed evidence",
            "payload length cannot be represented",
        )
    })
}

fn signature_error(error: &ArtifactError) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Integrity,
        "verify signed evidence",
        error.to_string(),
    )
}
