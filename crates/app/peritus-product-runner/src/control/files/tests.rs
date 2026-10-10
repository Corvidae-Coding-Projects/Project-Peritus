use super::*;
use crate::{
    attachment::ValidatedFileText,
    control::{ControlIntent, ControlOperation, ConversationId, ConversationRecord, QueueIntent},
};
use peritus_patch::WorkspacePath;
use peritus_types::{ActorId, ArtifactId, WorkspaceId};

fn operation(index: u8, revision: u64, intent: ControlIntent) -> ControlOperation {
    ControlOperation::new(
        OperationId::new([index; 16]).expect("operation"),
        ConversationId::new([2; 16]).expect("conversation"),
        ActorId::new([3; 16]).expect("actor"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        revision,
        intent,
    )
}
fn version(index: u8, text: &str) -> FileVersion {
    let bytes = ValidatedFileText::new(text.as_bytes().to_vec()).expect("text");
    FileVersion::new(
        OperationId::new([index; 16]).expect("operation"),
        ArtifactId::new([index; 16]).expect("artifact"),
        FileObservation::new(bytes.digest(), bytes.bytes(), (0, bytes.bytes()), bytes.digest())
            .expect("observation"),
        peritus_codec::sha256(b"exact proof"),
    )
    .expect("version")
}
fn attach(mode: FileMode) -> (ConversationRecord, FileAttachment) {
    let record = ConversationRecord::apply(
        None,
        &operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Files".to_owned()).expect("title"),
            },
        ),
    )
    .expect("create")
    .0;
    let bytes = record.canonical_bytes().expect("legacy bytes");
    assert!(!String::from_utf8(bytes.clone()).expect("UTF-8").contains("\"files\""));
    assert_eq!(ConversationRecord::parse(&bytes).expect("parse"), record);
    let source = FileSource::workspace(
        peritus_codec::sha256(b"folder"),
        &WorkspacePath::new("src/main.rs").expect("path"),
        FileRange::All,
        mode,
    )
    .expect("source");
    let file = FileAttachment::new(source, version(2, "first\r\n")).expect("file");
    let record = ConversationRecord::apply(
        Some(&record),
        &operation(
            2,
            1,
            ControlIntent::AttachFile {
                file: file.clone(),
                text: ControlText::new("Use this file".to_owned()).expect("caption"),
            },
        ),
    )
    .expect("attach")
    .0;
    (record, file)
}

#[test]
fn refresh_preserves_original_version_and_fences_previous_context_generation() {
    let (record, file) = attach(FileMode::RefreshOnRequest);
    let generation = record.inputs().generation();
    let refreshed = ConversationRecord::apply(
        Some(&record),
        &operation(
            3,
            2,
            ControlIntent::RefreshFile {
                attachment: file.operation(),
                previous: file.operation(),
                version: version(3, "second\n"),
            },
        ),
    )
    .expect("refresh")
    .0;
    assert!(refreshed.inputs().generation() > generation);
    let selected = &refreshed.files().entries()[0];
    assert_eq!(selected.file(), &file);
    assert_eq!(selected.current(), &version(3, "second\n"));
    assert_eq!(selected.refreshes().len(), 1);
    assert_eq!(
        ConversationRecord::parse(&refreshed.canonical_bytes().expect("encode")).expect("reopen"),
        refreshed
    );
    assert_eq!(
        ConversationRecord::apply(
            Some(&refreshed),
            &operation(
                4,
                3,
                ControlIntent::RefreshFile {
                    attachment: file.operation(),
                    previous: file.operation(),
                    version: version(4, "third"),
                }
            )
        ),
        Err(ControlError::StaleRevision)
    );
}

#[test]
fn historical_files_retain_bytes_and_references_without_reusing_refresh_consent() {
    let (record, file) = attach(FileMode::RefreshOnRequest);
    let mut frozen = record.files().historical_snapshot();
    assert_eq!(frozen.entries()[0].mode(), FileMode::Snapshot);
    assert_eq!(frozen.entries()[0].file(), &file);
    assert_eq!(frozen.entries()[0].current(), record.files().entries()[0].current());
    frozen.validate(record.inputs()).unwrap();
    let mut inputs = record.inputs().clone();
    let result =
        frozen.refresh(&mut inputs, file.operation(), file.operation(), &version(3, "changed"));
    assert_eq!(result, Err(ControlError::InvalidInput));
    assert_eq!(inputs, *record.inputs());
    assert_eq!(record.files().entries()[0].mode(), FileMode::RefreshOnRequest);
}

