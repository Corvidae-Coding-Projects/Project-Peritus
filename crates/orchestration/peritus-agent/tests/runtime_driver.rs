//! Durable cooperative-driver integration with fake provider and crash recovery.

mod budget_fixture;
mod common;

use std::collections::VecDeque;
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;

use budget_fixture::{LedgerBudgetPort, model_budget};
use common::*;
use peritus_agent::{
    AgentBudgetPlan, AgentBudgetReservation, AgentCommandKind, AgentDriver, AgentDriverError,
    AgentFailure, AgentFailureKind, CompletionProposal, CompletionRequest, ModelCallId,
    ProviderAdvance, ProviderRetryClass, ProviderRetryRecord, SafeText, TerminalKind, ToolOrdinal,
    ToolResultStatus, ToolSideEffect, TranscriptDigests, TransitionIdentity, load_agent_replay,
};
use peritus_budget::{BudgetAmounts, UsageFinality};
use peritus_codec::CodecLimits;
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, Continuation, EventEnvelope, FinishReason, GenerationConfig, ItemId, ItemKind,
    Message, ModelEvent, ModelLimits, ModelName, ModelRequest, OutputLimitEnforcement,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderName, ProviderProfile,
    ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, ResponseId, ResumeKind,
    Role, StateMode, StreamFragment, StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ContinuationRestoreOutcome, ModelProvider, ModelStream,
    OwnedModelStream, PersistedContinuation, ProviderCoreError,
};
use peritus_types::{
    ActionId, BudgetId, BudgetReservationId, CommandId, EventId, ProviderProfileId, Sha256Digest,
};
use tempfile::TempDir;

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

struct ScriptedStream {
    events: VecDeque<EventEnvelope>,
}

impl ModelStream for ScriptedStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.events.pop_front()) })
    }
}

struct FakeProvider {
    profile: ProviderProfile,
    events: Mutex<Option<VecDeque<EventEnvelope>>>,
}

impl ModelProvider for FakeProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            let events = self
                .events
                .lock()
                .map_err(|_| ProviderCoreError::configuration("fake_provider", "lock failed"))?
                .take()
                .ok_or_else(|| {
                    ProviderCoreError::configuration("fake_provider", "script already consumed")
                })?;
            Ok(OwnedModelStream::new(ScriptedStream { events }, cancellation))
        })
    }

    fn restore_continuation<'a>(
        &'a self,
        persisted: &'a PersistedContinuation,
    ) -> BoxFuture<'a, Result<ContinuationRestoreOutcome, ProviderCoreError>> {
        Box::pin(async move {
            if self.profile.state_mode() == StateMode::BackgroundResumable
                && self.profile.resume_kind() == ResumeKind::ExactCursor
                && persisted.profile_id() == self.profile.profile_id()
                && persisted.profile_revision() == self.profile.revision()
                && persisted.continuation().sequence().is_some()
            {
                Ok(ContinuationRestoreOutcome::Restored(persisted.continuation().clone()))
            } else {
                Ok(ContinuationRestoreOutcome::Unsupported)
            }
        })
    }
}

