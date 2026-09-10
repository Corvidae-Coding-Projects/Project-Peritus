use super::*;
use peritus_product_runner::control::{ControlIntent, ControlText, OperationId};
use peritus_types::{ActorId, WorkspaceId};

#[path = "tests/context.rs"]
mod context;
#[path = "tests/files.rs"]
mod files;
#[path = "tests/goal.rs"]
mod goal;
#[path = "tests/images.rs"]
mod images;
#[path = "tests/reservations.rs"]
mod reservations;

fn store(root: &Path) -> ControlStore {
    ControlStore::open(root, StoreId::new([1; 16]).expect("store")).expect("open")
}
fn operation(index: u32, revision: u64, intent: ControlIntent) -> ControlOperation {
    let mut identity = [1; 16];
    identity[..4].copy_from_slice(&index.to_be_bytes());
    ControlOperation::new(
        OperationId::new(identity).expect("operation"),
        ConversationId::new([2; 16]).expect("conversation"),
        ActorId::new([3; 16]).expect("actor"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        revision,
        intent,
    )
}
fn create() -> ControlOperation {
    operation(
        1,
        0,
        ControlIntent::CreateConversation {
            title: ControlText::new("Original title".to_owned()).expect("title"),
        },
    )
}

#[test]
fn exact_receipt_survives_later_mutation_and_reopen_without_reapplying_old_state() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    let create = create();
    let receipt = journal.accept(&create).expect("create");
    let rename = operation(
        2,
        1,
        ControlIntent::RenameConversation {
            title: ControlText::new("Renamed".to_owned()).expect("title"),
        },
    );
    journal.accept(&rename).expect("rename");
    assert_eq!(journal.accept(&create).expect("replay create"), receipt);
    assert_eq!(
        journal.load(create.conversation()).expect("load").expect("record").title(),
        "Renamed"
    );
    drop(journal);
    let mut journal = store(root.path());
    assert_eq!(journal.accept(&create).expect("restart replay"), receipt);
    let current = journal.load(create.conversation()).expect("replay root").expect("record");
    assert_eq!(current.revision(), 2);
    assert_eq!(current.title(), "Renamed");
}

#[test]
fn stale_revision_and_changed_payload_reject_without_receipt_or_state_change() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    let create = create();
    journal.accept(&create).expect("create");
    let stale = operation(2, 0, ControlIntent::PinConversation { pinned: true });
    assert!(matches!(journal.accept(&stale), Err(Error::Control(ControlError::StaleRevision))));
    assert!(journal.resolve(&stale).expect("absent").is_none());
    let reused = operation(1, 1, ControlIntent::PinConversation { pinned: true });
    assert!(matches!(
        journal.accept(&reused),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
    let record = journal.load(create.conversation()).expect("load").expect("record");
    assert_eq!(record.revision(), 1);
    assert!(!record.pinned());
}

#[test]
fn control_store_has_one_owner_and_rejects_wrong_store_identity_after_release() {
    let root = tempfile::tempdir().expect("root");
    let first = store(root.path());
    assert!(ControlStore::open(root.path(), StoreId::new([1; 16]).expect("store")).is_err());
    drop(first);
    assert!(ControlStore::open(root.path(), StoreId::new([5; 16]).expect("wrong store")).is_err());
}

#[test]
#[allow(
    clippy::used_underscore_binding,
    reason = "the regression duplicates the otherwise unread RAII ownership guard to model descriptor inheritance"
)]
fn closing_the_store_releases_ownership_even_while_a_duplicated_handle_exists() {
    let root = tempfile::tempdir().expect("root");
    let journal = store(root.path());
    // A concurrent subprocess fork can temporarily inherit this open file description,
    // even though close-on-exec prevents it from surviving the eventual exec.
    let inherited = journal._owner.0.try_clone().expect("duplicate owner handle");
    drop(journal);
    let reopened = store(root.path());
    assert!(reopened.load(create().conversation()).expect("empty root").is_none());
    drop(inherited);
}

#[test]
fn sqlite_full_never_acknowledges_or_publishes_a_partial_control_transition() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    let create = create();
    journal.accept(&create).expect("create");
    let pages = journal.journal.storage_pages().expect("pages");
    journal.journal.limit_storage_pages(pages.page_count()).expect("freeze page capacity");
    let mut last = journal.load(create.conversation()).expect("load").expect("record");
    let mut exhausted = false;
    for index in 2..=512 {
        let change = operation(
            index,
            last.revision(),
            ControlIntent::RenameConversation {
                title: ControlText::new(format!("title {index}: {}", "x".repeat(230)))
                    .expect("title"),
            },
        );
        if journal.accept(&change).is_err() {
            assert!(journal.resolve(&change).expect("no false receipt").is_none());
            assert_eq!(journal.load(create.conversation()).expect("prior root intact"), Some(last));
            exhausted = true;
            break;
        }
        last = journal.load(create.conversation()).expect("load").expect("record");
    }
    assert!(exhausted, "the bounded fixture must reach SQLite's actual full condition");
}

