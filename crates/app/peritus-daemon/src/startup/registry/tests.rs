use std::{fs, path::Path};

use peritus_approval::CredentialRegistrySnapshot;
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::RevisionNumber;
use tempfile::TempDir;

use crate::{ApprovalRegistryDeclaration, DaemonErrorCode};

use super::bootstrap;

#[test]
fn fresh_store_installs_configured_registry() {
    let temporary = TempDir::new().expect("temporary registry store");
    let declaration = declaration(temporary.path(), 1, 1);
    let mut journal = open(&temporary);

    bootstrap(&mut journal, &declaration).expect("fresh registry bootstrap");

    let current = journal.current_credential_registry().expect("installed current registry");
    assert_eq!((current.revision(), current.generation()), (1, 1));
    assert_eq!(event_count(&journal), 1);
}

#[test]
fn exact_restart_is_idempotent() {
    let temporary = TempDir::new().expect("temporary registry store");
    let declaration = declaration(temporary.path(), 1, 1);
    let mut journal = open(&temporary);
    bootstrap(&mut journal, &declaration).expect("first registry bootstrap");
    drop(journal);

    let mut reopened = open(&temporary);
    bootstrap(&mut reopened, &declaration).expect("exact restart bootstrap");

    let current = reopened.current_credential_registry().expect("restarted current registry");
    assert_eq!((current.revision(), current.generation()), (1, 1));
    assert_eq!(event_count(&reopened), 1);
}

#[test]
fn exact_successor_advances_registry() {
    let temporary = TempDir::new().expect("temporary registry store");
    let first = declaration(temporary.path(), 1, 1);
    let mut journal = open(&temporary);
    bootstrap(&mut journal, &first).expect("first registry bootstrap");
    let successor = declaration(temporary.path(), 2, 2);

    bootstrap(&mut journal, &successor).expect("successor registry bootstrap");

    let current = journal.current_credential_registry().expect("successor current registry");
    assert_eq!((current.revision(), current.generation()), (2, 2));
    assert_eq!(event_count(&journal), 2);
}

#[test]
fn non_successor_configuration_is_rejected_as_drift() {
    let temporary = TempDir::new().expect("temporary registry store");
    let first = declaration(temporary.path(), 1, 1);
    let mut journal = open(&temporary);
    bootstrap(&mut journal, &first).expect("first registry bootstrap");
    let drift = declaration(temporary.path(), 1, 2);

    let error = bootstrap(&mut journal, &drift).expect_err("same revision generation drift");

    assert_eq!(error.code_kind(), DaemonErrorCode::RecoveryRequired);
    let current = journal.current_credential_registry().expect("unchanged current registry");
    assert_eq!((current.revision(), current.generation()), (1, 1));
    assert_eq!(event_count(&journal), 1);
}

fn declaration(root: &Path, revision: u64, generation: u64) -> ApprovalRegistryDeclaration {
    let revision = RevisionNumber::new(revision).expect("positive test revision");
    let snapshot = CredentialRegistrySnapshot::new(revision, Vec::new()).expect("test registry");
    let path = root.join(format!("approval-registry-{}.bin", revision.get()));
    fs::write(&path, snapshot.canonical_bytes().expect("canonical test registry"))
        .expect("write test registry");
    ApprovalRegistryDeclaration::new(path, generation).expect("test registry declaration")
}

fn open(temporary: &TempDir) -> SqliteJournal {
    SqliteJournal::open(
        temporary.path().join("journal.sqlite3"),
        StoreId::new([0x41; 16]).expect("test store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("open test journal")
}

fn event_count(journal: &SqliteJournal) -> usize {
    journal.global_events_after(0, 16).expect("registry event window").records().len()
}
