//! Complete authoritative D2 review-run state.

use peritus_types::{
    CommandId, EventId, EventSequence, FindingId, ReviewCycleId, RunId, Sha256Digest,
};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::{
    Finding, ObservedWaiver, OscillationReport, QuorumReport, ReviewBinding, ReviewCycle,
    ReviewLimits,
};

/// Integrity frontier for immutable review facts archived before the active candidate window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewHistoryFrontier {
    through_sequence: u64,
    through_event: Option<EventId>,
    through_state_digest: Sha256Digest,
    parent_digest: Sha256Digest,
    page_count: u64,
    cycle_count: u64,
    submission_count: u64,
    finding_count: u64,
    disposition_count: u64,
    waiver_count: u64,
    current_binding_digest: Sha256Digest,
    current_unconserved_count: u64,
    current_unconserved_xor: Sha256Digest,
    digest: Sha256Digest,
}

impl ReviewHistoryFrontier {
    pub(super) const fn empty() -> Self {
        Self {
            through_sequence: 0,
            through_event: None,
            through_state_digest: Sha256Digest::new([0; 32]),
            parent_digest: Sha256Digest::new([0; 32]),
            page_count: 0,
            cycle_count: 0,
            submission_count: 0,
            finding_count: 0,
            disposition_count: 0,
            waiver_count: 0,
            current_binding_digest: Sha256Digest::new([0; 32]),
            current_unconserved_count: 0,
            current_unconserved_xor: Sha256Digest::new([0; 32]),
            digest: Sha256Digest::new([0; 32]),
        }
    }

    pub(super) fn for_binding(binding_digest: Sha256Digest) -> Self {
        let mut value = Self { current_binding_digest: binding_digest, ..Self::empty() };
        value.digest = crate::canonical::history_frontier_digest(value);
        value
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        through_sequence: u64,
        through_event: Option<EventId>,
        through_state_digest: Sha256Digest,
        parent_digest: Sha256Digest,
        page_count: u64,
        cycle_count: u64,
        submission_count: u64,
        finding_count: u64,
        disposition_count: u64,
        waiver_count: u64,
        current_binding_digest: Sha256Digest,
        current_unconserved_count: u64,
        current_unconserved_xor: Sha256Digest,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            through_sequence,
            through_event,
            through_state_digest,
            parent_digest,
            page_count,
            cycle_count,
            submission_count,
            finding_count,
            disposition_count,
            waiver_count,
            current_binding_digest,
            current_unconserved_count,
            current_unconserved_xor,
            digest,
        }
    }

    /// Last event sequence whose predecessor state is covered by immutable history.
    #[must_use]
    pub const fn through_sequence(self) -> u64 {
        self.through_sequence
    }
    /// Last event identity covered by immutable history.
    #[must_use]
    pub const fn through_event(self) -> Option<EventId> {
        self.through_event
    }
    /// Exact state digest at the archived frontier.
    #[must_use]
    pub const fn through_state_digest(self) -> Sha256Digest {
        self.through_state_digest
    }
    /// Digest of the preceding immutable-history frontier.
    #[must_use]
    pub const fn parent_digest(self) -> Sha256Digest {
        self.parent_digest
    }
    /// Number of immutable candidate-history pages represented by this frontier.
    #[must_use]
    pub const fn page_count(self) -> u64 {
        self.page_count
    }
    /// Total archived reviewer cycles; this is an observation, never an allowance.
    #[must_use]
    pub const fn cycle_count(self) -> u64 {
        self.cycle_count
    }
    /// Total archived submissions; this is an observation, never an allowance.
    #[must_use]
    pub const fn submission_count(self) -> u64 {
        self.submission_count
    }
    /// Total archived findings; this is an observation, never an allowance.
    #[must_use]
    pub const fn finding_count(self) -> u64 {
        self.finding_count
    }
    /// Total archived disposition facts; this is an observation, never an allowance.
    #[must_use]
    pub const fn disposition_count(self) -> u64 {
        self.disposition_count
    }
    /// Total archived waiver observations; this is an observation, never an allowance.
    #[must_use]
    pub const fn waiver_count(self) -> u64 {
        self.waiver_count
    }
    /// Binding whose current conservation accumulator is represented here.
    #[must_use]
    pub const fn current_binding_digest(self) -> Sha256Digest {
        self.current_binding_digest
    }
    /// Exact number of archived current findings that still require conservation.
    #[must_use]
    pub const fn current_unconserved_count(self) -> u64 {
        self.current_unconserved_count
    }
    /// Order-independent integrity accumulator over archived unconserved finding identities.
    #[must_use]
    pub const fn current_unconserved_xor(self) -> Sha256Digest {
        self.current_unconserved_xor
    }
    /// Chained digest binding every archived page frontier.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }

    pub(super) fn validate(self) -> Result<(), ReviewError> {
        let zero = Sha256Digest::new([0; 32]);
        let legacy_empty = self == Self::empty();
        let unpaged = self.page_count == 0;
        let malformed_unpaged = unpaged
            && (self.through_sequence != 0
                || self.through_event.is_some()
                || self.through_state_digest != zero
                || self.parent_digest != zero
                || self.cycle_count != 0
                || self.submission_count != 0
                || self.finding_count != 0
                || self.disposition_count != 0
                || self.waiver_count != 0
                || self.current_unconserved_count != 0
                || self.current_unconserved_xor != zero);
        if (!legacy_empty
            && (self.current_binding_digest == zero
                || malformed_unpaged
                || crate::canonical::history_frontier_digest(self) != self.digest))
            || (self.current_unconserved_count == 0 && self.current_unconserved_xor != zero)
        {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "review history frontier is empty-shaped or digest-inconsistent",
            ));
        }
        Ok(())
    }
}

