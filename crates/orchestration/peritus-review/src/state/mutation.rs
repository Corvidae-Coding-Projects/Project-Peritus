//! Reducer-only mutation helpers for otherwise immutable D2 state.

use peritus_types::{CommandId, EventId, EventSequence, FindingId, Sha256Digest};

use super::{ReviewRunPhase, ReviewRunState, ReviewTerminal, ReviewTerminalKind};
use crate::{Finding, ObservedWaiver, OscillationReport, QuorumReport, ReviewBinding, ReviewCycle};

pub const fn set_state_digest(state: &mut ReviewRunState, digest: Sha256Digest) {
    state.state_digest = digest;
}

pub const fn set_phase(state: &mut ReviewRunState, phase: ReviewRunPhase) {
    state.phase = phase;
}

pub fn advance_cursor(
    state: &mut ReviewRunState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) {
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.state_digest = Sha256Digest::new([0; 32]);
    state.used_commands.push(command_id);
}

pub fn replace_binding(state: &mut ReviewRunState, binding: ReviewBinding) {
    for cycle in &mut state.cycles {
        if cycle.assignment().binding_digest() == state.binding.digest() {
            cycle.phase = crate::ReviewCyclePhase::Invalidated;
        }
    }
    state.binding = binding;
}

pub fn push_cycle(state: &mut ReviewRunState, cycle: ReviewCycle) {
    state.cycles.push(cycle);
}

pub fn cycle_mut(
    state: &mut ReviewRunState,
    cycle_id: peritus_types::ReviewCycleId,
) -> Option<&mut ReviewCycle> {
    state.cycles.iter_mut().find(|cycle| cycle.id() == cycle_id)
}

pub fn insert_findings(state: &mut ReviewRunState, findings: impl IntoIterator<Item = Finding>) {
    state.findings.extend(findings);
    state.findings.sort_unstable_by_key(Finding::id);
}

pub fn finding_mut(state: &mut ReviewRunState, finding_id: FindingId) -> Option<&mut Finding> {
    state
        .findings
        .binary_search_by_key(&finding_id, Finding::id)
        .ok()
        .map(|index| &mut state.findings[index])
}

pub fn push_waiver(state: &mut ReviewRunState, waiver: ObservedWaiver) {
    state.waivers.push(waiver);
}

pub fn recompute(state: &mut ReviewRunState) {
    state.quorum = QuorumReport::evaluate(&state.binding, &state.cycles);
    let unconserved = state.unconserved_current_findings();
    state.oscillation = OscillationReport::evaluate(
        &state.binding,
        &state.cycles,
        &state.findings,
        state.quorum.complete() && unconserved.is_empty(),
    );
}

pub fn terminal(state: &mut ReviewRunState, terminal: ReviewTerminal) {
    state.terminal = Some(terminal);
    state.phase = ReviewRunPhase::Terminal;
}

pub fn make_terminal(
    kind: ReviewTerminalKind,
    unconserved_findings: Vec<FindingId>,
    quorum: QuorumReport,
    oscillation: OscillationReport,
    cause_digest: Sha256Digest,
) -> ReviewTerminal {
    let mut terminal = ReviewTerminal::from_wire(
        kind,
        unconserved_findings,
        quorum,
        oscillation,
        cause_digest,
        Sha256Digest::new([0; 32]),
    );
    terminal.digest = crate::canonical::terminal_digest(&terminal);
    terminal
}

pub fn set_cycle_submission(cycle: &mut ReviewCycle, submission: crate::ReviewSubmission) {
    cycle.submission = Some(submission);
    cycle.phase = crate::ReviewCyclePhase::Submitted;
}

pub const fn set_cycle_phase(cycle: &mut ReviewCycle, phase: crate::ReviewCyclePhase) {
    cycle.phase = phase;
}

pub fn push_disposition(finding: &mut Finding, record: crate::DispositionRecord) {
    finding.dispositions.push(record);
}

pub const fn set_superseded_by(finding: &mut Finding, canonical: FindingId) {
    finding.superseded_by = Some(canonical);
}

pub fn merge_sources_and_evidence(target: &mut Finding, source: &Finding) {
    target.sources.extend_from_slice(source.sources());
    target.sources.sort_unstable();
    target.sources.dedup();
    target.evidence.extend_from_slice(source.evidence());
    target.evidence.sort_unstable();
    target.evidence.dedup();
    target.dispositions.extend_from_slice(source.dispositions());
}
