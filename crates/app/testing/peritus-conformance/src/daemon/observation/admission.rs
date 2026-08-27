//! Session, command, readiness, framing, and non-authority observations.

/// Terminal outcome of one session or context exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonSessionOutcome {
    /// A compatible authenticated session was established or resumed.
    Established,
    /// Negotiation completed with an explicit incompatible result.
    Incompatible,
    /// Admission rejected an identity or context mismatch.
    Rejected,
}

/// Direct session-boundary facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonSessionObservation {
    outcome: DaemonSessionOutcome,
    session_stable_on_resume: bool,
    principal_binding_matches: bool,
    negotiated_context_matches: bool,
    durable_mutations: u64,
    external_effects: u64,
}

impl DaemonSessionObservation {
    /// Creates one complete session observation.
    #[must_use]
    pub const fn new(
        outcome: DaemonSessionOutcome,
        session_stable_on_resume: bool,
        principal_binding_matches: bool,
        negotiated_context_matches: bool,
        durable_mutations: u64,
        external_effects: u64,
    ) -> Self {
        Self {
            outcome,
            session_stable_on_resume,
            principal_binding_matches,
            negotiated_context_matches,
            durable_mutations,
            external_effects,
        }
    }

    /// Returns the observed terminal outcome.
    #[must_use]
    pub const fn outcome(self) -> DaemonSessionOutcome {
        self.outcome
    }

    /// Returns whether the nonzero server-selected session resumed with exact stable identity.
    #[must_use]
    pub const fn session_stable_on_resume(self) -> bool {
        self.session_stable_on_resume
    }

    /// Returns whether the live peer equals its durable actor binding.
    #[must_use]
    pub const fn principal_binding_matches(self) -> bool {
        self.principal_binding_matches
    }

    /// Returns whether the post-hello context matches the negotiated relationship.
    #[must_use]
    pub const fn negotiated_context_matches(self) -> bool {
        self.negotiated_context_matches
    }

    /// Returns durable mutations observed during the exercise.
    #[must_use]
    pub const fn durable_mutations(self) -> u64 {
        self.durable_mutations
    }

    /// Returns external effects observed during the exercise.
    #[must_use]
    pub const fn external_effects(self) -> u64 {
        self.external_effects
    }
}

/// Terminal outcome of one durable command exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonCommandOutcome {
    /// A new command committed an exact positive event range.
    Committed,
    /// An exact retry returned its retained result.
    Replayed,
    /// The idempotency key was already bound to another digest.
    Conflict,
    /// An ambiguous command remains or was reconciled as indeterminate.
    Indeterminate,
    /// Admission rejected the command before append.
    Rejected,
}

/// Direct application-command and journal facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonCommandObservation {
    outcome: DaemonCommandOutcome,
    committed_events: u64,
    response_range_exact: bool,
    original_identity_reconciled: bool,
    replacement_commands: u64,
    new_durable_appends: u64,
    new_external_effects: u64,
}

impl DaemonCommandObservation {
    /// Creates one complete command observation.
    #[must_use]
    pub const fn new(
        outcome: DaemonCommandOutcome,
        committed_events: u64,
        response_range_exact: bool,
        original_identity_reconciled: bool,
        replacement_commands: u64,
        new_durable_appends: u64,
        new_external_effects: u64,
    ) -> Self {
        Self {
            outcome,
            committed_events,
            response_range_exact,
            original_identity_reconciled,
            replacement_commands,
            new_durable_appends,
            new_external_effects,
        }
    }

    /// Returns the stable command outcome.
    #[must_use]
    pub const fn outcome(self) -> DaemonCommandOutcome {
        self.outcome
    }

    /// Returns the number of events in the reported committed range.
    #[must_use]
    pub const fn committed_events(self) -> u64 {
        self.committed_events
    }

    /// Returns whether response cursors equal the committed global range.
    #[must_use]
    pub const fn response_range_exact(self) -> bool {
        self.response_range_exact
    }

    /// Returns whether recovery resolved the original command identity and digest.
    #[must_use]
    pub const fn original_identity_reconciled(self) -> bool {
        self.original_identity_reconciled
    }

    /// Returns newly manufactured replacement command identities.
    #[must_use]
    pub const fn replacement_commands(self) -> u64 {
        self.replacement_commands
    }

