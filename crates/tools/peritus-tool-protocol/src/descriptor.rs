//! Immutable versioned tool descriptor.

mod encoding;

use crate::{
    BoundedText, ControlSet, ImplementationIdentity, JsonLimits, ProgressContract, ProtocolError,
    ProtocolErrorKind, Schema, SchemaContract, SchemaDigest, SemanticVersion,
};
use peritus_policy::{OperationClass, OperationDescriptor};
use peritus_types::CapabilityName;

/// Declared effect surface, independent from rendered prose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SideEffectClass {
    /// Observation only.
    None,
    /// Mutates the isolated workspace through C1.
    Workspace,
    /// Starts an owned C2 process.
    Process,
    /// Causes an externally visible effect through a lower gateway.
    External,
}

/// Whether dispatch requires an exact committed mutation lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseRequirement {
    /// Surplus lease authority is rejected.
    None,
    /// One exact committed lease use is required.
    Required,
}

/// Replay semantics for an exactly identical completed call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdempotencySemantics {
    /// The prior exact terminal envelope may be returned without another effect.
    ReplayTerminal,
    /// The outcome is reported but the effect is never automatically repeated.
    ReportPriorOutcome,
}

/// Tool protocol compatibility range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolCompatibility {
    minimum: u16,
    maximum: u16,
}

impl ProtocolCompatibility {
    /// Version-one protocol compatibility.
    pub const V1: Self = Self { minimum: 1, maximum: 1 };

    /// Version-two paged-progress protocol compatibility.
    pub const V2: Self = Self { minimum: 2, maximum: 2 };

    /// Version-three negotiated-JSON protocol compatibility.
    pub const V3: Self = Self { minimum: 3, maximum: 3 };

    /// Creates an inclusive, nonzero compatibility range.
    ///
    /// # Errors
    ///
    /// Rejects version zero or an inverted range.
    pub const fn new(minimum: u16, maximum: u16) -> Result<Self, ProtocolError> {
        if minimum == 0 || minimum > maximum {
            Err(ProtocolError::new(
                ProtocolErrorKind::InvalidVersion,
                String::new(),
                "protocol compatibility range is invalid",
            ))
        } else {
            Ok(Self { minimum, maximum })
        }
    }

    /// Returns the oldest supported protocol version.
    #[must_use]
    pub const fn minimum(self) -> u16 {
        self.minimum
    }
    /// Returns the newest supported protocol version.
    #[must_use]
    pub const fn maximum(self) -> u16 {
        self.maximum
    }
}

/// Immutable resource ceilings advertised by a descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolLimits {
    timeout_millis: Option<u64>,
    output_bytes: Option<u64>,
    model_bytes: u32,
    human_bytes: u32,
    progress_events: u32,
    progress_contract: ProgressContract,
    artifacts: u16,
    control_bytes: u32,
    json_limits: JsonLimits,
    protocol_version: u16,
}

impl ToolLimits {
    /// Creates complete nonzero resource ceilings.
    ///
    /// # Errors
    ///
    /// Rejects any zero resource ceiling.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        timeout_millis: u64,
        output_bytes: u64,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
        control_bytes: u32,
    ) -> Result<Self, ProtocolError> {
        Self::with_optional_timeout(
            Some(timeout_millis),
            output_bytes,
            model_bytes,
            human_bytes,
            progress_events,
            artifacts,
            control_bytes,
        )
    }

    /// Creates resource ceilings with an optional caller-owned wall deadline.
    ///
    /// # Errors
    /// Rejects zero bounds; absence of a deadline is represented by `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_optional_timeout(
        timeout_millis: Option<u64>,
        output_bytes: u64,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
        control_bytes: u32,
    ) -> Result<Self, ProtocolError> {
        Self::with_optional_output(
            timeout_millis,
            Some(output_bytes),
            model_bytes,
            human_bytes,
            progress_events,
            artifacts,
            control_bytes,
        )
    }

    /// Creates descriptor limits with optional wall-time and cumulative output ceilings.
    ///
    /// Rendering, event, artifact, and control sizes remain selected physical capacities.
    ///
    /// # Errors
    /// Rejects selected zero ceilings or zero physical capacities.
    #[allow(clippy::too_many_arguments)]
    pub fn with_optional_output(
        timeout_millis: Option<u64>,
        output_bytes: Option<u64>,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
        control_bytes: u32,
    ) -> Result<Self, ProtocolError> {
        if timeout_millis == Some(0)
            || output_bytes == Some(0)
            || model_bytes == 0
            || human_bytes == 0
            || progress_events == 0
            || artifacts == 0
            || control_bytes == 0
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "tool_limits",
                "every tool resource ceiling must be nonzero",
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
            control_bytes,
            json_limits: JsonLimits::PRODUCTION,
            protocol_version: 1,
        })
    }

    /// Selects version-two paged progress for newly advertised work.
    ///
    /// The progress value is a per-update physical page capacity rather than a lifetime quota.
    #[must_use]
    pub const fn with_paged_progress(mut self) -> Self {
        self.progress_contract = ProgressContract::PagedV2;
        if self.protocol_version < 2 {
            self.protocol_version = 2;
        }
        self
    }

    /// Selects the version-three per-frame JSON contract advertised by this descriptor.
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
    /// Process settlement metadata has its own JSON envelope bound and is not stream output.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        match self.output_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative output ceiling.
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
    /// Returns the per-control input ceiling.
    #[must_use]
    pub const fn control_bytes(self) -> u32 {
        self.control_bytes
    }

    /// Returns the maximum JSON frame contract admitted by this descriptor.
    #[must_use]
    pub const fn json_limits(self) -> JsonLimits {
        self.json_limits
    }

    /// Returns the canonical protocol version selected by these limits.
    #[must_use]
    pub const fn protocol_version(self) -> u16 {
        self.protocol_version
    }
}

