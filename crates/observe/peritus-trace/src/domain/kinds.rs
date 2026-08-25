//! Closed span, diagnostic, outcome, and status vocabularies.

/// Closed span role vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpanKind {
    /// Agent-turn orchestration work.
    AgentTurn,
    /// Model-provider work.
    Provider,
    /// Tool planning or dispatch work.
    Tool,
    /// Gate planning or execution work.
    Gate,
    /// One proposed or dispatched action.
    Action,
    /// Crash or startup recovery work.
    Recovery,
    /// Internal content-free bookkeeping.
    Internal,
}

impl SpanKind {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::AgentTurn => 1,
            Self::Provider => 2,
            Self::Tool => 3,
            Self::Gate => 4,
            Self::Action => 5,
            Self::Recovery => 6,
            Self::Internal => 7,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::AgentTurn),
            2 => Some(Self::Provider),
            3 => Some(Self::Tool),
            4 => Some(Self::Gate),
            5 => Some(Self::Action),
            6 => Some(Self::Recovery),
            7 => Some(Self::Internal),
            _ => None,
        }
    }
}

/// Closed terminal span outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpanOutcome {
    /// Operation completed successfully.
    Ok,
    /// Operation failed explicitly.
    Error,
    /// Operation was cancelled.
    Cancelled,
    /// A configured resource budget was exhausted.
    Exhausted,
    /// Operation timed out.
    TimedOut,
    /// Completion could not be determined.
    Indeterminate,
}

impl SpanOutcome {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Ok => 1,
            Self::Error => 2,
            Self::Cancelled => 3,
            Self::Exhausted => 4,
            Self::TimedOut => 5,
            Self::Indeterminate => 6,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Ok),
            2 => Some(Self::Error),
            3 => Some(Self::Cancelled),
            4 => Some(Self::Exhausted),
            5 => Some(Self::TimedOut),
            6 => Some(Self::Indeterminate),
            _ => None,
        }
    }
}

/// Stable content-free diagnostic event vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// A provider request began.
    ProviderRequestStarted,
    /// A provider request completed.
    ProviderRequestCompleted,
    /// A provider request failed.
    ProviderRequestFailed,
    /// A tool dispatch began.
    ToolDispatchStarted,
    /// A tool dispatch completed.
    ToolDispatchCompleted,
    /// A tool dispatch failed.
    ToolDispatchFailed,
    /// A gate evaluation began.
    GateStarted,
    /// A gate passed with complete evidence.
    GatePassed,
    /// A gate failed its candidate.
    GateFailed,
    /// A gate was dependency blocked.
    GateBlocked,
    /// Budget was reserved.
    BudgetReserved,
    /// Budget usage was charged.
    BudgetCharged,
    /// Budget was exhausted.
    BudgetExhausted,
    /// A bounded retry was scheduled.
    RetryScheduled,
    /// Cancellation was requested.
    CancellationRequested,
    /// Cancellation was observed.
    CancellationObserved,
    /// Recovery began.
    RecoveryStarted,
    /// Recovery completed.
    RecoveryCompleted,
    /// Recovery failed.
    RecoveryFailed,
    /// A resource counter was observed.
    ResourceObserved,
    /// An exporter failed explicitly.
    ExporterFailed,
    /// A bounded buffer dropped or rejected an item.
    BufferDropped,
    /// Shutdown flushing began.
    ShutdownStarted,
    /// Shutdown flushing completed.
    ShutdownCompleted,
}

impl DiagnosticCode {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::ProviderRequestStarted => 1,
            Self::ProviderRequestCompleted => 2,
            Self::ProviderRequestFailed => 3,
            Self::ToolDispatchStarted => 10,
            Self::ToolDispatchCompleted => 11,
            Self::ToolDispatchFailed => 12,
            Self::GateStarted => 20,
            Self::GatePassed => 21,
            Self::GateFailed => 22,
            Self::GateBlocked => 23,
            Self::BudgetReserved => 30,
            Self::BudgetCharged => 31,
            Self::BudgetExhausted => 32,
            Self::RetryScheduled => 40,
            Self::CancellationRequested => 50,
            Self::CancellationObserved => 51,
            Self::RecoveryStarted => 60,
            Self::RecoveryCompleted => 61,
            Self::RecoveryFailed => 62,
            Self::ResourceObserved => 70,
            Self::ExporterFailed => 80,
            Self::BufferDropped => 81,
            Self::ShutdownStarted => 90,
            Self::ShutdownCompleted => 91,
        }
    }

    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::ProviderRequestStarted),
            2 => Some(Self::ProviderRequestCompleted),
            3 => Some(Self::ProviderRequestFailed),
            10 => Some(Self::ToolDispatchStarted),
            11 => Some(Self::ToolDispatchCompleted),
            12 => Some(Self::ToolDispatchFailed),
            20 => Some(Self::GateStarted),
            21 => Some(Self::GatePassed),
            22 => Some(Self::GateFailed),
            23 => Some(Self::GateBlocked),
            30 => Some(Self::BudgetReserved),
            31 => Some(Self::BudgetCharged),
            32 => Some(Self::BudgetExhausted),
            40 => Some(Self::RetryScheduled),
            50 => Some(Self::CancellationRequested),
            51 => Some(Self::CancellationObserved),
            60 => Some(Self::RecoveryStarted),
            61 => Some(Self::RecoveryCompleted),
            62 => Some(Self::RecoveryFailed),
            70 => Some(Self::ResourceObserved),
            80 => Some(Self::ExporterFailed),
            81 => Some(Self::BufferDropped),
            90 => Some(Self::ShutdownStarted),
            91 => Some(Self::ShutdownCompleted),
            _ => None,
        }
    }
}

/// Stable status values allowed in default diagnostics and metrics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StatusCode {
    /// Operation is pending.
    Pending,
    /// Operation succeeded.
    Success,
    /// Candidate or operation failed.
    Failure,
    /// Infrastructure failed.
    InfrastructureFailure,
    /// Operation was cancelled.
    Cancelled,
    /// Operation timed out.
    TimedOut,
    /// Result is indeterminate.
    Indeterminate,
}

impl StatusCode {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Pending => 1,
            Self::Success => 2,
            Self::Failure => 3,
            Self::InfrastructureFailure => 4,
            Self::Cancelled => 5,
            Self::TimedOut => 6,
            Self::Indeterminate => 7,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Pending),
            2 => Some(Self::Success),
            3 => Some(Self::Failure),
            4 => Some(Self::InfrastructureFailure),
            5 => Some(Self::Cancelled),
            6 => Some(Self::TimedOut),
            7 => Some(Self::Indeterminate),
            _ => None,
        }
    }
}

/// One span lifecycle or diagnostic observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationKind {
    /// Opens a new span.
    SpanStarted(SpanKind),
    /// Adds one content-free diagnostic to an open span.
    Diagnostic(DiagnosticCode),
    /// Closes an open span exactly once.
    SpanEnded(SpanOutcome),
}
