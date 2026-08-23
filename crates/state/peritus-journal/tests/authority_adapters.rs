//! Durable authority-clock and credential-registry adapter coverage.

mod support;

use peritus_approval::CredentialRegistrySnapshot;
use peritus_journal::{
    CredentialRegistryInstall, ExpectedAuthorityEpoch, HeadExpectation, JournalErrorKind,
};
use peritus_types::RevisionNumber;
use tempfile::TempDir;

use support::{aggregate, command, event, open, registry_plan};

#[test]
fn registry_rejects_non_increasing_currentness() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let registry_aggregate = aggregate(peritus_journal::AggregateKind::CredentialRegistry, 50);
    let first_snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .expect("initial empty registry");
    let first_install =
        CredentialRegistryInstall::new(None, 1, &first_snapshot).expect("initial registry install");
    journal
        .append(registry_plan(
            registry_aggregate,
            HeadExpectation::Absent(registry_aggregate),
            command(50),
            event(50),
            None,
            1,
            first_install,
        ))
        .expect("install first registry");
    let current = journal.current_credential_registry().expect("current registry");
    assert_eq!((current.revision(), current.generation()), (1, 1));
    assert_eq!(current.digest(), first_snapshot.digest().expect("initial registry digest"));

    let head = journal.head(registry_aggregate).expect("registry head").expect("present");
    let second_snapshot = CredentialRegistrySnapshot::new(
        RevisionNumber::new(2).expect("second registry revision"),
        Vec::new(),
    )
    .expect("second empty registry");
    let non_increasing = CredentialRegistryInstall::new(Some(1), 1, &second_snapshot)
        .expect("structurally valid registry successor");
    assert_eq!(
        journal
            .append(registry_plan(
                registry_aggregate,
                HeadExpectation::Present(head),
                command(51),
                event(51),
                Some(event(50)),
                2,
                non_increasing,
            ))
            .expect_err("registry generation must increase")
            .kind(),
        JournalErrorKind::StaleRegistry
    );
    assert_eq!(journal.current_credential_registry().expect("unchanged").revision(), 1);

    let increasing = CredentialRegistryInstall::new(Some(1), 2, &second_snapshot)
        .expect("increasing registry successor");
    journal
        .append(registry_plan(
            registry_aggregate,
            HeadExpectation::Present(head),
            command(52),
            event(52),
            Some(event(50)),
            2,
            increasing,
        ))
        .expect("install second registry");
    let current = journal.current_credential_registry().expect("second registry");
    assert_eq!((current.revision(), current.generation()), (2, 2));
    assert_eq!(current.digest(), second_snapshot.digest().expect("second registry digest"));
}

#[test]
fn authority_clock_rejects_stale_allocation_and_survives_restart() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let first_epoch =
        journal.allocate_authority_epoch(ExpectedAuthorityEpoch::Absent).expect("first epoch");
    assert_eq!(first_epoch.get(), 1);
    assert_eq!(
        journal
            .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
            .expect_err("stale absent authority expectation")
            .kind(),
        JournalErrorKind::StaleAuthorityEpoch
    );
    let second_epoch = journal
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Current(first_epoch.epoch()))
        .expect("second authority epoch");
    assert_eq!(second_epoch.get(), 2);
    drop(journal);
    assert_eq!(
        open(&temp)
            .current_authority_epoch()
            .expect("authority epoch after restart")
            .expect("epoch present")
            .get(),
        2
    );
}
