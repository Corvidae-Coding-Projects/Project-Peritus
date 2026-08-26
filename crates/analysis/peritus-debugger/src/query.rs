//! Immutable content-addressed trace-selection queries.

#![allow(
    clippy::redundant_pub_crate,
    reason = "canonical encoding helpers cross sibling private modules without entering the public facade"
)]

use crate::{
    AnalysisSubject, DebuggerError, DebuggerErrorKind, DebuggerLimits, DebuggerOperation,
    DebuggerRecovery,
};
use peritus_trace::{ObservationKind, SpanOutcome, TraceId};
use peritus_types::Sha256Digest;

const QUERY_SCHEMA_VERSION: u16 = 1;
const QUERY_CANONICAL_DOMAIN: &[u8] = b"peritus-e2-trace-selection-query-v1\0";

/// Causal-ancestor inclusion policy for selected observations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CausalClosure {
    /// Retain only events directly matching the query filters.
    SelectedOnly,
    /// Add every same-subject transitive causal predecessor.
    IncludeAncestors,
}

/// Optional inclusive monotonic-time interval.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicWindow {
    start: u64,
    end: u64,
}

impl MonotonicWindow {
    /// Creates a nonzero, non-reversed inclusive window.
    ///
    /// # Errors
    ///
    /// Rejects a zero endpoint or an end before the start.
    pub fn new(start: u64, end: u64) -> Result<Self, DebuggerError> {
        if start == 0 || end < start {
            Err(query_error("monotonic window is zero or reversed"))
        } else {
            Ok(Self { start, end })
        }
    }
    /// Returns the inclusive start tick.
    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }
    /// Returns the inclusive end tick.
    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }
    /// Returns whether a monotonic tick lies inside the window.
    #[must_use]
    pub const fn contains(self, tick: u64) -> bool {
        self.start <= tick && tick <= self.end
    }
}

/// Closed optional query filters. Empty allowlists are rejected.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservationFilter {
    kinds: Option<Vec<ObservationKind>>,
    outcomes: Option<Vec<SpanOutcome>>,
}

impl ObservationFilter {
    /// Creates canonical optional observation-kind and span-outcome allowlists.
    ///
    /// # Errors
    ///
    /// Rejects empty, duplicate, or descending allowlists.
    pub fn new(
        kinds: Option<Vec<ObservationKind>>,
        outcomes: Option<Vec<SpanOutcome>>,
    ) -> Result<Self, DebuggerError> {
        validate_optional_set(kinds.as_deref(), "observation-kind")?;
        validate_optional_set(outcomes.as_deref(), "span-outcome")?;
        Ok(Self { kinds, outcomes })
    }

    /// Borrows the optional observation-kind allowlist.
    #[must_use]
    pub fn kinds(&self) -> Option<&[ObservationKind]> {
        self.kinds.as_deref()
    }
    /// Borrows the optional terminal-outcome allowlist.
    #[must_use]
    pub fn outcomes(&self) -> Option<&[SpanOutcome]> {
        self.outcomes.as_deref()
    }

    pub(crate) fn matches(&self, kind: ObservationKind) -> bool {
        let kind_match = self.kinds.as_ref().is_none_or(|kinds| kinds.binary_search(&kind).is_ok());
        let outcome_match = self.outcomes.as_ref().is_none_or(|outcomes| match kind {
            ObservationKind::SpanEnded(outcome) => outcomes.binary_search(&outcome).is_ok(),
            _ => false,
        });
        kind_match && outcome_match
    }
}

/// Complete immutable selection query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceSelectionQuery {
    digest: Sha256Digest,
    schema_version: u16,
    subjects: Vec<AnalysisSubject>,
    trace_ids: Option<Vec<TraceId>>,
    time_window: Option<MonotonicWindow>,
    filter: ObservationFilter,
    causal_closure: CausalClosure,
    limits: DebuggerLimits,
    canonical_bytes: Vec<u8>,
}

