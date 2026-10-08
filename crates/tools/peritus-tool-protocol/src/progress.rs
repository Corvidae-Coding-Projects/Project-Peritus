//! Ordered bounded progress envelope.

use crate::{
    BoundedJson, BoundedText, CanonicalEnvelope, JsonLimits, PreparedToolCall, ProgressContract,
    ProtocolError, ProtocolErrorKind,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, Sha256Digest};

/// Closed progress classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressKind {
    /// The implementation accepted the invocation.
    Started,
    /// Structured output or state advanced.
    Update,
    /// A control was applied.
    Control,
    /// Cancellation or deadline handling began.
    Stopping,
    /// Recovery observation was performed.
    Recovery,
}

/// One invocation-bound progress event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolProgress {
    action_id: ActionId,
    prepared_digest: Sha256Digest,
    encoding_version: u16,
    json_limits: JsonLimits,
    sequence: u64,
    kind: ProgressKind,
    observed_at: AuthorityInstant,
    structured: Option<BoundedJson>,
    model_rendering: BoundedText,
}

impl ToolProgress {
    /// Creates one bounded progress event.
    ///
    /// # Errors
    ///
    /// Rejects legacy sequences outside the call's lifetime ceiling. Paged calls validate their
    /// physical page size at the execution-update boundary instead of limiting their lifetime.
    pub fn new(
        prepared: &PreparedToolCall,
        sequence: impl TryInto<u64>,
        kind: ProgressKind,
        observed_at: AuthorityInstant,
        structured: Option<BoundedJson>,
        model_rendering: BoundedText,
    ) -> Result<Self, ProtocolError> {
        let sequence = sequence.try_into().map_err(|_| {
            ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "progress.sequence",
                "progress sequence is not representable as an unsigned frontier",
            )
        })?;
        if let Some(value) = structured.as_ref() {
            value.validate_limits(prepared.call().limits().json_limits())?;
        }
        if (prepared.call().limits().progress_contract() == ProgressContract::LifetimeV1
            && sequence >= u64::from(prepared.call().limits().progress_events()))
            || model_rendering.as_str().len() > prepared.call().limits().model_bytes() as usize
            || observed_at.epoch() != prepared.call().deadline().epoch()
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "progress",
                "progress sequence, rendering, or authority epoch exceeds the call envelope",
            ));
        }
        Ok(Self {
            action_id: prepared.call().action_id(),
            prepared_digest: prepared.prepared_digest(),
            encoding_version: prepared.call().limits().protocol_version(),
            json_limits: prepared.call().limits().json_limits(),
            sequence,
            kind,
            observed_at,
            structured,
            model_rendering,
        })
    }

    /// Returns the producing action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns the zero-based sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns the progress contract which selected this envelope's canonical encoding.
    #[must_use]
    pub const fn progress_contract(&self) -> ProgressContract {
        match self.encoding_version {
            1 => ProgressContract::LifetimeV1,
            _ => ProgressContract::PagedV2,
        }
    }
    /// Returns the structured JSON frame contract carried by this envelope.
    #[must_use]
    pub const fn json_limits(&self) -> JsonLimits {
        self.json_limits
    }
    /// Returns the closed progress kind.
    #[must_use]
    pub const fn kind(&self) -> ProgressKind {
        self.kind
    }
    /// Returns the observation instant.
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
    /// Borrows optional structured progress.
    #[must_use]
    pub const fn structured(&self) -> Option<&BoundedJson> {
        self.structured.as_ref()
    }
    /// Borrows the bounded model rendering.
    #[must_use]
    pub const fn model_rendering(&self) -> &BoundedText {
        &self.model_rendering
    }

    /// Returns canonical progress-envelope bytes in the admitted progress-contract version.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let contract = self.encoding_version;
        let mut bytes = crate::wire::begin_version(4, contract);
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.prepared_digest.as_bytes());
        if contract >= 3 {
            bytes.extend_from_slice(&self.json_limits.canonical_bytes());
        }
        if contract == 1 {
            crate::wire::u32_value(&mut bytes, u32::try_from(self.sequence).unwrap_or(u32::MAX));
        } else {
            crate::wire::u64_value(&mut bytes, self.sequence);
        }
        bytes.push(match self.kind {
            ProgressKind::Started => 1,
            ProgressKind::Update => 2,
            ProgressKind::Control => 3,
            ProgressKind::Stopping => 4,
            ProgressKind::Recovery => 5,
        });
        crate::wire::instant(&mut bytes, self.observed_at);
        match &self.structured {
            Some(value) => {
                bytes.push(1);
                crate::wire::bytes(&mut bytes, value.canonical_bytes());
            }
            None => bytes.push(0),
        }
        crate::wire::text(&mut bytes, self.model_rendering.as_str());
        bytes
    }

    /// Decodes an exact durable progress envelope.
    ///
    /// Page-chain and prepared-call identity validation remain the durable reader's responsibility.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let envelope = CanonicalEnvelope::parse(bytes, bytes.len().max(1))?;
        if envelope.family() != 4 {
            return Err(progress_decode_error("canonical progress family is invalid"));
        }
        let encoding_version = envelope.version();
        let mut reader = ProgressReader::new(envelope.payload());
        let action_id = ActionId::new(reader.array::<16>()?)
            .map_err(|_| progress_decode_error("canonical progress action is invalid"))?;
        let prepared_digest = Sha256Digest::new(reader.array::<32>()?);
        let json_limits = if encoding_version >= 3 {
            JsonLimits::from_wire(reader.u64()?, reader.u64()?, reader.u64()?, reader.u64()?)?
        } else {
            JsonLimits::PRODUCTION
        };
        let sequence = if encoding_version == 1 {
            u64::from(reader.u32()?)
        } else {
            reader.u64()?
        };
        let kind = match reader.byte()? {
            1 => ProgressKind::Started,
            2 => ProgressKind::Update,
            3 => ProgressKind::Control,
            4 => ProgressKind::Stopping,
            5 => ProgressKind::Recovery,
            _ => return Err(progress_decode_error("canonical progress kind is invalid")),
        };
        let epoch = peritus_types::Generation::new(reader.u64()?)
            .map_err(|_| progress_decode_error("canonical progress epoch is invalid"))?;
        let observed_at = AuthorityInstant::new(epoch, reader.u64()?);
        let structured = match reader.byte()? {
            0 => None,
            1 => {
                let json = reader.length_prefixed()?;
                let json = core::str::from_utf8(json)
                    .map_err(|_| progress_decode_error("canonical progress JSON is not UTF-8"))?;
                Some(BoundedJson::parse(json, json_limits)?)
            }
            _ => return Err(progress_decode_error("canonical progress JSON tag is invalid")),
        };
        let rendering = reader.length_prefixed()?;
        let rendering = core::str::from_utf8(rendering)
            .map_err(|_| progress_decode_error("canonical progress rendering is not UTF-8"))?;
        let model_rendering = BoundedText::new(rendering.to_owned())?;
        if !reader.is_empty() {
            return Err(progress_decode_error("canonical progress has trailing bytes"));
        }
        Ok(Self {
            action_id,
            prepared_digest,
            encoding_version,
            json_limits,
            sequence,
            kind,
            observed_at,
            structured,
            model_rendering,
        })
    }
}

struct ProgressReader<'a> {
    remaining: &'a [u8],
}

impl<'a> ProgressReader<'a> {
    const fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProtocolError> {
        self.take(N)?.try_into().map_err(|_| progress_decode_error("canonical progress is truncated"))
    }

    fn byte(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn length_prefixed(&mut self) -> Result<&'a [u8], ProtocolError> {
        let length = usize::try_from(self.u64()?)
            .map_err(|_| progress_decode_error("canonical progress length exceeds this host"))?;
        self.take(length)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        if self.remaining.len() < length {
            return Err(progress_decode_error("canonical progress is truncated"));
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}

fn progress_decode_error(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEnvelope, "progress", detail)
}
