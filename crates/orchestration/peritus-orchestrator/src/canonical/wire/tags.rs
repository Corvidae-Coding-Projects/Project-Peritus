//! Explicit closed protocol tags for E0 wire families.

use peritus_codec::{CanonicalReader, CodecError};

use crate::child::CancellationClassificationKind;
use crate::{
    ChildAggregateKind, ChildTerminalClass, DirectiveDeliveryState, DirectiveDestination,
    DirectiveKind, GateObservationClass, HandoffKind, HandoffRole, KernelAcceptanceOutcome,
    OrchestratorTerminalKind, ReviewObservationClass, TerminalCause,
};

pub const fn handoff_kind_tag(value: HandoffKind) -> u8 {
    match value {
        HandoffKind::Writer => 1,
        HandoffKind::Reviewer => 2,
        HandoffKind::Fixer => 3,
    }
}
pub fn read_handoff_kind(reader: &mut CanonicalReader<'_>) -> Result<HandoffKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(HandoffKind::Writer),
        2 => Ok(HandoffKind::Reviewer),
        3 => Ok(HandoffKind::Fixer),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn handoff_role_tag(value: HandoffRole) -> u8 {
    match value {
        HandoffRole::Orchestrator => 1,
        HandoffRole::Writer => 2,
        HandoffRole::Reviewer => 3,
        HandoffRole::Fixer => 4,
    }
}
pub fn read_handoff_role(reader: &mut CanonicalReader<'_>) -> Result<HandoffRole, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(HandoffRole::Orchestrator),
        2 => Ok(HandoffRole::Writer),
        3 => Ok(HandoffRole::Reviewer),
        4 => Ok(HandoffRole::Fixer),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn destination_tag(value: DirectiveDestination) -> u8 {
    match value {
        DirectiveDestination::Scheduler => 1,
        DirectiveDestination::Collaboration => 2,
        DirectiveDestination::Agent => 3,
        DirectiveDestination::Gates => 4,
        DirectiveDestination::Review => 5,
        DirectiveDestination::QualityEvaluator => 6,
        DirectiveDestination::Kernel => 7,
    }
}
pub fn read_destination(
    reader: &mut CanonicalReader<'_>,
) -> Result<DirectiveDestination, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(DirectiveDestination::Scheduler),
        2 => Ok(DirectiveDestination::Collaboration),
        3 => Ok(DirectiveDestination::Agent),
        4 => Ok(DirectiveDestination::Gates),
        5 => Ok(DirectiveDestination::Review),
        6 => Ok(DirectiveDestination::QualityEvaluator),
        7 => Ok(DirectiveDestination::Kernel),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn directive_kind_tag(value: DirectiveKind) -> u8 {
    match value {
        DirectiveKind::StartWriter => 1,
        DirectiveKind::StartGates => 2,
        DirectiveKind::StartReview => 3,
        DirectiveKind::StartFixer => 4,
        DirectiveKind::EvaluateAcceptance => 5,
        DirectiveKind::FinalizeChildren => 6,
        DirectiveKind::BeginKernelAcceptance => 7,
        DirectiveKind::EvaluateKernelAcceptance => 8,
        DirectiveKind::PauseChildren => 9,
        DirectiveKind::CancelChildren => 10,
        DirectiveKind::ResumeChildren => 11,
    }
}
pub fn read_directive_kind(reader: &mut CanonicalReader<'_>) -> Result<DirectiveKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(DirectiveKind::StartWriter),
        2 => Ok(DirectiveKind::StartGates),
        3 => Ok(DirectiveKind::StartReview),
        4 => Ok(DirectiveKind::StartFixer),
        5 => Ok(DirectiveKind::EvaluateAcceptance),
        6 => Ok(DirectiveKind::FinalizeChildren),
        7 => Ok(DirectiveKind::BeginKernelAcceptance),
        8 => Ok(DirectiveKind::EvaluateKernelAcceptance),
        9 => Ok(DirectiveKind::PauseChildren),
        10 => Ok(DirectiveKind::CancelChildren),
        11 => Ok(DirectiveKind::ResumeChildren),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn delivery_tag(value: DirectiveDeliveryState) -> u8 {
    match value {
        DirectiveDeliveryState::Ready => 1,
        DirectiveDeliveryState::Published => 2,
        DirectiveDeliveryState::Acknowledged => 3,
    }
}
pub fn read_delivery(
    reader: &mut CanonicalReader<'_>,
) -> Result<DirectiveDeliveryState, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(DirectiveDeliveryState::Ready),
        2 => Ok(DirectiveDeliveryState::Published),
        3 => Ok(DirectiveDeliveryState::Acknowledged),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn child_kind_tag(value: ChildAggregateKind) -> u8 {
    match value {
        ChildAggregateKind::Agent => 1,
        ChildAggregateKind::Gates => 2,
        ChildAggregateKind::Review => 3,
        ChildAggregateKind::Scheduler => 4,
        ChildAggregateKind::Collaboration => 5,
        ChildAggregateKind::Kernel => 6,
    }
}
pub fn read_child_kind(reader: &mut CanonicalReader<'_>) -> Result<ChildAggregateKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ChildAggregateKind::Agent),
        2 => Ok(ChildAggregateKind::Gates),
        3 => Ok(ChildAggregateKind::Review),
        4 => Ok(ChildAggregateKind::Scheduler),
        5 => Ok(ChildAggregateKind::Collaboration),
        6 => Ok(ChildAggregateKind::Kernel),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn child_terminal_tag(value: ChildTerminalClass) -> u8 {
    match value {
        ChildTerminalClass::Completed => 1,
        ChildTerminalClass::Failed => 2,
        ChildTerminalClass::Cancelled => 3,
        ChildTerminalClass::Indeterminate => 4,
        ChildTerminalClass::NeedsHuman => 5,
        ChildTerminalClass::NeedsChanges => 6,
        ChildTerminalClass::Accepted => 7,
    }
}
pub fn read_child_terminal(
    reader: &mut CanonicalReader<'_>,
) -> Result<ChildTerminalClass, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ChildTerminalClass::Completed),
        2 => Ok(ChildTerminalClass::Failed),
        3 => Ok(ChildTerminalClass::Cancelled),
        4 => Ok(ChildTerminalClass::Indeterminate),
        5 => Ok(ChildTerminalClass::NeedsHuman),
        6 => Ok(ChildTerminalClass::NeedsChanges),
        7 => Ok(ChildTerminalClass::Accepted),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn cancellation_classification_tag(value: CancellationClassificationKind) -> u8 {
    match value {
        CancellationClassificationKind::Unreachable => 1,
        CancellationClassificationKind::Ambiguous => 2,
    }
}

pub fn read_cancellation_classification(
    reader: &mut CanonicalReader<'_>,
) -> Result<CancellationClassificationKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(CancellationClassificationKind::Unreachable),
        2 => Ok(CancellationClassificationKind::Ambiguous),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn gate_class_tag(value: GateObservationClass) -> u8 {
    match value {
        GateObservationClass::Passed => 1,
        GateObservationClass::CandidateFailed => 2,
        GateObservationClass::InfrastructureFailed => 3,
        GateObservationClass::Cancelled => 4,
        GateObservationClass::Indeterminate => 5,
    }
}
pub fn read_gate_class(
    reader: &mut CanonicalReader<'_>,
) -> Result<GateObservationClass, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(GateObservationClass::Passed),
        2 => Ok(GateObservationClass::CandidateFailed),
        3 => Ok(GateObservationClass::InfrastructureFailed),
        4 => Ok(GateObservationClass::Cancelled),
        5 => Ok(GateObservationClass::Indeterminate),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn review_class_tag(value: ReviewObservationClass) -> u8 {
    match value {
        ReviewObservationClass::NeedsFix => 1,
        ReviewObservationClass::Completed => 2,
        ReviewObservationClass::NeedsHuman => 3,
        ReviewObservationClass::Failed => 4,
        ReviewObservationClass::Cancelled => 5,
    }
}
pub fn read_review_class(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReviewObservationClass, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ReviewObservationClass::NeedsFix),
        2 => Ok(ReviewObservationClass::Completed),
        3 => Ok(ReviewObservationClass::NeedsHuman),
        4 => Ok(ReviewObservationClass::Failed),
        5 => Ok(ReviewObservationClass::Cancelled),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn kernel_outcome_tag(value: KernelAcceptanceOutcome) -> u8 {
    match value {
        KernelAcceptanceOutcome::Begun => 1,
        KernelAcceptanceOutcome::Accepted => 2,
        KernelAcceptanceOutcome::NeedsChanges => 3,
        KernelAcceptanceOutcome::Cancelled => 4,
    }
}
pub fn read_kernel_outcome(
    reader: &mut CanonicalReader<'_>,
) -> Result<KernelAcceptanceOutcome, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(KernelAcceptanceOutcome::Begun),
        2 => Ok(KernelAcceptanceOutcome::Accepted),
        3 => Ok(KernelAcceptanceOutcome::NeedsChanges),
        4 => Ok(KernelAcceptanceOutcome::Cancelled),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn terminal_kind_tag(value: OrchestratorTerminalKind) -> u8 {
    match value {
        OrchestratorTerminalKind::Accepted => 1,
        OrchestratorTerminalKind::Rejected => 2,
        OrchestratorTerminalKind::Failed => 3,
        OrchestratorTerminalKind::Exhausted => 4,
        OrchestratorTerminalKind::NeedsHuman => 5,
        OrchestratorTerminalKind::Cancelled => 6,
    }
}
pub fn read_terminal_kind(
    reader: &mut CanonicalReader<'_>,
) -> Result<OrchestratorTerminalKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(OrchestratorTerminalKind::Accepted),
        2 => Ok(OrchestratorTerminalKind::Rejected),
        3 => Ok(OrchestratorTerminalKind::Failed),
        4 => Ok(OrchestratorTerminalKind::Exhausted),
        5 => Ok(OrchestratorTerminalKind::NeedsHuman),
        6 => Ok(OrchestratorTerminalKind::Cancelled),
        _ => Err(super::unknown(offset)),
    }
}

pub const fn terminal_cause_tag(value: TerminalCause) -> u8 {
    match value {
        TerminalCause::KernelAccepted => 1,
        TerminalCause::ExplicitRejection => 2,
        TerminalCause::ExplicitFailure => 3,
        TerminalCause::ExplicitExhaustion => 4,
        TerminalCause::WriterFailed => 5,
        TerminalCause::FixerFailed => 6,
        TerminalCause::GateCandidateFailed => 7,
        TerminalCause::GateInfrastructureFailed => 8,
        TerminalCause::ReviewFailed => 9,
        TerminalCause::ReviewNeedsHuman => 10,
        TerminalCause::ReviewOscillation => 11,
        TerminalCause::AcceptanceEvaluationFailed => 12,
        TerminalCause::KernelAcceptanceFailed => 13,
        TerminalCause::CoordinationFailed => 14,
        TerminalCause::KernelNeedsChanges => 15,
        TerminalCause::WriterLimit => 16,
        TerminalCause::FixerLimit => 17,
        TerminalCause::GateLimit => 18,
        TerminalCause::ReviewLimit => 19,
        TerminalCause::RevisionLimit => 20,
        TerminalCause::HandoffLimit => 21,
        TerminalCause::DirectiveLimit => 22,
        TerminalCause::ObservationLimit => 23,
        TerminalCause::ChildAmbiguous => 24,
        TerminalCause::CancellationReconciled => 25,
    }
}
pub fn read_terminal_cause(reader: &mut CanonicalReader<'_>) -> Result<TerminalCause, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(TerminalCause::KernelAccepted),
        2 => Ok(TerminalCause::ExplicitRejection),
        3 => Ok(TerminalCause::ExplicitFailure),
        4 => Ok(TerminalCause::ExplicitExhaustion),
        5 => Ok(TerminalCause::WriterFailed),
        6 => Ok(TerminalCause::FixerFailed),
        7 => Ok(TerminalCause::GateCandidateFailed),
        8 => Ok(TerminalCause::GateInfrastructureFailed),
        9 => Ok(TerminalCause::ReviewFailed),
        10 => Ok(TerminalCause::ReviewNeedsHuman),
        11 => Ok(TerminalCause::ReviewOscillation),
        12 => Ok(TerminalCause::AcceptanceEvaluationFailed),
        13 => Ok(TerminalCause::KernelAcceptanceFailed),
        14 => Ok(TerminalCause::CoordinationFailed),
        15 => Ok(TerminalCause::KernelNeedsChanges),
        16 => Ok(TerminalCause::WriterLimit),
        17 => Ok(TerminalCause::FixerLimit),
        18 => Ok(TerminalCause::GateLimit),
        19 => Ok(TerminalCause::ReviewLimit),
        20 => Ok(TerminalCause::RevisionLimit),
        21 => Ok(TerminalCause::HandoffLimit),
        22 => Ok(TerminalCause::DirectiveLimit),
        23 => Ok(TerminalCause::ObservationLimit),
        24 => Ok(TerminalCause::ChildAmbiguous),
        25 => Ok(TerminalCause::CancellationReconciled),
        _ => Err(super::unknown(offset)),
    }
}