impl TraceSelectionQuery {
    /// Validates, freezes, canonicalizes, and content-addresses one selection query.
    ///
    /// # Errors
    ///
    /// Rejects empty/noncanonical subjects, invalid optional allowlists, and bound excess.
    pub fn new(
        subjects: Vec<AnalysisSubject>,
        trace_ids: Option<Vec<TraceId>>,
        time_window: Option<MonotonicWindow>,
        filter: ObservationFilter,
        causal_closure: CausalClosure,
        limits: DebuggerLimits,
    ) -> Result<Self, DebuggerError> {
        if subjects.is_empty() || subjects.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
            return Err(query_error("subjects must be nonempty, strictly ordered, and unique"));
        }
        limits.check(
            crate::DebuggerLimit::Subjects,
            subjects.len(),
            DebuggerOperation::ValidateQuery,
        )?;
        validate_optional_set(trace_ids.as_deref(), "trace identity")?;
        if let Some(ids) = &trace_ids {
            limits.check(
                crate::DebuggerLimit::Traces,
                ids.len(),
                DebuggerOperation::ValidateQuery,
            )?;
        }
        let mut query = Self {
            digest: Sha256Digest::new([0; 32]),
            schema_version: QUERY_SCHEMA_VERSION,
            subjects,
            trace_ids,
            time_window,
            filter,
            causal_closure,
            limits,
            canonical_bytes: Vec::new(),
        };
        query.canonical_bytes = query.encode_without_id();
        query.digest =
            crate::identity::domain_digest(QUERY_CANONICAL_DOMAIN, &query.canonical_bytes);
        Ok(query)
    }

    /// Rechecks a caller-supplied identity against canonical query content.
    ///
    /// # Errors
    ///
    /// Rejects an identity that aliases different query bytes.
    pub fn check_digest(&self, claimed: Sha256Digest) -> Result<(), DebuggerError> {
        if claimed == self.digest {
            Ok(())
        } else {
            Err(query_error("claimed query identity differs from canonical content"))
        }
    }

    /// Returns the schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }
    /// Returns the content-derived query identity used as the expected manifest identity.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Borrows subjects in canonical subject-ID order.
    #[must_use]
    pub fn subjects(&self) -> &[AnalysisSubject] {
        &self.subjects
    }
    /// Borrows the optional canonical trace allowlist.
    #[must_use]
    pub fn trace_ids(&self) -> Option<&[TraceId]> {
        self.trace_ids.as_deref()
    }
    /// Returns the optional inclusive monotonic window.
    #[must_use]
    pub const fn time_window(&self) -> Option<MonotonicWindow> {
        self.time_window
    }
    /// Borrows observation filters.
    #[must_use]
    pub const fn filter(&self) -> &ObservationFilter {
        &self.filter
    }
    /// Returns the causal closure policy.
    #[must_use]
    pub const fn causal_closure(&self) -> CausalClosure {
        self.causal_closure
    }
    /// Returns complete job limits.
    #[must_use]
    pub const fn limits(&self) -> DebuggerLimits {
        self.limits
    }
    /// Borrows canonical schema-v1 query bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub(crate) fn subject_for_binding(
        &self,
        binding: peritus_trace::CausalBinding,
    ) -> Option<&AnalysisSubject> {
        self.subjects.iter().find(|subject| subject.owns(binding))
    }

    pub(crate) fn directly_matches(&self, observation: &peritus_trace::Observation) -> bool {
        self.trace_ids.as_ref().is_none_or(|ids| ids.binary_search(&observation.trace_id()).is_ok())
            && self
                .time_window
                .is_none_or(|window| window.contains(observation.time().monotonic_tick()))
            && self.filter.matches(observation.kind())
    }

    fn encode_without_id(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(QUERY_CANONICAL_DOMAIN);
        bytes.extend_from_slice(&self.schema_version.to_be_bytes());
        encode_len(&mut bytes, self.subjects.len());
        for subject in &self.subjects {
            encode_blob(&mut bytes, &subject.canonical_bytes());
        }
        encode_option(&mut bytes, self.trace_ids.as_ref(), |output, ids| {
            encode_len(output, ids.len());
            for id in ids {
                output.extend_from_slice(id.as_bytes());
            }
        });
        encode_option(&mut bytes, self.time_window.as_ref(), |output, window| {
            output.extend_from_slice(&window.start.to_be_bytes());
            output.extend_from_slice(&window.end.to_be_bytes());
        });
        encode_option(&mut bytes, self.filter.kinds.as_ref(), |output, kinds| {
            encode_len(output, kinds.len());
            for kind in kinds {
                encode_observation_kind(output, *kind);
            }
        });
        encode_option(&mut bytes, self.filter.outcomes.as_ref(), |output, outcomes| {
            encode_len(output, outcomes.len());
            for outcome in outcomes {
                output.push(span_outcome_tag(*outcome));
            }
        });
        bytes.push(match self.causal_closure {
            CausalClosure::SelectedOnly => 1,
            CausalClosure::IncludeAncestors => 2,
        });
        for value in self.limits.values() {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }
}

