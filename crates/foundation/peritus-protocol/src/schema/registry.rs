//! Stable message-family registry.

/// One immutable canonical frame family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageFamily {
    /// Nonzero frame-header family tag.
    pub tag: u16,
    /// Stable kebab-case family name.
    pub name: &'static str,
    /// Current nonzero schema version.
    pub schema_version: u16,
    /// Whether decoded values remain inert records rather than reconstructible requests.
    pub inert_only: bool,
}

/// Complete B3 version-one family registry, strictly ordered by tag.
pub const FAMILIES: &[MessageFamily] = &[
    MessageFamily { tag: 1, name: "kernel-command", schema_version: 1, inert_only: false },
    MessageFamily { tag: 2, name: "command-envelope", schema_version: 1, inert_only: false },
    MessageFamily { tag: 3, name: "kernel-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 4, name: "kernel-error", schema_version: 1, inert_only: true },
    MessageFamily { tag: 5, name: "lifecycle-phase", schema_version: 1, inert_only: true },
    MessageFamily { tag: 10, name: "budget-command", schema_version: 1, inert_only: false },
    MessageFamily { tag: 11, name: "budget-amounts", schema_version: 1, inert_only: false },
    MessageFamily { tag: 12, name: "budget-snapshot", schema_version: 1, inert_only: true },
    MessageFamily { tag: 13, name: "reservation-snapshot", schema_version: 1, inert_only: true },
    MessageFamily { tag: 14, name: "budget-receipt", schema_version: 1, inert_only: true },
    MessageFamily { tag: 15, name: "budget-error", schema_version: 1, inert_only: true },
    MessageFamily { tag: 20, name: "action-intent", schema_version: 1, inert_only: false },
    MessageFamily { tag: 21, name: "policy-definition", schema_version: 1, inert_only: false },
    MessageFamily {
        tag: 22,
        name: "policy-amendment-content",
        schema_version: 1,
        inert_only: true,
    },
    MessageFamily { tag: 23, name: "policy-amendment", schema_version: 1, inert_only: false },
    MessageFamily {
        tag: 30,
        name: "acceptance-contract-content",
        schema_version: 1,
        inert_only: true,
    },
    MessageFamily { tag: 31, name: "acceptance-contract", schema_version: 1, inert_only: false },
    MessageFamily { tag: 40, name: "agent-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 41, name: "agent-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 42, name: "agent-state", schema_version: 1, inert_only: true },
    MessageFamily { tag: 50, name: "gate-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 51, name: "gate-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 52, name: "gate-state", schema_version: 1, inert_only: true },
    MessageFamily { tag: 53, name: "review-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 54, name: "review-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 55, name: "review-state", schema_version: 1, inert_only: true },
    MessageFamily { tag: 60, name: "trace-observation", schema_version: 1, inert_only: true },
    MessageFamily { tag: 70, name: "scheduler-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 71, name: "scheduler-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 72, name: "scheduler-state", schema_version: 1, inert_only: true },
    MessageFamily { tag: 73, name: "collaboration-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 74, name: "collaboration-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 75, name: "collaboration-state", schema_version: 1, inert_only: true },
    MessageFamily { tag: 76, name: "orchestrator-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 77, name: "orchestrator-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 78, name: "orchestrator-state", schema_version: 1, inert_only: true },
    MessageFamily { tag: 79, name: "harness-command", schema_version: 1, inert_only: true },
    MessageFamily { tag: 80, name: "harness-event", schema_version: 1, inert_only: true },
    MessageFamily { tag: 81, name: "harness-state", schema_version: 1, inert_only: true },
];
