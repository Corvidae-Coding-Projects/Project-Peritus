//! Stable application family, payload, and error-code allocation registry.

use crate::AppErrorCode;

/// One stable payload allocation within an outer application family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AppPayloadDescriptor {
    /// Nonzero payload tag.
    pub tag: u16,
    /// Stable kebab-case payload name.
    pub name: &'static str,
}

/// One immutable schema-v1 application family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppFamilyDescriptor {
    /// PRTS family tag.
    pub tag: u16,
    /// Stable kebab-case family name.
    pub name: &'static str,
    /// Current nonzero schema version.
    pub schema_version: u16,
    /// Closed payload allocations in ascending tag order.
    pub payloads: &'static [AppPayloadDescriptor],
}

/// One stable public error allocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ErrorCodeDescriptor {
    /// Numeric wire code.
    pub tag: u16,
    /// Stable kebab-case code.
    pub name: &'static str,
}

const CLIENT_HELLO: &[AppPayloadDescriptor] =
    &[AppPayloadDescriptor { tag: 1, name: "client-hello" }];

const SERVER_HELLO: &[AppPayloadDescriptor] = &[
    AppPayloadDescriptor { tag: 1, name: "compatible" },
    AppPayloadDescriptor { tag: 2, name: "downgraded" },
    AppPayloadDescriptor { tag: 3, name: "incompatible" },
];

const REQUESTS: &[AppPayloadDescriptor] = &[
    AppPayloadDescriptor { tag: 1, name: "submit-command" },
    AppPayloadDescriptor { tag: 2, name: "subscribe" },
    AppPayloadDescriptor { tag: 3, name: "open-artifact" },
    AppPayloadDescriptor { tag: 4, name: "cancel-artifact" },
    AppPayloadDescriptor { tag: 5, name: "answer-prompt" },
    AppPayloadDescriptor { tag: 6, name: "cancel-prompt" },
    AppPayloadDescriptor { tag: 7, name: "attach-terminal" },
    AppPayloadDescriptor { tag: 8, name: "terminal-input" },
    AppPayloadDescriptor { tag: 9, name: "terminal-resize" },
    AppPayloadDescriptor { tag: 10, name: "detach-terminal" },
    AppPayloadDescriptor { tag: 11, name: "cancel-terminal" },
    AppPayloadDescriptor { tag: 12, name: "daemon-status" },
    AppPayloadDescriptor { tag: 13, name: "shutdown" },
];

const RESPONSES: &[AppPayloadDescriptor] = &[
    AppPayloadDescriptor { tag: 1, name: "command-result" },
    AppPayloadDescriptor { tag: 2, name: "subscription-started" },
    AppPayloadDescriptor { tag: 3, name: "artifact-opened" },
    AppPayloadDescriptor { tag: 4, name: "prompt-accepted" },
    AppPayloadDescriptor { tag: 5, name: "terminal-attached" },
    AppPayloadDescriptor { tag: 6, name: "acknowledged" },
    AppPayloadDescriptor { tag: 7, name: "daemon-status" },
    AppPayloadDescriptor { tag: 8, name: "shutdown-accepted" },
    AppPayloadDescriptor { tag: 9, name: "error" },
];

const EVENTS: &[AppPayloadDescriptor] = &[
    AppPayloadDescriptor { tag: 1, name: "domain-event" },
    AppPayloadDescriptor { tag: 2, name: "subscription-gap" },
    AppPayloadDescriptor { tag: 3, name: "backpressure" },
    AppPayloadDescriptor { tag: 4, name: "artifact-metadata" },
    AppPayloadDescriptor { tag: 5, name: "artifact-chunk" },
    AppPayloadDescriptor { tag: 6, name: "artifact-complete" },
    AppPayloadDescriptor { tag: 7, name: "prompt-requested" },
    AppPayloadDescriptor { tag: 8, name: "terminal-output" },
    AppPayloadDescriptor { tag: 9, name: "terminal-exited" },
    AppPayloadDescriptor { tag: 10, name: "readiness-changed" },
    AppPayloadDescriptor { tag: 11, name: "diagnostic" },
    AppPayloadDescriptor { tag: 12, name: "heartbeat" },
    AppPayloadDescriptor { tag: 13, name: "shutdown-progress" },
    AppPayloadDescriptor { tag: 14, name: "shutdown-complete" },
];

const CONTROLS: &[AppPayloadDescriptor] = &[
    AppPayloadDescriptor { tag: 1, name: "acknowledge" },
    AppPayloadDescriptor { tag: 2, name: "cancel-subscription" },
    AppPayloadDescriptor { tag: 3, name: "cancel-artifact" },
    AppPayloadDescriptor { tag: 4, name: "cancel-prompt" },
    AppPayloadDescriptor { tag: 5, name: "cancel-terminal" },
    AppPayloadDescriptor { tag: 6, name: "subscription" },
    AppPayloadDescriptor { tag: 7, name: "heartbeat-reply" },
];

