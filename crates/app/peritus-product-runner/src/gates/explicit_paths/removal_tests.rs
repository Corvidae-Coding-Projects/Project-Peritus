use super::*;

fn cancellation_transcript(root: &Path) -> String {
    format!(
        "User round 1:\nCreate the following:\n- {0}/out/staging/plan.json\n- {0}/out/history.log\n\n\
         User round 2:\nRead {0}/out/staging/plan.json. Update {0}/out/history.log.\n\
         Remove temporary files and directories under {0}/out/staging.\n",
        root.display(),
    )
}

#[test]
fn later_cleanup_retires_prior_child_presence_but_preserves_other_outputs() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("out")).expect("out");
    fs::write(root.path().join("out/history.log"), "started\ncancelled\n").expect("audit");
    let transcript = cancellation_transcript(root.path());

    let record = run(root.path(), &transcript, &[PathBuf::from("out/history.log")]);
    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert_eq!(required_outputs(root.path(), &transcript), vec![PathBuf::from("out/history.log")]);

    fs::remove_file(root.path().join("out/history.log")).expect("remove audit");
    assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(1));
}

#[test]
fn later_cleanup_rejects_surviving_artifacts_instead_of_requiring_them() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(root.path().join("out/staging")).expect("staging");
    fs::write(root.path().join("out/staging/plan.json"), "{}").expect("draft");
    fs::write(root.path().join("out/history.log"), "started\ncancelled\n").expect("audit");

    let record = run(root.path(), &cancellation_transcript(root.path()), &[]);
    assert_eq!(record.exit_code, Some(1), "{}", record.output);
    assert!(record.output.contains("path remains after explicit removal"), "{}", record.output);
}

#[test]
fn later_exact_removal_and_recreation_follow_chronological_order() {
    let root = tempfile::tempdir().expect("root");
    let removed = "Create `draft.json`.\nDelete `draft.json`.\nRead `draft.json` in the old log.";
    assert_eq!(run(root.path(), removed, &[]).exit_code, Some(0));
    let recreated = format!("{removed}\nCreate `draft.json` again.");
    assert_eq!(run(root.path(), &recreated, &[]).exit_code, Some(1));
    fs::write(root.path().join("draft.json"), "{}").expect("recreated");
    assert_eq!(run(root.path(), &recreated, &[]).exit_code, Some(0));
    assert_eq!(run(root.path(), removed, &[]).exit_code, Some(1));
}

#[test]
fn negated_conditional_quoted_and_descriptive_removals_do_not_override_presence() {
    let root = tempfile::tempdir().expect("root");
    for instruction in [
        "Do not delete `draft.json`.",
        "Never remove `draft.json`.",
        "If cancellation arrives, delete `draft.json`.",
        "Delete `draft.json` unless retention is required.",
        "Read the old note: delete `draft.json`.",
        "The report says delete `draft.json`.",
        "Record `delete` beside `draft.json`.",
        "Remove the field `draft.json` from `report.json`.",
    ] {
        let transcript = format!("Create `draft.json`.\n{instruction}");
        assert_eq!(
            required_outputs(root.path(), &transcript),
            vec![PathBuf::from("draft.json")],
            "{instruction}"
        );
        assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(1), "{instruction}");
    }
}

#[test]
fn cleanup_checks_unknown_children_and_keeps_similarly_named_siblings_required() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(root.path().join("out/staging")).expect("staging");
    fs::create_dir_all(root.path().join("out/staging-old")).expect("sibling");
    fs::write(root.path().join("out/staging-old/keep.json"), "{}").expect("sibling output");
    let transcript = "Create `out/staging/plan.json`.\nCreate `out/staging-old/keep.json`.\nRemove files and directories under `out/staging`.";
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(0));
    fs::create_dir(root.path().join("out/staging/unlisted")).expect("unlisted temporary directory");
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(1));
    fs::remove_dir(root.path().join("out/staging/unlisted")).expect("cleanup");
    fs::remove_file(root.path().join("out/staging-old/keep.json")).expect("remove sibling");
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(1));
}

