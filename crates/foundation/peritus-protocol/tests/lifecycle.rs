//! Exhaustive B0 lifecycle wire contract tests.

use peritus_codec::{CanonicalEncode, CodecErrorKind, CodecLimits, decode_message, encode_message};
use peritus_kernel::{
    AcceptancePhase, ActionPhase, AttemptPhase, AuthorityInputKind, CommandEnvelope, KernelCommand,
    KernelErrorKind, KernelEventKind, LifecycleEntity, ReviewPhase, RunPhase, SessionPhase,
    TurnPhase, WaiverPhase,
};
use peritus_policy::ActorRole;
use peritus_protocol::schema::{
    KERNEL_COMMAND_VARIANTS, KERNEL_ERROR_VARIANTS, KERNEL_EVENT_VARIANTS, KERNEL_SUBJECT_VARIANTS,
    LIFECYCLE_PHASE_VARIANTS,
};
use peritus_protocol::{
    CommandEnvelopeDto, KernelCommandDto, KernelErrorDto, KernelEventDto, KernelSubjectDto,
    LifecyclePhaseDto,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, AttemptId, CommandId, EnvironmentId, EventId,
    EventSequence, FindingId, Generation, HarnessId, PolicyId, ProviderProfileId, ReviewCycleId,
    RevisionNumber, RevisionTuple, RunId, SessionId, Sha256Digest, TurnId, WorkspaceId,
};

const LIMITS: CodecLimits = CodecLimits::PRODUCTION;

fn fixture_id<T, E: core::fmt::Debug>(
    byte: u8,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> T {
    constructor([byte; 16]).expect("nonzero fixture id")
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        fixture_id(1, AcceptanceSpecId::new),
        fixture_id(2, HarnessId::new),
        fixture_id(3, WorkspaceId::new),
        Generation::new(4).expect("one based"),
        RevisionNumber::new(5).expect("one based"),
        fixture_id(6, PolicyId::new),
        fixture_id(7, ProviderProfileId::new),
    )
}

fn roundtrip<T>(value: &T) -> T
where
    T: CanonicalEncode + peritus_codec::CanonicalDecode,
{
    decode_message(&encode_message(value, LIMITS).expect("encode"), LIMITS).expect("decode")
}

#[test]
fn every_command_variant_roundtrips() {
    let run_id = fixture_id(11, RunId::new);
    let attempt_id = fixture_id(12, AttemptId::new);
    let turn_id = fixture_id(13, TurnId::new);
    let action_id = fixture_id(14, ActionId::new);
    let review_id = fixture_id(15, ReviewCycleId::new);
    let finding_id = fixture_id(16, FindingId::new);
    let commands = vec![
        KernelCommand::PauseSession,
        KernelCommand::ResumeSession,
        KernelCommand::CloseSession,
        KernelCommand::StartRun { run_id },
        KernelCommand::PauseRun { run_id },
        KernelCommand::ResumeRun { run_id },
        KernelCommand::CancelRun { run_id },
        KernelCommand::FailRun { run_id },
        KernelCommand::ExhaustRun { run_id },
        KernelCommand::RejectRun { run_id },
        KernelCommand::StartAttempt { run_id, attempt_id },
        KernelCommand::ResumeAttempt { run_id, attempt_id },
        KernelCommand::SubmitAttempt { run_id, attempt_id },
        KernelCommand::FailAttempt { run_id, attempt_id },
        KernelCommand::ExhaustAttempt { run_id, attempt_id },
        KernelCommand::StartTurn { attempt_id, turn_id },
        KernelCommand::CompleteTurn { attempt_id, turn_id },
        KernelCommand::FailTurn { attempt_id, turn_id },
        KernelCommand::CancelTurn { attempt_id, turn_id },
        KernelCommand::ProposeAction {
            turn_id,
            action_id,
            digest: Sha256Digest::new([17; 32]),
            actor_id: fixture_id(18, ActorId::new),
            role: ActorRole::Writer,
            environment_id: fixture_id(19, EnvironmentId::new),
        },
        KernelCommand::AuthorizeAction { action_id },
        KernelCommand::DispatchAction { action_id },
        KernelCommand::CompleteAction { action_id },
        KernelCommand::FailAction { action_id },
        KernelCommand::CancelAction { action_id },
        KernelCommand::RequestReview { run_id, attempt_id, review_id },
        KernelCommand::BeginReview { review_id },
        KernelCommand::SubmitReview { review_id },
        KernelCommand::InvalidateReview { review_id },
        KernelCommand::RequestWaiver { run_id, review_id, finding_id },
        KernelCommand::GrantWaiver { finding_id },
        KernelCommand::DenyWaiver { finding_id },
        KernelCommand::InvalidateWaiver { finding_id },
        KernelCommand::BeginAcceptance { run_id },
        KernelCommand::EvaluateAcceptance { run_id },
    ];

    assert_eq!(commands.len(), 35);
    for (index, command) in commands.into_iter().enumerate() {
        let value = KernelCommandDto::from(command.clone());
        let encoded = encode_message(&value, LIMITS).expect("encode command");
        assert_eq!(
            u16::from_be_bytes([encoded[16], encoded[17]]),
            KERNEL_COMMAND_VARIANTS[index].tag
        );
        let decoded = roundtrip(&value);
        assert_eq!(decoded.into_domain(), command);
    }
}

