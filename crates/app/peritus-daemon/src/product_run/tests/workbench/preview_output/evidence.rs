//! Persisted exact-output bindings survive restart and reject altered command evidence.
use super::*;

pub(super) fn assert_retained(
    service: &ProductRunService,
    run: RunId,
    command: &WorkbenchCommand,
    finalized: bool,
) -> u64 {
    let records = service.inner.records.read().expect("records");
    let record = records.get(&run).expect("run");
    let operation = record.preview.operations.get(&command.operation()).expect("check");
    let evidence = operation.behavior_evidence.as_ref().expect("durable evidence");
    let matched = evidence.observed_match;
    assert_eq!(matched.artifact_digest().is_some(), finalized);
    assert_eq!(matched.end_byte() - matched.start_byte(), 12);
    assert_eq!(matched.matched_bytes_digest(), peritus_codec::sha256(b"EARLY_SIGNAL").into_bytes());
    assert!(matched.observed_stream_bytes() > 128 * 1024);
    assert_eq!(operation.fingerprint, command.fingerprint().expect("fingerprint"));
    matched.start_byte()
}

pub(super) fn reject_corrupt_and_preserve_legacy(
    service: &ProductRunService,
    run: RunId,
    command: &WorkbenchCommand,
) {
    let records = service.inner.records.read().expect("records");
    super::super::super::super::persistence::test_behavior_evidence(
        records.get(&run).expect("run"),
        command.operation(),
    );
}
