//! Typed conformance failures with stable machine-readable categories.

use crate::{CaseId, FailureCode, ObservationValue, ReportText, ReportTextError};

/// High-level analysis category derived from typed case failures.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureKind {
    /// The subject completed the case but violated an asserted contract.
    ContractViolation,
    /// Setup, execution machinery, panic containment, or teardown failed.
    Infrastructure,
}

/// The runner phase in which an unwinding panic was caught.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailurePhase {
    /// Suite metadata or case enumeration.
    SuiteDefinition,
    /// Subject metadata discovery.
    SubjectDefinition,
    /// Case metadata discovery.
    CaseDefinition,
    /// Subject creation.
    Setup,
    /// The case body.
    Exercise,
    /// Subject teardown.
    Teardown,
}

/// An ordinary contract assertion that was not satisfied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssertionFailure {
    code: FailureCode,
    summary: ReportText,
    expected: Option<ObservationValue>,
    actual: Option<ObservationValue>,
}

impl AssertionFailure {
    /// Creates an assertion failure with optional typed expected and actual values.
    #[must_use]
    pub const fn new(
        code: FailureCode,
        summary: ReportText,
        expected: Option<ObservationValue>,
        actual: Option<ObservationValue>,
    ) -> Self {
        Self { code, summary, expected, actual }
    }

    /// Returns the stable assertion category.
    #[must_use]
    pub const fn code(&self) -> &FailureCode {
        &self.code
    }

    /// Returns the human-readable failure summary.
    #[must_use]
    pub const fn summary(&self) -> &ReportText {
        &self.summary
    }

    /// Returns the expected value when safely representable in a report.
    #[must_use]
    pub const fn expected(&self) -> Option<&ObservationValue> {
        self.expected.as_ref()
    }

    /// Returns the observed value when safely representable in a report.
    #[must_use]
    pub const fn actual(&self) -> Option<&ObservationValue> {
        self.actual.as_ref()
    }
}

/// A typed subject setup or teardown failure supplied by a subject factory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectFailure {
    code: FailureCode,
    summary: ReportText,
}

impl SubjectFailure {
    /// Creates a subject lifecycle failure.
    #[must_use]
    pub const fn new(code: FailureCode, summary: ReportText) -> Self {
        Self { code, summary }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> &FailureCode {
        &self.code
    }

    /// Returns the bounded or redacted summary supplied by the factory.
    #[must_use]
    pub const fn summary(&self) -> &ReportText {
        &self.summary
    }
}

/// One normalized message from caught Rust unwinding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanicMessage {
    text: ReportText,
    original_length: usize,
}

impl PanicMessage {
    /// The maximum number of UTF-8 bytes retained from a panic message.
    pub const MAX_LENGTH: usize = ReportText::MAX_LENGTH;

    pub(crate) fn normalized(message: &str) -> Self {
        match ReportText::new(message) {
            Ok(text) => Self { text, original_length: message.len() },
            Err(ReportTextError::Empty) => Self {
                text: ReportText::literal("panic payload contained an empty message"),
                original_length: 0,
            },
            Err(ReportTextError::TooLong) => Self {
                text: ReportText::literal(
                    "panic message omitted because it exceeded report limits",
                ),
                original_length: message.len(),
            },
        }
    }

    /// Returns the bounded message or explicit omission diagnostic.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }

    /// Returns the validated report text.
    #[must_use]
    pub const fn text(&self) -> &ReportText {
        &self.text
    }

    /// Returns the original UTF-8 byte length before an oversized message was omitted.
    #[must_use]
    pub const fn original_length(&self) -> usize {
        self.original_length
    }

    /// Returns whether the original message exceeded the report-text limit and was omitted.
    #[must_use]
    pub const fn was_oversized(&self) -> bool {
        self.original_length > ReportText::MAX_LENGTH
    }
}

/// One or more panics caught while running or destroying one asynchronous operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanicFailure {
    phase: FailurePhase,
    messages: Vec<PanicMessage>,
}

impl PanicFailure {
    pub(crate) const fn new(phase: FailurePhase, messages: Vec<PanicMessage>) -> Self {
        Self { phase, messages }
    }

    pub(crate) fn push_message(&mut self, message: PanicMessage) {
        self.messages.push(message);
    }

    /// Returns the phase whose operation panicked.
    #[must_use]
    pub const fn phase(&self) -> FailurePhase {
        self.phase
    }

    /// Returns each caught panic in deterministic occurrence order.
    #[must_use]
    pub fn messages(&self) -> &[PanicMessage] {
        &self.messages
    }
}

/// The primary failure produced before teardown of one case subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaseFailure {
    /// The case returned a typed failed assertion.
    Assertion(AssertionFailure),
    /// The subject factory returned a typed setup failure.
    Setup(SubjectFailure),
    /// Setup or case execution unwound with a caught panic.
    Panic(PanicFailure),
}

impl CaseFailure {
    /// Returns the analysis category derived from this typed failure.
    #[must_use]
    pub const fn kind(&self) -> FailureKind {
        match self {
            Self::Assertion(_) => FailureKind::ContractViolation,
            Self::Setup(_) | Self::Panic(_) => FailureKind::Infrastructure,
        }
    }
}

/// Failure produced while tearing down a successfully created subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TeardownFailure {
    /// The subject factory returned a typed teardown failure.
    Subject(SubjectFailure),
    /// Teardown unwound with a caught panic.
    Panic(PanicFailure),
}

impl TeardownFailure {
    /// Returns the analysis category for teardown failures.
    #[must_use]
    pub const fn kind(&self) -> FailureKind {
        FailureKind::Infrastructure
    }
}

/// Details of a duplicate case identifier that invalidated a suite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateCaseIdFailure {
    id: CaseId,
}

impl DuplicateCaseIdFailure {
    pub(crate) const fn new(id: CaseId) -> Self {
        Self { id }
    }

    /// Returns the duplicated identifier.
    #[must_use]
    pub const fn id(&self) -> &CaseId {
        &self.id
    }
}

/// A definition failure that prevents every case from running.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SuiteFailure {
    /// Two cases declared the same stable identifier.
    DuplicateCaseId(DuplicateCaseIdFailure),
    /// Suite, subject, or case definition code unwound.
    Panic(PanicFailure),
}
