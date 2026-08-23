//! Stable B0 lifecycle discriminant metadata for generated clients.

/// One named closed-union discriminant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VariantTag {
    /// Primary `u16` tag.
    pub tag: u16,
    /// Nested `u16` tag used by lifecycle phases.
    pub subtag: Option<u16>,
    /// Stable kebab-case variant name.
    pub name: &'static str,
}

/// One named set of version-one discriminants.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VariantSet {
    /// Stable kebab-case set name.
    pub name: &'static str,
    /// Complete immutable variant sequence.
    pub variants: &'static [VariantTag],
}

const fn tag(tag: u16, name: &'static str) -> VariantTag {
    VariantTag { tag, subtag: None, name }
}

const fn phase(family: u16, tag: u16, name: &'static str) -> VariantTag {
    VariantTag { tag: family, subtag: Some(tag), name }
}

/// All 35 B0 command discriminants in canonical tag order.
pub const KERNEL_COMMAND_VARIANTS: &[VariantTag] = &[
    tag(1, "pause-session"),
    tag(2, "resume-session"),
    tag(3, "close-session"),
    tag(4, "start-run"),
    tag(5, "pause-run"),
    tag(6, "resume-run"),
    tag(7, "cancel-run"),
    tag(8, "fail-run"),
    tag(9, "exhaust-run"),
    tag(10, "reject-run"),
    tag(11, "start-attempt"),
    tag(12, "resume-attempt"),
    tag(13, "submit-attempt"),
    tag(14, "fail-attempt"),
    tag(15, "exhaust-attempt"),
    tag(16, "start-turn"),
    tag(17, "complete-turn"),
    tag(18, "fail-turn"),
    tag(19, "cancel-turn"),
    tag(20, "propose-action"),
    tag(21, "authorize-action"),
    tag(22, "dispatch-action"),
    tag(23, "complete-action"),
    tag(24, "fail-action"),
    tag(25, "cancel-action"),
    tag(26, "request-review"),
    tag(27, "begin-review"),
    tag(28, "submit-review"),
    tag(29, "invalidate-review"),
    tag(30, "request-waiver"),
    tag(31, "grant-waiver"),
    tag(32, "deny-waiver"),
    tag(33, "invalidate-waiver"),
    tag(34, "begin-acceptance"),
    tag(35, "evaluate-acceptance"),
];

/// All 37 B0 event-kind discriminants in canonical tag order.
pub const KERNEL_EVENT_VARIANTS: &[VariantTag] = &[
    tag(1, "session-opened"),
    tag(2, "session-paused"),
    tag(3, "session-resumed"),
    tag(4, "session-closed"),
    tag(5, "run-started"),
    tag(6, "run-paused"),
    tag(7, "run-resumed"),
    tag(8, "run-cancelled"),
    tag(9, "run-failed"),
    tag(10, "run-exhausted"),
    tag(11, "run-rejected"),
    tag(12, "attempt-started"),
    tag(13, "attempt-resumed"),
    tag(14, "attempt-submitted"),
    tag(15, "attempt-failed"),
    tag(16, "attempt-exhausted"),
    tag(17, "turn-started"),
    tag(18, "turn-completed"),
    tag(19, "turn-failed"),
    tag(20, "turn-cancelled"),
    tag(21, "action-proposed"),
    tag(22, "action-authorized"),
    tag(23, "action-dispatched"),
    tag(24, "action-completed"),
    tag(25, "action-failed"),
    tag(26, "action-cancelled"),
    tag(27, "review-requested"),
    tag(28, "review-begun"),
    tag(29, "review-submitted"),
    tag(30, "review-invalidated"),
    tag(31, "waiver-requested"),
    tag(32, "waiver-granted"),
    tag(33, "waiver-denied"),
    tag(34, "waiver-invalidated"),
    tag(35, "acceptance-begun"),
    tag(36, "acceptance-accepted"),
    tag(37, "acceptance-needs-changes"),
];

