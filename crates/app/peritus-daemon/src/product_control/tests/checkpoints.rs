//! Automatic before-images remain durable without consuming the bounded user projection.

use super::*;
use peritus_product_runner::control::{
    CheckpointFileMode, CheckpointFileVersion, CheckpointId, CheckpointPath, CheckpointReferences,
    UserCheckpoint,
};

const LEGACY_PROJECTION_SIZE: usize = 64;
#[path = "checkpoints/snapshots.rs"]
mod snapshots;

#[path = "checkpoints/manifest_pages.rs"]
mod manifest_pages;

#[test]
fn full_legacy_automatic_projection_accepts_new_paths_and_a_user_checkpoint_after_restart() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create conversation");
    let run = [9; 16];

    for index in 0..LEGACY_PROJECTION_SIZE {
        let index = u32::try_from(index).unwrap() + 10;
        let revision = journal.load(create().conversation()).unwrap().unwrap().revision();
        let checkpoint = automatic(index, revision, run);
        journal
            .accept_checkpoint(
                &operation(index, revision, ControlIntent::CreateCheckpoint(checkpoint)),
                &[None],
            )
            .expect("legacy automatic checkpoint");
    }
    assert_eq!(
        journal.load(create().conversation()).unwrap().unwrap().checkpoints().len(),
        LEGACY_PROJECTION_SIZE
    );

    let mut next = None;
    for offset in 0..=LEGACY_PROJECTION_SIZE {
        let index = 100 + u32::try_from(offset).unwrap();
        let revision = journal.load(create().conversation()).unwrap().unwrap().revision();
        let checkpoint = automatic(index, revision, run);
        journal
            .accept_checkpoint(
                &operation(
                    index,
                    revision,
                    ControlIntent::CreateAutomaticCheckpoint(checkpoint.clone()),
                ),
                &[None],
            )
            .expect("new replay-only automatic checkpoint");
        next = Some(checkpoint);
    }
    let next = next.unwrap();
    assert_eq!(
        journal.load(create().conversation()).unwrap().unwrap().checkpoints().len(),
        LEGACY_PROJECTION_SIZE,
        "more than one projection's worth of new automatic paths stays outside the root"
    );
    let postimage =
        CheckpointFileVersion::present(sha256(b"owned"), 5, CheckpointFileMode::Regular);
    let revision = journal.load(create().conversation()).unwrap().unwrap().revision();
    assert!(matches!(
        journal.accept(&operation(
            899,
            revision,
            ControlIntent::SealAutomaticCheckpoint {
                checkpoint: next.id(),
                run: [8; 16],
                versions: vec![(next.paths()[0].path().to_owned(), postimage)],
            },
        )),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
    assert_eq!(
        journal.load(create().conversation()).unwrap().unwrap().revision(),
        revision,
        "a foreign run cannot seal or advance an automatic checkpoint"
    );
    journal
        .accept(&operation(
            900,
            revision,
            ControlIntent::SealAutomaticCheckpoint {
                checkpoint: next.id(),
                run,
                versions: vec![(next.paths()[0].path().to_owned(), postimage)],
            },
        ))
        .expect("seal replay-only checkpoint");

    let revision = journal.load(create().conversation()).unwrap().unwrap().revision();
    let user = UserCheckpoint::new(
        CheckpointId::new(
            *operation(901, revision, ControlIntent::PinConversation { pinned: true })
                .id()
                .as_bytes(),
        )
        .unwrap(),
        "user checkpoint after legacy capacity".to_owned(),
        CheckpointReferences::new(revision, 0, 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    journal
        .accept_checkpoint(
            &operation(901, revision, ControlIntent::CreateCheckpoint(user.clone())),
            &[],
        )
        .expect("legacy automatic entries do not consume user allowance");
    drop(journal);

    let journal = store(root.path());
    let record = journal.load(create().conversation()).unwrap().unwrap();
    assert_eq!(record.checkpoints().len(), LEGACY_PROJECTION_SIZE + 1);
    assert_eq!(record.checkpoints().last(), Some(&user));
    let recovered = journal
        .load_checkpoint(create().conversation(), next.id())
        .unwrap()
        .expect("automatic checkpoint replayed after restart");
    assert_eq!(recovered.paths()[0].checkpoint(), CheckpointFileVersion::Absent);
    assert_eq!(recovered.paths()[0].owned_postchange(), Some(postimage));
}

fn automatic(index: u32, revision: u64, run: [u8; 16]) -> UserCheckpoint {
    let id = CheckpointId::new(
        *operation(index, revision, ControlIntent::PinConversation { pinned: true })
            .id()
            .as_bytes(),
    )
    .unwrap();
    UserCheckpoint::automatic(
        id,
        "Automatic checkpoint before owned mutation".to_owned(),
        CheckpointReferences::new(revision, 0, 0, None),
        vec![
            CheckpointPath::new(format!("path-{index}.txt"), CheckpointFileVersion::Absent)
                .unwrap(),
        ],
        Vec::new(),
        Vec::new(),
        run,
    )
    .unwrap()
}
