use super::*;
use crate::product_control::inputs::{InputAdmission, tests::request};
use peritus_product_runner::{
    AttachmentReadRequest,
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
    let file = match attach.intent() {
        ControlIntent::AttachFile { file, .. } => file.clone(),
        _ => unreachable!("attachment helper returns attach-file intent"),
    };
    let start =
        operation(3, 2, ControlIntent::StartExecution { run: [5; 16], settings_digest: [6; 32] });
    journal.accept(&start).expect("start");
    let capture = journal.capture_execution(&start).expect("capture");
    let prompt = capture.inputs().conversation();
    assert!(prompt.contains(&file.operation().to_string()));
    assert!(prompt.contains("attachment_read"));
    assert!(!prompt.contains("exact selected text"));
    let invocation = InvocationId::new([11; 16]).expect("invocation");
    assert!(journal.prepare_inputs(&capture, invocation, &request("caption only")).is_err());
    assert!(matches!(
        journal.prepare_inputs(&capture, invocation, &request(prompt)).expect("exact request"),
        InputAdmission::Accepted(_)
    ));
    let observation = file.initial().observation();
    let page_request = AttachmentReadRequest::new(
        file.operation(),
        file.initial().operation(),
        observation.source_digest(),
        observation.digest(),
        observation.source_bytes(),
        observation.range(),
        observation.range().0,
        32 * 1024,
    )
    .expect("read selected bytes");
    let page = journal.read_file_page(&start, page_request).expect("selected page");
    assert_eq!(page.text(), "exact selected text\r\n");
    let current_revision = journal
        .load(create().conversation())
        .expect("load before deselect")
        .expect("record before deselect")
        .revision();
    journal
        .accept(&operation(
            4,
            current_revision,
            ControlIntent::SelectFile { attachment: file.operation(), selected: false },
        ))
        .expect("deselect");
    assert!(matches!(
        journal.prepare_inputs(
            &capture,
            InvocationId::new([13; 16]).expect("stale invocation"),
            &request(prompt),
        ),
        Ok(InputAdmission::Stale)
    ));
    assert!(journal.read_file_page(&start, page_request).is_err());
    let capture = journal.capture_execution(&start).expect("current capture");
    assert!(!capture.inputs().conversation().contains(&file.operation().to_string()));
    drop(journal);
    let journal = store(root.path());
    let record = journal.load(create().conversation()).expect("replay").expect("record");
    assert!(!record.files().entries()[0].selected());
    assert_eq!(record.inputs().invocations().len(), 1);
}

#[test]
fn large_selected_file_uses_exact_metadata_and_continues_through_immutable_pages() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    let bytes = vec![b'x'; 17 * 1024 * 1024];
    let text = ValidatedFileText::new(bytes).expect("large text");
    let attach = attachment(&text);
    let file = match attach.intent() {
        ControlIntent::AttachFile { file, .. } => file.clone(),
        _ => unreachable!("attachment helper returns attach-file intent"),
    };
    journal.accept_file(&attach, &text, b"confirmed preview".to_vec()).expect("attach");
    let start =
        operation(3, 2, ControlIntent::StartExecution { run: [5; 16], settings_digest: [6; 32] });
    journal.accept(&start).expect("start");
    let captured = journal.capture_execution(&start).expect("capture");
    let prompt = captured.conversation_with_guidance().expect("live conversation view");
    assert!(prompt.contains("attachment_read"));
    assert!(prompt.contains(&file.operation().to_string()));
    assert!(!prompt.contains(&"x".repeat(1024)));
    journal
        .prepare_inputs(
            &captured,
            InvocationId::new([12; 16]).expect("invocation"),
            &request(&prompt),
        )
        .expect("bind metadata prompt");

    let observation = file.initial().observation();
    let first_request = AttachmentReadRequest::new(
        file.operation(),
        file.initial().operation(),
        observation.source_digest(),
        observation.digest(),
        observation.source_bytes(),
        observation.range(),
        observation.range().0,
        32 * 1024,
    )
    .expect("first range request");
    let first = journal.read_file_page(&start, first_request).expect("first page");
    assert_eq!(first.text().len(), 32 * 1024);
    assert_eq!(first.next_offset(), Some(32 * 1024));
    let stale_request = AttachmentReadRequest::new(
        file.operation(),
        file.initial().operation(),
        observation.source_digest(),
        peritus_types::Sha256Digest::new([99; 32]),
        observation.source_bytes(),
        observation.range(),
        observation.range().0,
        32 * 1024,
    )
    .expect("structurally valid but stale digest");
    assert!(journal.read_file_page(&start, stale_request).is_err());
    let second_request = AttachmentReadRequest::new(
        file.operation(),
        file.initial().operation(),
        observation.source_digest(),
        observation.digest(),
        observation.source_bytes(),
        observation.range(),
        first.next_offset().expect("continuation"),
        32 * 1024,
    )
    .expect("second range request");
    let second = journal.read_file_page(&start, second_request).expect("second page");
    assert_eq!(second.offset(), 32 * 1024);
    assert_eq!(second.text(), "x".repeat(32 * 1024));
}

