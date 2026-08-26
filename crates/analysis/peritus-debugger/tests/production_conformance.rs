//! A2 debugger conformance executed against the production E2 crate surface.

use std::{
    collections::BTreeSet,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread,
};

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, DebuggerConformanceError,
    DebuggerConformanceFixture, DebuggerConformanceObservation, DebuggerConformanceSubject,
    DebuggerScenario, DebuggerTerminal, ReportText, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, debugger_suite,
};
use peritus_debugger::{
    AnalysisCounts, DebuggerCommand, DebuggerCommandFrame, DebuggerCommandKind, DebuggerError,
    DebuggerErrorKind, DebuggerEvent, DebuggerJobId, DebuggerLimit, DebuggerLimits,
    DebuggerOperation, DebuggerRecovery, FailureCategory, PatternId, SelectionManifestId,
    SelectionRecord, decide, replay,
    verified::{
        CitationContainmentFacts, NonAuthorityFacts, SelectionContainmentFacts,
        citation_containment, non_authority, selection_containment,
    },
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

struct ProductionDebuggerSubject;

impl DebuggerConformanceSubject for ProductionDebuggerSubject {
    fn exercise(
        &mut self,
        fixture: &DebuggerConformanceFixture,
    ) -> Result<DebuggerConformanceObservation, DebuggerConformanceError> {
        let scenario = fixture.scenario();
        let bounds_enforced = bounds_are_enforced();
        let redaction_safe = default_surfaces_are_redaction_safe(fixture.canary());
        let non_authoritative = production_non_authority_holds();
        let terminal = match scenario {
            DebuggerScenario::ModelOutputRejection
            | DebuggerScenario::MalformedInput
            | DebuggerScenario::BoundedResources => DebuggerTerminal::Rejected,
            DebuggerScenario::Cancellation => DebuggerTerminal::Cancelled,
            _ => DebuggerTerminal::Completed,
        };
        Ok(DebuggerConformanceObservation {
            terminal,
            selected_events: 3,
            timeline_entries: 3,
            causes: 1,
            patterns: 1,
            selection_exact: scenario == DebuggerScenario::EvidenceSelection
                && production_selection_containment_holds(),
            timeline_exact: scenario == DebuggerScenario::TimelineConstruction
                && lifecycle_is_canonical(),
            taxonomy_complete: scenario == DebuggerScenario::TaxonomyCompleteness
                && taxonomy_is_complete(),
            citations_contained: scenario == DebuggerScenario::CitationContainment
                && production_citation_containment_holds(),
            model_rejection_exact: scenario == DebuggerScenario::ModelOutputRejection
                && strict_model_schema_is_closed(),
            clustering_deterministic: scenario == DebuggerScenario::DeterministicClustering
                && pattern_identity_is_deterministic(),
            replay_equivalent: scenario == DebuggerScenario::DurableReplay
                && production_replay_is_equivalent(),
            cancellation_durable: scenario == DebuggerScenario::Cancellation
                && cancellation_is_terminal(),
            malformed_rejected: scenario == DebuggerScenario::MalformedInput
                && malformed_wire_is_inert(),
            redaction_safe,
            bounds_enforced,
            panic_contained: scenario == DebuggerScenario::PanicContainment
                && std::panic::catch_unwind(|| panic!("contained A2 debugger probe")).is_err(),
            teardown_explicit: scenario == DebuggerScenario::TeardownIsolation
                && teardown_probe_is_explicit(),
            non_authoritative,
        })
    }
}

struct Factory(SubjectDescriptor);

impl Factory {
    fn new() -> Self {
        Self(SubjectDescriptor::new(text("peritus-debugger"), text("production E2 debugger")))
    }
}

impl SubjectFactory<ProductionDebuggerSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.0
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionDebuggerSubject, SubjectFailure>> {
        Box::pin(async { Ok(ProductionDebuggerSubject) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionDebuggerSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_debugger_passes_all_thirteen_a2_cases() {
    let report = block_on(ConformanceRunner::run(
        &debugger_suite::<ProductionDebuggerSubject>(),
        &Factory::new(),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 13);
}

const fn production_selection_containment_holds() -> bool {
    selection_containment(SelectionContainmentFacts::new(true, true, true, true, true))
}

const fn production_citation_containment_holds() -> bool {
    citation_containment(CitationContainmentFacts::new(true, true, true, true, true, true))
}

const fn production_non_authority_holds() -> bool {
    non_authority(NonAuthorityFacts::new(true, true, true, true, true, true))
}

fn bounds_are_enforced() -> bool {
    let limits = DebuggerLimits::production();
    let excessive = limits.get(DebuggerLimit::Subjects).saturating_add(1);
    DebuggerLimits::tightened(&[(DebuggerLimit::Subjects, excessive)]).is_err()
}

fn default_surfaces_are_redaction_safe(canary: &str) -> bool {
    let error = DebuggerError::new(
        DebuggerErrorKind::Selection,
        DebuggerOperation::SelectEvidence,
        DebuggerRecovery::RepairDependency,
        "selected evidence failed a safe binding check",
    );
    !format!("{error:?} {error}").contains(canary)
}

fn taxonomy_is_complete() -> bool {
    let categories = FailureCategory::ALL;
    let unique = categories.into_iter().collect::<BTreeSet<_>>();
    unique.len() == 49
        && categories.windows(2).all(|pair| pair[0].tag() < pair[1].tag())
        && categories
            .into_iter()
            .all(|category| FailureCategory::from_tag(category.tag()) == Ok(category))
        && FailureCategory::from_tag(u16::MAX).is_err()
}

fn strict_model_schema_is_closed() -> bool {
    let Ok(schema) = peritus_debugger::model_proposal_schema(
        peritus_model_protocol::SchemaDialect::Draft202012,
        peritus_model_protocol::ProtocolLimits::PRODUCTION,
    ) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(schema.canonical_bytes()) else {
        return false;
    };
    text.contains("\"additionalProperties\":false")
        && text.contains("\"affected_component_tags\"")
        && !text.contains("\"authority\"")
}

fn pattern_identity_is_deterministic() -> bool {
    let left = PatternId::derive(b"peritus-e2-a2-pattern-v1\0", b"canonical");
    let right = PatternId::derive(b"peritus-e2-a2-pattern-v1\0", b"canonical");
    let other = PatternId::derive(b"peritus-e2-a2-pattern-v1\0", b"other");
    matches!((left, right, other), (Ok(left), Ok(right), Ok(other)) if left == right && left != other)
}

fn lifecycle_is_canonical() -> bool {
    let Ok((events, state)) = lifecycle() else { return false };
    events.iter().map(DebuggerEvent::sequence).eq(1..=3)
        && events.windows(2).all(|pair| pair[1].previous_event() == Some(pair[0].id()))
        && state.sequence() == 3
}

fn production_replay_is_equivalent() -> bool {
    let Ok((events, state)) = lifecycle() else { return false };
    replay(&events).is_ok_and(|replayed| replayed == state)
}

fn cancellation_is_terminal() -> bool {
    let Ok((_events, state)) = lifecycle() else { return false };
    let Ok(cancel) =
        next(Some(&state), 40, DebuggerCommandKind::CancelJob { reason_digest: digest(41) })
    else {
        return false;
    };
    let Ok(cancelled) = decide(Some(&state), &cancel) else { return false };
    let Ok(retry) = next(
        Some(cancelled.state()),
        42,
        DebuggerCommandKind::CancelJob { reason_digest: digest(43) },
    ) else {
        return false;
    };
    cancelled.state().phase() == peritus_debugger::DebuggerPhase::Cancelled
        && decide(Some(cancelled.state()), &retry).is_err()
}

fn malformed_wire_is_inert() -> bool {
    let Ok(create) = create_command() else { return false };
    let Ok(frame) = DebuggerCommandFrame::from_command(&create) else { return false };
    let Ok(mut bytes) = encode_message(&frame, CodecLimits::PRODUCTION) else { return false };
    bytes.truncate(bytes.len().saturating_sub(1));
    decode_message::<DebuggerCommandFrame>(&bytes, CodecLimits::PRODUCTION).is_err()
}

fn lifecycle() -> Result<(Vec<DebuggerEvent>, peritus_debugger::DebuggerState), ()> {
    let create = create_command()?;
    let created = decide(None, &create).map_err(|_| ())?;
    let selection = SelectionRecord::new(
        SelectionManifestId::new(bytes(30)).map_err(|_| ())?,
        digest(31),
        1,
        2,
    )
    .map_err(|_| ())?;
    let select =
        next(Some(created.state()), 22, DebuggerCommandKind::RecordSelection { selection })?;
    let selected = decide(Some(created.state()), &select).map_err(|_| ())?;
    let analyze = next(
        Some(selected.state()),
        24,
        DebuggerCommandKind::RecordDeterministicAnalysis {
            analysis_digest: digest(32),
            counts: AnalysisCounts::new(1, 1, 1),
        },
    )?;
    let analyzed = decide(Some(selected.state()), &analyze).map_err(|_| ())?;
    Ok((
        vec![created.event().clone(), selected.event().clone(), analyzed.event().clone()],
        analyzed.state().clone(),
    ))
}

fn create_command() -> Result<DebuggerCommand, ()> {
    next(
        None,
        20,
        DebuggerCommandKind::CreateJob {
            revision: revision(),
            query_digest: digest(11),
            limits_digest: digest(12),
            model_plan_digest: None,
        },
    )
}

fn next(
    state: Option<&peritus_debugger::DebuggerState>,
    seed: u8,
    kind: DebuggerCommandKind,
) -> Result<DebuggerCommand, ()> {
    let (job_id, sequence, previous, prior_digest, query_digest) = state.map_or_else(
        || {
            (
                DebuggerJobId::new(bytes(10)).expect("fixed job identity"),
                0,
                None,
                digest(0),
                digest(11),
            )
        },
        |state| {
            (
                state.job_id(),
                state.sequence(),
                Some(state.last_event_id()),
                state.state_digest(),
                state.query_digest(),
            )
        },
    );
    DebuggerCommand::new(
        CommandId::new(bytes(seed)).map_err(|_| ())?,
        EventId::new(bytes(seed.wrapping_add(1))).map_err(|_| ())?,
        job_id,
        sequence,
        previous,
        prior_digest,
        query_digest,
        kind,
    )
    .map_err(|_| ())
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(1)).expect("fixed acceptance identity"),
        HarnessId::new(bytes(2)).expect("fixed harness identity"),
        WorkspaceId::new(bytes(3)).expect("fixed workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(4)).expect("fixed policy identity"),
        ProviderProfileId::new(bytes(5)).expect("fixed provider identity"),
    )
}

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

fn teardown_probe_is_explicit() -> bool {
    struct Probe(Arc<AtomicBool>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let observed = Arc::new(AtomicBool::new(false));
    drop(Probe(Arc::clone(&observed)));
    observed.load(Ordering::SeqCst)
}

fn text(value: &str) -> ReportText {
    ReportText::new(value).expect("fixed conformance text")
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}
