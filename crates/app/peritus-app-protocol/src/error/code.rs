//! Stable machine-actionable application error codes.

/// Stable application-protocol failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AppErrorCode {
    /// The PRTS primitive format is unsupported.
    UnsupportedFormat = 1,
    /// The frame family is unsupported in this position.
    UnsupportedFamily = 2,
    /// The family schema version is unsupported.
    UnsupportedSchema = 3,
    /// A closed enum used an unassigned tag.
    UnknownTag = 4,
    /// A canonical frame or value is malformed.
    MalformedFrame = 5,
    /// Input ended before its declared value was complete.
    TruncatedFrame = 6,
    /// Bytes remained after the declared frame or value.
    TrailingBytes = 7,
    /// A configured resource ceiling was exceeded.
    LimitExceeded = 8,
    /// A nominal identity violated its invariant.
    InvalidIdentifier = 9,
    /// A version or range violated its invariant.
    InvalidVersion = 10,
    /// Peers share no negotiable protocol version.
    IncompatibleVersion = 11,
    /// The server cannot provide a required feature.
    MissingRequiredFeature = 12,
    /// A resource-limit set was invalid.
    InvalidLimits = 13,
    /// A request named the wrong negotiated or durable session.
    SessionMismatch = 20,
    /// One actor reused an idempotency key within a durable session for a different request.
    IdempotencyConflict = 21,
    /// The bounded idempotency window has no free entry.
    IdempotencyCapacity = 22,
    /// The command's expected revision is not current.
    StaleRevision = 23,
    /// The exact B3 command frame is not a registered command.
    InvalidCommandFrame = 24,
    /// Outer request metadata disagrees with the exact B3 envelope.
    CommandBindingMismatch = 25,
    /// A committed event cursor range is empty, reversed, or unrepresentable.
    InvalidEventRange = 26,
    /// An operation is illegal in the current subscription state.
    SubscriptionState = 30,
    /// The subscriber cannot receive a contiguous event range.
    SubscriptionGap = 31,
    /// An acknowledgement exceeds the contiguous delivered prefix.
    IllegalAcknowledgement = 32,
    /// The negotiated in-flight delivery window is full.
    Backpressure = 33,
    /// An operation is illegal in the current artifact-transfer state.
    ArtifactState = 40,
    /// An artifact chunk offset is not the next contiguous offset.
    ArtifactOrdering = 41,
    /// Artifact bytes disagree with the declared total size.
    ArtifactSize = 42,
    /// The final artifact digest does not match the declaration.
    ArtifactDigest = 43,
    /// A prompt answer does not match the outstanding prompt.
    PromptMismatch = 50,
    /// A prompt answer is stale or already consumed.
    PromptStale = 51,
    /// An operation is illegal in the current terminal state.
    TerminalState = 60,
    /// A terminal chunk sequence is not contiguous.
    TerminalOrdering = 61,
    /// A mutating request was submitted through a read-only protocol relationship.
    ReadOnly = 70,
    /// The daemon has not reached the required readiness state.
    NotReady = 71,
    /// The operation was explicitly cancelled.
    Cancelled = 72,
    /// An invariant failed without a more specific public category.
    Internal = 255,
}

