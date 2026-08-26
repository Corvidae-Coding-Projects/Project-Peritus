//! Stable opaque C0 outbox directives and exact claim fences.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_journal::{OutboxId, OutboxMessage, OutboxState};
use peritus_types::Sha256Digest;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerJobId, DebuggerOperation, DebuggerRecovery,
    ModelAnalysisId, ReportRecord,
};

/// Destination for provider-neutral optional model-analysis effects.
pub const MODEL_ANALYSIS_DESTINATION: &str = "peritus.debugger.model-analysis.v1";
/// Destination for report artifact/evidence publication effects.
pub const PUBLICATION_DESTINATION: &str = "peritus.debugger.publish-report.v1";

const MODEL_DOMAIN: &[u8] = b"peritus.debugger.model-directive.v1\0";
const PUBLICATION_DOMAIN: &[u8] = b"peritus.debugger.publication-directive.v1\0";
const MODEL_ID_DOMAIN: &[u8] = b"peritus.debugger.model-outbox.v1\0";
const PUBLICATION_ID_DOMAIN: &[u8] = b"peritus.debugger.publication-outbox.v1\0";

/// Complete stable directive for one exact model attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelDirective {
    job_id: DebuggerJobId,
    model_id: ModelAnalysisId,
    attempt: u16,
    plan_digest: Sha256Digest,
    request_digest: Sha256Digest,
    not_before_tick: u64,
}

impl ModelDirective {
    /// Creates a stable model-attempt directive.
    ///
    /// # Errors
    /// Rejects the reserved attempt zero.
    pub fn new(
        job_id: DebuggerJobId,
        model_id: ModelAnalysisId,
        attempt: u16,
        plan_digest: Sha256Digest,
        request_digest: Sha256Digest,
        not_before_tick: u64,
    ) -> Result<Self, DebuggerError> {
        if attempt == 0 {
            return Err(invalid("model directive attempt is zero"));
        }
        Ok(Self { job_id, model_id, attempt, plan_digest, request_digest, not_before_tick })
    }

    /// Owning debugger job.
    #[must_use]
    pub const fn job_id(self) -> DebuggerJobId {
        self.job_id
    }
    /// Stable model-analysis identity.
    #[must_use]
    pub const fn model_id(self) -> ModelAnalysisId {
        self.model_id
    }
    /// One-based model attempt.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Frozen complete plan digest.
    #[must_use]
    pub const fn plan_digest(self) -> Sha256Digest {
        self.plan_digest
    }
    /// Frozen C5 request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Earliest caller monotonic tick eligible to run.
    #[must_use]
    pub const fn not_before_tick(self) -> u64 {
        self.not_before_tick
    }

    pub(crate) fn canonical_bytes(self) -> Result<Vec<u8>, DebuggerError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(MODEL_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.job_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.model_id.as_bytes()).map_err(codec)?;
        writer.write_u16(self.attempt).map_err(codec)?;
        writer.write_fixed(self.plan_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.request_digest.as_bytes()).map_err(codec)?;
        writer.write_u64(self.not_before_tick).map_err(codec)?;
        Ok(writer.into_bytes())
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, DebuggerError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != MODEL_DOMAIN {
            return Err(corrupt("unsupported model directive domain"));
        }
        let value = Self::new(
            DebuggerJobId::new(reader.read_fixed().map_err(codec)?)?,
            ModelAnalysisId::new(reader.read_fixed().map_err(codec)?)?,
            reader.read_u16().map_err(codec)?,
            Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            reader.read_u64().map_err(codec)?,
        )?;
        reader.finish().map_err(codec)?;
        Ok(value)
    }

    pub(crate) fn outbox_id(self) -> Result<OutboxId, DebuggerError> {
        derived_outbox_id(MODEL_ID_DOMAIN, self.model_id.as_bytes(), self.attempt)
    }
}

/// Complete stable directive for publishing one committed report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationDirective {
    job_id: DebuggerJobId,
    report: ReportRecord,
}

impl PublicationDirective {
    /// Creates a publication directive for exact committed report bytes.
    #[must_use]
    pub const fn new(job_id: DebuggerJobId, report: ReportRecord) -> Self {
        Self { job_id, report }
    }
    /// Owning debugger job.
    #[must_use]
    pub const fn job_id(self) -> DebuggerJobId {
        self.job_id
    }
    /// Exact committed report record.
    #[must_use]
    pub const fn report(self) -> ReportRecord {
        self.report
    }