#[test]
fn envelope_preserves_revision_and_causal_predecessor() {
    let envelope = CommandEnvelope::new(
        fixture_id(20, CommandId::new),
        fixture_id(21, EventId::new),
        Some(fixture_id(22, EventId::new)),
        revision(),
    );
    assert_eq!(roundtrip(&CommandEnvelopeDto::from(envelope)).into_domain(), envelope);
}

#[test]
fn every_event_kind_and_subject_roundtrips_as_inert_data() {
    use KernelEventKind as K;
    let kinds = [
        K::SessionOpened,
        K::SessionPaused,
        K::SessionResumed,
        K::SessionClosed,
        K::RunStarted,
        K::RunPaused,
        K::RunResumed,
        K::RunCancelled,
        K::RunFailed,
        K::RunExhausted,
        K::RunRejected,
        K::AttemptStarted,
        K::AttemptResumed,
        K::AttemptSubmitted,
        K::AttemptFailed,
        K::AttemptExhausted,
        K::TurnStarted,
        K::TurnCompleted,
        K::TurnFailed,
        K::TurnCancelled,
        K::ActionProposed,
        K::ActionAuthorized,
        K::ActionDispatched,
        K::ActionCompleted,
        K::ActionFailed,
        K::ActionCancelled,
        K::ReviewRequested,
        K::ReviewBegun,
        K::ReviewSubmitted,
        K::ReviewInvalidated,
        K::WaiverRequested,
        K::WaiverGranted,
        K::WaiverDenied,
        K::WaiverInvalidated,
        K::AcceptanceBegun,
        K::AcceptanceAccepted,
        K::AcceptanceNeedsChanges,
    ];
    let subjects = [
        KernelSubjectDto::Session(fixture_id(30, SessionId::new)),
        KernelSubjectDto::Run(fixture_id(31, RunId::new)),
        KernelSubjectDto::Attempt(fixture_id(32, AttemptId::new)),
        KernelSubjectDto::Turn(fixture_id(33, TurnId::new)),
        KernelSubjectDto::Action(fixture_id(34, ActionId::new)),
        KernelSubjectDto::Review(fixture_id(35, ReviewCycleId::new)),
        KernelSubjectDto::Waiver(fixture_id(36, FindingId::new)),
        KernelSubjectDto::Acceptance(fixture_id(37, RunId::new)),
    ];
    assert_eq!(kinds.len(), 37);
    for (index, kind) in kinds.into_iter().enumerate() {
        let value = KernelEventDto {
            id: fixture_id(40, EventId::new),
            command_id: fixture_id(41, CommandId::new),
            sequence: EventSequence::new(u64::try_from(index + 1).expect("small"))
                .expect("one based"),
            previous_event_id: Some(fixture_id(42, EventId::new)),
            revision: revision(),
            kind,
            subject: subjects[index % subjects.len()],
        };
        let encoded = encode_message(&value, LIMITS).expect("encode event");
        assert_eq!(
            u16::from_be_bytes([encoded[169], encoded[170]]),
            KERNEL_EVENT_VARIANTS[index].tag
        );
        assert_eq!(
            u16::from_be_bytes([encoded[171], encoded[172]]),
            KERNEL_SUBJECT_VARIANTS[index % KERNEL_SUBJECT_VARIANTS.len()].tag
        );
        assert_eq!(roundtrip(&value), value);
    }
}