#[test]
fn withdrawal_and_incorporation_cas_have_one_winner_and_replay_preserves_exact_binding() {
    use peritus_product_runner::control::{
        InputId, InputSelection, InputState, InvocationId, QueueIntent,
    };
    for withdrawal_first in [false, true] {
        let root = tempfile::tempdir().expect("root");
        let mut journal = store(root.path());
        journal.accept(&create()).expect("create");
        let id = InputId::new([10; 16]).expect("input");
        let selected = InputSelection::new(id, 1).expect("selection");
        journal
            .accept(&operation(
                2,
                1,
                ControlIntent::Queue(QueueIntent::Enqueue {
                    id,
                    text: ControlText::new("exact user steering".to_owned()).expect("text"),
                    dependencies: Vec::new(),
                }),
            ))
            .expect("enqueue");
        let withdraw = operation(3, 2, ControlIntent::Queue(QueueIntent::Withdraw(selected)));
        let captured = journal
            .capture_inputs(
                create().conversation(),
                ActorId::new([3; 16]).expect("actor"),
                WorkspaceId::new([4; 16]).expect("workspace"),
            )
            .expect("capture");
        let request =
            crate::product_control::inputs::tests::request(captured.inputs().conversation());
        let invocation = InvocationId::new([11; 16]).expect("invocation");
        let incorporate = operation(
            4,
            2,
            ControlIntent::Queue(QueueIntent::Incorporate {
                invocation,
                request_digest: request.fingerprint().expect("digest").digest().into_bytes(),
                manifest_digest: crate::product_control::inputs::RequestArchive::new(
                    &captured, invocation, &request,
                )
                .expect("archive")
                .manifest_digest()
                .expect("manifest digest"),
                items: vec![selected],
            }),
        );
        let (winner, loser) =
            if withdrawal_first { (&withdraw, &incorporate) } else { (&incorporate, &withdraw) };
        let archive = || {
            crate::product_control::inputs::RequestArchive::new(&captured, invocation, &request)
                .expect("archive")
        };
        assert!(matches!(
            journal.accept(&incorporate),
            Err(Error::Control(ControlError::InvalidInput))
        ));
        let original_receipt =
            journal.accept_archived(winner, (!withdrawal_first).then(archive)).expect("winner");
        assert!(matches!(
            journal.accept_archived(loser, withdrawal_first.then(archive)),
            Err(Error::Control(ControlError::StaleRevision))
        ));
        assert!(journal.resolve(loser).expect("no false receipt").is_none());
        drop(journal);
        let mut journal = store(root.path());
        assert_eq!(journal.accept(winner).expect("original receipt"), original_receipt);
        let record = journal.load(create().conversation()).expect("load").expect("record");
        assert_eq!(record.revision(), 3);
        let item = record.inputs().latest(id).expect("input");
        assert_eq!(
            item.state(),
            if withdrawal_first { InputState::Withdrawn } else { InputState::Incorporated }
        );
        assert_eq!(record.inputs().invocations().len(), usize::from(!withdrawal_first));
        if !withdrawal_first {
            assert_eq!(record.inputs().invocations()[0].items(), &[selected]);
            assert_eq!(
                journal.accepted_request(&incorporate).expect("archive"),
                Some(request.canonical_bytes().expect("canonical"))
            );
        }
    }
}

#[test]
fn request_artifact_disk_full_leaves_inputs_pending_and_publishes_no_archive() {
    use peritus_product_runner::control::{InputId, InvocationId, QueueIntent};
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    journal
        .accept(&operation(
            2,
            1,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([10; 16]).expect("input"),
                text: ControlText::new("governing input".to_owned()).expect("text"),
                dependencies: Vec::new(),
            }),
        ))
        .expect("enqueue");
    let captured = journal
        .capture_inputs(
            create().conversation(),
            ActorId::new([3; 16]).expect("actor"),
            WorkspaceId::new([4; 16]).expect("workspace"),
        )
        .expect("capture");
    let pages = journal.journal.storage_pages().expect("pages");
    journal.journal.limit_storage_pages(pages.page_count()).expect("freeze disk capacity");
    let request = crate::product_control::inputs::tests::request(&format!(
        "{}\n{}",
        captured.inputs().conversation(),
        "x".repeat(128 * 1024)
    ));
    assert!(
        journal
            .prepare_inputs(&captured, InvocationId::new([11; 16]).expect("invocation"), &request)
            .is_err()
    );
    let record = journal.load(create().conversation()).expect("load").expect("record");
    assert_eq!(record.revision(), 2);
    assert!(record.inputs().invocations().is_empty());
    assert_eq!(record.inputs().capture().expect("capture").pending().len(), 1);
    let connection =
        rusqlite::Connection::open(root.path().join("control.sqlite3")).expect("inspection");
    let archives: i64 = connection
        .query_row(
            "SELECT count(*) FROM state_records WHERE namespace IN (3404, 3405)",
            [],
            |row| row.get(0),
        )
        .expect("archive count");
    assert_eq!(archives, 0, "both request artifacts roll back with incorporation");
}
