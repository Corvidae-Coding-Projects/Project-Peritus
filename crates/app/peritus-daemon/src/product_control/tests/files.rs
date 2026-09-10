use super::*;
use crate::product_control::inputs::{InputAdmission, tests::request};
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{FileAttachment, FileObservation, FileRange, FileSource, FileVersion, InvocationId},
};
use peritus_types::ArtifactId;

fn attachment(text: &ValidatedFileText) -> ControlOperation {
    let id = operation(2, 1, ControlIntent::PinConversation { pinned: false }).id();
    let observation =
        FileObservation::new(text.digest(), text.bytes(), (0, text.bytes()), text.digest())
            .expect("metadata");
    let version = FileVersion::new(
        id,
        ArtifactId::new([8; 16]).expect("artifact"),
        observation,
        sha256(b"confirmed preview"),
    )
    .expect("version");
    let source = FileSource::imported(
        ControlText::new("reference.txt".to_owned()).expect("label"),
        FileRange::All,
    )
    .expect("source");
    operation(
        2,
        1,
        ControlIntent::AttachFile {
            file: FileAttachment::new(source, version).expect("file"),
            text: ControlText::new("Consider this reference".to_owned()).expect("caption"),
        },
    )
}

#[test]
fn confirmed_file_round_trips_restart_and_only_exact_included_context_is_admitted() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    let text = ValidatedFileText::new(b"exact selected text\r\n".to_vec()).expect("text");
    let attach = attachment(&text);
    assert!(journal.accept(&attach).is_err(), "metadata is not consent or bytes");
    assert!(journal.accept_file(&attach, &text, b"wrong consent".to_vec()).is_err());
    let receipt =
        journal.accept_file(&attach, &text, b"confirmed preview".to_vec()).expect("accept");
    drop(journal);
    let mut journal = store(root.path());
    assert_eq!(journal.resolve(&attach).expect("resolve"), Some(receipt));
    let capture = journal
        .capture_inputs(
            create().conversation(),
            ActorId::new([3; 16]).expect("owner"),
            WorkspaceId::new([4; 16]).expect("workspace"),
        )
        .expect("capture");
    assert!(capture.inputs().conversation().contains("exact selected text\\r\\n"));
    let invocation = InvocationId::new([11; 16]).expect("invocation");
    assert!(journal.prepare_inputs(&capture, invocation, &request("caption only")).is_err());
    assert!(matches!(
        journal
            .prepare_inputs(&capture, invocation, &request(capture.inputs().conversation()))
            .expect("exact request"),
        InputAdmission::Accepted(_)
    ));
    journal
        .accept(&operation(
            3,
            3,
            ControlIntent::SelectFile { attachment: attach.id(), selected: false },
        ))
        .expect("deselect");
    let capture = journal
        .capture_inputs(
            create().conversation(),
            ActorId::new([3; 16]).expect("owner"),
            WorkspaceId::new([4; 16]).expect("workspace"),
        )
        .expect("capture");
    assert!(!capture.inputs().conversation().contains("exact selected text"));
    drop(journal);
    let journal = store(root.path());
    let record = journal.load(create().conversation()).expect("replay").expect("record");
    assert!(!record.files().entries()[0].selected());
    assert_eq!(record.inputs().invocations().len(), 1);
}