/// All 16 B0 kernel-error discriminants in canonical tag order.
pub const KERNEL_ERROR_VARIANTS: &[VariantTag] = &[
    tag(1, "revision-mismatch"),
    tag(2, "contract-mismatch"),
    tag(3, "causal-head-mismatch"),
    tag(4, "duplicate-command"),
    tag(5, "duplicate-event"),
    tag(6, "missing-entity"),
    tag(7, "duplicate-entity"),
    tag(8, "parent-mismatch"),
    tag(9, "illegal-phase"),
    tag(10, "missing-authority-input"),
    tag(11, "authority-mismatch"),
    tag(12, "budget-unavailable"),
    tag(13, "budget-exceeded"),
    tag(14, "live-child"),
    tag(15, "sequence-overflow"),
    tag(16, "invalid-aggregate"),
];

/// All eight B0 event-subject discriminants in canonical tag order.
pub const KERNEL_SUBJECT_VARIANTS: &[VariantTag] = &[
    tag(1, "session"),
    tag(2, "run"),
    tag(3, "attempt"),
    tag(4, "turn"),
    tag(5, "action"),
    tag(6, "review"),
    tag(7, "waiver"),
    tag(8, "acceptance"),
];

/// All 44 B0 lifecycle phases as `(family, phase)` discriminants.
pub const LIFECYCLE_PHASE_VARIANTS: &[VariantTag] = &[
    phase(1, 1, "session-open"),
    phase(1, 2, "session-paused"),
    phase(1, 3, "session-closed"),
    phase(2, 1, "run-pending"),
    phase(2, 2, "run-running"),
    phase(2, 3, "run-paused"),
    phase(2, 4, "run-reviewing"),
    phase(2, 5, "run-fixing"),
    phase(2, 6, "run-accepted"),
    phase(2, 7, "run-rejected"),
    phase(2, 8, "run-cancelled"),
    phase(2, 9, "run-failed"),
    phase(2, 10, "run-exhausted"),
    phase(3, 1, "attempt-active"),
    phase(3, 2, "attempt-submitted"),
    phase(3, 3, "attempt-reviewing"),
    phase(3, 4, "attempt-fixing"),
    phase(3, 5, "attempt-accepted"),
    phase(3, 6, "attempt-failed"),
    phase(3, 7, "attempt-cancelled"),
    phase(3, 8, "attempt-exhausted"),
    phase(4, 1, "turn-active"),
    phase(4, 2, "turn-completed"),
    phase(4, 3, "turn-failed"),
    phase(4, 4, "turn-cancelled"),
    phase(5, 1, "action-proposed"),
    phase(5, 2, "action-authorized"),
    phase(5, 3, "action-dispatched"),
    phase(5, 4, "action-succeeded"),
    phase(5, 5, "action-failed"),
    phase(5, 6, "action-cancelled"),
    phase(6, 1, "review-requested"),
    phase(6, 2, "review-active"),
    phase(6, 3, "review-submitted"),
    phase(6, 4, "review-invalidated"),
    phase(7, 1, "waiver-requested"),
    phase(7, 2, "waiver-granted"),
    phase(7, 3, "waiver-denied"),
    phase(7, 4, "waiver-invalidated"),
    phase(8, 1, "acceptance-pending"),
    phase(8, 2, "acceptance-evaluating"),
    phase(8, 3, "acceptance-needs-changes"),
    phase(8, 4, "acceptance-accepted"),
    phase(8, 5, "acceptance-terminated"),
];

/// Complete generated-client lifecycle discriminant registry.
pub const LIFECYCLE_VARIANTS: &[VariantSet] = &[
    VariantSet { name: "kernel-command", variants: KERNEL_COMMAND_VARIANTS },
    VariantSet { name: "kernel-event", variants: KERNEL_EVENT_VARIANTS },
    VariantSet { name: "kernel-error", variants: KERNEL_ERROR_VARIANTS },
    VariantSet { name: "kernel-subject", variants: KERNEL_SUBJECT_VARIANTS },
    VariantSet { name: "lifecycle-phase", variants: LIFECYCLE_PHASE_VARIANTS },
];
