//! Direct descriptor, effect, result, control, and replay observations.

use super::ToolDisposition;

/// Descriptor and schema facts observed from one registration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "schema, operation, implementation, and bound observations are independent"
)]
pub struct ToolDescriptorObservation {
    name: String,
    schema_digest: [u8; 32],
    repeated_schema_digest: [u8; 32],
    operation_matches: bool,
    implementation_matches: bool,
    schema_bounded: bool,
}

impl ToolDescriptorObservation {
    /// Creates complete descriptor observations.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "descriptor observations remain independently falsifiable"
    )]
    pub const fn new(
        name: String,
        schema_digest: [u8; 32],
        repeated_schema_digest: [u8; 32],
        operation_matches: bool,
        implementation_matches: bool,
        schema_bounded: bool,
    ) -> Self {
        Self {
            name,
            schema_digest,
            repeated_schema_digest,
            operation_matches,
            implementation_matches,
            schema_bounded,
        }
    }

    /// Returns the exact tool name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns whether repeated generation produced the same digest.
    #[must_use]
    pub fn deterministic(&self) -> bool {
        self.schema_digest == self.repeated_schema_digest
    }
    /// Returns whether the implementation matches its B1 operation.
    #[must_use]
    pub const fn operation_matches(&self) -> bool {
        self.operation_matches
    }
    /// Returns whether the registered implementation identity matched.
    #[must_use]
    pub const fn implementation_matches(&self) -> bool {
        self.implementation_matches
    }
    /// Returns whether schema construction enforced protocol bounds.
    #[must_use]
    pub const fn schema_bounded(&self) -> bool {
        self.schema_bounded
    }
}

/// Exact target-effect counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ToolEffectObservation {
    permits_created: u64,
    permits_consumed: u64,
    dispatcher_starts: u64,
    target_effects: u64,
}

impl ToolEffectObservation {
    /// Creates exact effect counts.
    #[must_use]
    pub const fn new(
        permits_created: u64,
        permits_consumed: u64,
        dispatcher_starts: u64,
        target_effects: u64,
    ) -> Self {
        Self { permits_created, permits_consumed, dispatcher_starts, target_effects }
    }
    /// Returns permits constructed by the router.
    #[must_use]
    pub const fn permits_created(self) -> u64 {
        self.permits_created
    }
    /// Returns permits moved into a dispatcher.
    #[must_use]
    pub const fn permits_consumed(self) -> u64 {
        self.permits_consumed
    }
    /// Returns dispatcher start calls.
    #[must_use]
    pub const fn dispatcher_starts(self) -> u64 {
        self.dispatcher_starts
    }
    /// Returns lower target effects.
    #[must_use]
    pub const fn target_effects(self) -> u64 {
        self.target_effects
    }
    /// Returns whether any permit, dispatch, or target effect occurred.
    #[must_use]
    pub const fn any(self) -> bool {
        self.permits_created != 0
            || self.permits_consumed != 0
            || self.dispatcher_starts != 0
            || self.target_effects != 0
    }
}

/// Structured terminal-envelope observations.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "terminal envelope facts are independently asserted"
)]
pub struct ToolResultObservation {
    structured_result: bool,
    structured_failure: bool,
    human_bytes: u64,
    model_bytes: u64,
    artifact_count: u64,
    retryable: bool,
    timing_present: bool,
    truncation_declared: bool,
}

impl ToolResultObservation {
    /// Creates complete terminal-envelope observations.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        reason = "result-envelope facts remain independent"
    )]
    pub const fn new(
        structured_result: bool,
        structured_failure: bool,
        human_bytes: u64,
        model_bytes: u64,
        artifact_count: u64,
        retryable: bool,
        timing_present: bool,
        truncation_declared: bool,
    ) -> Self {
        Self {
            structured_result,
            structured_failure,
            human_bytes,
            model_bytes,
            artifact_count,
            retryable,
            timing_present,
            truncation_declared,
        }
    }
    /// Returns whether a structured success payload exists.
    #[must_use]
    pub const fn structured_result(&self) -> bool {
        self.structured_result
    }
    /// Returns whether a stable structured failure exists.
    #[must_use]
    pub const fn structured_failure(&self) -> bool {
        self.structured_failure
    }
    /// Returns bounded human-rendering bytes.
    #[must_use]
    pub const fn human_bytes(&self) -> u64 {
        self.human_bytes
    }
    /// Returns bounded model-rendering bytes.
    #[must_use]
    pub const fn model_bytes(&self) -> u64 {
        self.model_bytes
    }
    /// Returns referenced artifacts.
    #[must_use]
    pub const fn artifact_count(&self) -> u64 {
        self.artifact_count
    }
    /// Returns declared retryability.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }
    /// Returns whether timing metadata exists.
    #[must_use]
    pub const fn timing_present(&self) -> bool {
        self.timing_present
    }
    /// Returns whether truncation was labelled explicitly.
    #[must_use]
    pub const fn truncation_declared(&self) -> bool {
        self.truncation_declared
    }
}