pub mod mutation;
mod terminal;

pub use terminal::{ReviewTerminal, ReviewTerminalKind};

/// Closed review-run lifecycle.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewRunPhase {
    /// Assignments, submissions, and finding lifecycle commands may be admitted.
    Active = 1,
    /// A truthful immutable terminal was committed.
    Terminal = 2,
    /// Review progress is durably suspended without changing findings, quorum, or limits.
    Paused = 3,
}

/// Complete deterministic replayable review aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRunState {
    run_id: RunId,
    limits: ReviewLimits,
    binding: ReviewBinding,
    phase: ReviewRunPhase,
    sequence: EventSequence,
    last_event_id: EventId,
    state_digest: Sha256Digest,
    history: ReviewHistoryFrontier,
    cycles: Vec<ReviewCycle>,
    findings: Vec<Finding>,
    waivers: Vec<ObservedWaiver>,
    quorum: QuorumReport,
    oscillation: OscillationReport,
    used_commands: Vec<CommandId>,
    terminal: Option<ReviewTerminal>,
}

impl ReviewRunState {
    pub(super) fn genesis(
        run_id: RunId,
        limits: ReviewLimits,
        binding: ReviewBinding,
        sequence: EventSequence,
        event_id: EventId,
        command_id: CommandId,
    ) -> Self {
        let quorum = QuorumReport::evaluate(&binding, &[]);
        let oscillation = OscillationReport::evaluate(&binding, &[], &[], false);
        let history = if binding.uses_paged_history() {
            ReviewHistoryFrontier::for_binding(binding.digest())
        } else {
            ReviewHistoryFrontier::empty()
        };
        Self {
            run_id,
            limits,
            binding,
            phase: ReviewRunPhase::Active,
            sequence,
            last_event_id: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            history,
            cycles: Vec::new(),
            findings: Vec::new(),
            waivers: Vec::new(),
            quorum,
            oscillation,
            used_commands: vec![command_id],
            terminal: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        run_id: RunId,
        limits: ReviewLimits,
        binding: ReviewBinding,
        phase: ReviewRunPhase,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        cycles: Vec<ReviewCycle>,
        findings: Vec<Finding>,
        waivers: Vec<ObservedWaiver>,
        quorum: QuorumReport,
        oscillation: OscillationReport,
        used_commands: Vec<CommandId>,
        terminal: Option<ReviewTerminal>,
    ) -> Self {
        Self::from_wire_v2(
            run_id,
            limits,
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
            ReviewHistoryFrontier::empty(),
            cycles,
            findings,
            waivers,
            quorum,
            oscillation,
            used_commands,
            terminal,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire_v2(
        run_id: RunId,
        limits: ReviewLimits,
        binding: ReviewBinding,
        phase: ReviewRunPhase,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        history: ReviewHistoryFrontier,
        cycles: Vec<ReviewCycle>,
        findings: Vec<Finding>,
        waivers: Vec<ObservedWaiver>,
        quorum: QuorumReport,
        oscillation: OscillationReport,
        used_commands: Vec<CommandId>,
        terminal: Option<ReviewTerminal>,
    ) -> Self {
        Self {
            run_id,
            limits,
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
            history,
            cycles,
            findings,
            waivers,
            quorum,
            oscillation,
            used_commands,
            terminal,
        }
    }

    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns all immutable D2 bounds.
    #[must_use]
    pub const fn limits(&self) -> ReviewLimits {
        self.limits
    }
    /// Returns the current exact contract/candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &ReviewBinding {
        &self.binding
    }
    /// Returns the closed lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> ReviewRunPhase {
        self.phase
    }
    /// Returns the latest one-based event sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns the latest event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns the canonical complete-state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns the exact immutable-history integrity frontier.
    #[must_use]
    pub const fn history(&self) -> ReviewHistoryFrontier {
        self.history
    }
    /// Returns the bounded active/materialized cycle window in canonical page order.
    #[must_use]
    pub const fn cycles(&self) -> &[ReviewCycle] {
        self.cycles.as_slice()
    }
    /// Returns findings materialized in the bounded checkpoint in stable identity order.
    #[must_use]
    pub const fn findings(&self) -> &[Finding] {
        self.findings.as_slice()
    }
    /// Returns external waivers materialized in the bounded checkpoint in event order.
    #[must_use]
    pub const fn waivers(&self) -> &[ObservedWaiver] {
        self.waivers.as_slice()
    }
    /// Returns the current independent quorum report.
    #[must_use]
    pub const fn quorum(&self) -> &QuorumReport {
        &self.quorum
    }
    /// Returns the current deterministic oscillation report.
    #[must_use]
    pub const fn oscillation(&self) -> &OscillationReport {
        &self.oscillation
    }
    /// Returns the bounded recent command-identity cache in event order.
    #[must_use]
    pub const fn used_commands(&self) -> &[CommandId] {
        self.used_commands.as_slice()
    }
    /// Returns the truthful terminal summary, when committed.
    #[must_use]
    pub const fn terminal(&self) -> Option<&ReviewTerminal> {
        self.terminal.as_ref()
    }

    /// Looks up one retained cycle.
    #[must_use]
    pub fn cycle(&self, cycle_id: ReviewCycleId) -> Option<&ReviewCycle> {
        self.cycles.iter().find(|cycle| cycle.id() == cycle_id)
    }

    /// Looks up one retained finding.
    #[must_use]
    pub fn finding(&self, finding_id: FindingId) -> Option<&Finding> {
        self.findings
            .binary_search_by_key(&finding_id, Finding::id)
            .ok()
            .map(|index| &self.findings[index])
    }

    /// Returns whether a retained cycle belongs to the exact current binding.
    #[must_use]
    pub fn cycle_is_current(&self, cycle: &ReviewCycle) -> bool {
        cycle.assignment().binding_digest() == self.binding.digest()
            && cycle.assignment().revision() == self.binding.revision()
    }

    /// Returns whether a finding originated under the exact current binding.
    #[must_use]
    pub fn finding_is_current(&self, finding: &Finding) -> bool {
        finding.revision() == self.binding.revision()
            && (self.binding.uses_paged_history()
                || self
                    .cycle(finding.origin().cycle_id())
                    .is_some_and(|cycle| {
                        cycle.assignment().binding_digest() == self.binding.digest()
                    }))
    }

    /// Returns canonical identities of unconserved current findings.
    #[must_use]
    pub fn unconserved_current_findings(&self) -> Vec<FindingId> {
        self.findings
            .iter()
            .filter(|finding| self.finding_is_current(finding) && !finding.is_conserved())
            .map(Finding::id)
            .collect()
    }

    /// Returns the exact count of current findings lacking a permitted closure.
    #[must_use]
    pub fn unconserved_current_count(&self) -> u64 {
        let active = self.unconserved_current_findings().len() as u64;
        if self.binding.uses_paged_history() {
            self.history.current_unconserved_count().saturating_add(active)
        } else {
            active
        }
    }

    /// Returns the integrity accumulator for the exact current unconserved identity set.
    #[must_use]
    pub fn unconserved_current_xor(&self) -> Sha256Digest {
        let active = crate::state::mutation::finding_xor(&self.unconserved_current_findings());
        if self.binding.uses_paged_history() {
            crate::state::mutation::xor_digest(self.history.current_unconserved_xor(), active)
        } else {
            active
        }
    }

    /// Returns whether current quorum and finding conservation permit D2 completion.
    #[must_use]
    pub fn completion_ready(&self) -> bool {
        self.quorum.complete() && self.unconserved_current_count() == 0
    }

    /// Conservative deterministic upper estimate used before canonical storage admission.
    #[must_use]
    pub fn estimated_encoded_bytes(&self) -> u64 {
        let finding_bytes = |finding: &Finding| {
            let text = [
                finding.description(),
                finding.reproduction(),
                finding.expected_behavior(),
                finding.remediation(),
            ]
            .iter()
            .fold(0_u64, |value, text| value.saturating_add(text.len() as u64));
            let paths = finding
                .locations()
                .iter()
                .fold(0_u64, |value, location| value.saturating_add(location.path().len() as u64));
            text.saturating_add(paths)
                .saturating_add((finding.evidence().len() as u64).saturating_mul(16))
                .saturating_add((finding.requirements().len() as u64).saturating_mul(32))
                .saturating_add((finding.sources().len() as u64).saturating_mul(32))
                .saturating_add((finding.dispositions().len() as u64).saturating_mul(256))
                .saturating_add(finding.dispositions().iter().fold(0_u64, |value, record| {
                    value.saturating_add((record.evidence().len() as u64).saturating_mul(16))
                }))
                .saturating_add(512)
        };
        let retained_findings = self
            .findings
            .iter()
            .fold(0_u64, |total, finding| total.saturating_add(finding_bytes(finding)));
        let submitted_findings = self.cycles.iter().fold(0_u64, |total, cycle| {
            cycle.submission().map_or(total, |submission| {
                submission
                    .findings()
                    .iter()
                    .fold(total, |value, finding| value.saturating_add(finding_bytes(finding)))
            })
        });
        2_048_u64
            .saturating_add(retained_findings)
            .saturating_add(submitted_findings)
            .saturating_add((self.cycles.len() as u64).saturating_mul(1_024))
            .saturating_add((self.waivers.len() as u64).saturating_mul(256))
            .saturating_add((self.used_commands.len() as u64).saturating_mul(16))
    }

    pub(super) fn validate_inert(&self) -> Result<(), ReviewError> {
        self.binding.validate(self.limits)?;
        self.history.validate()?;
        if self.binding.uses_paged_history() {
            if self.history.current_binding_digest() != self.binding.digest()
                || (self.history.page_count() > 0
                    && self.history.through_sequence() >= self.sequence.get())
            {
                return Err(reject(
                    ReviewErrorKind::ReplayMismatch,
                    "review history frontier does not precede the active checkpoint",
                ));
            }
        } else if self.history != ReviewHistoryFrontier::empty() {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "legacy review binding contains a paged-history frontier",
            ));
        }
        if self.cycles.len() > usize::from(self.limits.cycles())
            || self.cycles.len() > usize::from(self.limits.assignments())
            || self.findings.len() > self.limits.findings() as usize
            || self.waivers.len() > self.limits.findings() as usize
            || self.used_commands.len() > 65_535
            || self.estimated_encoded_bytes() > self.limits.state_bytes()
        {
            return Err(reject(
                ReviewErrorKind::LimitExceeded,
                "decoded review state exceeds its immutable bounds",
            ));
        }
        let cycles_noncanonical = if self.binding.uses_paged_history() {
            self.cycles.windows(2).any(|pair| {
                (pair[0].ordinal().get(), pair[0].id())
                    >= (pair[1].ordinal().get(), pair[1].id())
            })
        } else {
            self.cycles.windows(2).any(|pair| pair[0].ordinal() >= pair[1].ordinal())
        };
        if cycles_noncanonical
            || self.findings.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        {
            return Err(reject(
                ReviewErrorKind::NonCanonical,
                "decoded state collections are duplicated or not canonical",
            ));
        }
        for cycle in &self.cycles {
            cycle.validate_inert(&self.binding, self.limits)?;
        }
        for finding in &self.findings {
            finding.validate(self.binding.blocking_severity(), self.limits)?;
            let source_cycle = self.cycle(finding.origin().cycle_id());
            if source_cycle.is_none() && !self.binding.uses_paged_history() {
                return Err(reject(
                    ReviewErrorKind::UnknownIdentity,
                    "decoded finding origin cycle is absent",
                ));
            }
            if source_cycle.is_some_and(|cycle| {
                cycle.assignment().reviewer().actor_id() != finding.origin().reviewer()
            }) {
                return Err(reject(
                    ReviewErrorKind::BindingMismatch,
                    "decoded finding origin reviewer differs from its cycle",
                ));
            }
        }
        let expected_quorum = if self.binding.uses_paged_history() && self.quorum.complete() {
            self.quorum.clone()
        } else {
            QuorumReport::evaluate(&self.binding, &self.cycles)
        };
        let unconserved = self.unconserved_current_findings();
        let expected_oscillation = OscillationReport::evaluate(
            &self.binding,
            &self.cycles,
            &self.findings,
            expected_quorum.complete() && unconserved.is_empty(),
        );
        let mut commands = self.used_commands.clone();
        commands.sort_unstable();
        commands.dedup();
        if commands.len() != self.used_commands.len()
            || self.quorum != expected_quorum
            || self.oscillation != expected_oscillation
            || (self.phase == ReviewRunPhase::Terminal) != self.terminal.is_some()
            || crate::canonical::state_digest(self) != self.state_digest
        {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "decoded state identity, terminal, or digest invariant differs",
            ));
        }
        if let Some(terminal) = &self.terminal
            && (terminal.uses_paged_history() != self.binding.uses_paged_history()
                || terminal.digest != crate::canonical::terminal_digest(terminal)
                || terminal.unconserved_findings != unconserved
                || terminal.unconserved_count != self.unconserved_current_count()
                || terminal.unconserved_xor != self.unconserved_current_xor()
                || terminal.quorum != self.quorum
                || terminal.oscillation != self.oscillation
                || (terminal.kind == ReviewTerminalKind::Completed
                    && (!self.quorum.complete()
                        || self.unconserved_current_count() != 0
                        || self.oscillation.triggered())))
        {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "decoded terminal summary is not truthful or canonical",
            ));
        }
        Ok(())
    }
}
