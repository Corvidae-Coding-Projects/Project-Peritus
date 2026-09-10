use super::{
    CHECKPOINTS, FOLDER, LIBRARY, MODELS, PRODUCT, Partition, REVIEW, REWINDS, WORKBENCH,
    test_filters,
};
use crate::ci_shard::{Operation, cargo_command, selected_packages};
use crate::metadata;
use std::path::Path;

const PARTITIONS: [(Partition, &str); 8] = [
    (Partition::Product, "test-daemon-product"),
    (Partition::Folder, "test-daemon-folder"),
    (Partition::Workbench, "test-daemon-workbench"),
    (Partition::Checkpoints, "test-daemon-checkpoints"),
    (Partition::Models, "test-daemon-models"),
    (Partition::Library, "test-daemon-library"),
    (Partition::Review, "test-daemon-review"),
    (Partition::Rewinds, "test-daemon-rewinds"),
];

#[test]
fn windows_namespaces_form_an_exhaustive_nonoverlapping_partition() {
    let mut operations = vec![Operation::TestDaemon];
    operations.extend(PARTITIONS.map(|(partition, _)| Operation::TestDaemonPartition(partition)));
    for name in [
        "authority::owner::test".to_owned(),
        "product_run::persistence::test".to_owned(),
        "second_runtime_cannot_acquire_a_live_state_root".to_owned(),
        format!("{PRODUCT}test"),
        format!("{PRODUCT}future_module::test"),
        format!("{FOLDER}test"),
        format!("{FOLDER}future_module::test"),
        format!("{WORKBENCH}test"),
        format!("{WORKBENCH}future_module::test"),
        format!("{REWINDS}rewind_test"),
        format!("{MODELS}test"),
        format!("{LIBRARY}future_module::test"),
        format!("{REVIEW}test"),
        format!("{CHECKPOINTS}test"),
        format!("{CHECKPOINTS}future_module::test"),
    ] {
        let owners = operations
            .iter()
            .filter(|operation| matches_filters(&name, &test_filters(**operation, true)))
            .count();
        assert_eq!(owners, 1, "test must execute exactly once: {name}");
    }
    assert!(test_filters(Operation::TestDaemon, false).is_empty());
    assert!(test_filters(Operation::Test, true).is_empty());
}

#[test]
fn each_additional_job_is_a_locked_serial_daemon_library_test() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let policy = metadata::architecture_policy(root).expect("architecture");
    let cargo = metadata::cargo_metadata(root).expect("metadata");
    for (partition, spelling) in PARTITIONS {
        let operation = Operation::TestDaemonPartition(partition);
        assert_eq!(Operation::parse(spelling), Some(operation));
        assert_eq!(
            selected_packages(&policy, &cargo, operation, "app-shell").expect("daemon"),
            ["peritus-daemon"]
        );
        assert!(selected_packages(&policy, &cargo, operation, "app-runner").is_err());
        let command = cargo_command(root, operation, &["peritus-daemon"]);
        let arguments = command.get_args().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>();
        for required in ["--locked", "--all-features", "--lib", "--test-threads=1"] {
            assert!(arguments.iter().any(|arg| arg == required));
        }
        assert!(!arguments.iter().any(|arg| matches!(arg.as_ref(), "--all-targets" | "--ignored")));
    }
}

fn matches_filters(name: &str, filters: &[&str]) -> bool {
    let mut positive_filter = None;
    let mut arguments = filters.iter();
    while let Some(argument) = arguments.next() {
        if *argument == "--skip" {
            if name.contains(arguments.next().expect("skip filter")) {
                return false;
            }
        } else {
            positive_filter = Some(*argument);
        }
    }
    positive_filter.is_none_or(|filter| name.contains(filter))
}