/// Replay observations for one second submission.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "prior result, duplicate effect, conflict, and ambiguity are independent observations"
)]
pub struct ToolReplayObservation {
    prior_result_returned: bool,
    second_effect: bool,
    conflict_rejected: bool,
    indeterminate_rejected: bool,
}

impl ToolReplayObservation {
    /// Creates exact replay observations.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "replay observations remain independently falsifiable"
    )]
    pub const fn new(
        prior_result_returned: bool,
        second_effect: bool,
        conflict_rejected: bool,
        indeterminate_rejected: bool,
    ) -> Self {
        Self { prior_result_returned, second_effect, conflict_rejected, indeterminate_rejected }
    }
    /// Returns whether the exact recorded result was returned.
    #[must_use]
    pub const fn prior_result_returned(self) -> bool {
        self.prior_result_returned
    }
    /// Returns whether replay duplicated a target effect.
    #[must_use]
    pub const fn second_effect(self) -> bool {
        self.second_effect
    }
    /// Returns whether conflicting bound bytes were rejected.
    #[must_use]
    pub const fn conflict_rejected(self) -> bool {
        self.conflict_rejected
    }
    /// Returns whether indeterminate prior state was rejected.
    #[must_use]
    pub const fn indeterminate_rejected(self) -> bool {
        self.indeterminate_rejected
    }
}

/// Complete direct observation returned by a tool conformance subject.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "schema, exposure, control, and lifecycle observations are independent"
)]
pub struct ToolConformanceObservation {
    disposition: ToolDisposition,
    descriptor: Option<ToolDescriptorObservation>,
    schema_accepted: bool,
    exposed: bool,
    canonical_exposure: bool,
    effects: ToolEffectObservation,
    result: ToolResultObservation,
    progress_sequences: Vec<u64>,
    control_observed: bool,
    execution_joined: bool,
    replay: ToolReplayObservation,
}

impl ToolConformanceObservation {
    /// Creates one complete C4 observation.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        reason = "router boundary facts remain independent"
    )]
    pub const fn new(
        disposition: ToolDisposition,
        descriptor: Option<ToolDescriptorObservation>,
        schema_accepted: bool,
        exposed: bool,
        canonical_exposure: bool,
        effects: ToolEffectObservation,
        result: ToolResultObservation,
        progress_sequences: Vec<u64>,
        control_observed: bool,
        execution_joined: bool,
        replay: ToolReplayObservation,
    ) -> Self {
        Self {
            disposition,
            descriptor,
            schema_accepted,
            exposed,
            canonical_exposure,
            effects,
            result,
            progress_sequences,
            control_observed,
            execution_joined,
            replay,
        }
    }

    /// Returns terminal disposition.
    #[must_use]
    pub const fn disposition(&self) -> ToolDisposition {
        self.disposition
    }
    /// Borrows descriptor observations.
    #[must_use]
    pub const fn descriptor(&self) -> Option<&ToolDescriptorObservation> {
        self.descriptor.as_ref()
    }
    /// Returns whether schema validation accepted the call.
    #[must_use]
    pub const fn schema_accepted(&self) -> bool {
        self.schema_accepted
    }
    /// Returns whether the tool was exposed to the requested role/capability set.
    #[must_use]
    pub const fn exposed(&self) -> bool {
        self.exposed
    }
    /// Returns whether exposure order was canonical and duplicate-free.
    #[must_use]
    pub const fn canonical_exposure(&self) -> bool {
        self.canonical_exposure
    }
    /// Returns exact effect observations.
    #[must_use]
    pub const fn effects(&self) -> ToolEffectObservation {
        self.effects
    }
    /// Borrows terminal result observations.
    #[must_use]
    pub const fn result(&self) -> &ToolResultObservation {
        &self.result
    }
    /// Returns progress sequence numbers in observation order.
    #[must_use]
    pub fn progress_sequences(&self) -> &[u64] {
        &self.progress_sequences
    }
    /// Returns whether the requested control reached the owned execution.
    #[must_use]
    pub const fn control_observed(&self) -> bool {
        self.control_observed
    }
    /// Returns whether execution and support work were completely joined.
    #[must_use]
    pub const fn execution_joined(&self) -> bool {
        self.execution_joined
    }
    /// Returns replay observations.
    #[must_use]
    pub const fn replay(&self) -> ToolReplayObservation {
        self.replay
    }
}
