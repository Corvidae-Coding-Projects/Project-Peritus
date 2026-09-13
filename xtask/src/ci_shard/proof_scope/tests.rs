use crate::ci_shard::{self, Operation};
use std::fs;

#[test]
fn missing_policy_cannot_leave_previous_success_artifacts() {
    let root = std::env::temp_dir()
        .join(format!("peritus-proof-preflight-cleanup-{}", std::process::id()));
    let directory = root.join("target/formal-scope/app-runner");
    fs::create_dir_all(&directory).expect("create stale report fixture");
    for file in ["selected-functions.json", "old-verus-output.txt", "source-inputs.json"] {
        fs::write(directory.join(file), b"stale success").expect("write stale report");
    }
    let result = ci_shard::run(&root, Operation::VerusVerifyStrict, "app-runner");
    let remaining = fs::read_dir(&directory).expect("fresh generated directory").count();
    fs::remove_dir_all(&root).expect("remove generated fixture");
    assert!(result.is_err());
    assert_eq!(remaining, 0, "metadata failure must not retain any old proof artifacts");
}
