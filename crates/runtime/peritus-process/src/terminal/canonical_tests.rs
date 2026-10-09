//! Complete terminal canonicalization regressions.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    CancellationReason, EscalationRecord, OsExitObservation, OutputArtifact, OutputCompleteness,
    OutputStream, OutputSummary, ProcessInstant, ProcessResourceDimension,
    ProcessResourceObservation, ResourceFidelity, StopTrigger, StreamAccounting,
    TerminalDisposition, TerminalRecovery, TerminalResult,
};

use super::{decode_terminal, encode_terminal, terminal_digest};

#[test]
fn complete_terminal_encoding_round_trips() {
    let terminal = complete_terminal();
    let bytes = encode_terminal(&terminal).expect("encode terminal");
    assert_eq!(decode_terminal(&bytes).expect("decode terminal"), terminal);
}

#[test]
fn corrupt_stream_accounting_is_rejected_during_decode() {
    let terminal = complete_terminal();
    let mut bytes = encode_terminal(&terminal).expect("encode terminal");
    let accounting = [0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 1];
    let offset = bytes
        .windows(accounting.len())
        .position(|window| window == accounting)
        .expect("canonical stream accounting");
    bytes[offset + 15] = 9;
    let error = decode_terminal(&bytes).expect_err("retained bytes cannot exceed observed bytes");
    assert_eq!(error.code(), crate::ErrorCode::CorruptRecovery);
}

#[test]
fn terminal_digest_binds_every_durable_fact_group() {
    let terminal = complete_terminal();

    let mut changed = terminal.clone();
    changed.process_id = ProcessId::new([3; 16]).expect("process");
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.plan_digest = Sha256Digest::new([4; 32]);
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.disposition = TerminalDisposition::TimedOut;
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.os_exit = OsExitObservation::Code(9);
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.first_trigger = Some(StopTrigger::new(8, CancellationReason::Deadline));
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.escalation = EscalationRecord::new(false, true, true);
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.started_at = Some(ProcessInstant::from_millis(4));
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.ended_at = ProcessInstant::from_millis(10);
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.output = OutputSummary::new(changed.output.streams().to_vec(), 4);
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.resources[0] = ProcessResourceObservation::new(
        ProcessResourceDimension::WallTimeMilliseconds,
        9,
        10,
        ResourceFidelity::Enforced,
    );
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.artifacts[0] = OutputArtifact::new(
        OutputStream::Stdout,
        Sha256Digest::new([5; 32]),
        7,
        0,
        7,
        OutputCompleteness::Truncated,
    );
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.escalation = EscalationRecord::new(true, true, false);
    changed.tree_cleanup_complete = false;
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.support_tasks_joined = false;
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.artifact_publication_complete = false;
    assert_digest_changed(&terminal, &changed);

    let mut changed = terminal.clone();
    changed.recovery = TerminalRecovery::ReopenedTerminal;
    assert_digest_changed(&terminal, &changed);
}

fn complete_terminal() -> TerminalResult {
    let stream = StreamAccounting::from_persisted(
        OutputStream::Stdout,
        8,
        7,
        1,
        OutputCompleteness::Truncated,
    )
    .expect("stream accounting");
    let mut terminal = TerminalResult::new(
        ProcessId::new([1; 16]).expect("process"),
        Sha256Digest::new([2; 32]),
        TerminalDisposition::OutputLimit,
        OsExitObservation::SignalName("TERM".to_owned()),
        Some(StopTrigger::new(7, CancellationReason::OutputLimit)),
        EscalationRecord::new(true, true, true),
        Some(ProcessInstant::from_millis(5)),
        ProcessInstant::from_millis(9),
        OutputSummary::new(vec![stream], 3),
        all_resource_observations(),
        true,
        true,
        TerminalRecovery::OriginalOwner,
    );
    terminal.add_artifact(OutputArtifact::new(
        OutputStream::Stdout,
        Sha256Digest::new([4; 32]),
        7,
        0,
        7,
        OutputCompleteness::Truncated,
    ));
    terminal.mark_artifacts_complete();
    terminal
}

fn all_resource_observations() -> Vec<ProcessResourceObservation> {
    [
        ProcessResourceDimension::WallTimeMilliseconds,
        ProcessResourceDimension::CpuTimeMilliseconds,
        ProcessResourceDimension::MemoryBytes,
        ProcessResourceDimension::DiskBytes,
        ProcessResourceDimension::OutputBytes,
        ProcessResourceDimension::ProcessCount,
        ProcessResourceDimension::OpenHandles,
        ProcessResourceDimension::ConcurrencySlots,
    ]
    .into_iter()
    .zip(1_u64..)
    .map(|(dimension, value)| {
        ProcessResourceObservation::new(dimension, value, value + 9, ResourceFidelity::Enforced)
    })
    .collect()
}

fn assert_digest_changed(baseline: &TerminalResult, changed: &TerminalResult) {
    assert_ne!(
        terminal_digest(baseline).expect("baseline digest"),
        terminal_digest(changed).expect("changed digest")
    );
}
