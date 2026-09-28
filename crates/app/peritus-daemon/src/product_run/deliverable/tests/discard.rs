use super::super::discard::{recover_completed, save_completed};
use super::*;

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
