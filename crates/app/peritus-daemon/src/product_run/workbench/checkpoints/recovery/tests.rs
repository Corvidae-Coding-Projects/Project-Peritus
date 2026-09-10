use peritus_app_protocol::{
    ControlOperationId, ConversationId as PublicConversationId, WorkbenchQuery,
    WorkbenchRewindDisposition, WorkbenchRewindPath, WorkbenchRewindPreview,
    WorkbenchRewindRequest,
};
use peritus_product_runner::control::{
    CheckpointFileMode, CheckpointFileVersion, CheckpointId, CheckpointPath, CheckpointReferences,
    UserCheckpoint,
};
use peritus_types::WorkspaceId;

use super::{classify_restore_paths, public_version};

#[test]
fn postclassification_requires_unchanged_covered_paths_to_remain_at_checkpoint_bytes() {
    let workspace = WorkspaceId::new([31; 16]).expect("workspace");
    let query =
        WorkbenchQuery::new(PublicConversationId::new([32; 16]).expect("conversation"), workspace);
    let checkpoint_id = ControlOperationId::new([33; 16]).expect("checkpoint");
    let request = WorkbenchRewindRequest::new(query, 7, checkpoint_id).expect("request");
    let restore_target = version(b"restore target");
    let restore_before = version(b"restore before");
    let unchanged = version(b"unchanged");
    let drifted = version(b"third party");
    let preview = WorkbenchRewindPreview::new(
        request,
        vec![
            WorkbenchRewindPath::new(
                "a.txt".to_owned(),
                public_version(restore_target),
                Some(public_version(restore_before)),
                public_version(restore_before),
                WorkbenchRewindDisposition::Restore,
            )
            .expect("restore path"),
            WorkbenchRewindPath::new(
                "b.txt".to_owned(),
                public_version(unchanged),
                Some(public_version(unchanged)),
                public_version(unchanged),
                WorkbenchRewindDisposition::Unchanged,
            )
            .expect("unchanged path"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("preview");
    let recovery = UserCheckpoint::new(
        CheckpointId::new([34; 16]).expect("recovery checkpoint"),
        "recovery".to_owned(),
        CheckpointReferences::new(7, 1, 1, None),
        vec![
            CheckpointPath::new("a.txt".to_owned(), restore_before).expect("a"),
            CheckpointPath::new("b.txt".to_owned(), unchanged).expect("b"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("recovery checkpoint");

    assert_eq!(
        classify_restore_paths(&preview, &recovery, Some(&[restore_target, unchanged])),
        (false, true)
    );
    assert_eq!(
        classify_restore_paths(&preview, &recovery, Some(&[restore_target, drifted])),
        (false, false),
        "a covered path shown unchanged cannot drift while recovery claims all-post"
    );
}

fn version(bytes: &[u8]) -> CheckpointFileVersion {
    CheckpointFileVersion::present(
        peritus_codec::sha256(bytes),
        bytes.len() as u64,
        CheckpointFileMode::Regular,
    )
}
