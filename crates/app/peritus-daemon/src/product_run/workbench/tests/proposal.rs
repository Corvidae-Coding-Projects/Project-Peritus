use super::*;
use peritus_product_runner::control::{BriefField, FileSelection, PublicReplyReference};

struct Fixture {
    directory: tempfile::TempDir,
    store: ControlStore,
    store_id: peritus_journal::StoreId,
    actor: ActorId,
    start: ControlOperation,
    reply: PublicReplyReference,
    text: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("directory");
        let store_id = peritus_journal::StoreId::new([81; 16]).expect("store");
        let store = ControlStore::open(directory.path(), store_id).expect("open");
        let actor = ActorId::new([82; 16]).expect("actor");
        let start = ControlOperation::new(
            OperationId::new([83; 16]).expect("start"),
            ConversationId::new([84; 16]).expect("conversation"),
            actor,
            WorkspaceId::new([85; 16]).expect("workspace"),
            2,
            ControlIntent::StartExecution { run: [86; 16], settings_digest: [87; 32] },
        );
        let placeholder = PublicReplyReference::new(
            start.id(),
            InvocationId::new([88; 16]).expect("invocation"),
            [1; 32],
            1,
        )
        .expect("placeholder");
        let text = format!(
            "{}\nComplete acceptance criteria end here.\n",
            "Precise criterion 🦉 with source attribution.\n".repeat(1100)
        );
        let mut fixture =
            Self { directory, store, store_id, actor, start, reply: placeholder, text };
        fixture.apply(
            1,
            ControlIntent::CreateConversation {
                title: ControlText::new("Long proposal".into()).expect("title"),
            },
        );
        fixture.apply(
            2,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([89; 16]).expect("input"),
                text: ControlText::new("Propose a plan".into()).expect("text"),
                dependencies: Vec::new(),
            }),
        );
        fixture.store.accept(&fixture.start).expect("start");
        let capture = fixture.store.capture_execution(&fixture.start).expect("capture");
        fixture
            .store
            .prepare_inputs(
                &capture,
                InvocationId::new([88; 16]).expect("invocation"),
                &crate::product_control::test_request(capture.inputs().conversation()),
            )
            .expect("incorporate");
        fixture.store.publish_reply(&fixture.start, &fixture.text).expect("publish long reply");
        fixture.reply = fixture.record().replies()[0].clone();
        fixture
    }
    fn record(&self) -> ConversationRecord {
        self.store.load(self.start.conversation()).expect("load").expect("record")
    }
    fn command(&self, id: u8) -> WorkbenchCommand {
        WorkbenchCommand::new(
            peritus_app_protocol::ControlOperationId::new([id; 16]).expect("operation"),
            WorkbenchQuery::new(
                peritus_app_protocol::ConversationId::new(*self.start.conversation().as_bytes())
                    .expect("conversation"),
                WorkspaceId::new(*self.start.workspace_bytes()).expect("workspace"),
            ),
            self.record().revision(),
            WorkbenchIntent::AcceptBriefProposal {
                field: WorkbenchBriefField::Acceptance,
                proposal: peritus_app_protocol::ControlOperationId::new(
                    *self.reply.operation().as_bytes(),
                )
                .expect("proposal"),
                digest: self.reply.digest(),
            },
        )
    }
    fn apply(&mut self, id: u8, intent: ControlIntent) {
        let revision = self
            .store
            .load(self.start.conversation())
            .expect("load")
            .map_or(0, |record| record.revision());
        let operation = ControlOperation::new(
            OperationId::new([id; 16]).expect("operation"),
            self.start.conversation(),
            self.actor,
            WorkspaceId::new(*self.start.workspace_bytes()).expect("workspace"),
            revision,
            intent,
        );
        self.store.accept(&operation).expect("accept mutation");
    }
    fn read_all(&self, file: &FileSelection) -> String {
        let observed = file.current().observation();
        let mut result = String::new();
        let mut offset = 0;
        loop {
            let request = peritus_product_runner::AttachmentReadRequest::new(
                file.file().operation(),
                file.current().operation(),
                observed.source_digest(),
                observed.digest(),
                observed.source_bytes(),
                observed.range(),
                offset,
                4096,
            )
            .expect("page request");
            let page =
                self.store.read_file_page(&self.start, request).expect("read selected bytes");
            result.push_str(page.text());
            let Some(next) = page.next_offset() else { break };
            offset = next;
        }
        result
    }
}

#[test]
fn large_proposal_acceptance_retains_exact_bytes_authorship_and_revision_lifecycle() {
    let mut fixture = Fixture::new();
    let command = fixture.command(90);
    let operation = domain_operation_with_store(&fixture.store, fixture.actor, &command)
        .expect("metadata-only mapping");
    assert!(
        matches!(operation.intent(), ControlIntent::AcceptBriefProposal { reply, .. } if reply == &fixture.reply)
    );
    assert!(
        fixture.store.accept(&operation).is_err(),
        "unverified metadata cannot admit proposal bytes"
    );
    let receipt =
        mapping::proposal::accept(&mut fixture.store, &operation, fixture.actor, &command)
            .expect("explicit acceptance");
    let accepted = fixture.record();
    assert_eq!(accepted.revision(), command.expected_revision() + 1);
    let binding = accepted.brief().bindings()[0];
    let input = accepted.inputs().latest(binding.selected().id()).expect("brief source");
    assert_eq!(input.author_bytes(), fixture.actor.as_bytes());
    assert!(input.text().contains("attachment_read"));
    let file = &accepted.files().entries()[0];
    assert_eq!(file.file().source().proposal(), Some(&fixture.reply));
    assert_eq!(file.file().input_revision(), Some(binding.selected().revision()));
    assert_eq!(fixture.read_all(file), fixture.text);
    drop(fixture.store);
    fixture.store =
        ControlStore::open(fixture.directory.path(), fixture.store_id).expect("restart");
    let remapped = domain_operation_with_store(&fixture.store, fixture.actor, &command)
        .expect("map after restart");
    assert_eq!(resolve_user_operation(&fixture.store, &remapped).expect("replay"), Some(receipt));
    assert_eq!(fixture.read_all(file), fixture.text);

    fixture.apply(
        91,
        ControlIntent::Queue(QueueIntent::Hold { selected: binding.selected(), held: true }),
    );
    let held = fixture.record();
    assert!(
        held.eligible_files(held.inputs().capture().expect("held capture").included()).is_empty()
    );
    fixture.apply(
        92,
        ControlIntent::Queue(QueueIntent::Hold { selected: binding.selected(), held: false }),
    );
    assert_eq!(fixture.read_all(file), fixture.text);
    fixture.apply(
        93,
        ControlIntent::SetBrief {
            field: BriefField::Acceptance,
            text: ControlText::new("Revised user-written criterion".into()).expect("text"),
        },
    );
    let edited = fixture.record();
    assert_eq!(edited.brief().bindings()[0].selected().id(), binding.selected().id());
    assert_ne!(edited.brief().bindings()[0].selected().revision(), binding.selected().revision());
    assert!(
        edited
            .eligible_files(edited.inputs().capture().expect("edited capture").included())
            .is_empty(),
        "replacement text must not inherit old model proposal bytes"
    );
    assert_eq!(
        edited.files().entries()[0].file().source().proposal(),
        Some(&fixture.reply),
        "immutable history remains"
    );
}
