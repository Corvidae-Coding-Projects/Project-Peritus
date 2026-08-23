//! Forward apply, failpoint reconciliation, and explicit backup restoration tests.

mod support;

use std::fs;

use peritus_migrations::{
    BackupPolicy, MigrationDescriptor, MigrationErrorCode, MigrationFailpoint, MigrationHooks,
    MigrationRegistry, RecoveryAction, RecoveryState,
};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use support::{config, create_database, engine, operation, version};

struct FailOnce {
    point: MigrationFailpoint,
    failed: bool,
}

impl MigrationHooks for FailOnce {
    fn should_fail(&mut self, failpoint: MigrationFailpoint) -> bool {
        if !self.failed && failpoint == self.point {
            self.failed = true;
            true
        } else {
            false
        }
    }
}

#[test]
fn successful_risky_apply_creates_backup_and_records_version() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut engine = engine(&temp);
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    assert!(plan.backup_required());
    let applied = engine.apply(&plan, operation(1)).expect("apply");
    assert_eq!(applied.from_version(), 0);
    assert_eq!(applied.target_version(), version(1));
    assert!(applied.backup_path().expect("backup path").is_file());
    assert_eq!(engine.preflight(version(1)).expect("postflight").plan().steps().len(), 0);
}

#[test]
fn applied_history_digest_drift_is_terminal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = create_database(&temp);
    let configuration = config(&temp, database.clone());
    let mut engine = peritus_migrations::MigrationEngine::open(
        configuration.clone(),
        MigrationRegistry::current(),
    )
    .expect("engine");
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    engine.apply(&plan, operation(8)).expect("apply");
    drop(engine);

    let connection = rusqlite::Connection::open(database).expect("tamper fixture");
    connection
        .execute("UPDATE schema_migrations SET source_digest = zeroblob(32)", [])
        .expect("tamper applied digest");
    drop(connection);
    let reopened =
        peritus_migrations::MigrationEngine::open(configuration, MigrationRegistry::current())
            .expect("reopen tampered history");
    assert_eq!(
        reopened.preflight(version(1)).expect_err("history digest drift").code(),
        MigrationErrorCode::DigestDrift,
    );
}

#[test]
fn pre_backup_failure_is_restart_visible_without_schema_progress() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut engine = engine(&temp);
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let mut hook = FailOnce { point: MigrationFailpoint::BeforeBackup, failed: false };
    assert_eq!(
        engine
            .apply_with_hooks(&plan, operation(2), &mut hook)
            .expect_err("backup boundary failure")
            .code(),
        MigrationErrorCode::InjectedFailure,
    );
    assert_eq!(
        engine.reconcile().expect("reconcile").actions(),
        &[RecoveryAction::ResumeBackup(operation(2))],
    );
    assert_eq!(
        engine.preflight(version(1)).expect("still version zero").plan().current_version(),
        0
    );
}

#[test]
fn completed_backup_survives_interrupted_apply_start() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut engine = engine(&temp);
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let mut hook = FailOnce { point: MigrationFailpoint::AfterBackup, failed: false };
    assert_eq!(
        engine
            .apply_with_hooks(&plan, operation(5), &mut hook)
            .expect_err("post-backup interruption")
            .code(),
        MigrationErrorCode::InjectedFailure,
    );
    drop(engine);

    let restarted = support::engine(&temp);
    assert_eq!(
        restarted.reconcile().expect("restart reconciliation").actions(),
        &[RecoveryAction::ResumeApply(operation(5))],
    );
}

#[test]
fn corrupted_durable_backup_blocks_schema_apply() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut engine = engine(&temp);
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let mut hook = FailOnce { point: MigrationFailpoint::AfterBackup, failed: false };
    engine.apply_with_hooks(&plan, operation(7), &mut hook).expect_err("stop after durable backup");
    let backup =
        temp.path().join("backups").join(format!("migration-{}-from-0.sqlite3", "07".repeat(16)));
    fs::write(backup, b"corrupt backup").expect("corrupt durable backup");

    assert_eq!(
        engine.apply(&plan, operation(7)).expect_err("corrupt backup blocks apply").code(),
        MigrationErrorCode::BackupFailed,
    );
    assert_eq!(
        engine.reconcile().expect("reconcile after backup corruption").actions(),
        &[RecoveryAction::ResumeApply(operation(7))],
    );
}