    /// Returns new durable appends produced by the observed request attempt.
    #[must_use]
    pub const fn new_durable_appends(self) -> u64 {
        self.new_durable_appends
    }

    /// Returns new external effects produced by the observed request attempt.
    #[must_use]
    pub const fn new_external_effects(self) -> u64 {
        self.new_external_effects
    }
}

/// Published daemon readiness relevant to admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonReadiness {
    /// Safe bounded diagnostics are available but mutation is disabled.
    ReadyReadOnly,
    /// Ordinary read and mutation admission are available.
    ReadyReadWrite,
    /// The daemon cannot safely serve even diagnostic traffic.
    Unavailable,
}

/// Direct facts from one readiness-admission exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonAdmissionObservation {
    readiness: DaemonReadiness,
    read_admitted: bool,
    mutation_admitted: bool,
    effect_workers_started: u64,
}

impl DaemonAdmissionObservation {
    /// Creates one readiness-admission observation.
    #[must_use]
    pub const fn new(
        readiness: DaemonReadiness,
        read_admitted: bool,
        mutation_admitted: bool,
        effect_workers_started: u64,
    ) -> Self {
        Self { readiness, read_admitted, mutation_admitted, effect_workers_started }
    }

    /// Returns published readiness.
    #[must_use]
    pub const fn readiness(self) -> DaemonReadiness {
        self.readiness
    }

    /// Returns whether an explicitly classified read was admitted.
    #[must_use]
    pub const fn read_admitted(self) -> bool {
        self.read_admitted
    }

    /// Returns whether a mutation was admitted.
    #[must_use]
    pub const fn mutation_admitted(self) -> bool {
        self.mutation_admitted
    }

    /// Returns the number of effect workers started in this readiness state.
    #[must_use]
    pub const fn effect_workers_started(self) -> u64 {
        self.effect_workers_started
    }
}

/// Direct malformed-frame handling facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonFrameObservation {
    rejected: bool,
    payload_bytes_allocated: u64,
    requests_dispatched: u64,
    external_effects: u64,
}

impl DaemonFrameObservation {
    /// Creates one framing observation.
    #[must_use]
    pub const fn new(
        rejected: bool,
        payload_bytes_allocated: u64,
        requests_dispatched: u64,
        external_effects: u64,
    ) -> Self {
        Self { rejected, payload_bytes_allocated, requests_dispatched, external_effects }
    }

    /// Returns whether the malformed frame was rejected.
    #[must_use]
    pub const fn rejected(self) -> bool {
        self.rejected
    }

    /// Returns payload bytes allocated before rejection.
    #[must_use]
    pub const fn payload_bytes_allocated(self) -> u64 {
        self.payload_bytes_allocated
    }

    /// Returns application requests dispatched from the malformed frame.
    #[must_use]
    pub const fn requests_dispatched(self) -> u64 {
        self.requests_dispatched
    }

    /// Returns external effects reached by the malformed frame.
    #[must_use]
    pub const fn external_effects(self) -> u64 {
        self.external_effects
    }
}

/// Direct facts proving a report-only surface is not an authority path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonNonAuthorityObservation {
    observation_reported: bool,
    authority_appends: u64,
    external_effects: u64,
    acceptance_inferred: bool,
}

impl DaemonNonAuthorityObservation {
    /// Creates one non-authority observation.
    #[must_use]
    pub const fn new(
        observation_reported: bool,
        authority_appends: u64,
        external_effects: u64,
        acceptance_inferred: bool,
    ) -> Self {
        Self { observation_reported, authority_appends, external_effects, acceptance_inferred }
    }

    /// Returns whether the diagnostic or telemetry observation remained usable.
    #[must_use]
    pub const fn observation_reported(self) -> bool {
        self.observation_reported
    }

    /// Returns authoritative appends attributable to the report-only operation.
    #[must_use]
    pub const fn authority_appends(self) -> u64 {
        self.authority_appends
    }

    /// Returns external effects attributable to the report-only operation.
    #[must_use]
    pub const fn external_effects(self) -> u64 {
        self.external_effects
    }

    /// Returns whether the observation was incorrectly treated as acceptance.
    #[must_use]
    pub const fn acceptance_inferred(self) -> bool {
        self.acceptance_inferred
    }
}