#[test]
fn empty_confirmed_file_has_an_explicit_terminal_attachment_page() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    let text = ValidatedFileText::new(Vec::new()).expect("empty text is valid");
    let attach = attachment(&text);
    let file = match attach.intent() {
        ControlIntent::AttachFile { file, .. } => file.clone(),
        _ => unreachable!("attachment helper returns attach-file intent"),
    };
    journal.accept_file(&attach, &text, b"confirmed preview".to_vec()).expect("attach");
    let start =
        operation(3, 2, ControlIntent::StartExecution { run: [5; 16], settings_digest: [6; 32] });
    journal.accept(&start).expect("start");
    let observation = file.initial().observation();
    let request = AttachmentReadRequest::new(
        file.operation(),
        file.initial().operation(),
        observation.source_digest(),
        observation.digest(),
        0,
        (0, 0),
        0,
        4,
    )
    .expect("empty range is valid");
    let page = journal.read_file_page(&start, request).expect("empty terminal page");
    assert!(page.text().is_empty());
    assert_eq!(page.next_offset(), None);
}

#[test]
fn message_bundle_is_one_atomic_input_with_exact_large_sources_and_restart_replay() {
    use peritus_product_runner::control::{InputId, InputState, QueueIntent};
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    let message = ValidatedFileText::new("Read src/exact.rs λ界\n".repeat(20_000).into_bytes())
        .expect("message");
    let attached = ValidatedFileText::new("immutable source data\n".repeat(20_000).into_bytes())
        .expect("attachment");
    let op = operation(7, 1, ControlIntent::PinConversation { pinned: false });
    let input = InputId::new(*op.id().as_bytes()).expect("input");
    let files = [
        (&message, FileSource::user_message()),
        (
            &attached,
            FileSource::imported(
                ControlText::new("data.txt".to_owned()).expect("label"),
                FileRange::All,
            )
            .expect("source"),
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (text, source))| {
        let id = OperationId::new([u8::try_from(30 + index).expect("id"); 16]).expect("operation");
        let observation =
            FileObservation::new(text.digest(), text.bytes(), (0, text.bytes()), text.digest())
                .expect("metadata");
        FileAttachment::for_message(
            source,
            FileVersion::new(
                id,
                ArtifactId::new(*id.as_bytes()).expect("artifact"),
                observation,
                sha256(b"exact bundle consent"),
            )
            .expect("version"),
            input,
        )
        .expect("shared input")
    })
    .collect::<Vec<_>>();
    let submit = operation(
        7,
        1,
        ControlIntent::SubmitMessage {
            text: ControlText::new(
                "Read complete user message reference before acting.".to_owned(),
            )
            .expect("text"),
            files,
        },
    );
    assert!(journal.accept_files(&submit, &[(&message, b"exact bundle consent")]).is_err());
    let unchanged = journal.load(create().conversation()).expect("load").expect("conversation");
    assert_eq!(unchanged.revision(), 1);
    assert!(unchanged.inputs().revisions().is_empty());
    assert!(unchanged.files().entries().is_empty());
    let receipt = journal
        .accept_files(
            &submit,
            &[(&message, b"exact bundle consent"), (&attached, b"exact bundle consent")],
        )
        .expect("atomic message");
    drop(journal);
    let mut journal = store(root.path());
    assert_eq!(journal.resolve(&submit).expect("resolve"), Some(receipt));
    let record = journal.load(create().conversation()).expect("load").expect("conversation");
    assert_eq!(record.inputs().revisions().len(), 1);
    assert_eq!(record.files().entries().len(), 2);
    assert_eq!(record.inputs().latest(input).expect("message").state(), InputState::Queued);
    let start =
        operation(8, 2, ControlIntent::StartExecution { run: [5; 16], settings_digest: [6; 32] });
    journal.accept(&start).expect("start");
    let capture = journal.capture_execution(&start).expect("capture");
    assert!(capture.reference_authority_context().contains("Read src/exact.rs λ界"));
    assert!(!capture.reference_authority_context().contains("immutable source data"));
    assert!(!capture.inputs().conversation().contains("Read src/exact.rs λ界"));
    let selection = record.inputs().latest(input).expect("message").selection();
    journal
        .accept(&operation(
            9,
            3,
            ControlIntent::Queue(QueueIntent::Hold { selected: selection, held: true }),
        ))
        .expect("hold entire message");
    let record = journal.load(create().conversation()).expect("load").expect("conversation");
    assert!(
        record.files().eligible(record.inputs().capture().expect("capture").included()).is_empty()
    );
}