impl AppErrorCode {
    /// Returns the permanently assigned numeric wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        self as u16
    }

    /// Recovers a code from its permanently assigned numeric wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        Some(match tag {
            1 => Self::UnsupportedFormat,
            2 => Self::UnsupportedFamily,
            3 => Self::UnsupportedSchema,
            4 => Self::UnknownTag,
            5 => Self::MalformedFrame,
            6 => Self::TruncatedFrame,
            7 => Self::TrailingBytes,
            8 => Self::LimitExceeded,
            9 => Self::InvalidIdentifier,
            10 => Self::InvalidVersion,
            11 => Self::IncompatibleVersion,
            12 => Self::MissingRequiredFeature,
            13 => Self::InvalidLimits,
            20 => Self::SessionMismatch,
            21 => Self::IdempotencyConflict,
            22 => Self::IdempotencyCapacity,
            23 => Self::StaleRevision,
            24 => Self::InvalidCommandFrame,
            25 => Self::CommandBindingMismatch,
            26 => Self::InvalidEventRange,
            30 => Self::SubscriptionState,
            31 => Self::SubscriptionGap,
            32 => Self::IllegalAcknowledgement,
            33 => Self::Backpressure,
            40 => Self::ArtifactState,
            41 => Self::ArtifactOrdering,
            42 => Self::ArtifactSize,
            43 => Self::ArtifactDigest,
            50 => Self::PromptMismatch,
            51 => Self::PromptStale,
            60 => Self::TerminalState,
            61 => Self::TerminalOrdering,
            70 => Self::ReadOnly,
            71 => Self::NotReady,
            72 => Self::Cancelled,
            255 => Self::Internal,
            _ => return None,
        })
    }

    /// Returns the stable kebab-case machine name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "unsupported-format",
            Self::UnsupportedFamily => "unsupported-family",
            Self::UnsupportedSchema => "unsupported-schema",
            Self::UnknownTag => "unknown-tag",
            Self::MalformedFrame => "malformed-frame",
            Self::TruncatedFrame => "truncated-frame",
            Self::TrailingBytes => "trailing-bytes",
            Self::LimitExceeded => "limit-exceeded",
            Self::InvalidIdentifier => "invalid-identifier",
            Self::InvalidVersion => "invalid-version",
            Self::IncompatibleVersion => "incompatible-version",
            Self::MissingRequiredFeature => "missing-required-feature",
            Self::InvalidLimits => "invalid-limits",
            Self::SessionMismatch => "session-mismatch",
            Self::IdempotencyConflict => "idempotency-conflict",
            Self::IdempotencyCapacity => "idempotency-capacity",
            Self::StaleRevision => "stale-revision",
            Self::InvalidCommandFrame => "invalid-command-frame",
            Self::CommandBindingMismatch => "command-binding-mismatch",
            Self::InvalidEventRange => "invalid-event-range",
            Self::SubscriptionState => "subscription-state",
            Self::SubscriptionGap => "subscription-gap",
            Self::IllegalAcknowledgement => "illegal-acknowledgement",
            Self::Backpressure => "backpressure",
            Self::ArtifactState => "artifact-state",
            Self::ArtifactOrdering => "artifact-ordering",
            Self::ArtifactSize => "artifact-size",
            Self::ArtifactDigest => "artifact-digest",
            Self::PromptMismatch => "prompt-mismatch",
            Self::PromptStale => "prompt-stale",
            Self::TerminalState => "terminal-state",
            Self::TerminalOrdering => "terminal-ordering",
            Self::ReadOnly => "read-only",
            Self::NotReady => "not-ready",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
        }
    }

    /// Returns the default retry classification for this code.
    #[must_use]
    pub const fn default_retry(self) -> RetryDisposition {
        match self {
            Self::Backpressure | Self::NotReady => RetryDisposition::AfterRecovery,
            Self::SubscriptionGap | Self::SessionMismatch => RetryDisposition::Reconnect,
            Self::StaleRevision => RetryDisposition::NewRequest,
            _ => RetryDisposition::Never,
        }
    }

    /// Returns the default subsystem responsible for this code.
    #[must_use]
    pub const fn default_subsystem(self) -> ResponsibleSubsystem {
        match self {
            Self::UnsupportedFormat
            | Self::UnsupportedFamily
            | Self::UnsupportedSchema
            | Self::UnknownTag
            | Self::MalformedFrame
            | Self::TruncatedFrame
            | Self::TrailingBytes
            | Self::LimitExceeded
            | Self::InvalidIdentifier => ResponsibleSubsystem::Codec,
            Self::InvalidVersion
            | Self::IncompatibleVersion
            | Self::MissingRequiredFeature
            | Self::InvalidLimits => ResponsibleSubsystem::Negotiation,
            Self::SessionMismatch | Self::ReadOnly => ResponsibleSubsystem::Session,
            Self::IdempotencyConflict
            | Self::IdempotencyCapacity
            | Self::StaleRevision
            | Self::InvalidCommandFrame
            | Self::CommandBindingMismatch
            | Self::InvalidEventRange => ResponsibleSubsystem::Command,
            Self::SubscriptionState
            | Self::SubscriptionGap
            | Self::IllegalAcknowledgement
            | Self::Backpressure => ResponsibleSubsystem::Subscription,
            Self::ArtifactState
            | Self::ArtifactOrdering
            | Self::ArtifactSize
            | Self::ArtifactDigest => ResponsibleSubsystem::Artifact,
            Self::PromptMismatch | Self::PromptStale => ResponsibleSubsystem::Prompt,
            Self::TerminalState | Self::TerminalOrdering => ResponsibleSubsystem::Terminal,
            Self::NotReady | Self::Cancelled => ResponsibleSubsystem::Daemon,
            Self::Internal => ResponsibleSubsystem::Internal,
        }
    }
}

