//! Reusable deterministic replay conformance contract and cases.

use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

/// Exact replay result observed from a subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayObservation {
    positions: Vec<u64>,
    state: Vec<u8>,
    external_effects: u64,
}

/// Stable replay-subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayConformanceError {
    /// The replay source could not be written, reopened, or read.
    Infrastructure,
}

impl ReplayObservation {
    /// Creates a replay observation supplied by a subject adapter.
    #[must_use]
    pub const fn new(positions: Vec<u64>, state: Vec<u8>, external_effects: u64) -> Self {
        Self { positions, state, external_effects }
    }

    /// Borrows the replayed one-based positions.
    #[must_use]
    pub fn positions(&self) -> &[u64] {
        &self.positions
    }

    /// Borrows the final projected state.
    #[must_use]
    pub fn state(&self) -> &[u8] {
        &self.state
    }

    /// Returns external effects observed during replay.
    #[must_use]
    pub const fn external_effects(&self) -> u64 {
        self.external_effects
    }
}

/// Adapter implemented by a journal/projection pair under conformance test.
pub trait ReplayConformanceSubject: Send {
    /// Persists the supplied exact frames as one replay history.
    ///
    /// # Errors
    ///
    /// Returns a stable failure when the history cannot be durably prepared.
    fn seed(&mut self, frames: &[Vec<u8>]) -> Result<(), ReplayConformanceError>;

    /// Replays the persisted history from genesis through a deterministic length-prefixed fold.
    ///
    /// # Errors
    ///
    /// Returns a stable failure when replay cannot complete.
    fn replay_from_genesis(&mut self) -> Result<ReplayObservation, ReplayConformanceError>;

    /// Closes and reopens the durable state used as the replay source.
    ///
    /// # Errors
    ///
    /// Returns a stable failure when restart cannot complete.
    fn restart(&mut self) -> Result<(), ReplayConformanceError>;
}

struct ReplayCase {
    descriptor: CaseDescriptor,
    restart: bool,
}

impl<S: ReplayConformanceSubject> ConformanceCase<S> for ReplayCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { run_replay(subject, self.restart) })
    }
}

/// Returns the production replay conformance suite.
#[must_use]
pub fn replay_suite<S: ReplayConformanceSubject>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.replay"),
            ReportText::literal("Deterministic replay-from-genesis and no-effect contract"),
        ),
        vec![
            Box::new(replay_case(
                "peritus.replay.deterministic",
                "Repeated replay produces one exact state without effects",
                false,
            )),
            Box::new(replay_case(
                "peritus.replay.restart",
                "Replay after restart matches the canonical fold",
                true,
            )),
        ],
    )
}

fn replay_case(id: &'static str, summary: &'static str, restart: bool) -> ReplayCase {
    ReplayCase {
        descriptor: CaseDescriptor::new(CaseId::catalog(id), ReportText::literal(summary)),
        restart,
    }
}

fn run_replay<S: ReplayConformanceSubject>(subject: &mut S, restart: bool) -> CaseResult {
    let frames = vec![b"writer".to_vec(), b"reviewer".to_vec(), b"fixer".to_vec()];
    if subject.seed(&frames).is_err() {
        return failed("PERITUS-REPLAY-CONFORMANCE-001", "could not seed replay history");
    }
    if restart && subject.restart().is_err() {
        return failed("PERITUS-REPLAY-CONFORMANCE-002", "could not restart replay source");
    }
    let Ok(first) = subject.replay_from_genesis() else {
        return failed("PERITUS-REPLAY-CONFORMANCE-003", "first replay failed");
    };
    let Ok(second) = subject.replay_from_genesis() else {
        return failed("PERITUS-REPLAY-CONFORMANCE-004", "second replay failed");
    };
    let expected = reference_fold(&frames);
    let correct = first == second
        && first.positions() == [1, 2, 3]
        && first.state() == expected
        && first.external_effects() == 0;
    let observations = vec![
        Observation::new(
            ObservationId::catalog("deterministic"),
            ObservationValue::Boolean(first == second),
        ),
        Observation::new(
            ObservationId::catalog("external-effects"),
            ObservationValue::Unsigned(first.external_effects()),
        ),
    ];
    if correct {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(
            observations,
            assertion("PERITUS-REPLAY-CONFORMANCE-005", "replay differs from canonical fold"),
        )
    }
}

fn reference_fold(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut state = Vec::new();
    for frame in frames {
        state.extend_from_slice(&(frame.len() as u64).to_be_bytes());
        state.extend_from_slice(frame);
    }
    state
}

fn failed(code: &'static str, summary: &'static str) -> CaseResult {
    CaseResult::failed(Vec::new(), assertion(code, summary))
}

fn assertion(code: &'static str, summary: &'static str) -> AssertionFailure {
    AssertionFailure::new(FailureCode::catalog(code), ReportText::literal(summary), None, None)
}
