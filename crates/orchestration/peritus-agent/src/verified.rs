//! Executable D0 invariants with Verus specifications.

use crate::{ActivePhase, AgentCommandKind, AgentPhase, TerminalKind};

#[doc = " Checks whether some accepted command can produce this pair of valid phase tags."]
#[doc = ""]
#[doc = " This is deliberately weaker than command admission. The production-called command phase"]
#[doc = " kernel below also uses the actual command variant and required aggregate facts."]
#[must_use]
pub const fn phase_transition_valid(before: u8, after: u8) -> bool {
    before <= 12
        && after <= 12
        && ((before == 0 && after == 1)
            || (before == 1 && after == 2)
            || (before == 2 && (after == 1 || after == 2 || after == 3 || after == 7))
            || (before == 3 && after == 4)
            || (before == 4 && (after == 4 || after == 5))
            || (before == 5 && (after == 5 || after == 6))
            || (before == 6 && after == 0)
            || (before <= 7 && after == 8)
            || (before == 8 && after <= 7)
            || (before <= 8 && after == 9)
            || (before == 9 && after == 12)
            || (before == 7 && after == 10)
            || (before <= 9 && after == 11))
}
#[doc = " Exact rejection class for non-control command phase admission."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseTransitionRejection {
    IllegalPhase,
    ProviderNotInFlight,
    CompletionAbsent,
    ControlCommand,
}
#[doc = " Computes the successor phase used by both reduction and replay for every non-control command."]
pub const fn command_phase_transition(
    before: AgentPhase,
    kind: &AgentCommandKind,
    provider_in_flight: bool,
    completion_present: bool,
) -> Result<AgentPhase, PhaseTransitionRejection> {
    match kind {
        AgentCommandKind::ContextPrepared(_) => required_phase(
            before,
            AgentPhase::Active(ActivePhase::PreparingContext),
            AgentPhase::Active(ActivePhase::RequestingModel),
        ),
        AgentCommandKind::ModelRequestStarted { .. } => required_phase(
            before,
            AgentPhase::Active(ActivePhase::RequestingModel),
            AgentPhase::Active(ActivePhase::StreamingResponse),
        ),
        AgentCommandKind::ProviderEventObserved(_) => required_phase(
            before,
            AgentPhase::Active(ActivePhase::StreamingResponse),
            AgentPhase::Active(ActivePhase::StreamingResponse),
        ),
        AgentCommandKind::ProviderRetryScheduled(_) => {
            if before.tag() != AgentPhase::Active(ActivePhase::StreamingResponse).tag() {
                Err(PhaseTransitionRejection::IllegalPhase)
            } else if !provider_in_flight {
                Err(PhaseTransitionRejection::ProviderNotInFlight)
            } else {
                Ok(AgentPhase::Active(ActivePhase::RequestingModel))
            }
        }
        AgentCommandKind::ToolCallsProposed { .. } => required_phase(
            before,
            AgentPhase::Active(ActivePhase::StreamingResponse),
            AgentPhase::Active(ActivePhase::ProposedToolCalls),
        ),
        AgentCommandKind::CompletionProposed { .. } => required_phase(
            before,
            AgentPhase::Active(ActivePhase::StreamingResponse),
            AgentPhase::Active(ActivePhase::ProposedCompletion),
        ),
        AgentCommandKind::AuthorizationStarted => required_phase(
            before,
            AgentPhase::Active(ActivePhase::ProposedToolCalls),
            AgentPhase::Active(ActivePhase::AwaitingAuthorization),
        ),
        AgentCommandKind::ToolAuthorized { .. } | AgentCommandKind::ToolDenied { .. } => {
            required_phase(
                before,
                AgentPhase::Active(ActivePhase::AwaitingAuthorization),
                AgentPhase::Active(ActivePhase::AwaitingAuthorization),
            )
        }
        AgentCommandKind::ToolExecutionStarted => required_phase(
            before,
            AgentPhase::Active(ActivePhase::AwaitingAuthorization),
            AgentPhase::Active(ActivePhase::ExecutingTools),
        ),
        AgentCommandKind::ToolDispatched { .. }
        | AgentCommandKind::ToolActivated { .. }
        | AgentCommandKind::ToolProgressObserved { .. }
        | AgentCommandKind::ToolCompleted { .. } => required_phase(
            before,
            AgentPhase::Active(ActivePhase::ExecutingTools),
            AgentPhase::Active(ActivePhase::ExecutingTools),
        ),
        AgentCommandKind::ResultRecordingStarted => required_phase(
            before,
            AgentPhase::Active(ActivePhase::ExecutingTools),
            AgentPhase::Active(ActivePhase::RecordingResults),
        ),
        AgentCommandKind::ResultsRecorded { .. } => required_phase(
            before,
            AgentPhase::Active(ActivePhase::RecordingResults),
            AgentPhase::Active(ActivePhase::PreparingContext),
        ),
        AgentCommandKind::Failed(_) | AgentCommandKind::Exhausted(_) => nonterminal_failure(before),
        AgentCommandKind::CompletionCommitted => {
            if before.tag() != AgentPhase::Active(ActivePhase::ProposedCompletion).tag() {
                Err(PhaseTransitionRejection::IllegalPhase)
            } else if !completion_present {
                Err(PhaseTransitionRejection::CompletionAbsent)
            } else {
                Ok(AgentPhase::Terminal(TerminalKind::Completed))
            }
        }
        AgentCommandKind::Paused
        | AgentCommandKind::Resumed { .. }
        | AgentCommandKind::CancellationRequested
        | AgentCommandKind::CancellationFinished => Err(PhaseTransitionRejection::ControlCommand),
    }
}
const fn required_phase(
    before: AgentPhase,
    expected: AgentPhase,
    after: AgentPhase,
) -> Result<AgentPhase, PhaseTransitionRejection> {
    if before.tag() == expected.tag() {
        Ok(after)
    } else {
        Err(PhaseTransitionRejection::IllegalPhase)
    }
}
const fn nonterminal_failure(before: AgentPhase) -> Result<AgentPhase, PhaseTransitionRejection> {
    match before {
        AgentPhase::Terminal(_) => Err(PhaseTransitionRejection::IllegalPhase),
        _ => Ok(AgentPhase::Terminal(TerminalKind::Failed)),
    }
}
#[doc = " Checks every independent completion predicate."]
#[must_use]
#[allow(clippy::fn_params_excessive_bools, reason = "formal completion facts remain independent")]
pub const fn completion_eligible(
    normal_terminal: bool,
    no_incomplete_items: bool,
    usage_settled: bool,
    no_pending_tools: bool,
    revision_fresh: bool,
    no_indeterminate_effect: bool,
) -> bool {
    normal_terminal
        && no_incomplete_items
        && usage_settled
        && no_pending_tools
        && revision_fresh
        && no_indeterminate_effect
}
#[doc = " A proposal cannot itself dispatch an effect or accept a run."]
#[must_use]
pub const fn proposal_has_no_effect(dispatches: bool, accepts: bool) -> bool {
    !dispatches && !accepts
}
#[doc = " Checked counters remain bounded without clamping."]
#[must_use]
pub const fn counter_within_limit(counter: u64, limit: u64) -> bool {
    counter <= limit
}
#[doc = " Stable results preserve proposal ordinals exactly."]
#[must_use]
pub const fn tool_result_order_valid(expected_ordinal: u16, actual_ordinal: u16) -> bool {
    expected_ordinal == actual_ordinal
}
