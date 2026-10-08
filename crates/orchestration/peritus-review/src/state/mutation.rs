//! Reducer-only mutation helpers for otherwise immutable D2 state.

use peritus_types::{CommandId, EventId, EventSequence, FindingId, Sha256Digest};

use super::{ReviewRunPhase, ReviewRunState, ReviewTerminal, ReviewTerminalKind};
use crate::{Finding, ObservedWaiver, OscillationReport, QuorumReport, ReviewBinding, ReviewCycle};
use crate::error::{ReviewError, ReviewErrorKind, reject};

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

/// Moves predecessor facts out of the bounded V2 checkpoint and into its immutable-event frontier.
pub fn archive_predecessor(
    state: &mut ReviewRunState,
    force_cycles: bool,
    force_page: bool,
) -> Result<(), ReviewError> {
    if !state.binding.uses_paged_history() {
        return Ok(());
    }
    if !force_cycles && !force_page {
        return Ok(());
    }
    let archive_cycles = force_cycles || (force_page && state.quorum.complete());
    let archived_cycles = if archive_cycles {
        state
            .cycles
            .iter()
            .filter(|cycle| force_cycles || cycle.phase() != crate::ReviewCyclePhase::Assigned)
            .count()
    } else {
        0
    };
    let archived_submissions = if archive_cycles {
        state
            .cycles
            .iter()
            .filter(|cycle| {
                (force_cycles || cycle.phase() != crate::ReviewCyclePhase::Assigned)
                    && cycle.submission().is_some()
            })
            .count()
    } else {
        0
    };
    let archived_findings = state.findings.len();
    let archived_dispositions = state.findings.iter().map(|finding| finding.dispositions().len()).sum::<usize>();
    let archived_waivers = state.waivers.len();

    if archived_cycles == 0 && archived_findings == 0 && archived_waivers == 0 {
        state.used_commands.clear();
        return Ok(());
    }

    let mut history = state.history;
    let add = |value: u64, amount: usize| {
        value.checked_add(u64::try_from(amount).map_err(|_| {
            reject(ReviewErrorKind::LimitExceeded, "review history count cannot be represented")
        })?)
        .ok_or_else(|| reject(ReviewErrorKind::LimitExceeded, "review history count overflowed"))
    };
    history.parent_digest = history.digest;
    history.through_sequence = state.sequence.get();
    history.through_event = Some(state.last_event_id);
    history.through_state_digest = state.state_digest;
    history.page_count = history.page_count.checked_add(1).ok_or_else(|| {
        reject(ReviewErrorKind::LimitExceeded, "review history page sequence overflowed")
    })?;
    history.cycle_count = add(history.cycle_count, archived_cycles)?;
    history.submission_count = add(history.submission_count, archived_submissions)?;
    history.finding_count = add(history.finding_count, archived_findings)?;
    history.disposition_count = add(history.disposition_count, archived_dispositions)?;
    history.waiver_count = add(history.waiver_count, archived_waivers)?;
    for finding in &state.findings {
        if state.finding_is_current(finding) && !finding.is_conserved() {
            history.current_unconserved_count = history
                .current_unconserved_count
                .checked_add(1)
                .ok_or_else(|| {
                    reject(ReviewErrorKind::LimitExceeded, "current finding count overflowed")
                })?;
            history.current_unconserved_xor = xor_digest(
                history.current_unconserved_xor,
                finding_identity_digest(finding.id()),
            );
        }
    }
    history.digest = crate::canonical::history_frontier_digest(history);
    state.history = history;
    if archive_cycles {
        if force_cycles {
            state.cycles.clear();
        } else {
            state.cycles.retain(|cycle| cycle.phase() == crate::ReviewCyclePhase::Assigned);
        }
    }
    state.findings.clear();
    state.waivers.clear();
    state.used_commands.clear();
    Ok(())
}

/// Removes hydrated archived findings from the current aggregate before they become active facts.
pub fn detach_hydrated_findings(
    state: &mut ReviewRunState,
    findings: &[Finding],
) -> Result<(), ReviewError> {
    if !state.binding.uses_paged_history() {
        return Ok(());
    }
    for finding in findings {
        if finding.revision() == state.binding.revision() && !finding.is_conserved() {
            state.history.current_unconserved_count = state
                .history
                .current_unconserved_count
                .checked_sub(1)
                .ok_or_else(|| {
                    reject(
                        ReviewErrorKind::ReplayMismatch,
                        "hydrated finding is absent from the conservation frontier",
                    )
                })?;
            state.history.current_unconserved_xor = xor_digest(
                state.history.current_unconserved_xor,
                finding_identity_digest(finding.id()),
            );
        }
    }
    state.history.digest = crate::canonical::history_frontier_digest(state.history);
    Ok(())
}

