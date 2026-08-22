//! Exact, one-shot scripted calls and streams.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

/// One exact expected request and caller-owned outcome.
#[derive(Debug)]
pub struct ExpectedCall<Request, Outcome> {
    expected: Request,
    outcome: Outcome,
}

impl<Request, Outcome> ExpectedCall<Request, Outcome> {
    /// Creates one exact script step.
    #[must_use]
    pub const fn new(expected: Request, outcome: Outcome) -> Self {
        Self { expected, outcome }
    }

    /// Borrows the expected request.
    #[must_use]
    pub const fn expected(&self) -> &Request {
        &self.expected
    }
}

/// One request observed by a scripted boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedCall<Request> {
    ordinal: u64,
    request: Request,
    matched: bool,
}

impl<Request> ObservedCall<Request> {
    /// Returns the one-based observation ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    /// Borrows the exact observed request.
    #[must_use]
    pub const fn request(&self) -> &Request {
        &self.request
    }

    /// Returns whether the request matched and consumed the next step.
    #[must_use]
    pub const fn matched(&self) -> bool {
        self.matched
    }
}

/// The stable category of a script protocol violation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScriptViolationKind {
    /// A request differed from the next exact expectation.
    RequestMismatch,
    /// A request arrived after every expectation was consumed.
    UnexpectedCall,
    /// The observation ordinal could not be represented.
    OrdinalOverflow,
}

/// A call did not follow an exact script.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScriptViolation {
    kind: ScriptViolationKind,
    ordinal: u64,
}

impl ScriptViolation {
    /// Returns the violation category.
    #[must_use]
    pub const fn kind(self) -> ScriptViolationKind {
        self.kind
    }

    /// Returns the one-based call ordinal, or [`u64::MAX`] for ordinal overflow.
    #[must_use]
    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }

    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self.kind {
            ScriptViolationKind::RequestMismatch => "PERITUS-TEST-SCRIPT-001",
            ScriptViolationKind::UnexpectedCall => "PERITUS-TEST-SCRIPT-002",
            ScriptViolationKind::OrdinalOverflow => "PERITUS-TEST-SCRIPT-003",
        }
    }
}

impl fmt::Display for ScriptViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "script violation {:?} at call {}", self.kind, self.ordinal)
    }
}

impl Error for ScriptViolation {}

/// Proof that a one-shot script still contains unconsumed steps.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScriptIncomplete {
    remaining: usize,
}

impl ScriptIncomplete {
    /// Returns the exact number of unconsumed steps.
    #[must_use]
    pub const fn remaining(self) -> usize {
        self.remaining
    }

    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        "PERITUS-TEST-SCRIPT-004"
    }
}

impl fmt::Display for ScriptIncomplete {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "script has {} unconsumed steps", self.remaining)
    }
}

impl Error for ScriptIncomplete {}

/// A non-cloneable, exact, ordered sequence of request/outcome pairs.
#[derive(Debug)]
pub struct ScriptedCalls<Request, Outcome> {
    remaining: VecDeque<ExpectedCall<Request, Outcome>>,
    observed: Vec<ObservedCall<Request>>,
}

impl<Request, Outcome> ScriptedCalls<Request, Outcome> {
    /// Creates a script in iterator order.
    #[must_use]
    pub fn new(steps: impl IntoIterator<Item = ExpectedCall<Request, Outcome>>) -> Self {
        Self { remaining: steps.into_iter().collect(), observed: Vec::new() }
    }

    /// Returns observed requests in exact call order.
    #[must_use]
    pub fn observed(&self) -> &[ObservedCall<Request>] {
        &self.observed
    }

    /// Borrows the next expected request without consuming it.
    #[must_use]
    pub fn peek_expected(&self) -> Option<&Request> {
        self.remaining.front().map(ExpectedCall::expected)
    }

    /// Returns the number of unconsumed outcomes.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.remaining.len()
    }

    /// Verifies that every outcome was consumed by a matching request.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptIncomplete`] with the exact remaining count.
    pub fn verify_complete(&self) -> Result<(), ScriptIncomplete> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(ScriptIncomplete { remaining: self.remaining.len() })
        }
    }
}

impl<Request: PartialEq, Outcome> ScriptedCalls<Request, Outcome> {
    /// Records `request` and returns the one-shot outcome for an exact match.
    ///
    /// A mismatch is recorded but does not consume the expected request or its outcome.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptViolation`] for mismatch, exhaustion, or ordinal overflow.
    pub fn respond(&mut self, request: Request) -> Result<Outcome, ScriptViolation> {
        let ordinal =
            u64::try_from(self.observed.len()).ok().and_then(|value| value.checked_add(1)).ok_or(
                ScriptViolation { kind: ScriptViolationKind::OrdinalOverflow, ordinal: u64::MAX },
            )?;
        let Some(step) = self.remaining.front() else {
            self.observed.push(ObservedCall { ordinal, request, matched: false });
            return Err(ScriptViolation { kind: ScriptViolationKind::UnexpectedCall, ordinal });
        };
        let matched = step.expected == request;
        self.observed.push(ObservedCall { ordinal, request, matched });
        if !matched {
            return Err(ScriptViolation { kind: ScriptViolationKind::RequestMismatch, ordinal });
        }
        match self.remaining.pop_front() {
            Some(step) => Ok(step.outcome),
            None => Err(ScriptViolation { kind: ScriptViolationKind::UnexpectedCall, ordinal }),
        }
    }
}

/// Failure to consume a stream step counter exactly.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StreamError {
    /// The consumed-step count could not be represented.
    CountOverflow,
}

impl StreamError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        "PERITUS-TEST-STREAM-001"
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("scripted stream consumed-step counter overflowed")
    }
}

impl Error for StreamError {}

/// A non-cloneable exact sequence of caller-owned stream steps.
///
/// A step may itself be a result, interruption, duplicate, or protocol-owned control item.
#[derive(Debug)]
pub struct ScriptedStream<Step> {
    remaining: VecDeque<Step>,
    consumed: u64,
}

impl<Step> ScriptedStream<Step> {
    /// Creates a stream script in iterator order.
    #[must_use]
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self { remaining: steps.into_iter().collect(), consumed: 0 }
    }

    /// Consumes the next exact step, or returns `None` at script end.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::CountOverflow`] instead of wrapping the observation count.
    pub fn next_step(&mut self) -> Result<Option<Step>, StreamError> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        let consumed = self.consumed.checked_add(1).ok_or(StreamError::CountOverflow)?;
        let step = self.remaining.pop_front();
        self.consumed = consumed;
        Ok(step)
    }

    /// Returns the number of consumed steps.
    #[must_use]
    pub const fn consumed(&self) -> u64 {
        self.consumed
    }

    /// Returns the number of remaining steps.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.remaining.len()
    }

    /// Verifies that no scripted steps remain.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptIncomplete`] with the exact remaining count.
    pub fn verify_complete(&self) -> Result<(), ScriptIncomplete> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(ScriptIncomplete { remaining: self.remaining.len() })
        }
    }
}