#[test]
fn fake_provider_completion_is_durable_and_restarts_from_exact_state() {
    block_on(async {
        let temp = TempDir::new().expect("temp");
        let mut journal = open(&temp);
        let mut driver = AgentDriver::start(
            &mut journal,
            binding(),
            limits(64),
            identity(11),
            CodecLimits::PRODUCTION,
        )
        .expect("durable genesis");
        driver
            .drive_once(&mut journal, identity(12), AgentCommandKind::ContextPrepared(context()))
            .expect("context");

        let profile = profile();
        let provider =
            FakeProvider { profile: profile.clone(), events: Mutex::new(Some(text_response())) };
        let (mut budget_port, mut budget) = model_budget(120, revision());
        driver
            .start_model_once(
                &mut journal,
                identity(13),
                ModelCallId::new(digest(41)).expect("call"),
                &mut budget,
                &mut budget_port,
                digest(42),
                &provider,
                request(&profile),
                ProtocolLimits::PRODUCTION,
                CancellationToken::new(),
            )
            .await
            .expect("start model");
        for seed in 14..=19 {
            assert!(matches!(
                driver.drive_model_once(&mut journal, identity(seed)).await.expect("provider step"),
                ProviderAdvance::Envelope { .. }
            ));
        }
        assert!(matches!(
            driver.drive_model_once(&mut journal, identity(20)).await.expect("closed"),
            ProviderAdvance::Closed
        ));

        driver
            .observe_model_budget_once(
                &mut budget,
                &mut budget_port,
                digest(43),
                0,
                UsageFinality::Final,
            )
            .expect("final usage");
        let terminal = driver.model_terminal_record(&budget).expect("terminal");
        let proposal = CompletionProposal::new(
            SafeText::new("implemented and verified the requested change".to_owned())
                .expect("summary"),
            Vec::new(),
            vec![
                SafeText::new("acceptance remains external to D0".to_owned()).expect("uncertainty"),
            ],
            revision(),
            TranscriptDigests::new(digest(20), terminal.response_digest(), digest(27)),
            CompletionRequest::RunGates,
        )
        .expect("proposal");
        driver
            .drive_once(
                &mut journal,
                identity(21),
                AgentCommandKind::CompletionProposed { terminal, proposal },
            )
            .expect("propose completion");
        driver
            .drive_once(&mut journal, identity(22), AgentCommandKind::CompletionCommitted)
            .expect("commit completion");
        assert_eq!(driver.state().terminal_kind(), Some(TerminalKind::Completed));

        let restored =
            AgentDriver::restore(&journal, binding(), limits(64), CodecLimits::PRODUCTION)
                .expect("restore");
        assert_eq!(restored.state(), driver.state());
        assert!(restored.recovery_report().is_clean());
        assert_eq!(
            load_agent_replay(&journal, binding().turn_id()).expect("records").events().len(),
            11
        );
    });
}

#[test]
fn exact_provider_duplicate_is_recorded_without_reapplying_semantics() {
    block_on(async {
        let temp = TempDir::new().expect("temp");
        let mut journal = open(&temp);
        let mut driver = AgentDriver::start(
            &mut journal,
            binding(),
            limits(64),
            identity(50),
            CodecLimits::PRODUCTION,
        )
        .expect("start");
        driver
            .drive_once(&mut journal, identity(51), AgentCommandKind::ContextPrepared(context()))
            .expect("context");
        let profile = profile();
        let provider_event_id =
            peritus_model_protocol::EventId::new("event-1".to_owned()).expect("event ID");
        let started = EventEnvelope::new(
            1,
            Some(1),
            Some(provider_event_id.clone()),
            digest(70),
            ModelEvent::ResponseStarted { response_id: None, model: None },
        )
        .expect("started");
        let provider = FakeProvider {
            profile: profile.clone(),
            events: Mutex::new(Some(VecDeque::from([
                started.clone(),
                started,
                EventEnvelope::new(
                    2,
                    Some(2),
                    Some(peritus_model_protocol::EventId::new("event-2".to_owned()).expect("ID")),
                    digest(71),
                    ModelEvent::Finish(FinishReason::Stop),
                )
                .expect("finish"),
                EventEnvelope::new(
                    3,
                    Some(3),
                    Some(peritus_model_protocol::EventId::new("event-3".to_owned()).expect("ID")),
                    digest(72),
                    ModelEvent::ResponseCompleted,
                )
                .expect("complete"),
            ]))),
        };
        let (mut budget_port, mut budget) = model_budget(130, revision());
        driver
            .start_model_once(
                &mut journal,
                identity(52),
                ModelCallId::new(digest(73)).expect("call"),
                &mut budget,
                &mut budget_port,
                digest(74),
                &provider,
                request(&profile),
                ProtocolLimits::PRODUCTION,
                CancellationToken::new(),
            )
            .await
            .expect("model");
        assert!(matches!(
            driver.drive_model_once(&mut journal, identity(53)).await.expect("first"),
            ProviderAdvance::Envelope {
                transition: peritus_model_protocol::ReducerTransition::Applied,
                ..
            }
        ));
        assert!(matches!(
            driver.drive_model_once(&mut journal, identity(54)).await.expect("duplicate"),
            ProviderAdvance::Envelope {
                transition: peritus_model_protocol::ReducerTransition::DuplicateIgnored,
                ..
            }
        ));
        driver.drive_model_once(&mut journal, identity(55)).await.expect("finish");
        driver.drive_model_once(&mut journal, identity(56)).await.expect("terminal");
        assert_eq!(driver.state().model().cursor(), 3);
        assert_eq!(driver.state().counters().provider_events(), 4);
    });
}