/// Complete immutable descriptor bound to an authenticated B1 operation.
#[derive(Debug, Eq, PartialEq)]
pub struct ToolDescriptor {
    name: CapabilityName,
    version: SemanticVersion,
    schema: Schema,
    schema_digest: SchemaDigest,
    descriptor_digest: SchemaDigest,
    operation: OperationDescriptor,
    side_effect: SideEffectClass,
    lease: LeaseRequirement,
    idempotency: IdempotencySemantics,
    implementation: ImplementationIdentity,
    limits: ToolLimits,
    controls: ControlSet,
    compatibility: ProtocolCompatibility,
    description: BoundedText,
}

impl ToolDescriptor {
    /// Creates and deterministically hashes an immutable descriptor.
    ///
    /// # Errors
    ///
    /// Rejects a name mismatch, a side-effect/operation/lease contradiction, or a schema that
    /// cannot be represented and validated within the descriptor's selected JSON frame.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: CapabilityName,
        version: SemanticVersion,
        schema: Schema,
        operation: OperationDescriptor,
        side_effect: SideEffectClass,
        lease: LeaseRequirement,
        idempotency: IdempotencySemantics,
        implementation: ImplementationIdentity,
        limits: ToolLimits,
        controls: ControlSet,
        compatibility: ProtocolCompatibility,
        description: BoundedText,
    ) -> Result<Self, ProtocolError> {
        if operation.name() != &name
            || !effect_refines(side_effect, lease, operation.operation_class())
            || compatibility.minimum() > limits.protocol_version()
            || compatibility.maximum() < limits.protocol_version()
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::DescriptorMismatch,
                "descriptor.operation",
                "tool name, side effect, lease, and B1 operation do not refine exactly",
            ));
        }
        if schema.contract() == SchemaContract::JsonSchema202012
            && limits.protocol_version() < ProtocolCompatibility::V3.minimum()
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::DescriptorMismatch,
                "descriptor.schema",
                "the current schema contract requires negotiated version-three JSON limits",
            ));
        }
        schema.validate_definition(limits.json_limits())?;
        let schema_digest = schema.digest();
        let mut result = Self {
            name,
            version,
            schema,
            schema_digest,
            descriptor_digest: SchemaDigest::new(peritus_types::Sha256Digest::new([0; 32])),
            operation,
            side_effect,
            lease,
            idempotency,
            implementation,
            limits,
            controls,
            compatibility,
            description,
        };
        result.descriptor_digest =
            SchemaDigest::new(peritus_codec::sha256(&result.canonical_bytes()));
        Ok(result)
    }

    /// Returns the exact capability/tool name.
    #[must_use]
    pub const fn name(&self) -> &CapabilityName {
        &self.name
    }
    /// Returns the semantic tool version.
    #[must_use]
    pub const fn version(&self) -> SemanticVersion {
        self.version
    }
    /// Borrows the exact validated schema.
    #[must_use]
    pub const fn schema(&self) -> &Schema {
        &self.schema
    }
    /// Returns the canonical schema digest.
    #[must_use]
    pub const fn schema_digest(&self) -> SchemaDigest {
        self.schema_digest
    }
    /// Returns the full descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor_digest
    }
    /// Borrows the authenticated B1 operation descriptor.
    #[must_use]
    pub const fn operation(&self) -> &OperationDescriptor {
        &self.operation
    }
    /// Returns the declared effect class.
    #[must_use]
    pub const fn side_effect(&self) -> SideEffectClass {
        self.side_effect
    }
    /// Returns the exact lease rule.
    #[must_use]
    pub const fn lease_requirement(&self) -> LeaseRequirement {
        self.lease
    }
    /// Returns exact replay semantics.
    #[must_use]
    pub const fn idempotency(&self) -> IdempotencySemantics {
        self.idempotency
    }
    /// Borrows the immutable dispatcher identity.
    #[must_use]
    pub const fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.implementation
    }
    /// Returns immutable descriptor limits.
    #[must_use]
    pub const fn limits(&self) -> ToolLimits {
        self.limits
    }
    /// Returns supported controls.
    #[must_use]
    pub const fn controls(&self) -> ControlSet {
        self.controls
    }
    /// Returns protocol compatibility.
    #[must_use]
    pub const fn compatibility(&self) -> ProtocolCompatibility {
        self.compatibility
    }
    /// Borrows bounded human-facing description.
    #[must_use]
    pub const fn description(&self) -> &BoundedText {
        &self.description
    }

    /// Returns deterministic descriptor bytes used for hashing and fixtures.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        encoding::canonical(self)
    }
}

const fn effect_refines(
    effect: SideEffectClass,
    lease: LeaseRequirement,
    operation: OperationClass,
) -> bool {
    match operation {
        OperationClass::Inspection => {
            matches!((effect, lease), (SideEffectClass::None, LeaseRequirement::None))
        }
        OperationClass::WorkspaceMutation | OperationClass::RepositoryHistoryMutation => {
            matches!((effect, lease), (SideEffectClass::Workspace, LeaseRequirement::Required))
        }
        OperationClass::Execution
        | OperationClass::RawEffect
        | OperationClass::DependencyEnvironment => {
            matches!((effect, lease), (SideEffectClass::Process, LeaseRequirement::None))
        }
        OperationClass::Network
        | OperationClass::SecretUse
        | OperationClass::ExternalSideEffect
        | OperationClass::Acceptance
        | OperationClass::Waiver
        | OperationClass::PolicyAmendment
        | OperationClass::HarnessPromotion
        | OperationClass::HumanAuthority => {
            matches!((effect, lease), (SideEffectClass::External, LeaseRequirement::None))
        }
    }
}
