//! Real process interruption at the durable publication boundaries.

use super::*;
use serde::{Deserialize, Serialize};
use std::{
    process::Child,
    time::{Duration, Instant},
};

// Preparation starts multiple native Git processes and flushes retained state. This is a
// crash-consistency test, so allow slow runner I/O without imposing a startup speed contract.
const PREPARATION_TIMEOUT: Duration = Duration::from_mins(2);
const LOCK_RELEASE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize, Serialize)]
struct KilledRestore {
    path: PathBuf,
    binding: [u8; 32],
    digest: [u8; 32],
    stage: fault::Stage,
    barrier: PathBuf,
}

#[test]
fn process_kill_recovers_preparation_and_each_published_effect() {
    const CHILD: &str = "PERITUS_DISCARD_UNIT_TEST_CHILD";
    if let Some(argument) = std::env::var_os(CHILD) {
        let restore: KilledRestore = serde_json::from_str(argument.to_str().unwrap()).unwrap();
        fault::install(restore.stage, restore.barrier);
        execute(&restore.path, restore.binding, restore.digest).unwrap();
        panic!("the parent must terminate the child at the requested publication barrier");
    }
    for stage in [
        fault::Stage::Prepared,
        fault::Stage::Source,
        fault::Stage::Index,
        fault::Stage::Head,
        fault::Stage::Archive,
        fault::Stage::Completed,
    ] {
        let fixture = match stage {
            fault::Stage::Head | fault::Stage::Prepared => {
                let root = repository();
                let child = repository();
                let nested = root.path().join("nested");
                fs::rename(child.keep(), &nested).unwrap();
                let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
                fs::write(nested.join("tracked.txt"), "candidate committed\n").unwrap();
                git(&nested, &["commit", "-am", "candidate"], None).unwrap();
                Fixture::changed(root, baseline)
            }
            fault::Stage::Archive => {
                let root = repository();
                let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
                let added = repository();
                fs::rename(added.keep(), root.path().join("added")).unwrap();
                Fixture::changed(root, baseline)
            }
            _ => Fixture::new(),
        };
        let barrier = fixture.records.path().join("barrier");
        let argument = KilledRestore {
            path: fixture.path.clone(),
            binding: fixture.binding,
            digest: fixture.digest,
            stage,
            barrier: barrier.clone(),
        };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "candidate::managed::transaction::tests::crash::process_kill_recovers_preparation_and_each_published_effect", "--nocapture"])
            .env(CHILD, serde_json::to_string(&argument).unwrap())
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::inherit())
            .spawn().unwrap();
        wait_for_barrier(&mut child, stage, &barrier);
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        if stage == fault::Stage::Prepared {
            // The prepared native Git child sees EOF when its parent dies. Recovery
            // waits for Git itself to release the lock; it never deletes that lock.
            let lock = fixture.repository.path().join("nested/.git/HEAD.lock");
            let deadline = Instant::now() + LOCK_RELEASE_TIMEOUT;
            while lock.exists() {
                assert!(
                    Instant::now() < deadline,
                    "native Git did not release its own lock after EOF"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let prior = WorkspaceCheckpoint::capture(fixture.repository.path()).unwrap();
        for _ in 0..2 {
            inspect(&fixture.path, fixture.binding, fixture.digest).unwrap();
        }
        assert_eq!(WorkspaceCheckpoint::capture(fixture.repository.path()).unwrap(), prior);
        execute(&fixture.path, fixture.binding, fixture.digest)
            .unwrap_or_else(|error| panic!("{stage:?} retry: {error}"));
        assert!(matches!(
            inspect(&fixture.path, fixture.binding, fixture.digest).unwrap(),
            Some(DiscardTransactionState::Completed(_))
        ));
        let root = fixture.repository.path();
        fs::write(root.join("tracked.txt"), "later human work\n").unwrap();
        git(root, &["add", "tracked.txt"], None).unwrap();
        let before = WorkspaceCheckpoint::capture(root).unwrap();
        let index = fs::read(root.join(".git/index")).unwrap();
        execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
        assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    }
}

fn wait_for_barrier(child: &mut Child, stage: fault::Stage, barrier: &Path) {
    let started = Instant::now();
    let deadline = started + PREPARATION_TIMEOUT;
    let expected = format!("{stage:?}\n");
    while match fs::read_to_string(barrier) {
        Ok(contents) => contents != expected,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{stage:?} publication barrier could not be read: {error}");
        }
    } {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("{stage:?} child exited before barrier: {status}");
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "{stage:?} child did not reach publication barrier after {:?}",
                started.elapsed()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
