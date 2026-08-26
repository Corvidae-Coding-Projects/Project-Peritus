//! Durable bounded diagnostics for unsuccessful materializations.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::{EventId, Sha256Digest};

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationPlanId, MaterializationRecovery,
};

const FAILURE_DOMAIN: &[u8] = b"peritus.harness.materialization-failure.v1\0";

/// Closed settled materialization failure vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterializationFailureCode {
    /// A required finalized artifact was unavailable.
    ArtifactUnavailable,
    /// Artifact bytes disagreed with committed metadata.
    ArtifactMismatch,
    /// The target workspace was stale.
    StaleWorkspace,
    /// Target-owned authorization was rejected.
    AuthorizationRejected,
    /// The checked patch failed with proven rollback.
    PatchRejected,
    /// Git candidate creation failed.
    CandidateRejected,
    /// C1 could not finalize the candidate manifest.
    ManifestFinalization,
    /// C1 state became indeterminate and requires reconciliation.
    Indeterminate,
    /// Observations conflict with the committed plan.
    Conflict,
}

/// Durable bounded diagnostic for one settled or quarantined materialization attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationFailure {
    plan_id: MaterializationPlanId,
    plan_digest: Sha256Digest,
    code: MaterializationFailureCode,
    diagnostic_digest: Sha256Digest,
    observed_at_millis: u64,
    causal_event_id: EventId,
}

impl MaterializationFailure {
    /// Constructs an exact failure record without retaining unbounded diagnostic text.
    #[must_use]
    pub const fn new(
        plan_id: MaterializationPlanId,
        plan_digest: Sha256Digest,
        code: MaterializationFailureCode,
        diagnostic_digest: Sha256Digest,
        observed_at_millis: u64,
        causal_event_id: EventId,
    ) -> Self {
        Self { plan_id, plan_digest, code, diagnostic_digest, observed_at_millis, causal_event_id }
    }

    /// Returns the affected plan.
    #[must_use]
    pub const fn plan_id(&self) -> MaterializationPlanId {
        self.plan_id
    }
    /// Returns the exact plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> MaterializationFailureCode {
        self.code
    }
    /// Returns the digest of bounded external diagnostics.
    #[must_use]
    pub const fn diagnostic_digest(&self) -> Sha256Digest {
        self.diagnostic_digest
    }
    /// Returns the observation time.
    #[must_use]
    pub const fn observed_at_millis(&self) -> u64 {
        self.observed_at_millis
    }
    /// Returns the causal event identity.
    #[must_use]
    pub const fn causal_event_id(&self) -> EventId {
        self.causal_event_id
    }

    /// Returns canonical failure bytes.
    ///
    /// # Errors
    /// Returns a codec failure when configured E1 bounds cannot represent this record.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, MaterializationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(FAILURE_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.plan_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.plan_digest.as_bytes()).map_err(codec)?;
        writer.write_u8(failure_tag(self.code)).map_err(codec)?;
        writer.write_fixed(self.diagnostic_digest.as_bytes()).map_err(codec)?;
        writer.write_u64(self.observed_at_millis).map_err(codec)?;
        writer.write_fixed(self.causal_event_id.as_bytes()).map_err(codec)?;
        Ok(writer.into_bytes())
    }

    /// Decodes exact canonical failure bytes.
    ///
    /// # Errors
    /// Rejects malformed, unknown-tag, or trailing bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, MaterializationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_fixed::<43>().map_err(codec)?.as_slice() != FAILURE_DOMAIN {
            return Err(invalid("materialization failure domain separator differs"));
        }
        let value = Self {
            plan_id: MaterializationPlanId::decode(reader.read_fixed().map_err(codec)?)?,
            plan_digest: Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            code: decode_failure(reader.read_u8().map_err(codec)?)?,
            diagnostic_digest: Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            observed_at_millis: reader.read_u64().map_err(codec)?,
            causal_event_id: EventId::new(reader.read_fixed().map_err(codec)?)
                .map_err(|_| invalid("failure causal event identity is zero"))?,
        };
        reader.finish().map_err(codec)?;
        Ok(value)
    }
}

const fn failure_tag(code: MaterializationFailureCode) -> u8 {
    match code {
        MaterializationFailureCode::ArtifactUnavailable => 1,
        MaterializationFailureCode::ArtifactMismatch => 2,
        MaterializationFailureCode::StaleWorkspace => 3,
        MaterializationFailureCode::AuthorizationRejected => 4,
        MaterializationFailureCode::PatchRejected => 5,
        MaterializationFailureCode::CandidateRejected => 6,
        MaterializationFailureCode::ManifestFinalization => 7,
        MaterializationFailureCode::Indeterminate => 8,
        MaterializationFailureCode::Conflict => 9,
    }
}

fn decode_failure(tag: u8) -> Result<MaterializationFailureCode, MaterializationError> {
    match tag {
        1 => Ok(MaterializationFailureCode::ArtifactUnavailable),
        2 => Ok(MaterializationFailureCode::ArtifactMismatch),
        3 => Ok(MaterializationFailureCode::StaleWorkspace),
        4 => Ok(MaterializationFailureCode::AuthorizationRejected),
        5 => Ok(MaterializationFailureCode::PatchRejected),
        6 => Ok(MaterializationFailureCode::CandidateRejected),
        7 => Ok(MaterializationFailureCode::ManifestFinalization),
        8 => Ok(MaterializationFailureCode::Indeterminate),
        9 => Ok(MaterializationFailureCode::Conflict),
        _ => Err(invalid("unknown materialization failure code")),
    }
}

fn codec(error: peritus_codec::CodecError) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Codec,
        MaterializationRecovery::Quarantine,
        error.to_string(),
    )
}

fn invalid(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Receipt,
        MaterializationRecovery::Quarantine,
        detail,
    )
}
