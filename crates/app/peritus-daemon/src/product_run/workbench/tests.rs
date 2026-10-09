use super::*;
use peritus_app_protocol::WorkbenchBriefField;
use peritus_product_runner::control::{
    ControlIntent, ControlText, InputId, InvocationId, OperationId, QueueIntent,
};
use peritus_types::WorkspaceId;

#[test]
fn accepting_an_exact_public_reply_creates_a_user_confirmed_brief_revision() {
    let root = tempfile::tempdir().expect("root");
    let store_id = peritus_journal::StoreId::new([1; 16]).expect("store");
    let mut store = ControlStore::open(root.path(), store_id).expect("open");
    let actor = ActorId::new([3; 16]).expect("actor");
    let workspace = WorkspaceId::new([4; 16]).expect("workspace");
    let conversation = ConversationId::new([2; 16]).expect("conversation");
    let operation = |id: u8, revision, intent| {
        ControlOperation::new(
            OperationId::new([id; 16]).expect("operation"),
            conversation,
            actor,
            workspace,
            revision,
            intent,
        )
    };
    store
        .accept(&operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Brief proposals".to_owned()).expect("title"),
            },
        ))
        .expect("create");
    store
        .accept(&operation(
            2,
            1,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([5; 16]).expect("input"),
                text: ControlText::new("Initial question".to_owned()).expect("text"),
                dependencies: Vec::new(),
            }),
        ))
        .expect("input");
    let start =
        operation(3, 2, ControlIntent::StartExecution { run: [6; 16], settings_digest: [7; 32] });
    store.accept(&start).expect("start");
    let captured = store.capture_execution(&start).expect("capture");
    let invocation = InvocationId::new([8; 16]).expect("invocation");
    let request = crate::product_control::test_request(captured.inputs().conversation());
    store.prepare_inputs(&captured, invocation, &request).expect("incorporate");
    let reply_text = "Agent-proposed restart criterion";
    store.publish_reply(&start, reply_text).expect("reply");
    let record = store.load(conversation).expect("load").expect("record");
    let reply = record.replies().first().expect("reply reference");
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new(*conversation.as_bytes()).expect("conversation"),
        workspace,
    );
    let command = WorkbenchCommand::new(
        peritus_app_protocol::ControlOperationId::new([12; 16]).expect("operation"),
        query,
        record.revision(),
        WorkbenchIntent::AcceptBriefProposal {
            field: WorkbenchBriefField::Acceptance,
            proposal: peritus_app_protocol::ControlOperationId::new(*reply.operation().as_bytes())
                .expect("proposal"),
            digest: reply.digest(),
        },
    );
    let mapped = domain_operation_with_store(&store, actor, &command).expect("map proposal");
    assert!(matches!(
        mapped.intent(),
        ControlIntent::SetBrief { field: peritus_product_runner::control::BriefField::Acceptance, text }
            if text.as_str() == reply_text
    ));
    store.accept(&mapped).expect("accept proposal");
    let accepted = store.load(conversation).expect("load").expect("record");
    let binding = accepted.brief().bindings().first().expect("binding");
    let source = accepted.inputs().latest(binding.selected().id()).expect("source");
    assert_eq!(source.text(), reply_text);
    assert_eq!(source.author_bytes(), actor.as_bytes());

    let changed = WorkbenchCommand::new(
        command.operation(),
        query,
        command.expected_revision(),
        WorkbenchIntent::AcceptBriefProposal {
            field: WorkbenchBriefField::Acceptance,
            proposal: match command.intent() {
                WorkbenchIntent::AcceptBriefProposal { proposal, .. } => *proposal,
                _ => panic!("unexpected workbench intent"),
            },
            digest: peritus_codec::sha256(b"different"),
        },
    );
    assert!(matches!(
        domain_operation_with_store(&store, actor, &changed),
        Err(Error::Control(ControlError::StaleRevision))
    ));
    drop(store);
    let store = ControlStore::open(root.path(), store_id).expect("restart");
    assert!(store.resolve(&mapped).expect("resolve").is_some());
    let accepted = store.load(conversation).expect("load").expect("record");
    assert_eq!(
        accepted
            .inputs()
            .latest(accepted.brief().bindings()[0].selected().id())
            .expect("source")
            .text(),
        reply_text
    );
}
