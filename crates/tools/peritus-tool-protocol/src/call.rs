//! Versioned bounded proposed tool-call envelope.

use crate::{
    BoundedJson, IdempotencyKey, JsonLimits, ProtocolError, ProtocolErrorKind, SemanticVersion,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, CapabilityName, RevisionTuple};

/// Meaning of the progress capacity carried by a call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressContract {
    /// Version-one calls use the value as a lifetime event ceiling.
    LifetimeV1,
    /// Version-two calls use the value as one physical observation-page capacity.
    PagedV2,
}

/// Per-call ceilings that can only narrow an immutable descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallLimits {
    timeout_millis: Option<u64>,
    output_bytes: Option<u64>,
    model_bytes: u32,
    human_bytes: u32,
    progress_events: u32,
    progress_contract: ProgressContract,
    artifacts: u16,
    json_limits: JsonLimits,
    protocol_version: u16,
}

impl CallLimits {
    /// Creates complete nonzero call ceilings.
    ///
    /// # Errors
    ///
    /// Rejects any zero ceiling.
    pub fn new(
        timeout_millis: u64,
        output_bytes: u64,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
    ) -> Result<Self, ProtocolError> {
        Self::with_optional_timeout(
            Some(timeout_millis),
            output_bytes,
            model_bytes,
            human_bytes,
            progress_events,
            artifacts,
        )
    }

    /// Creates resource ceilings with an optional caller-owned wall deadline.
    ///
    /// # Errors
    /// Rejects zero bounds; absence of a deadline is represented by `None`.
    pub fn with_optional_timeout(
        timeout_millis: Option<u64>,
        output_bytes: u64,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
    ) -> Result<Self, ProtocolError> {
        Self::with_optional_output(
            timeout_millis,
            Some(output_bytes),
            model_bytes,
            human_bytes,
            progress_events,
            artifacts,
        )
    }

    /// Creates call limits with optional wall-time and cumulative output ceilings.
    ///
    /// Rendering, event, and artifact counts remain selected physical capacities.
    ///
    /// # Errors
    /// Rejects selected zero ceilings or zero physical capacities.
    pub fn with_optional_output(
        timeout_millis: Option<u64>,
        output_bytes: Option<u64>,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
    ) -> Result<Self, ProtocolError> {
        if timeout_millis == Some(0)
            || output_bytes == Some(0)
            || model_bytes == 0
            || human_bytes == 0
            || progress_events == 0
            || artifacts == 0
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::CallLimit,
                "call.limits",
                "every call ceiling must be nonzero",
            ));
        }
        Ok(Self {
            timeout_millis,
            output_bytes,
            model_bytes,
            human_bytes,
            progress_events,
            progress_contract: ProgressContract::LifetimeV1,
            artifacts,
            json_limits: JsonLimits::PRODUCTION,
            protocol_version: 1,
        })
    }

    /// Selects version-two paged progress for newly admitted work.
    ///
    /// The existing progress value becomes a per-update physical page capacity. It never limits
    /// the lifetime amount of work or the number of pages an invocation may publish.
    #[must_use]
    pub const fn with_paged_progress(mut self) -> Self {
        self.progress_contract = ProgressContract::PagedV2;
        if self.protocol_version < 2 {
            self.protocol_version = 2;
        }
        self
    }

    /// Selects the version-three per-frame JSON contract.
    ///
    /// Version-three calls retain paged progress and carry these exact parser/writer limits in
    /// every canonical envelope that contains structured JSON.
    #[must_use]
    pub const fn with_json_limits(mut self, limits: JsonLimits) -> Self {
        self.progress_contract = ProgressContract::PagedV2;
        self.json_limits = limits;
        self.protocol_version = 3;
        self
    }

    /// Returns the wall-time ceiling.
    #[must_use]
    pub const fn timeout_millis(self) -> Option<u64> {
        self.timeout_millis
    }
    /// Returns the output ceiling, or zero when absent.
    ///
    /// Process tools count archived stream bytes independently of bounded terminal metadata.
    /// Other tools also count inline structured payload bytes.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        match self.output_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative stream/inline output ceiling.
    #[must_use]
    pub const fn output_limit(self) -> Option<u64> {
        self.output_bytes
    }
    /// Returns the model rendering ceiling.
    #[must_use]
    pub const fn model_bytes(self) -> u32 {
        self.model_bytes
    }
    /// Returns the human rendering ceiling.
    #[must_use]
    pub const fn human_bytes(self) -> u32 {
        self.human_bytes
    }
    /// Returns the progress capacity selected by [`Self::progress_contract`].
    #[must_use]
    pub const fn progress_events(self) -> u32 {
        self.progress_events
    }
    /// Returns whether progress is lifetime-bounded V1 or page-bounded V2.
    #[must_use]
    pub const fn progress_contract(self) -> ProgressContract {
        self.progress_contract
    }
    /// Returns the artifact-reference ceiling.
    #[must_use]
    pub const fn artifacts(self) -> u16 {
        self.artifacts
    }

    /// Returns the selected JSON frame contract.
    #[must_use]
    pub const fn json_limits(self) -> JsonLimits {
        self.json_limits
    }

    /// Returns the canonical protocol version selected by these limits.
    #[must_use]
    pub const fn protocol_version(self) -> u16 {
        self.protocol_version
    }

    pub(crate) const fn fits(self, descriptor: crate::ToolLimits) -> bool {
        self.protocol_version == descriptor.protocol_version()
            && self.json_limits.fits(descriptor.json_limits())
            && (match (self.timeout_millis, descriptor.timeout_millis()) {
            (Some(call), Some(maximum)) => call <= maximum,
            (_, None) => true,
            (None, Some(_)) => false,
        }) && allowance_fits(self.output_limit(), descriptor.output_limit())
            && self.model_bytes <= descriptor.model_bytes()
            && self.human_bytes <= descriptor.human_bytes()
            && self.progress_contract == descriptor.progress_contract()
            && self.progress_events <= descriptor.progress_events()
            && self.artifacts <= descriptor.artifacts()
    }

    pub(crate) fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(63);
        bytes.extend_from_slice(&self.timeout_millis.unwrap_or(0).to_be_bytes());
        bytes.extend_from_slice(&self.output_bytes().to_be_bytes());
        bytes.extend_from_slice(&self.model_bytes.to_be_bytes());
        bytes.extend_from_slice(&self.human_bytes.to_be_bytes());
        bytes.extend_from_slice(&self.progress_events.to_be_bytes());
        bytes.extend_from_slice(&self.artifacts.to_be_bytes());
        if self.progress_contract == ProgressContract::PagedV2 {
            bytes.push(2);
        }
        if self.protocol_version >= 3 {
            bytes.extend_from_slice(&self.json_limits.canonical_bytes());
        }
        bytes
    }
}

