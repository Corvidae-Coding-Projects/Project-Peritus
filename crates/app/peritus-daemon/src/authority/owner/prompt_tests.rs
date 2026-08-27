//! Real-journal restart tests for durable prompt restoration.

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, CorrelationId,
    PromptAnswer, PromptAnswerPayload, PromptBinding, PromptCorrelation, PromptKind,
    ProtocolContext, ProtocolId, ProtocolVersion, RequestId, UserInputValue, encode_app_message,
};
use peritus_journal::{
    ApplicationPrincipalKind, NewApplicationPrincipal, NewApplicationSession, SqliteJournal,
    SqliteJournalOptions, StoreId,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, SessionId, Sha256Digest, WorkspaceId,
};

use super::prompt::{answer, register, status};
use crate::prompt::{AuthorityClock, PromptBroker, PromptBrokerLimits};
use crate::{DaemonLifecycle, PromptTerminalStatus, StartupPhase};

#[test]
fn settled_prompt_restores_its_terminal_status_after_restart() {
    let directory = tempfile::tempdir().expect("temporary prompt store");
    let path = directory.path().join("prompt-restart.sqlite3");
    let store_id = StoreId::new(identity(1)).expect("store identity");
    let actor_id = ActorId::new(identity(2)).expect("actor identity");
    let session_id = SessionId::new(identity(3)).expect("session identity");
    let binding = binding(actor_id, session_id);
    let answer_value = PromptAnswer::new(
        binding.correlation(),
        PromptAnswerPayload::UserInput(UserInputValue::Confirmation(true)),
        64,
    )
    .expect("bounded prompt answer");
    let request_id = RequestId::new(identity(4)).expect("request identity");
    let request = AppRequestEnvelope::new(
        ProtocolContext::new(
            ProtocolId::new(identity(5)).expect("protocol identity"),
            ProtocolVersion::new(1, 0).expect("protocol version"),
            session_id,
        ),
        request_id,
        CorrelationId::new(identity(6)).expect("correlation identity"),
        AppRequestPayload::AnswerPrompt(answer_value.clone()),
    )
    .expect("answer request");
    let frame = encode_app_message(&AppMessage::Request(request), AppProtocolLimits::PRODUCTION)
        .expect("canonical answer frame");

    {
        let mut journal = open_journal(&path, store_id, actor_id, session_id);
        let lifecycle = ready_lifecycle();
        let mut broker = PromptBroker::new(PromptBrokerLimits::new(4).expect("prompt limits"));
        assert_eq!(
            register(
                &mut journal,
                &mut broker,
                &lifecycle,
                actor_id,
                session_id,
                binding.clone(),
                64,
            )
            .expect("register prompt"),
            PromptTerminalStatus::AwaitingAnswer,
        );
        assert_eq!(
            answer(
                &mut journal,
                &mut broker,
                &AuthorityClock::new(1).expect("authority clock"),
                &lifecycle,
                actor_id,
                session_id,
                request_id,
                answer_value,
                frame,
            )
            .expect("settle prompt"),
            PromptTerminalStatus::Answered,
        );
    }

    let mut reopened = SqliteJournal::open(&path, store_id, SqliteJournalOptions::default())
        .expect("reopen store");
    let lifecycle = ready_lifecycle();
    let mut restored = PromptBroker::new(PromptBrokerLimits::new(4).expect("prompt limits"));
    assert_eq!(
        register(
            &mut reopened,
            &mut restored,
            &lifecycle,
            actor_id,
            session_id,
            binding.clone(),
            64,
        )
        .expect("restore registered prompt"),
        PromptTerminalStatus::Answered,
    );
    assert_eq!(
        status(&restored, &lifecycle, actor_id, session_id, binding.correlation())
            .expect("restored prompt status"),
        PromptTerminalStatus::Answered,
    );
}

fn open_journal(
    path: &std::path::Path,
    store_id: StoreId,
    actor_id: ActorId,
    session_id: SessionId,
) -> SqliteJournal {
    let mut journal =
        SqliteJournal::open(path, store_id, SqliteJournalOptions::default()).expect("open store");
    journal
        .bind_application_principal(NewApplicationPrincipal::new(
            Sha256Digest::new([16; 32]),
            ApplicationPrincipalKind::UnixPeer,
            actor_id,
            Sha256Digest::new([17; 32]),
        ))
        .expect("bind application principal");
    journal
        .open_application_session(
            NewApplicationSession::new(session_id, actor_id, 1, 100, identity(7), 1, 0)
                .expect("session facts"),
        )
        .expect("open application session");
    journal
}

fn ready_lifecycle() -> DaemonLifecycle {
    let mut lifecycle = DaemonLifecycle::starting();
    for phase in [
        StartupPhase::Lock,
        StartupPhase::Migrate,
        StartupPhase::Journal,
        StartupPhase::Artifacts,
        StartupPhase::Evidence,
        StartupPhase::Projections,
        StartupPhase::AuthorityEpoch,
        StartupPhase::DomainRecovery,
        StartupPhase::EffectRecovery,
        StartupPhase::AppRecovery,
        StartupPhase::Outbox,
        StartupPhase::Ipc,
        StartupPhase::Ready,
    ] {
        lifecycle.advance(phase).expect("canonical startup phase");
    }
    lifecycle
}

fn binding(actor_id: ActorId, session_id: SessionId) -> PromptBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new(identity(8)).expect("acceptance identity"),
        HarnessId::new(identity(9)).expect("harness identity"),
        WorkspaceId::new(identity(10)).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(identity(11)).expect("policy identity"),
        ProviderProfileId::new(identity(12)).expect("provider identity"),
    );
    let correlation = PromptCorrelation::new(
        RequestId::new(identity(13)).expect("originating request"),
        peritus_app_protocol::PromptId::new(identity(14)).expect("prompt identity"),
        session_id,
        actor_id,
        revision,
        Sha256Digest::new([15; 32]),
        Generation::new(2).expect("cancellation generation"),
    );
    PromptBinding::new(PromptKind::UserInput, correlation, Vec::new(), Vec::new(), 1, 1)
        .expect("prompt binding")
}

const fn identity(value: u8) -> [u8; 16] {
    [value; 16]
}
