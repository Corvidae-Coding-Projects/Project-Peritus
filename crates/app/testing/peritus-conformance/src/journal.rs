//! Reusable authoritative-journal conformance contract and cases.

use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

/// Result classification for one append attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalAppendDisposition {
    /// A new event was durably committed.
    Committed,
    /// An exact retry resolved to the existing committed command.
    Idempotent,
}

/// Stable journal error classification required by the conformance cases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalConformanceError {
    /// The aggregate no longer satisfies the supplied absence/CAS precondition.
    StaleCas,
    /// The subject could not complete the operation for another reason.
    Infrastructure,
}

/// One fixed append input supplied by the A2 conformance suite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalAppendFixture {
    command_id: [u8; 16],
    event_id: [u8; 16],
    frame: Vec<u8>,
}

impl JournalAppendFixture {
    /// Returns the stable command identity.
    #[must_use]
    pub const fn command_id(&self) -> [u8; 16] {
        self.command_id
    }

    /// Returns the stable event identity.
    #[must_use]
    pub const fn event_id(&self) -> [u8; 16] {
        self.event_id
    }

    /// Borrows the complete canonical frame that must be retained byte-for-byte.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        &self.frame
    }
}

/// Observable outcome of an append attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalAppendObservation {
    disposition: JournalAppendDisposition,
    durable_sequence: u64,
}

impl JournalAppendObservation {
    /// Creates an append observation supplied by a subject adapter.
    #[must_use]
    pub const fn new(disposition: JournalAppendDisposition, durable_sequence: u64) -> Self {
        Self { disposition, durable_sequence }
    }

    /// Returns whether the attempt committed or resolved idempotently.
    #[must_use]
    pub const fn disposition(&self) -> JournalAppendDisposition {
        self.disposition
    }

    /// Returns the durable aggregate sequence observed after the attempt.
    #[must_use]
    pub const fn durable_sequence(&self) -> u64 {
        self.durable_sequence
    }
}

/// Exact durable state read back from the subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalSnapshot {
    event_count: u64,
    durable_sequence: u64,
    frame: Vec<u8>,
}

impl JournalSnapshot {
    /// Creates an exact journal snapshot supplied by a subject adapter.
    #[must_use]
    pub const fn new(event_count: u64, durable_sequence: u64, frame: Vec<u8>) -> Self {
        Self { event_count, durable_sequence, frame }
    }

    /// Returns the aggregate's immutable event count.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }

    /// Returns its current durable sequence.
    #[must_use]
    pub const fn durable_sequence(&self) -> u64 {
        self.durable_sequence
    }

    /// Borrows the stored exact event-frame bytes.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        &self.frame
    }
}

/// Adapter implemented by an authoritative journal under conformance test.
pub trait JournalConformanceSubject: Send {
    /// Appends `fixture` at sequence one with an aggregate-absent precondition.
    ///
    /// # Errors
    ///
    /// Returns the stable subject classification when the append does not commit or resolve.
    fn append_absent(
        &mut self,
        fixture: &JournalAppendFixture,
    ) -> Result<JournalAppendObservation, JournalConformanceError>;

    /// Closes and reopens the durable store without changing its contents.
    ///
    /// # Errors
    ///
    /// Returns [`JournalConformanceError::Infrastructure`] when restart cannot complete.
    fn restart(&mut self) -> Result<(), JournalConformanceError>;

    /// Reads the conformance aggregate's exact durable contents.
    ///
    /// # Errors
    ///
    /// Returns [`JournalConformanceError::Infrastructure`] when the durable state cannot be read.
    fn snapshot(&self) -> Result<JournalSnapshot, JournalConformanceError>;
}

struct JournalCase {
    descriptor: CaseDescriptor,
    kind: JournalCaseKind,
}

#[derive(Clone, Copy)]
enum JournalCaseKind {
    ExactRestart,
    DuplicateCommand,
    StaleCas,
}

impl<S: JournalConformanceSubject> ConformanceCase<S> for JournalCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move {
            match self.kind {
                JournalCaseKind::ExactRestart => exact_restart(subject),
                JournalCaseKind::DuplicateCommand => duplicate_command(subject),
                JournalCaseKind::StaleCas => stale_cas(subject),
            }
        })
    }
}