/// Starts current-finding aggregation for a newly bound candidate while retaining prior pages.
pub fn reset_history_binding(state: &mut ReviewRunState, binding_digest: Sha256Digest) {
    if state.binding.uses_paged_history() {
        state.history.current_binding_digest = binding_digest;
        state.history.current_unconserved_count = 0;
        state.history.current_unconserved_xor = Sha256Digest::new([0; 32]);
        state.history.digest = crate::canonical::history_frontier_digest(state.history);
    }
}

/// Keeps only the current disposition snapshot in the bounded V2 checkpoint.
pub fn compact_active_dispositions(state: &mut ReviewRunState) {
    if !state.binding.uses_paged_history() {
        return;
    }
    for finding in &mut state.findings {
        if finding.dispositions.len() > 1 {
            let historical = finding.dispositions.len() - 1;
            finding.dispositions.drain(..historical);
        }
    }
}

pub(crate) fn finding_identity_digest(finding_id: FindingId) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(b"peritus-d2-current-finding-v2\0");
    bytes.extend_from_slice(finding_id.as_bytes());
    peritus_codec::sha256(&bytes)
}

pub(crate) fn xor_digest(left: Sha256Digest, right: Sha256Digest) -> Sha256Digest {
    let mut bytes = left.into_bytes();
    for (byte, right) in bytes.iter_mut().zip(right.as_bytes()) {
        *byte ^= *right;
    }
    Sha256Digest::new(bytes)
}

pub(crate) fn finding_xor(findings: &[FindingId]) -> Sha256Digest {
    findings.iter().fold(Sha256Digest::new([0; 32]), |digest, finding| {
        xor_digest(digest, finding_identity_digest(*finding))
    })
}

pub fn replace_binding(state: &mut ReviewRunState, binding: ReviewBinding) {
    for cycle in &mut state.cycles {
        if cycle.assignment().binding_digest() == state.binding.digest() {
            cycle.phase = crate::ReviewCyclePhase::Invalidated;
        }
    }
    let digest = binding.digest();
    state.binding = binding;
    state.quorum = QuorumReport::evaluate(&state.binding, &state.cycles);
    reset_history_binding(state, digest);
}

pub fn push_cycle(state: &mut ReviewRunState, cycle: ReviewCycle) {
    state.cycles.push(cycle);
    state.cycles.sort_unstable_by_key(|cycle| (cycle.ordinal().get(), cycle.id()));
}

pub fn cycle_mut(
    state: &mut ReviewRunState,
    cycle_id: peritus_types::ReviewCycleId,
) -> Option<&mut ReviewCycle> {
    state.cycles.iter_mut().find(|cycle| cycle.id() == cycle_id)
}

pub fn remove_cycle(state: &mut ReviewRunState, cycle_id: peritus_types::ReviewCycleId) {
    state.cycles.retain(|cycle| cycle.id() != cycle_id);
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
    if !state.binding.uses_paged_history() || !state.quorum.complete() {
        state.quorum = QuorumReport::evaluate(&state.binding, &state.cycles);
    }
    let unconserved = state.unconserved_current_count();
    state.oscillation = OscillationReport::evaluate(
        &state.binding,
        &state.cycles,
        &state.findings,
        state.quorum.complete() && unconserved == 0,
    );
}

pub fn terminal(state: &mut ReviewRunState, terminal: ReviewTerminal) {
    state.terminal = Some(terminal);
    state.phase = ReviewRunPhase::Terminal;
}

pub fn make_terminal(
    paged: bool,
    kind: ReviewTerminalKind,
    unconserved_findings: Vec<FindingId>,
    unconserved_count: u64,
    unconserved_xor: Sha256Digest,
    quorum: QuorumReport,
    oscillation: OscillationReport,
    cause_digest: Sha256Digest,
) -> ReviewTerminal {
    let mut terminal = if paged {
        ReviewTerminal::from_wire_v2(
            kind,
            unconserved_findings,
            unconserved_count,
            unconserved_xor,
            quorum,
            oscillation,
            cause_digest,
            Sha256Digest::new([0; 32]),
        )
    } else {
        ReviewTerminal::from_wire(
            kind,
            unconserved_findings,
            quorum,
            oscillation,
            cause_digest,
            Sha256Digest::new([0; 32]),
        )
    };
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