/// Complete A3 application family registry.
pub const APP_FAMILIES: &[AppFamilyDescriptor] = &[
    AppFamilyDescriptor {
        tag: 94,
        name: "app-client-hello",
        schema_version: 1,
        payloads: CLIENT_HELLO,
    },
    AppFamilyDescriptor {
        tag: 95,
        name: "app-server-hello",
        schema_version: 1,
        payloads: SERVER_HELLO,
    },
    AppFamilyDescriptor { tag: 96, name: "app-request", schema_version: 1, payloads: REQUESTS },
    AppFamilyDescriptor { tag: 97, name: "app-response", schema_version: 1, payloads: RESPONSES },
    AppFamilyDescriptor { tag: 98, name: "app-event", schema_version: 1, payloads: EVENTS },
    AppFamilyDescriptor { tag: 99, name: "app-control", schema_version: 1, payloads: CONTROLS },
];

/// Complete stable schema-v1 application error registry.
pub const APP_ERROR_CODES: &[ErrorCodeDescriptor] = &[
    ErrorCodeDescriptor {
        tag: AppErrorCode::UnsupportedFormat.tag(),
        name: AppErrorCode::UnsupportedFormat.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::UnsupportedFamily.tag(),
        name: AppErrorCode::UnsupportedFamily.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::UnsupportedSchema.tag(),
        name: AppErrorCode::UnsupportedSchema.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::UnknownTag.tag(),
        name: AppErrorCode::UnknownTag.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::MalformedFrame.tag(),
        name: AppErrorCode::MalformedFrame.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::TruncatedFrame.tag(),
        name: AppErrorCode::TruncatedFrame.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::TrailingBytes.tag(),
        name: AppErrorCode::TrailingBytes.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::LimitExceeded.tag(),
        name: AppErrorCode::LimitExceeded.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::InvalidIdentifier.tag(),
        name: AppErrorCode::InvalidIdentifier.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::InvalidVersion.tag(),
        name: AppErrorCode::InvalidVersion.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::IncompatibleVersion.tag(),
        name: AppErrorCode::IncompatibleVersion.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::MissingRequiredFeature.tag(),
        name: AppErrorCode::MissingRequiredFeature.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::InvalidLimits.tag(),
        name: AppErrorCode::InvalidLimits.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::SessionMismatch.tag(),
        name: AppErrorCode::SessionMismatch.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::IdempotencyConflict.tag(),
        name: AppErrorCode::IdempotencyConflict.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::IdempotencyCapacity.tag(),
        name: AppErrorCode::IdempotencyCapacity.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::StaleRevision.tag(),
        name: AppErrorCode::StaleRevision.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::InvalidCommandFrame.tag(),
        name: AppErrorCode::InvalidCommandFrame.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::CommandBindingMismatch.tag(),
        name: AppErrorCode::CommandBindingMismatch.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::InvalidEventRange.tag(),
        name: AppErrorCode::InvalidEventRange.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::SubscriptionState.tag(),
        name: AppErrorCode::SubscriptionState.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::SubscriptionGap.tag(),
        name: AppErrorCode::SubscriptionGap.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::IllegalAcknowledgement.tag(),
        name: AppErrorCode::IllegalAcknowledgement.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::Backpressure.tag(),
        name: AppErrorCode::Backpressure.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::ArtifactState.tag(),
        name: AppErrorCode::ArtifactState.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::ArtifactOrdering.tag(),
        name: AppErrorCode::ArtifactOrdering.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::ArtifactSize.tag(),
        name: AppErrorCode::ArtifactSize.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::ArtifactDigest.tag(),
        name: AppErrorCode::ArtifactDigest.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::PromptMismatch.tag(),
        name: AppErrorCode::PromptMismatch.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::PromptStale.tag(),
        name: AppErrorCode::PromptStale.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::TerminalState.tag(),
        name: AppErrorCode::TerminalState.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::TerminalOrdering.tag(),
        name: AppErrorCode::TerminalOrdering.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::ReadOnly.tag(),
        name: AppErrorCode::ReadOnly.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::NotReady.tag(),
        name: AppErrorCode::NotReady.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::Cancelled.tag(),
        name: AppErrorCode::Cancelled.as_str(),
    },
    ErrorCodeDescriptor {
        tag: AppErrorCode::Internal.tag(),
        name: AppErrorCode::Internal.as_str(),
    },
];