/// Returns the production journal conformance suite.
#[must_use]
pub fn journal_suite<S: JournalConformanceSubject>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.journal"),
            ReportText::literal("Authoritative exact-byte journal commit and restart contract"),
        ),
        vec![
            Box::new(case(
                "peritus.journal.duplicate-command",
                "An exact command retry is idempotent",
                JournalCaseKind::DuplicateCommand,
            )),
            Box::new(case(
                "peritus.journal.exact-restart",
                "Exact committed frame bytes survive restart",
                JournalCaseKind::ExactRestart,
            )),
            Box::new(case(
                "peritus.journal.stale-cas",
                "A different genesis command is rejected after the head exists",
                JournalCaseKind::StaleCas,
            )),
        ],
    )
}

fn case(id: &'static str, summary: &'static str, kind: JournalCaseKind) -> JournalCase {
    JournalCase {
        descriptor: CaseDescriptor::new(CaseId::catalog(id), ReportText::literal(summary)),
        kind,
    }
}

fn exact_restart<S: JournalConformanceSubject>(subject: &mut S) -> CaseResult {
    let fixture = fixture(1);
    let Ok(observation) = subject.append_absent(&fixture) else {
        return failed("PERITUS-JOURNAL-CONFORMANCE-001", "genesis append failed");
    };
    if observation.disposition() != JournalAppendDisposition::Committed
        || observation.durable_sequence() != 1
    {
        return failed("PERITUS-JOURNAL-CONFORMANCE-002", "genesis was not committed once");
    }
    if subject.restart().is_err() {
        return failed("PERITUS-JOURNAL-CONFORMANCE-003", "durable store restart failed");
    }
    let Ok(snapshot) = subject.snapshot() else {
        return failed("PERITUS-JOURNAL-CONFORMANCE-004", "snapshot after restart failed");
    };
    snapshot_result(&snapshot, &fixture)
}

fn duplicate_command<S: JournalConformanceSubject>(subject: &mut S) -> CaseResult {
    let fixture = fixture(1);
    if subject.append_absent(&fixture).is_err() {
        return failed("PERITUS-JOURNAL-CONFORMANCE-005", "initial append failed");
    }
    let Ok(retry) = subject.append_absent(&fixture) else {
        return failed("PERITUS-JOURNAL-CONFORMANCE-006", "exact retry did not resolve");
    };
    if retry.disposition() != JournalAppendDisposition::Idempotent {
        return failed("PERITUS-JOURNAL-CONFORMANCE-007", "exact retry appended again");
    }
    let Ok(snapshot) = subject.snapshot() else {
        return failed("PERITUS-JOURNAL-CONFORMANCE-008", "snapshot after retry failed");
    };
    snapshot_result(&snapshot, &fixture)
}

fn stale_cas<S: JournalConformanceSubject>(subject: &mut S) -> CaseResult {
    if subject.append_absent(&fixture(1)).is_err() {
        return failed("PERITUS-JOURNAL-CONFORMANCE-009", "initial append failed");
    }
    match subject.append_absent(&fixture(2)) {
        Err(JournalConformanceError::StaleCas) => CaseResult::passed(vec![Observation::new(
            ObservationId::catalog("stale-cas-rejected"),
            ObservationValue::Boolean(true),
        )]),
        _ => failed(
            "PERITUS-JOURNAL-CONFORMANCE-010",
            "different command bypassed the stale aggregate precondition",
        ),
    }
}

fn snapshot_result(snapshot: &JournalSnapshot, fixture: &JournalAppendFixture) -> CaseResult {
    let exact = snapshot.event_count() == 1
        && snapshot.durable_sequence() == 1
        && snapshot.frame() == fixture.frame();
    let observations = vec![
        Observation::new(
            ObservationId::catalog("event-count"),
            ObservationValue::Unsigned(snapshot.event_count()),
        ),
        Observation::new(
            ObservationId::catalog("exact-frame"),
            ObservationValue::Boolean(snapshot.frame() == fixture.frame()),
        ),
    ];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(
            observations,
            assertion("PERITUS-JOURNAL-CONFORMANCE-011", "durable snapshot differs from commit"),
        )
    }
}

fn fixture(discriminator: u8) -> JournalAppendFixture {
    let mut frame = b"PRTS\0\x01\xea\x60\0\x01\0\0\0\0\0\x04A2J0".to_vec();
    *frame.last_mut().expect("fixed frame is nonempty") = b'0' + discriminator;
    JournalAppendFixture {
        command_id: [discriminator; 16],
        event_id: [discriminator.saturating_add(16); 16],
        frame,
    }
}

fn failed(code: &'static str, summary: &'static str) -> CaseResult {
    CaseResult::failed(Vec::new(), assertion(code, summary))
}

fn assertion(code: &'static str, summary: &'static str) -> AssertionFailure {
    AssertionFailure::new(FailureCode::catalog(code), ReportText::literal(summary), None, None)
}
