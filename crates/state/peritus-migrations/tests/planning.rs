//! Immutable registry and deterministic planning tests.

mod support;

use peritus_migrations::{
    ApplicationCompatibility, BackupPolicy, MigrationDescriptor, MigrationErrorCode, MigrationPlan,
    MigrationRegistry,
};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use support::version;

#[test]
fn production_registry_is_contiguous_through_application_schema_v10() {
    let registry = MigrationRegistry::current();
    registry.validate().expect("production registry");
    assert_eq!(registry.descriptors().len(), 10);
    assert_eq!(registry.latest().expect("latest migration"), version(10));

    let application = registry.descriptors().last().expect("application migration");
    assert_eq!(application.version(), version(10));
    assert_eq!(application.backup_policy(), BackupPolicy::Required);
    for object in [
        "CREATE TABLE app_principals",
        "CREATE TABLE app_sessions",
        "CREATE TABLE app_commands",
        "CREATE INDEX app_commands_state",
        "CREATE TABLE app_prompt_targets",
        "CREATE INDEX app_prompt_targets_state",
        "CREATE TABLE app_artifacts",
        "CREATE TABLE app_workspaces",
    ] {
        assert!(application.sql().contains(object), "missing application object: {object}");
    }
}

#[test]
fn source_digest_drift_and_registry_gaps_are_rejected() {
    let drifted: &'static [MigrationDescriptor] = Box::leak(
        vec![MigrationDescriptor::new(
            version(1),
            "test",
            "PRAGMA user_version = 1;\n",
            Sha256Digest::new([0; 32]),
            BackupPolicy::Required,
            0,
        )]
        .into_boxed_slice(),
    );
    assert_eq!(
        MigrationRegistry::from_static(drifted).validate().expect_err("digest drift").code(),
        MigrationErrorCode::DigestDrift,
    );

    let production_first = MigrationRegistry::current().descriptors()[0];
    let gap: &'static [MigrationDescriptor] = Box::leak(
        vec![
            production_first,
            MigrationDescriptor::new(
                version(3),
                "test",
                "PRAGMA user_version = 3;\n",
                Sha256Digest::new([0; 32]),
                BackupPolicy::NotRequired,
                0,
            ),
        ]
        .into_boxed_slice(),
    );
    assert_eq!(
        MigrationRegistry::from_static(gap).validate().expect_err("gap").code(),
        MigrationErrorCode::InvalidRegistry,
    );
}

#[test]
fn planning_rejects_insufficient_space_and_reverse_or_incompatible_ranges() {
    let compatibility = ApplicationCompatibility::new(0, version(1)).expect("compatibility");
    let error = MigrationPlan::from_observation(
        MigrationRegistry::current(),
        0,
        version(1),
        compatibility,
        1_000,
        1_000,
        1,
    )
    .expect_err("backup plus scratch exceeds observation");
    assert_eq!(error.code(), MigrationErrorCode::InsufficientSpace);

    assert_eq!(
        MigrationPlan::from_observation(
            MigrationRegistry::current(),
            0,
            version(1),
            ApplicationCompatibility::new(1, version(1)).expect("compatibility"),
            0,
            u64::MAX,
            0,
        )
        .expect_err("current version below application minimum")
        .code(),
        MigrationErrorCode::IncompatibleApplication,
    );

    assert_eq!(
        MigrationPlan::from_observation(
            MigrationRegistry::current(),
            1,
            version(1),
            ApplicationCompatibility::new(0, version(1)).expect("compatibility"),
            0,
            u64::MAX,
            0,
        )
        .expect("no-op target")
        .steps()
        .len(),
        0,
    );
}

#[test]
fn planning_rejects_reverse_ranges_and_checked_capacity_overflow() {
    const SECOND_SQL: &str = "PRAGMA user_version = 2;\n";
    const OVERFLOW_SQL: &str = "PRAGMA user_version = 1;\n";
    let second_digest = Sha256Digest::new(Sha256::digest(SECOND_SQL.as_bytes()).into());
    let two_versions: &'static [MigrationDescriptor] = Box::leak(
        vec![
            MigrationRegistry::current().descriptors()[0],
            MigrationDescriptor::new(
                version(2),
                "test",
                SECOND_SQL,
                second_digest,
                BackupPolicy::NotRequired,
                0,
            ),
        ]
        .into_boxed_slice(),
    );
    let registry = MigrationRegistry::from_static(two_versions);
    assert_eq!(
        MigrationPlan::from_observation(
            registry,
            2,
            version(1),
            ApplicationCompatibility::new(0, version(2)).expect("compatibility"),
            0,
            u64::MAX,
            0,
        )
        .expect_err("reverse target")
        .code(),
        MigrationErrorCode::ForwardOnly,
    );

    let overflow_digest = Sha256Digest::new(Sha256::digest(OVERFLOW_SQL.as_bytes()).into());
    let overflowing: &'static [MigrationDescriptor] = Box::leak(
        vec![MigrationDescriptor::new(
            version(1),
            "test",
            OVERFLOW_SQL,
            overflow_digest,
            BackupPolicy::NotRequired,
            u64::MAX,
        )]
        .into_boxed_slice(),
    );
    assert_eq!(
        MigrationPlan::from_observation(
            MigrationRegistry::from_static(overflowing),
            0,
            version(1),
            ApplicationCompatibility::new(0, version(1)).expect("compatibility"),
            0,
            u64::MAX,
            1,
        )
        .expect_err("checked capacity overflow")
        .code(),
        MigrationErrorCode::InvalidRegistry,
    );
}