#[test]
fn out_of_order_provider_stream_becomes_explicit_protocol_failure() {
    block_on(async {
        let temp = TempDir::new().expect("temp");
        let mut journal = open(&temp);
        let mut driver = AgentDriver::start(
            &mut journal,
            binding(),
            limits(64),
            identity(60),
            CodecLimits::PRODUCTION,
        )
        .expect("start");
        driver
            .drive_once(&mut journal, identity(61), AgentCommandKind::ContextPrepared(context()))
            .expect("context");
        let profile = profile();
        let provider = FakeProvider {
            profile: profile.clone(),
            events: Mutex::new(Some(VecDeque::from([
                envelope(1, ModelEvent::ResponseStarted { response_id: None, model: None }),
                envelope(3, ModelEvent::Heartbeat),
            ]))),
        };
        let (mut budget_port, mut budget) = model_budget(140, revision());
        driver
            .start_model_once(
                &mut journal,
                identity(62),
                ModelCallId::new(digest(74)).expect("call"),
                &mut budget,
                &mut budget_port,
                digest(75),
                &provider,
                request(&profile),
                ProtocolLimits::PRODUCTION,
                CancellationToken::new(),
            )
            .await
            .expect("model");
        driver.drive_model_once(&mut journal, identity(63)).await.expect("started");
        assert!(matches!(
            driver.drive_model_once(&mut journal, identity(64)).await,
            Err(AgentDriverError::Model(_))
        ));
        assert_eq!(driver.state().model().cursor(), 1);
        driver.cancel_model();
        driver
            .drive_once(
                &mut journal,
                identity(65),
                AgentCommandKind::Failed(AgentFailure::new(
                    AgentFailureKind::Protocol,
                    SafeText::new("provider stream was out of order".to_owned()).expect("detail"),
                )),
            )
            .expect("failure");
        assert_eq!(driver.state().terminal_kind(), Some(TerminalKind::Failed));
    });
}

#[test]
fn crash_restores_exact_provider_prefix_before_resuming_at_the_next_cursor() {
    block_on(async {
        let temp = TempDir::new().expect("temp");
        let mut journal = open(&temp);
        let root = BudgetId::new([150; 16]).expect("budget");
        let mut budget_port =
            LedgerBudgetPort::new(root, revision(), BudgetAmounts::from_units(0, 0, 2, 2, 1));
        persist_interrupted_provider_prefix(&mut journal, &mut budget_port, root).await;
        restore_and_complete_provider_prefix(&mut journal, &mut budget_port, root).await;
    });
}

async fn persist_interrupted_provider_prefix(
    journal: &mut SqliteJournal,
    budget_port: &mut LedgerBudgetPort,
    root: BudgetId,
) {
    let mut driver =
        AgentDriver::start(journal, binding(), limits(96), identity(70), CodecLimits::PRODUCTION)
            .expect("start");
    driver
        .drive_once(journal, identity(71), AgentCommandKind::ContextPrepared(context()))
        .expect("context");
    let profile = resumable_profile();
    let provider = FakeProvider {
        profile: profile.clone(),
        events: Mutex::new(Some(VecDeque::from([
            envelope(
                1,
                ModelEvent::ResponseStarted {
                    response_id: Some(ResponseId::new("response-1".to_owned()).expect("response")),
                    model: None,
                },
            ),
            envelope(2, ModelEvent::Heartbeat),
        ]))),
    };
    let mut budget = AgentBudgetReservation::begin(
        budget_port,
        AgentBudgetPlan::new(
            BudgetReservationId::new([153; 16]).expect("reservation"),
            root,
            revision(),
            ActionId::new([151; 16]).expect("action"),
            digest(152),
            BudgetAmounts::from_units(0, 0, 1, 0, 0),
            false,
        )
        .expect("initial plan"),
    )
    .expect("reserve");
    driver
        .start_model_once(
            journal,
            identity(72),
            ModelCallId::new(digest(153)).expect("call"),
            &mut budget,
            budget_port,
            digest(154),
            &provider,
            resumable_request(&profile, None, "initial-request"),
            ProtocolLimits::PRODUCTION,
            CancellationToken::new(),
        )
        .await
        .expect("initial model");
    driver.drive_model_once(journal, identity(73)).await.expect("response start");
    driver.drive_model_once(journal, identity(74)).await.expect("heartbeat");
    budget.finalize_ambiguous(budget_port, digest(155)).expect("interrupted attempt accounting");
}

