//! Request archive capacity is independent from the next provider view.

use super::*;

#[test]
fn large_request_and_long_history_recover_exactly_after_restart() {
    let root = tempfile::tempdir().expect("root");
    let store_id = peritus_journal::StoreId::new([1; 16]).expect("store");
    let mut store = ControlStore::open(root.path(), store_id).expect("open");
    store
        .accept(&operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Archive capacity".to_owned()).expect("title"),
            },
        ))
        .expect("create");
    let source = "source".repeat(1_000);
    store
        .accept(&operation(
            2,
            1,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([5; 16]).expect("input"),
                text: ControlText::new(source).expect("governing source"),
                dependencies: Vec::new(),
            }),
        ))
        .expect("enqueue");
    let captured = capture(&store);
    let limits = ProtocolLimits::ARCHIVE;
    let seed = request("fixture");
    let mut messages = vec![
        Message::new(
            Role::User,
            vec![ContentBlock::Text(
                BoundedText::new(captured.inputs().conversation().to_owned(), limits)
                    .expect("source"),
            )],
            limits,
        )
        .expect("message"),
    ];
    messages.push(
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Text(
                BoundedText::new("history".repeat(3 * 1024 * 1024), limits).expect("large history"),
            )],
            limits,
        )
        .expect("large message"),
    );
    for index in 0..ProtocolLimits::PRODUCTION.max_messages() {
        messages.push(
            Message::new(
                Role::User,
                vec![ContentBlock::Text(
                    BoundedText::new(format!("ordered message {index}"), limits).expect("text"),
                )],
                limits,
            )
            .expect("message"),
        );
    }
    let request = ModelRequest::new(
        &image_profile(false),
        seed.negotiated(),
        seed.request_id().clone(),
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        seed.options().clone(),
        limits,
    )
    .expect("archive request");
    let expected = request.canonical_bytes().expect("canonical request");
    assert!(expected.len() > 16 * 1024 * 1024);
    let invocation = InvocationId::new([6; 16]).expect("invocation");
    let admitted = store.prepare_inputs(&captured, invocation, &request).expect("admit");
    let InputAdmission::Accepted(receipt) = &admitted else { panic!("fresh request is stale") };
    let accepted_operation = store
        .operation(captured.conversation, receipt.operation())
        .expect("lookup")
        .expect("operation");
    drop(store);

    let mut store = ControlStore::open(root.path(), store_id).expect("restart");
    assert_eq!(
        store.prepare_inputs(&captured, invocation, &request).expect("exact retry"),
        admitted
    );
    let restored = store.accepted_request(&accepted_operation).expect("read").expect("archive");
    assert_eq!(restored, expected);
    let decoded = peritus_model_protocol::decode_request(
        &restored,
        &image_profile(false),
        request.request_id().clone(),
        limits,
    )
    .expect("decode archive");
    assert_eq!(decoded.messages(), request.messages());
    assert_eq!(
        decoded.fingerprint().expect("restored digest"),
        request.fingerprint().expect("digest")
    );
    let record = store.load(captured.conversation).expect("verified replay").expect("record");
    assert_eq!(record.inputs().invocations().len(), 1);
    let manifest = store.invocation_manifest(captured.conversation, invocation).expect("manifest");
    let (_, bytes, rows) = inspect_manifest(&manifest).expect("projection");
    assert_eq!(bytes, expected.len() as u64);
    assert_eq!(rows.len(), request.messages().len() + 1);
}