#[test]
fn abandoned_backup_temporary_is_replaced_on_retry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut engine = engine(&temp);
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let mut hook = FailOnce { point: MigrationFailpoint::BeforeBackup, failed: false };
    engine
        .apply_with_hooks(&plan, operation(6), &mut hook)
        .expect_err("interruption before backup");
    let temporary = temp
        .path()
        .join("backups")
        .join(format!("migration-{}-from-0.sqlite3.partial", "06".repeat(16)));
    fs::write(&temporary, b"abandoned partial backup").expect("write abandoned temporary");

    engine.apply(&plan, operation(6)).expect("retry replaces abandoned temporary");
    assert!(!temporary.exists());
}

#[test]
fn migration_owner_lock_rejects_concurrent_engine() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = create_database(&temp);
    let configuration = config(&temp, database);
    let first = peritus_migrations::MigrationEngine::open(
        configuration.clone(),
        MigrationRegistry::current(),
    )
    .expect("first engine");
    let concurrent = peritus_migrations::MigrationEngine::open(
        configuration.clone(),
        MigrationRegistry::current(),
    );
    assert_eq!(
        concurrent.err().expect("concurrent engine is rejected").code(),
        MigrationErrorCode::Io,
    );
    drop(first);
    peritus_migrations::MigrationEngine::open(configuration, MigrationRegistry::current())
        .expect("lock released when engine drops");
}

#[test]
fn apply_failure_requires_and_supports_explicit_backup_restore() {
    const BAD_SQL: &str = "THIS IS NOT VALID SQLITE;\n";
    let digest = Sha256Digest::new(Sha256::digest(BAD_SQL.as_bytes()).into());
    let descriptors: &'static [MigrationDescriptor] = Box::leak(
        vec![MigrationDescriptor::new(
            version(1),
            "bad-release",
            BAD_SQL,
            digest,
            BackupPolicy::Required,
            0,
        )]
        .into_boxed_slice(),
    );
    let registry = MigrationRegistry::from_static(descriptors);
    let temp = tempfile::tempdir().expect("tempdir");
    let database = create_database(&temp);
    let mut engine = peritus_migrations::MigrationEngine::open(config(&temp, database), registry)
        .expect("engine");
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    assert_eq!(
        engine.apply(&plan, operation(3)).expect_err("invalid SQL rolls back").code(),
        MigrationErrorCode::ApplyFailed,
    );
    assert_eq!(
        engine.reconcile().expect("reconcile").actions(),
        &[RecoveryAction::RestoreBackup(operation(3))],
    );
    let restored = engine.restore_backup(operation(3)).expect("explicit restore");
    assert_eq!(restored.state(), RecoveryState::Restored);
    assert_eq!(engine.preflight(version(1)).expect("restored current").plan().current_version(), 0);
}

#[test]
fn landed_ambiguous_commit_remains_applying_and_reconciles_applied_after_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = create_database(&temp);
    let config = config(&temp, database.clone());
    let mut engine =
        peritus_migrations::MigrationEngine::open(config.clone(), MigrationRegistry::current())
            .expect("engine");
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let mut hook = FailOnce { point: MigrationFailpoint::AfterCommit, failed: false };
    assert_eq!(
        engine
            .apply_with_hooks(&plan, operation(4), &mut hook)
            .expect_err("acknowledgement lost")
            .code(),
        MigrationErrorCode::Indeterminate,
    );
    drop(engine);

    let durable = rusqlite::Connection::open(&database).expect("inspect recovery state");
    let state: i64 = durable
        .query_row(
            "SELECT state FROM recovery_operations WHERE operation_id = ?1",
            [operation(4).as_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("read recovery state");
    assert_eq!(state, 3, "ambiguous commit must remain Applying for reconciliation");
    drop(durable);

    let restarted = peritus_migrations::MigrationEngine::open(config, MigrationRegistry::current())
        .expect("restart engine");
    assert_eq!(
        restarted.reconcile().expect("restart reconcile").actions(),
        &[RecoveryAction::ReconciledApplied(operation(4))],
    );
    assert_eq!(
        restarted.preflight(version(1)).expect("current after reconcile").plan().current_version(),
        1
    );
}