async fn restore_and_complete_provider_prefix(
    journal: &mut SqliteJournal,
    budget_port: &mut LedgerBudgetPort,
    root: BudgetId,
) {
    let mut driver = AgentDriver::restore(journal, binding(), limits(96), CodecLimits::PRODUCTION)
        .expect("restore prefix");
    assert!(driver.recovery_report().model_in_flight());
    let continuation = Continuation::new(
        ResponseId::new("response-1".to_owned()).expect("response"),
        None,
        Some(2),
    )
    .expect("continuation");
    let profile = resumable_profile();
    let request = resumable_request(&profile, Some(continuation.clone()), "resume-request");
    let request_digest = request.fingerprint().expect("fingerprint").digest();
    driver
        .drive_once(
            journal,
            identity(75),
            AgentCommandKind::ProviderRetryScheduled(ProviderRetryRecord::new(
                digest(156),
                request_digest,
                ProviderRetryClass::ExactResume { cursor: 2 },
            )),
        )
        .expect("exact retry intent");
    let mut budget = AgentBudgetReservation::begin(
        budget_port,
        AgentBudgetPlan::new(
            BudgetReservationId::new([157; 16]).expect("reservation"),
            root,
            revision(),
            ActionId::new([151; 16]).expect("action"),
            digest(152),
            BudgetAmounts::from_units(0, 0, 1, 0, 0),
            true,
        )
        .expect("retry plan"),
    )
    .expect("retry reserve");
    let provider = FakeProvider {
        profile,
        events: Mutex::new(Some(VecDeque::from([
            envelope(3, ModelEvent::Finish(FinishReason::Stop)),
            envelope(4, ModelEvent::ResponseCompleted),
        ]))),
    };
    assert_eq!(
        driver
            .restore_model_once(
                journal,
                identity(76),
                ModelCallId::new(digest(158)).expect("call"),
                &mut budget,
                budget_port,
                digest(159),
                &provider,
                request,
                ProtocolLimits::PRODUCTION,
                CancellationToken::new(),
            )
            .await
            .expect("resume"),
        ContinuationRestoreOutcome::Restored(continuation)
    );
    driver.drive_model_once(journal, identity(77)).await.expect("finish");
    driver.drive_model_once(journal, identity(78)).await.expect("completed");
    driver
        .observe_model_budget_once(&mut budget, budget_port, digest(160), 1, UsageFinality::Final)
        .expect("retry accounting");
    assert!(driver.model_terminal_record(&budget).expect("terminal").normal_terminal());
    assert_eq!(driver.state().model().cursor(), 4);
}

