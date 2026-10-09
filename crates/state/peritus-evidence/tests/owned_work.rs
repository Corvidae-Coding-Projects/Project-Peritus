//! Real public-interface continuation and failure replays on `SQLite` and artifact files.

mod support;
use peritus_evidence::{
    BundleLimits, BundlePreparation, BundleVerificationOperation, EvidenceAdmission,
    EvidenceCancellation, EvidenceErrorKind, EvidenceStore, EvidenceStoreOptions, assemble_bundle,
    publish_bundle, verify_bundle,
};
use rusqlite::Connection;
use std::{
    fs,
    io::{self, Cursor, Read, Write},
    num::NonZeroUsize,
    time::Duration,
};
use support::{Fixture, revision};

const fn step(bytes: usize) -> NonZeroUsize {
    NonZeroUsize::new(bytes).expect("positive case step")
}

struct FallibleOutput {
    bytes: Vec<u8>,
    stop: Option<usize>,
}
impl Write for FallibleOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let allowed = self
            .stop
            .map_or(bytes.len(), |stop| stop.saturating_sub(self.bytes.len()).min(bytes.len()));
        if allowed == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.bytes.extend_from_slice(&bytes[..allowed]);
        Ok(allowed)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct FramedInput {
    bytes: Cursor<Vec<u8>>,
    frame_end: u64,
    calls: usize,
}
impl Read for FramedInput {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        assert!(
            self.bytes.position() < self.frame_end,
            "framed verifier must never wait for EOF or consume next message"
        );
        self.calls += 1;
        if self.calls.is_multiple_of(7) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let wanted = bytes.len().min(13);
        self.bytes.read(&mut bytes[..wanted])
    }
}

#[test]
fn generated_chunk_schedules_preserve_exact_framed_cursors_and_publication() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let artifact =
        fixture.finalize(&(0..196_617_u32).map(|value| (value % 251) as u8).collect::<Vec<_>>());
    let position = fixture.append(&rev, Some(artifact));
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let record = store
        .admit(
            Fixture::draft(83, rev, position, vec![artifact], vec![]),
            &export,
            &fixture.artifacts,
        )
        .expect("admission");
    let limits = BundleLimits::default();
    let plan =
        store.plan_bundle(&[record.id()], &rev, &export, &fixture.artifacts, limits).expect("plan");
    let mut expected = Vec::new();
    let receipt = assemble_bundle(&plan, &fixture.artifacts, &mut expected, limits)
        .expect("standalone assembly");
    assert_eq!(receipt.byte_count(), plan.byte_count());
    for seed in [1_u64, 7, 91, 2026] {
        let mut random = seed;
        let mut preparation =
            BundlePreparation::new(&plan, &fixture.artifacts, limits).expect("owned staging");
        let cancellation = EvidenceCancellation::new();
        let mut pauses = 0;
        loop {
            random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let budget =
                usize::try_from((random >> 32) % 4096 + 1).expect("bounded generated budget");
            let previous = preparation.staged_bytes();
            let done = preparation
                .advance(step(budget), &cancellation)
                .expect("bounded preparation")
                .is_some();
            assert!(preparation.staged_bytes() - previous <= budget as u64);
            if pauses < 3 && preparation.artifact_offset() > 0 {
                let cancelled = EvidenceCancellation::new();
                cancelled.cancel();
                let observed = (preparation.staged_bytes(), preparation.artifact_offset());
                assert_eq!(
                    preparation
                        .advance(step(113), &cancelled)
                        .expect_err("cancel preparation")
                        .kind(),
                    EvidenceErrorKind::Cancelled
                );
                assert_eq!((preparation.staged_bytes(), preparation.artifact_offset()), observed);
                pauses += 1;
            }
            if done {
                break;
            }
        }
        assert_eq!(pauses, 3);
        let mut output = FallibleOutput {
            bytes: vec![],
            stop: Some(137 + usize::try_from(seed).expect("small seed")),
        };
        let mut operation = preparation.into_export().expect("authenticated stage");
        assert_eq!(
            operation
                .resume(&mut output, &cancellation)
                .expect_err("partial output failure")
                .kind(),
            EvidenceErrorKind::Io
        );
        assert_eq!(operation.cursor().byte_offset(), output.bytes.len() as u64);
        assert!(operation.receipt().is_none());
        output.stop = None;
        loop {
            let previous = output.bytes.len();
            let complete = operation
                .advance(&mut output, step(333), &cancellation)
                .expect("resume exact output")
                .is_some();
            assert!(output.bytes.len() - previous <= 333);
            if complete {
                break;
            }
        }
        assert_eq!(output.bytes, expected);
        assert_eq!(operation.receipt(), Some(receipt));
        verify_framed(&expected, receipt);
    }
    let destination = fixture.temp.path().join("published.evb");
    assert_eq!(
        publish_bundle(&plan, &fixture.artifacts, &destination, limits).expect("atomic publish"),
        receipt
    );
    assert_eq!(
        publish_bundle(&plan, &fixture.artifacts, &destination, limits)
            .expect("idempotent publish"),
        receipt
    );
    assert_eq!(fs::read(&destination).expect("published file"), expected);
    fs::write(&destination, b"another owner").expect("conflicting destination");
    assert!(publish_bundle(&plan, &fixture.artifacts, &destination, limits).is_err());
    assert_eq!(fs::read(&destination).expect("conflict preserved"), b"another owner");
}

