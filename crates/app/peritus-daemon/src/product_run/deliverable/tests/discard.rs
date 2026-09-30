use super::super::discard::{recover_completed, save_completed};
use super::*;

#[test]
fn reservation_alone_is_inert_and_a_matching_retry_can_complete_it() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"candidate\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    let deliverable = record.snapshot.deliverable().unwrap();
    let reserved =
        super::super::discard::Reservation::prepare(state.path(), &record, deliverable).unwrap();
    drop(reserved);
    assert!(!recover_completed(state.path(), &mut record).unwrap());
    assert_eq!(fs::read(repository.path().join("chosen.txt")).unwrap(), b"candidate\n");
    let deliverable = record.snapshot.deliverable().unwrap();
    let retry =
        super::super::discard::Reservation::prepare(state.path(), &record, deliverable).unwrap();
    discard_deliverable(deliverable).unwrap();
    retry.complete("Deliverable discarded").unwrap();
    assert!(recover_completed(state.path(), &mut record).unwrap());
}

#[test]
fn unknown_or_foreign_reservation_is_preserved() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"first candidate\n").unwrap();
    let original = qualified_record(candidate_record(&repository));
    let deliverable = original.snapshot.deliverable().unwrap();
    drop(
        super::super::discard::Reservation::prepare(state.path(), &original, deliverable).unwrap(),
    );
    let pending =
        state.path().join(format!("{}.discard-result.new", run_hex(original.request.run_id())));
    let original_reservation = fs::read(&pending).unwrap();
    fs::write(repository.path().join("chosen.txt"), b"later candidate\n").unwrap();
    let later = qualified_record(candidate_record(&repository));
    assert!(
        super::super::discard::Reservation::prepare(
            state.path(),
            &later,
            later.snapshot.deliverable().unwrap()
        )
        .is_err()
    );
    assert_eq!(fs::read(&pending).unwrap(), original_reservation);
    fs::write(&pending, b"foreign file\n").unwrap();
    assert!(
        super::super::discard::Reservation::prepare(state.path(), &original, deliverable).is_err()
    );
    assert_eq!(fs::read(&pending).unwrap(), b"foreign file\n");
}

#[test]
fn completed_reserved_record_recovers_after_rename_failure_without_touching_later_edits() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"candidate\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    let reservation = super::super::discard::Reservation::prepare(
        state.path(),
        &record,
        record.snapshot.deliverable().unwrap(),
    )
    .unwrap();
    let target = state.path().join(format!("{}.discard-result", run_hex(record.request.run_id())));
    fs::create_dir(&target).unwrap();
    discard_deliverable(record.snapshot.deliverable().unwrap()).unwrap();
    assert!(reservation.complete("Deliverable discarded").is_err());
    fs::remove_dir(&target).unwrap();
    fs::write(repository.path().join("chosen.txt"), b"new human draft\n").unwrap();
    assert!(recover_completed(state.path(), &mut record).unwrap());
    assert!(record.snapshot.deliverable().unwrap().discarded());
    assert_eq!(fs::read(repository.path().join("chosen.txt")).unwrap(), b"new human draft\n");
}

#[cfg(unix)]
#[test]
fn a_symlink_reservation_does_not_overwrite_its_target() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"candidate\n").unwrap();
    let mut record = qualified_record(candidate_record(&repository));
    let pending =
        state.path().join(format!("{}.discard-result.new", run_hex(record.request.run_id())));
    let foreign = state.path().join("foreign.txt");
    fs::write(&foreign, b"unrelated bytes\n").unwrap();
    std::os::unix::fs::symlink(&foreign, &pending).unwrap();
    assert!(
        super::super::discard::Reservation::prepare(
            state.path(),
            &record,
            record.snapshot.deliverable().unwrap()
        )
        .is_err()
    );
    assert!(recover_completed(state.path(), &mut record).is_err());
    assert_eq!(fs::read(&foreign).unwrap(), b"unrelated bytes\n");
    assert_eq!(fs::read(repository.path().join("chosen.txt")).unwrap(), b"candidate\n");
}

#[test]
fn a_live_reservation_cannot_be_reopened_by_a_second_control() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"candidate\n").unwrap();
    let record = qualified_record(candidate_record(&repository));
    let deliverable = record.snapshot.deliverable().unwrap();
    let reservation =
        super::super::discard::Reservation::prepare(state.path(), &record, deliverable).unwrap();
    assert!(
        super::super::discard::Reservation::prepare(state.path(), &record, deliverable).is_err()
    );
    drop(reservation);
    assert!(
        super::super::discard::Reservation::prepare(state.path(), &record, deliverable).is_ok()
    );
}

#[test]
fn completion_record_cannot_acknowledge_a_different_candidate_or_baseline() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"first candidate\n").unwrap();
    let mut original = qualified_record(candidate_record(&repository));
    save_completed(
        state.path(),
        &original,
        original.snapshot.deliverable().unwrap(),
        "Deliverable discarded",
    )
    .unwrap();
    fs::write(repository.path().join("chosen.txt"), b"new candidate\n").unwrap();
    let mut different = qualified_record(candidate_record(&repository));
    assert!(!recover_completed(state.path(), &mut different).unwrap());
    original.task_baseline_required = true;
    assert!(!recover_completed(state.path(), &mut original).unwrap());
    assert_eq!(fs::read(repository.path().join("chosen.txt")).unwrap(), b"new candidate\n");
    assert!(!different.snapshot.deliverable().unwrap().discarded());
}

#[test]
fn corrupt_completion_record_is_scoped_to_its_run_without_aborting_startup() {
    let repository = repository();
    let state = TempDir::new().unwrap();
    fs::write(repository.path().join("chosen.txt"), b"candidate\n").unwrap();
    let record = qualified_record(candidate_record(&repository));
    let run = record.request.run_id();
    let workspace = record.request.workspace_id();
    fs::write(state.path().join(format!("{}.discard-result", run_hex(run))), b"invalid receipt\n")
        .unwrap();
    let mut records = std::collections::BTreeMap::from([(run, record)]);
    crate::product_run::recovery::reconcile_restored_candidates(
        state.path(),
        &mut records,
        &std::collections::BTreeMap::from([(workspace, repository.path().to_path_buf())]),
    )
    .unwrap();
    assert!(!records[&run].candidate_actionable);
    assert!(!records[&run].snapshot.deliverable().unwrap().discarded());
    assert!(records[&run].snapshot.summary().contains("retain completed discard result"));
    assert_eq!(fs::read(repository.path().join("chosen.txt")).unwrap(), b"candidate\n");
}