#[test]
fn recreation_under_removed_directory_keeps_other_cleanup_requirements() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(root.path().join("out/staging/restarted")).expect("restart directory");
    fs::write(root.path().join("out/staging/restarted/current.json"), "{}").expect("current");
    let transcript = "Create `out/staging/old.json`.\nDelete directory `out/staging`.\nCreate `out/staging/restarted/current.json`.";
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(0));
    fs::write(root.path().join("out/staging/old.json"), "{}").expect("stale output");
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(1));
    fs::remove_file(root.path().join("out/staging/old.json")).expect("cleanup");
    let removed_again = format!("{transcript}\nDelete file `out/staging/restarted/current.json`.");
    assert_eq!(run(root.path(), &removed_again, &[]).exit_code, Some(1));
    fs::remove_file(root.path().join("out/staging/restarted/current.json"))
        .expect("remove current");
    assert_eq!(run(root.path(), &removed_again, &[]).exit_code, Some(0));
}

#[test]
fn removal_prunes_old_alternatives_without_dropping_unaffected_members() {
    let root = tempfile::tempdir().expect("root");
    let transcript =
        "Create at least one sample file named `first.txt` or `second.txt`.\nDelete `first.txt`.";
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(1));
    fs::write(root.path().join("second.txt"), "sample").expect("remaining alternative");
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(0));
    let removed_all = format!("{transcript}\nDelete `second.txt`.");
    fs::remove_file(root.path().join("second.txt")).expect("remove second");
    assert_eq!(run(root.path(), &removed_all, &[]).exit_code, Some(0));
}

#[test]
fn removal_paths_stay_inside_workspace_and_reject_parent_traversal() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("keep.json"), "protected").expect("outside file");
    let transcript =
        format!("Delete file `{}/keep.json`.\nDelete `../keep.json`.", outside.path().display());
    assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(0));
    assert_eq!(
        fs::read_to_string(outside.path().join("keep.json")).expect("outside intact"),
        "protected"
    );
}

#[test]
fn equivalent_current_directory_paths_share_the_same_latest_requirement() {
    let root = tempfile::tempdir().expect("root");
    let transcript = "Create `./out/staging/old.json`.\nDelete file `out/staging/old.json`.";
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(0));
    assert!(required_outputs(root.path(), transcript).is_empty());
}

#[test]
fn compound_update_create_declaration_keeps_every_current_output_required() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("out")).expect("out");
    fs::write(root.path().join("out/history.log"), "audit").expect("history");
    fs::write(root.path().join("out/state.json"), "{}").expect("state");
    let transcript = "Update/create:\n- `out/state.json`\n- `out/history.log`\n- `out/report.md`";
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(1));
    fs::write(root.path().join("out/report.md"), "cancelled").expect("report");
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(0));
    assert_eq!(required_outputs(root.path(), transcript).len(), 3);
}

#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_a_symlink_to_external_files() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("keep.json"), "protected").expect("outside file");
    fs::create_dir_all(root.path().join("out/staging")).expect("staging");
    symlink(outside.path(), root.path().join("out/staging/link")).expect("link");
    let record = run(root.path(), "Remove contents under `out/staging`.", &[]);
    assert_eq!(record.exit_code, Some(1));
    assert_eq!(
        fs::read_to_string(outside.path().join("keep.json")).expect("outside intact"),
        "protected"
    );

    fs::remove_file(root.path().join("out/staging/link")).expect("remove child link");
    fs::remove_dir(root.path().join("out/staging")).expect("remove staging");
    fs::remove_dir(root.path().join("out")).expect("remove out");
    symlink(outside.path(), root.path().join("out")).expect("ancestor link");
    let ancestor = run(root.path(), "Remove contents under `out/staging`.", &[]);
    assert_eq!(ancestor.exit_code, Some(1));
    assert!(ancestor.output.contains("path remains after explicit removal: out"));
}