#[test]
fn snapshot_or_held_references_cannot_refresh_and_deselection_retains_history() {
    let (snapshot, file) = attach(FileMode::Snapshot);
    assert_eq!(
        ConversationRecord::apply(
            Some(&snapshot),
            &operation(
                3,
                2,
                ControlIntent::RefreshFile {
                    attachment: file.operation(),
                    previous: file.operation(),
                    version: version(3, "new"),
                }
            )
        ),
        Err(ControlError::InvalidInput)
    );
    let (record, file) = attach(FileMode::RefreshOnRequest);
    let input = record.inputs().capture().expect("capture").included()[0];
    let held = ConversationRecord::apply(
        Some(&record),
        &operation(3, 2, ControlIntent::Queue(QueueIntent::Hold { selected: input, held: true })),
    )
    .expect("hold")
    .0;
    assert!(held.files().eligible(held.inputs().capture().expect("capture").included()).is_empty());
    assert_eq!(
        ConversationRecord::apply(
            Some(&held),
            &operation(
                4,
                3,
                ControlIntent::RefreshFile {
                    attachment: file.operation(),
                    previous: file.operation(),
                    version: version(4, "new"),
                }
            )
        ),
        Err(ControlError::InvalidInput)
    );
    let deselected = ConversationRecord::apply(
        Some(&record),
        &operation(
            5,
            2,
            ControlIntent::SelectFile { attachment: file.operation(), selected: false },
        ),
    )
    .expect("deselect")
    .0;
    assert!(!deselected.files().entries()[0].selected());
    assert_eq!(deselected.files().entries()[0].file(), &file);
}

#[test]
fn text_is_exact_and_invalid_ranges_binary_or_rebound_sources_reject() {
    let text = ValidatedFileText::new("α\r\n\t".as_bytes().to_vec()).expect("text");
    assert_eq!(text.text(), "α\r\n\t");
    assert!(!format!("{text:?}").contains('α'));
    assert!(ValidatedFileText::new(vec![0xce]).is_err());
    assert!(ValidatedFileText::new(b"a\0b".to_vec()).is_err());
    assert!(ValidatedFileText::new(Vec::new()).is_ok());
    assert!(FileRange::Bytes { start: 1, end: 1 }.validate().is_err());
    assert!(FileRange::Lines { first: 0, last: 1 }.validate().is_err());
    let (record, file) = attach(FileMode::Snapshot);
    let bytes = record.canonical_bytes().expect("encode");
    let json = String::from_utf8(bytes).expect("UTF-8").replace("src/main.rs", "../outside");
    assert!(ConversationRecord::parse(json.as_bytes()).is_err());
    assert_eq!(
        ConversationRecord::apply(
            Some(&record),
            &operation(
                3,
                2,
                ControlIntent::AttachFile {
                    file,
                    text: ControlText::new("rebind".to_owned()).expect("caption"),
                }
            )
        ),
        Err(ControlError::InvalidInput)
    );
}

#[test]
fn pinned_and_excluded_file_preference_use_the_same_effective_projection() {
    let (mut record, file) = attach(FileMode::Snapshot);
    record = ConversationRecord::apply(
        Some(&record),
        &operation(
            3,
            record.revision(),
            ControlIntent::SelectFile { attachment: file.operation(), selected: false },
        ),
    )
    .expect("deselect")
    .0;
    let included = record.inputs().capture().expect("capture").included().to_vec();
    assert!(record.eligible_files(&included).is_empty());
    record = ConversationRecord::apply(
        Some(&record),
        &operation(
            4,
            record.revision(),
            ControlIntent::SetContext {
                target: crate::control::ContextTarget::File(file.operation()),
                preference: Some(crate::control::ContextPreference::Pinned),
            },
        ),
    )
    .expect("pin")
    .0;
    let included = record.inputs().capture().expect("capture").included().to_vec();
    assert_eq!(record.eligible_files(&included).len(), 1);
    record = ConversationRecord::apply(
        Some(&record),
        &operation(
            5,
            record.revision(),
            ControlIntent::SetContext {
                target: crate::control::ContextTarget::File(file.operation()),
                preference: Some(crate::control::ContextPreference::Excluded),
            },
        ),
    )
    .expect("exclude")
    .0;
    let included = record.inputs().capture().expect("capture").included().to_vec();
    assert!(record.eligible_files(&included).is_empty());
}

#[test]
fn more_than_thirty_two_file_references_remain_valid_and_can_be_released() {
    let mut record = attach(FileMode::Snapshot).0;
    for index in 3..=35 {
        let source = FileSource::imported(
            ControlText::new(format!("file-{index}.txt")).expect("label"),
            FileRange::All,
        )
        .expect("source");
        let file = FileAttachment::new(source, version(index, "x")).expect("file");
        record = ConversationRecord::apply(
            Some(&record),
            &operation(
                index,
                record.revision(),
                ControlIntent::AttachFile {
                    file,
                    text: ControlText::new(format!("caption-{index}")).expect("caption"),
                },
            ),
        )
        .expect("attachment count has no host quota")
        .0;
    }
    assert_eq!(record.files().entries().len(), 34);
    let selection = record.inputs().capture().expect("capture").included()[0];
    record = ConversationRecord::apply(
        Some(&record),
        &operation(
            36,
            record.revision(),
            ControlIntent::Queue(QueueIntent::Hold { selected: selection, held: true }),
        ),
    )
    .expect("hold")
    .0;
    let (released, _) = ConversationRecord::apply(
        Some(&record),
        &operation(
            37,
            record.revision(),
            ControlIntent::Queue(QueueIntent::Hold { selected: selection, held: false }),
        ),
    )
    .expect("release is independent of attachment history length");
    assert_eq!(
        released.eligible_files(released.inputs().capture().expect("capture").included()).len(),
        34
    );
}
