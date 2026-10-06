use super::*;
use crate::{
    attachment::ValidatedFileText,
    control::{ControlIntent, ControlOperation, ConversationId, ConversationRecord, QueueIntent},
};
use peritus_patch::WorkspacePath;
use peritus_types::{ActorId, ArtifactId, WorkspaceId};

#[test]
fn legacy_file_source_and_conversation_keep_pre108_canonical_bytes() {
    let (record, file) = attach(FileMode::Snapshot);
    assert_eq!(serde_json::to_vec(file.source()).unwrap(), br#"{"origin":{"workspace":{"folder":[3,74,0,98,72,130,86,241,76,139,20,208,37,236,57,136,177,72,147,143,127,157,185,76,160,172,115,244,179,97,77,138],"path":"src/main.rs"}},"range":"all","mode":"snapshot"}"#);
    assert_eq!(
        peritus_codec::sha256(&record.canonical_bytes().unwrap()).into_bytes(),
        [
            202, 27, 247, 112, 144, 245, 167, 187, 99, 173, 129, 189, 81, 131, 211, 75, 253, 215,
            136, 77, 175, 86, 126, 96, 197, 26, 63, 208, 205, 197, 7, 108
        ]
    );
}

#[test]
fn extended_file_source_survives_exact_record_reopen_and_rejects_reinterpretation() {
    let (record, _) = attach(FileMode::Snapshot);
    let path = WorkspacePath::new(vec!["directory-component"; 257].join("/")).unwrap();
    assert!(path.as_str().len() > 4_096);
    let source = FileSource::workspace(
        peritus_codec::sha256(b"folder"),
        &path,
        FileRange::All,
        FileMode::Snapshot,
    )
    .unwrap();
    assert_eq!(source.path(), Some(path.as_str()));
    assert!(!format!("{source:?}").contains("directory-component"));
    let wire = serde_json::to_string(&source).unwrap();
    assert!(wire.contains("native_workspace"));
    assert_eq!(serde_json::from_str::<FileSource>(&wire).unwrap(), source);
    assert!(
        serde_json::from_str::<FileSource>(&wire.replace("native_workspace", "workspace")).is_err()
    );
    let mut foreign: serde_json::Value = serde_json::from_str(&wire).unwrap();
    foreign["origin"]["native_workspace"]["platform"] = serde_json::json!(255);
    assert!(serde_json::from_value::<FileSource>(foreign).is_err());
    let file = FileAttachment::new(source, version(3, "retained")).unwrap();
    let change = operation(
        3,
        2,
        ControlIntent::AttachFile {
            file,
            text: ControlText::new("extended path".to_owned()).unwrap(),
        },
    );
    let predecessor = record.canonical_bytes().unwrap();
    let (next, _) = ConversationRecord::apply(Some(&record), &change).unwrap();
    assert_eq!(record.canonical_bytes().unwrap(), predecessor);
    let bytes = next.canonical_bytes().unwrap();
    let reopened = ConversationRecord::parse(&bytes).unwrap();
    assert_eq!(reopened, next);
    assert_eq!(reopened.canonical_bytes().unwrap(), bytes);
    assert_eq!(reopened.files().entries()[1].file().source().path(), Some(path.as_str()));
    assert_eq!(
        ConversationRecord::apply(Some(&reopened), &change),
        Err(ControlError::StaleRevision)
    );
}

#[cfg(unix)]
#[test]
fn native_file_sources_preserve_exact_names_and_reject_unsafe_deserialized_paths() {
    for name in
        ["a:b", "a\\b", "name.", "NUL", "line\nbreak", "escape\u{1b}", "control\u{85}", "\u{2003}"]
    {
        let path = WorkspacePath::new(name).unwrap();
        let source = FileSource::workspace(
            peritus_codec::sha256(b"folder"),
            &path,
            FileRange::All,
            FileMode::Snapshot,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&source).unwrap();
        assert!(String::from_utf8(bytes.clone()).unwrap().contains("native_workspace"));
        assert_eq!(serde_json::from_slice::<FileSource>(&bytes).unwrap(), source);
        for invalid in ["../outside", ".git/config", "NUL\0name"] {
            let mut forged: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            forged["origin"]["native_workspace"]["path"] = serde_json::json!(invalid);
            assert!(serde_json::from_value::<FileSource>(forged).is_err());
        }
    }
}

#[test]
fn large_source_small_selection_keeps_canonical_replay_and_stale_revision_fencing() {
    let (record, _) = attach(FileMode::Snapshot);
    let text = ValidatedFileText::new(b"retained".to_vec()).unwrap();
    let start = 64 * 1024 * 1024;
    let end = start + text.bytes();
    let source = FileSource::workspace(
        peritus_codec::sha256(b"folder"),
        &WorkspacePath::new("large").unwrap(),
        FileRange::Bytes { start, end },
        FileMode::Snapshot,
    )
    .unwrap();
    let observation = FileObservation::new(
        peritus_codec::sha256(b"complete large source identity"),
        u64::MAX,
        (start, end),
        text.digest(),
    )
    .unwrap();
    assert!(observation.matches(&text));
    let file = FileAttachment::new(
        source,
        FileVersion::new(
            OperationId::new([3; 16]).unwrap(),
            ArtifactId::new([3; 16]).unwrap(),
            observation,
            peritus_codec::sha256(b"exact consent"),
        )
        .unwrap(),
    )
    .unwrap();
    let change = operation(
        3,
        2,
        ControlIntent::AttachFile {
            file,
            text: ControlText::new("large source selection".to_owned()).unwrap(),
        },
    );
    let predecessor = record.canonical_bytes().unwrap();
    let (next, _) = ConversationRecord::apply(Some(&record), &change).unwrap();
    assert_eq!(record.canonical_bytes().unwrap(), predecessor);
    let bytes = next.canonical_bytes().unwrap();
    let reopened = ConversationRecord::parse(&bytes).unwrap();
    assert_eq!(reopened, next);
    assert_eq!(reopened.canonical_bytes().unwrap(), bytes);
    assert_eq!(
        ConversationRecord::apply(Some(&reopened), &change),
        Err(ControlError::StaleRevision)
    );
    assert!(FileRange::Lines { first: 67_108_865, last: u32::MAX }.validate().is_ok());
    assert!(FileObservation::new(observation.source_digest(), 1, (1, 2), text.digest()).is_err());
    assert!(
        FileObservation::new(observation.source_digest(), u64::MAX, (2, 1), text.digest()).is_err()
    );
    assert!(FileObservation::new(observation.source_digest(), 1, (0, 1), text.digest()).is_err());
}

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