/// Machine-actionable retry guidance independent of diagnostic prose.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum RetryDisposition {
    /// Retrying cannot make the same request valid.
    Never = 1,
    /// Retry the exact same request and idempotency key.
    SameRequest = 2,
    /// Construct a new request against refreshed state.
    NewRequest = 3,
    /// Retry only after the named subsystem reports recovery.
    AfterRecovery = 4,
    /// Establish a new protocol relationship before retrying.
    Reconnect = 5,
}

impl RetryDisposition {
    /// Returns the permanently assigned numeric wire tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// Recovers retry guidance from its permanently assigned wire tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Never),
            2 => Some(Self::SameRequest),
            3 => Some(Self::NewRequest),
            4 => Some(Self::AfterRecovery),
            5 => Some(Self::Reconnect),
            _ => None,
        }
    }

    /// Returns the stable kebab-case machine name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::SameRequest => "same-request",
            Self::NewRequest => "new-request",
            Self::AfterRecovery => "after-recovery",
            Self::Reconnect => "reconnect",
        }
    }
}

/// Subsystem responsible for classifying or recovering one failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ResponsibleSubsystem {
    /// Canonical framing and decoding.
    Codec = 1,
    /// Version, feature, and limit negotiation.
    Negotiation = 2,
    /// Negotiated relationship and session binding.
    Session = 3,
    /// Command admission and exact binding.
    Command = 4,
    /// Event subscription delivery.
    Subscription = 5,
    /// Artifact transfer.
    Artifact = 6,
    /// Approval or user-input prompting.
    Prompt = 7,
    /// Terminal attachment and streaming.
    Terminal = 8,
    /// Daemon readiness and lifecycle.
    Daemon = 9,
    /// Internal invariant handling.
    Internal = 255,
}

impl ResponsibleSubsystem {
    /// Returns the permanently assigned numeric wire tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// Recovers a subsystem from its permanently assigned wire tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Codec),
            2 => Some(Self::Negotiation),
            3 => Some(Self::Session),
            4 => Some(Self::Command),
            5 => Some(Self::Subscription),
            6 => Some(Self::Artifact),
            7 => Some(Self::Prompt),
            8 => Some(Self::Terminal),
            9 => Some(Self::Daemon),
            255 => Some(Self::Internal),
            _ => None,
        }
    }

    /// Returns the stable kebab-case machine name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codec => "codec",
            Self::Negotiation => "negotiation",
            Self::Session => "session",
            Self::Command => "command",
            Self::Subscription => "subscription",
            Self::Artifact => "artifact",
            Self::Prompt => "prompt",
            Self::Terminal => "terminal",
            Self::Daemon => "daemon",
            Self::Internal => "internal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigned_error_tags_round_trip() {
        for code in [
            AppErrorCode::UnsupportedFormat,
            AppErrorCode::IdempotencyConflict,
            AppErrorCode::ArtifactDigest,
            AppErrorCode::Internal,
        ] {
            assert_eq!(AppErrorCode::from_tag(code.tag()), Some(code));
        }
        assert_eq!(AppErrorCode::from_tag(0), None);
    }
}