pub(crate) fn encode_len(bytes: &mut Vec<u8>, length: usize) {
    bytes.extend_from_slice(&u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
}

pub(crate) fn encode_blob(bytes: &mut Vec<u8>, value: &[u8]) {
    encode_len(bytes, value.len());
    bytes.extend_from_slice(value);
}

pub(crate) fn encode_observation_kind(bytes: &mut Vec<u8>, kind: ObservationKind) {
    match kind {
        ObservationKind::SpanStarted(span) => {
            bytes.push(1);
            bytes.push(span_kind_tag(span));
        }
        ObservationKind::Diagnostic(code) => {
            bytes.push(2);
            bytes.extend_from_slice(&diagnostic_tag(code).to_be_bytes());
        }
        ObservationKind::SpanEnded(outcome) => {
            bytes.push(3);
            bytes.push(span_outcome_tag(outcome));
        }
    }
}

pub(crate) const fn span_kind_tag(kind: peritus_trace::SpanKind) -> u8 {
    match kind {
        peritus_trace::SpanKind::AgentTurn => 1,
        peritus_trace::SpanKind::Provider => 2,
        peritus_trace::SpanKind::Tool => 3,
        peritus_trace::SpanKind::Gate => 4,
        peritus_trace::SpanKind::Action => 5,
        peritus_trace::SpanKind::Recovery => 6,
        peritus_trace::SpanKind::Internal => 7,
    }
}

pub(crate) const fn span_outcome_tag(outcome: SpanOutcome) -> u8 {
    match outcome {
        SpanOutcome::Ok => 1,
        SpanOutcome::Error => 2,
        SpanOutcome::Cancelled => 3,
        SpanOutcome::Exhausted => 4,
        SpanOutcome::TimedOut => 5,
        SpanOutcome::Indeterminate => 6,
    }
}

pub(crate) const fn diagnostic_tag(code: peritus_trace::DiagnosticCode) -> u16 {
    use peritus_trace::DiagnosticCode as D;
    match code {
        D::ProviderRequestStarted => 1,
        D::ProviderRequestCompleted => 2,
        D::ProviderRequestFailed => 3,
        D::ToolDispatchStarted => 10,
        D::ToolDispatchCompleted => 11,
        D::ToolDispatchFailed => 12,
        D::GateStarted => 20,
        D::GatePassed => 21,
        D::GateFailed => 22,
        D::GateBlocked => 23,
        D::BudgetReserved => 30,
        D::BudgetCharged => 31,
        D::BudgetExhausted => 32,
        D::RetryScheduled => 40,
        D::CancellationRequested => 50,
        D::CancellationObserved => 51,
        D::RecoveryStarted => 60,
        D::RecoveryCompleted => 61,
        D::RecoveryFailed => 62,
        D::ResourceObserved => 70,
        D::ExporterFailed => 80,
        D::BufferDropped => 81,
        D::ShutdownStarted => 90,
        D::ShutdownCompleted => 91,
        _ => u16::MAX,
    }
}

fn encode_option<T>(bytes: &mut Vec<u8>, value: Option<&T>, encode: impl FnOnce(&mut Vec<u8>, &T)) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        encode(bytes, value);
    }
}

fn validate_optional_set<T: Ord>(
    values: Option<&[T]>,
    name: &'static str,
) -> Result<(), DebuggerError> {
    if values
        .is_some_and(|items| items.is_empty() || items.windows(2).any(|pair| pair[0] >= pair[1]))
    {
        Err(query_error(match name {
            "observation-kind" => "observation-kind allowlist must be nonempty and canonical",
            "span-outcome" => "span-outcome allowlist must be nonempty and canonical",
            _ => "trace allowlist must be nonempty and canonical",
        }))
    } else {
        Ok(())
    }
}

fn query_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::InvalidInput,
        DebuggerOperation::ValidateQuery,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