#[test]
fn every_phase_roundtrips() {
    let phases = vec![
        LifecyclePhaseDto::Session(SessionPhase::Open),
        LifecyclePhaseDto::Session(SessionPhase::Paused),
        LifecyclePhaseDto::Session(SessionPhase::Closed),
        LifecyclePhaseDto::Run(RunPhase::Pending),
        LifecyclePhaseDto::Run(RunPhase::Running),
        LifecyclePhaseDto::Run(RunPhase::Paused),
        LifecyclePhaseDto::Run(RunPhase::Reviewing),
        LifecyclePhaseDto::Run(RunPhase::Fixing),
        LifecyclePhaseDto::Run(RunPhase::Accepted),
        LifecyclePhaseDto::Run(RunPhase::Rejected),
        LifecyclePhaseDto::Run(RunPhase::Cancelled),
        LifecyclePhaseDto::Run(RunPhase::Failed),
        LifecyclePhaseDto::Run(RunPhase::Exhausted),
        LifecyclePhaseDto::Attempt(AttemptPhase::Active),
        LifecyclePhaseDto::Attempt(AttemptPhase::Submitted),
        LifecyclePhaseDto::Attempt(AttemptPhase::Reviewing),
        LifecyclePhaseDto::Attempt(AttemptPhase::Fixing),
        LifecyclePhaseDto::Attempt(AttemptPhase::Accepted),
        LifecyclePhaseDto::Attempt(AttemptPhase::Failed),
        LifecyclePhaseDto::Attempt(AttemptPhase::Cancelled),
        LifecyclePhaseDto::Attempt(AttemptPhase::Exhausted),
        LifecyclePhaseDto::Turn(TurnPhase::Active),
        LifecyclePhaseDto::Turn(TurnPhase::Completed),
        LifecyclePhaseDto::Turn(TurnPhase::Failed),
        LifecyclePhaseDto::Turn(TurnPhase::Cancelled),
        LifecyclePhaseDto::Action(ActionPhase::Proposed),
        LifecyclePhaseDto::Action(ActionPhase::Authorized),
        LifecyclePhaseDto::Action(ActionPhase::Dispatched),
        LifecyclePhaseDto::Action(ActionPhase::Succeeded),
        LifecyclePhaseDto::Action(ActionPhase::Failed),
        LifecyclePhaseDto::Action(ActionPhase::Cancelled),
        LifecyclePhaseDto::Review(ReviewPhase::Requested),
        LifecyclePhaseDto::Review(ReviewPhase::Active),
        LifecyclePhaseDto::Review(ReviewPhase::Submitted),
        LifecyclePhaseDto::Review(ReviewPhase::Invalidated),
        LifecyclePhaseDto::Waiver(WaiverPhase::Requested),
        LifecyclePhaseDto::Waiver(WaiverPhase::Granted),
        LifecyclePhaseDto::Waiver(WaiverPhase::Denied),
        LifecyclePhaseDto::Waiver(WaiverPhase::Invalidated),
        LifecyclePhaseDto::Acceptance(AcceptancePhase::Pending),
        LifecyclePhaseDto::Acceptance(AcceptancePhase::Evaluating),
        LifecyclePhaseDto::Acceptance(AcceptancePhase::NeedsChanges),
        LifecyclePhaseDto::Acceptance(AcceptancePhase::Accepted),
        LifecyclePhaseDto::Acceptance(AcceptancePhase::Terminated),
    ];
    assert_eq!(phases.len(), 44);
    for (index, phase) in phases.into_iter().enumerate() {
        let encoded = encode_message(&phase, LIMITS).expect("encode phase");
        assert_eq!(
            u16::from_be_bytes([encoded[16], encoded[17]]),
            LIFECYCLE_PHASE_VARIANTS[index].tag
        );
        assert_eq!(
            u16::from_be_bytes([encoded[18], encoded[19]]),
            LIFECYCLE_PHASE_VARIANTS[index].subtag.expect("phase subtag")
        );
        assert_eq!(roundtrip(&phase), phase);
    }
}

#[test]
fn error_record_and_malformed_values_are_closed() {
    let kinds = [
        KernelErrorKind::RevisionMismatch,
        KernelErrorKind::ContractMismatch,
        KernelErrorKind::CausalHeadMismatch,
        KernelErrorKind::DuplicateCommand,
        KernelErrorKind::DuplicateEvent,
        KernelErrorKind::MissingEntity,
        KernelErrorKind::DuplicateEntity,
        KernelErrorKind::ParentMismatch,
        KernelErrorKind::IllegalPhase,
        KernelErrorKind::MissingAuthorityInput,
        KernelErrorKind::AuthorityMismatch,
        KernelErrorKind::BudgetUnavailable,
        KernelErrorKind::BudgetExceeded,
        KernelErrorKind::LiveChild,
        KernelErrorKind::SequenceOverflow,
        KernelErrorKind::InvalidAggregate,
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        let value = KernelErrorDto {
            kind,
            entity: Some(LifecycleEntity::Action),
            authority: Some(AuthorityInputKind::CapabilityUse),
        };
        let encoded = encode_message(&value, LIMITS).expect("encode error");
        assert_eq!(
            u16::from_be_bytes([encoded[16], encoded[17]]),
            KERNEL_ERROR_VARIANTS[index].tag
        );
        assert_eq!(roundtrip(&value), value);
    }

    let mut unknown = encode_message(&KernelCommandDto::from(KernelCommand::PauseSession), LIMITS)
        .expect("encode");
    unknown[16..18].copy_from_slice(&99u16.to_be_bytes());
    assert_eq!(
        decode_message::<KernelCommandDto>(&unknown, LIMITS).expect_err("unknown tag").kind(),
        CodecErrorKind::UnknownTag
    );

    let mut invalid_id = encode_message(
        &KernelCommandDto::from(KernelCommand::StartRun { run_id: fixture_id(50, RunId::new) }),
        LIMITS,
    )
    .expect("encode");
    invalid_id[18..34].fill(0);
    assert_eq!(
        decode_message::<KernelCommandDto>(&invalid_id, LIMITS).expect_err("zero id").kind(),
        CodecErrorKind::InvalidDomainValue
    );
}