#[test]
fn crash_after_tool_dispatch_is_classified_indeterminate_without_redispatch() {
    let temp = TempDir::new().expect("temp");
    let mut journal = open(&temp);
    let mut driver = AgentDriver::start(
        &mut journal,
        binding(),
        limits(64),
        identity(30),
        CodecLimits::PRODUCTION,
    )
    .expect("start");
    for (seed, kind) in [
        (31, AgentCommandKind::ContextPrepared(context())),
        (
            32,
            AgentCommandKind::ModelRequestStarted {
                call_id: ModelCallId::new(digest(24)).expect("call"),
                request_digest: digest(25),
            },
        ),
        (
            33,
            AgentCommandKind::ToolCallsProposed {
                terminal: terminal(),
                proposals: vec![tool(0, ToolSideEffect::Process)],
            },
        ),
        (34, AgentCommandKind::AuthorizationStarted),
        (
            35,
            AgentCommandKind::ToolAuthorized {
                ordinal: ToolOrdinal::new(0),
                authority_digest: digest(60),
            },
        ),
        (36, AgentCommandKind::ToolExecutionStarted),
        (37, AgentCommandKind::ToolDispatched { ordinal: ToolOrdinal::new(0) }),
    ] {
        driver.drive_once(&mut journal, identity(seed), kind).expect("transition");
    }
    drop(driver);

    let mut restored =
        AgentDriver::restore(&journal, binding(), limits(64), CodecLimits::PRODUCTION)
            .expect("restore");
    assert_eq!(restored.recovery_report().tool_ordinals(), &[ToolOrdinal::new(0)]);
    restored
        .classify_lost_tool_once(&mut journal, identity(38), ToolOrdinal::new(0))
        .expect("classify");
    let slot = &restored.state().tools().expect("tools").slots()[0];
    assert_eq!(slot.result().expect("result").status(), ToolResultStatus::Indeterminate);
    assert!(restored.recovery_report().is_clean());
    restored
        .drive_once(&mut journal, identity(39), AgentCommandKind::ResultRecordingStarted)
        .expect("recording");
    restored
        .drive_once(
            &mut journal,
            identity(40),
            AgentCommandKind::ResultsRecorded { transcript_digest: digest(87) },
        )
        .expect("results");
    assert!(restored.state().has_unresolved_indeterminate());
}

fn open(temp: &TempDir) -> SqliteJournal {
    SqliteJournal::open(
        temp.path().join("agent.sqlite3"),
        StoreId::new([210; 16]).expect("store"),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("journal")
}

fn identity(seed: u8) -> TransitionIdentity {
    TransitionIdentity::new(id16(seed, CommandId::new), id16(seed.wrapping_add(100), EventId::new))
}

fn text_response() -> VecDeque<EventEnvelope> {
    let item = ItemId::new("message-1".to_owned()).expect("item");
    VecDeque::from([
        envelope(1, ModelEvent::ResponseStarted { response_id: None, model: None }),
        envelope(
            2,
            ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ),
        envelope(
            3,
            ModelEvent::TextDelta {
                item_id: item.clone(),
                fragment: StreamFragment::new(b"done".to_vec(), ProtocolLimits::PRODUCTION)
                    .expect("fragment"),
            },
        ),
        envelope(4, ModelEvent::ItemCompleted(item)),
        envelope(5, ModelEvent::Finish(FinishReason::Stop)),
        envelope(6, ModelEvent::ResponseCompleted),
    ])
}

fn envelope(sequence: u64, event: ModelEvent) -> EventEnvelope {
    EventEnvelope::new(
        sequence,
        None,
        None,
        Sha256Digest::new([u8::try_from(sequence).expect("sequence"); 32]),
        event,
    )
    .expect("envelope")
}

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([5; 16]).expect("profile ID"),
        1,
        ProviderName::new("fake-provider".to_owned()).expect("provider"),
        ModelName::new("fake-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(8_192, 1_024, 8, 4, 64 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn resumable_profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([5; 16]).expect("profile ID"),
        1,
        ProviderName::new("fake-provider".to_owned()).expect("provider"),
        ModelName::new("fake-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::StoredState, Capability::ResumableResponse], &[])
            .expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(8_192, 1_024, 8, 4, 64 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::BackgroundResumable,
        ResumeKind::ExactCursor,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn request(profile: &ProviderProfile) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&[], &[], profile.limits()).expect("requested capabilities"),
    )
    .expect("negotiation");
    let message = Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new("work".to_owned(), limits).expect("text"))],
        limits,
    )
    .expect("message");
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(256, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("request-1".to_owned()).expect("request"),
        vec![message],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )
    .expect("model request")
}

fn resumable_request(
    profile: &ProviderProfile,
    continuation: Option<Continuation>,
    request_id: &str,
) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::StoredState, Capability::ResumableResponse],
            &[],
            profile.limits(),
        )
        .expect("requested capabilities"),
    )
    .expect("negotiation");
    let message = Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new("resume work".to_owned(), limits).expect("text"))],
        limits,
    )
    .expect("message");
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(256, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Disabled,
        PersistencePolicy::new(true, true).expect("background persistence"),
        continuation,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id.to_owned()).expect("request"),
        vec![message],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )
    .expect("model request")
}
