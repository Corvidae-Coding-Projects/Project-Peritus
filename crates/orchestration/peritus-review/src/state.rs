//! Complete authoritative D2 review-run state.

use peritus_types::{
    CommandId, EventId, EventSequence, FindingId, ReviewCycleId, RunId, Sha256Digest,
};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::{
    Finding, ObservedWaiver, OscillationReport, QuorumReport, ReviewBinding, ReviewCycle,
    ReviewLimits,
};

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
        Self {
            run_id,
            limits,
            binding,
            phase: ReviewRunPhase::Active,
            sequence,
            last_event_id: event_id,
            state_digest: Sha256Digest::new([0; 32]),
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
        Self {
            run_id,
            limits,
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
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
    /// Returns every historical cycle in ordinal order.
    #[must_use]
    pub const fn cycles(&self) -> &[ReviewCycle] {
        self.cycles.as_slice()
    }
    /// Returns every historical finding in stable identity order.
    #[must_use]
    pub const fn findings(&self) -> &[Finding] {
        self.findings.as_slice()
    }
    /// Returns every consumed external waiver in event order.
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
    /// Returns consumed command identities in event order.
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
            && self
                .cycle(finding.origin().cycle_id())
                .is_some_and(|cycle| cycle.assignment().binding_digest() == self.binding.digest())
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

    /// Returns whether current quorum and finding conservation permit D2 completion.
    #[must_use]
    pub fn completion_ready(&self) -> bool {
        self.quorum.complete() && self.unconserved_current_findings().is_empty()
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
        if self.cycles.windows(2).any(|pair| pair[0].ordinal() >= pair[1].ordinal())
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
            let source_cycle = self.cycle(finding.origin().cycle_id()).ok_or_else(|| {
                reject(ReviewErrorKind::UnknownIdentity, "decoded finding origin cycle is absent")
            })?;
            if source_cycle.assignment().reviewer().actor_id() != finding.origin().reviewer() {
                return Err(reject(
                    ReviewErrorKind::BindingMismatch,
                    "decoded finding origin reviewer differs from its cycle",
                ));
            }
        }
        let expected_quorum = QuorumReport::evaluate(&self.binding, &self.cycles);
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
            && (terminal.digest != crate::canonical::terminal_digest(terminal)
                || terminal.unconserved_findings != unconserved
                || terminal.quorum != self.quorum
                || terminal.oscillation != self.oscillation
                || (terminal.kind == ReviewTerminalKind::Completed
                    && (!self.quorum.complete()
                        || !unconserved.is_empty()
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