fn verify_framed(expected: &[u8], receipt: peritus_evidence::BundleReceipt) {
    let limits = BundleLimits::default();
    let cancellation = EvidenceCancellation::new();
    let frame_end = expected.len() as u64;
    let mut stream = expected.to_vec();
    stream.extend_from_slice(b"next-message");
    let mut verifier = BundleVerificationOperation::new(
        FramedInput { bytes: Cursor::new(stream), frame_end, calls: 0 },
        limits,
    );
    let mut retries = 0;
    let mut cancelled_once = false;
    loop {
        let previous = verifier.cursor();
        if !cancelled_once && previous.input_bytes() > 1000 {
            let cancelled = EvidenceCancellation::new();
            cancelled.cancel();
            assert_eq!(
                verifier.advance(step(107), &cancelled).expect_err("cancel verifier").kind(),
                EvidenceErrorKind::Cancelled
            );
            assert_eq!(verifier.cursor(), previous);
            cancelled_once = true;
        }
        match verifier.advance(step(777), &cancellation) {
            Ok(Some(observed)) => {
                assert_eq!(observed.bundle_digest(), receipt.bundle_digest());
                break;
            }
            Ok(None) => {}
            Err(error) => {
                assert_eq!(error.kind(), EvidenceErrorKind::Io);
                retries += 1;
            }
        }
        assert!(verifier.cursor().input_bytes() - previous.input_bytes() <= 777);
    }
    assert!(retries > 0 && cancelled_once);
    assert_eq!(verifier.cursor().input_bytes(), frame_end);
    let input = verifier.into_input();
    assert_eq!(input.bytes.position(), frame_end);
    assert_eq!(
        &input.bytes.get_ref()[usize::try_from(frame_end).expect("frame size")..],
        b"next-message"
    );
}

#[test]
fn preflight_and_source_integrity_fail_before_any_caller_output() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let artifact = fixture.finalize(b"original exact artifact");
    let position = fixture.append(&rev, Some(artifact));
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let record = store
        .admit(
            Fixture::draft(84, rev, position, vec![artifact], vec![]),
            &export,
            &fixture.artifacts,
        )
        .expect("admission");
    let plan = store
        .plan_bundle(&[record.id()], &rev, &export, &fixture.artifacts, BundleLimits::default())
        .expect("plan");
    let mut output = b"untouched".to_vec();
    let limit = BundleLimits::optional(None, None, Some(plan.byte_count() - 1))
        .expect("explicit byte limit");
    assert!(assemble_bundle(&plan, &fixture.artifacts, &mut output, limit).is_err());
    assert_eq!(output, b"untouched");
    fs::write(fixture.object_path(artifact), b"Original exact artifact")
        .expect("same-size corruption");
    assert_eq!(
        assemble_bundle(&plan, &fixture.artifacts, &mut output, BundleLimits::default())
            .expect_err("source corruption")
            .kind(),
        EvidenceErrorKind::CorruptArtifact
    );
    assert_eq!(output, b"untouched");
    fs::write(fixture.object_path(artifact), b"original exact artifact").expect("restore source");
    let mut exact = Vec::new();
    let limits =
        BundleLimits::optional(Some(1), None, Some(plan.byte_count())).expect("exact budget");
    assemble_bundle(&plan, &fixture.artifacts, &mut exact, limits).expect("exact budget works");
    verify_bundle(exact.as_slice(), limits).expect("exact framed length");
    let last = exact.len() - 1;
    exact[last] ^= 1;
    let mut verifier = BundleVerificationOperation::new(exact.as_slice(), limits);
    assert_eq!(
        verifier.resume(&EvidenceCancellation::new()).expect_err("root mismatch").kind(),
        EvidenceErrorKind::InvalidBundle
    );
    assert!(verifier.resume(&EvidenceCancellation::new()).is_err());
}

#[test]
fn admission_hashes_without_writer_lock_and_exact_retry_precedes_missing_dependencies() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let empty_export = fixture.export();
    let artifact = fixture.finalize(&vec![39; 131_101]);
    let position = fixture.append(&rev, Some(artifact));
    let export = fixture.export();
    let mut store = EvidenceStore::open(&fixture.path, EvidenceStoreOptions::new(Duration::ZERO))
        .expect("fail-fast store");
    let draft = Fixture::draft(85, rev, position, vec![artifact], vec![]);
    let mut operation =
        EvidenceAdmission::new(&mut store, draft.clone(), &export, &fixture.artifacts);
    let cancellation = EvidenceCancellation::new();
    assert!(operation.advance(step(97), &cancellation).expect("first hash chunk").is_none());
    let competing = Connection::open(&fixture.path).expect("independent writer");
    competing.execute_batch("BEGIN IMMEDIATE").expect("artifact hashing owns no writer lock");
    while operation.verified_artifacts() == 0 {
        operation.advance(step(997), &cancellation).expect("hash continues under other writer");
    }
    assert_eq!(
        operation.advance(step(9), &cancellation).expect_err("commit contention").recovery(),
        peritus_evidence::RecoveryAction::Retry
    );
    assert_eq!(operation.verified_artifacts(), 1);
    competing.execute_batch("ROLLBACK").expect("release competing writer");
    let admitted = operation.finish(&cancellation).expect("same draft commits after contention");
    drop(operation);
    fs::remove_file(fixture.object_path(artifact)).expect("dependency unavailable after commit");
    assert_eq!(
        store
            .admit(draft, &empty_export, &fixture.artifacts)
            .expect("exact committed receipt before dependency work"),
        admitted
    );
    let conflict = Fixture::draft(85, rev, position, vec![], vec![]);
    assert_eq!(
        store
            .admit(conflict, &empty_export, &fixture.artifacts)
            .expect_err("identity conflict precedes missing dependency")
            .kind(),
        EvidenceErrorKind::IdentityConflict
    );
}