const fn allowance_fits(requested: Option<u64>, maximum: Option<u64>) -> bool {
    match (requested, maximum) {
        (_, None) => true,
        (Some(requested), Some(maximum)) => requested <= maximum,
        (None, Some(_)) => false,
    }
}

/// One untrusted model-proposed call, validated for structural bounds only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCall {
    action_id: ActionId,
    name: CapabilityName,
    version: SemanticVersion,
    arguments: BoundedJson,
    limits: CallLimits,
    revision: RevisionTuple,
    deadline: AuthorityInstant,
    idempotency_key: IdempotencyKey,
}

impl ToolCall {
    /// Creates a complete versioned call envelope.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        action_id: ActionId,
        name: CapabilityName,
        version: SemanticVersion,
        arguments: BoundedJson,
        limits: CallLimits,
        revision: RevisionTuple,
        deadline: AuthorityInstant,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self { action_id, name, version, arguments, limits, revision, deadline, idempotency_key }
    }

    /// Returns the B0 action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Borrows the exact tool/capability name.
    #[must_use]
    pub const fn name(&self) -> &CapabilityName {
        &self.name
    }
    /// Returns the requested semantic version.
    #[must_use]
    pub const fn version(&self) -> SemanticVersion {
        self.version
    }
    /// Borrows the complete bounded arguments.
    #[must_use]
    pub const fn arguments(&self) -> &BoundedJson {
        &self.arguments
    }
    /// Returns narrowed call limits.
    #[must_use]
    pub const fn limits(&self) -> CallLimits {
        self.limits
    }
    /// Returns the exact authority revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the immutable authority-clock deadline.
    #[must_use]
    pub const fn deadline(&self) -> AuthorityInstant {
        self.deadline
    }
    /// Borrows the explicit idempotency identity.
    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    /// Returns the stable version-one canonical call envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin_version(2, self.limits.protocol_version());
        bytes.extend_from_slice(self.action_id.as_bytes());
        crate::wire::text(&mut bytes, self.name.as_str());
        crate::wire::u16_value(&mut bytes, self.version.major());
        crate::wire::u16_value(&mut bytes, self.version.minor());
        crate::wire::u16_value(&mut bytes, self.version.patch());
        crate::wire::bytes(&mut bytes, self.arguments.canonical_bytes());
        bytes.extend_from_slice(&self.limits.canonical_bytes());
        crate::wire::revision(&mut bytes, self.revision);
        crate::wire::instant(&mut bytes, self.deadline);
        crate::wire::text(&mut bytes, self.idempotency_key.as_str());
        bytes
    }
}