    pub(crate) fn canonical_bytes(self) -> Result<Vec<u8>, DebuggerError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(PUBLICATION_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.job_id.as_bytes()).map_err(codec)?;
        crate::aggregate::encode_report(&mut writer, self.report)?;
        Ok(writer.into_bytes())
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, DebuggerError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != PUBLICATION_DOMAIN {
            return Err(corrupt("unsupported publication directive domain"));
        }
        let job_id = DebuggerJobId::new(reader.read_fixed().map_err(codec)?)?;
        let report = ReportRecord::new(
            crate::ReportId::new(reader.read_fixed().map_err(codec)?)?,
            Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            reader.read_u64().map_err(codec)?,
        )?;
        reader.finish().map_err(codec)?;
        Ok(Self { job_id, report })
    }

    pub(crate) fn outbox_id(self) -> Result<OutboxId, DebuggerError> {
        derived_outbox_id(PUBLICATION_ID_DOMAIN, self.report.id().as_bytes(), 0)
    }
}

/// Exact claimed model directive, including the current C0 fence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelDirectiveClaim {
    directive: ModelDirective,
    fence: u64,
}

impl ModelDirectiveClaim {
    /// Validates and captures one claimed C0 model directive.
    ///
    /// # Errors
    /// Rejects the wrong destination, payload, identity, lifecycle state, or absent fence.
    pub fn from_message(message: &OutboxMessage) -> Result<Self, DebuggerError> {
        ensure_claimed(message, MODEL_ANALYSIS_DESTINATION)?;
        let directive = ModelDirective::decode(message.payload())?;
        if message.id() != directive.outbox_id()? {
            return Err(binding("model outbox identity differs from its payload"));
        }
        Ok(Self {
            directive,
            fence: message
                .fence()
                .ok_or_else(|| binding("claimed model directive has no fence"))?,
        })
    }
    /// Exact directive.
    #[must_use]
    pub const fn directive(self) -> ModelDirective {
        self.directive
    }
    /// Exact positive C0 claim fence.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }
}

/// Exact claimed publication directive, including the current C0 fence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationDirectiveClaim {
    directive: PublicationDirective,
    fence: u64,
}

impl PublicationDirectiveClaim {
    /// Validates and captures one claimed C0 publication directive.
    ///
    /// # Errors
    /// Rejects the wrong destination, payload, identity, lifecycle state, or absent fence.
    pub fn from_message(message: &OutboxMessage) -> Result<Self, DebuggerError> {
        ensure_claimed(message, PUBLICATION_DESTINATION)?;
        let directive = PublicationDirective::decode(message.payload())?;
        if message.id() != directive.outbox_id()? {
            return Err(binding("publication outbox identity differs from its payload"));
        }
        Ok(Self {
            directive,
            fence: message
                .fence()
                .ok_or_else(|| binding("claimed publication directive has no fence"))?,
        })
    }
    /// Exact directive.
    #[must_use]
    pub const fn directive(self) -> PublicationDirective {
        self.directive
    }
    /// Exact positive C0 claim fence.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }
}

/// Either exact effect lane claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerDirectiveClaim {
    /// Optional model-analysis effect.
    Model(ModelDirectiveClaim),
    /// Report publication effect.
    Publication(PublicationDirectiveClaim),
}

impl DebuggerDirectiveClaim {
    pub(crate) fn id(self) -> Result<OutboxId, DebuggerError> {
        match self {
            Self::Model(value) => value.directive.outbox_id(),
            Self::Publication(value) => value.directive.outbox_id(),
        }
    }
    pub(crate) const fn fence(self) -> u64 {
        match self {
            Self::Model(value) => value.fence,
            Self::Publication(value) => value.fence,
        }
    }
}

impl From<ModelDirectiveClaim> for DebuggerDirectiveClaim {
    fn from(value: ModelDirectiveClaim) -> Self {
        Self::Model(value)
    }
}

impl From<PublicationDirectiveClaim> for DebuggerDirectiveClaim {
    fn from(value: PublicationDirectiveClaim) -> Self {
        Self::Publication(value)
    }
}

fn ensure_claimed(message: &OutboxMessage, destination: &str) -> Result<(), DebuggerError> {
    if message.state() != OutboxState::Claimed || message.destination() != destination {
        return Err(binding("outbox message is not an exact claimed debugger directive"));
    }
    Ok(())
}

fn derived_outbox_id(
    domain: &[u8],
    identity: &[u8; 16],
    attempt: u16,
) -> Result<OutboxId, DebuggerError> {
    let mut bytes = Vec::with_capacity(domain.len() + identity.len() + 2);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(identity);
    bytes.extend_from_slice(&attempt.to_be_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    OutboxId::new(id).map_err(journal)
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelProtocol,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}

fn invalid(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::InvalidInput,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}

fn corrupt(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        detail,
    )
}

fn binding(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::Quarantine,
        detail,
    )
}

fn journal(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Journal,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::ReplayAggregate,
        error.to_string(),
    )
}
