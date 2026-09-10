use super::*;
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, MediaInput, Message, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderName,
    ProviderProfile, ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, ResumeKind,
    Role, StateMode, StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_product_runner::control::{ControlText, InputId, InputSelection};

pub(in crate::product_control) fn request(text: &str) -> ModelRequest {
    request_images(text, &[])
}

pub(in crate::product_control) fn image_profile(images: bool) -> ProviderProfile {
    let limits = ModelLimits::new(32_768, 4_096, 32, 1, 512 * 1024).expect("limits");
    let capabilities = if images { vec![Capability::ImageInput] } else { Vec::new() };
    ProviderProfile::new(
        peritus_types::ProviderProfileId::new([10; 16]).expect("profile"),
        1,
        ProviderName::new("scripted".to_owned()).expect("name"),
        ModelName::new("scripted".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&capabilities, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        limits,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

pub(in crate::product_control) fn request_images(
    text: &str,
    images: &[MediaInput],
) -> ModelRequest {
    let profile = image_profile(!images.is_empty());
    let required = if images.is_empty() { Vec::new() } else { vec![Capability::ImageInput] };
    let mut content = vec![ContentBlock::Text(
        BoundedText::new(text.to_owned(), ProtocolLimits::PRODUCTION).expect("text"),
    )];
    content.extend(images.iter().cloned().map(ContentBlock::Image));
    ModelRequest::new(
        &profile,
        negotiate(
            &profile,
            RequestedCapabilities::new(&required, &[], profile.limits()).expect("requested"),
        )
        .expect("negotiate"),
        RequestId::new("test-request".to_owned()).expect("request identity"),
        vec![Message::new(Role::User, content, ProtocolLimits::PRODUCTION).expect("message")],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(128, Vec::new(), None, None, None).expect("generation"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .expect("request")
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

fn capture(store: &ControlStore) -> CapturedConversation {
    store
        .capture_inputs(
            ConversationId::new([2; 16]).expect("conversation"),
            ActorId::new([3; 16]).expect("actor"),
            WorkspaceId::new([4; 16]).expect("workspace"),
        )
        .expect("capture")
}

#[test]
fn brief_edits_fence_stale_requests_and_seal_the_exact_revised_requirement_after_restart() {
    use peritus_product_runner::control::BriefField;
    let root = tempfile::tempdir().expect("root");
    let store_id = peritus_journal::StoreId::new([1; 16]).expect("store");
    let mut store = ControlStore::open(root.path(), store_id).expect("open");
    store
        .accept(&operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Brief admission".to_owned()).expect("title"),
            },
        ))
        .expect("create");
    store
        .accept(&operation(
            2,
            1,
            ControlIntent::SetBrief {
                field: BriefField::Objective,
                text: ControlText::new("Original user-confirmed objective".to_owned())
                    .expect("text"),
            },
        ))
        .expect("set brief");
    let stale = capture(&store);
    assert!(stale.inputs().conversation().contains("Current user-confirmed brief"));
    store
        .accept(&operation(
            3,
            2,
            ControlIntent::SetBrief {
                field: BriefField::Objective,
                text: ControlText::new("Corrected user-confirmed objective".to_owned())
                    .expect("text"),
            },
        ))
        .expect("edit brief");
    assert_eq!(
        store
            .prepare_inputs(
                &stale,
                InvocationId::new([10; 16]).expect("invocation"),
                &request(stale.inputs().conversation())
            )
            .expect("stale admission"),
        InputAdmission::Stale
    );
    let next = capture(&store);
    assert!(next.inputs().conversation().contains("Corrected user-confirmed objective"));
    assert!(!next.inputs().conversation().contains("Original user-confirmed objective"));
    let invocation = InvocationId::new([11; 16]).expect("invocation");
    store
        .prepare_inputs(&next, invocation, &request(next.inputs().conversation()))
        .expect("admit corrected request");
    drop(store);
    let store = ControlStore::open(root.path(), store_id).expect("restart");
    let record = store.load(next.conversation).expect("verify manifest").expect("record");
    assert_eq!(record.inputs().invocations().len(), 1);
    assert_eq!(record.brief().bindings()[0].selected().revision(), 2);
    assert_eq!(record.inputs().invocations()[0].invocation(), invocation);
    assert_eq!(capture(&store).inputs().conversation(), next.inputs().conversation());
}

#[test]
fn exact_request_admission_replays_after_restart_and_never_consumes_a_withdrawn_input() {
    for withdraw_first in [false, true] {
        let root = tempfile::tempdir().expect("root");
        let store_id = peritus_journal::StoreId::new([1; 16]).expect("store");
        let mut store = ControlStore::open(root.path(), store_id).expect("open");
        store
            .accept(&operation(
                1,
                0,
                ControlIntent::CreateConversation {
                    title: ControlText::new("test".to_owned()).expect("title"),
                },
            ))
            .expect("create");
        let input = InputId::new([5; 16]).expect("input");
        store
            .accept(&operation(
                2,
                1,
                ControlIntent::Queue(QueueIntent::Enqueue {
                    id: input,
                    text: ControlText::new("do not lose this correction".to_owned()).expect("text"),
                    dependencies: Vec::new(),
                }),
            ))
            .expect("enqueue");
        let view = capture(&store);
        let request = request(view.inputs().conversation());
        let invocation = InvocationId::new([6; 16]).expect("invocation");
        if withdraw_first {
            store
                .accept(&operation(
                    3,
                    2,
                    ControlIntent::Queue(QueueIntent::Withdraw(
                        InputSelection::new(input, 1).expect("selection"),
                    )),
                ))
                .expect("withdraw");
        }
        let admitted = store.prepare_inputs(&view, invocation, &request).expect("prepare");
        drop(store);
        let mut store = ControlStore::open(root.path(), store_id).expect("restart");
        assert_eq!(store.prepare_inputs(&view, invocation, &request).expect("replay"), admitted);
        let record = store.load(view.conversation).expect("load").expect("record");
        if withdraw_first {
            assert_eq!(admitted, InputAdmission::Stale);
            assert!(record.inputs().invocations().is_empty());
            assert!(capture(&store).inputs().conversation().is_empty());
        } else {
            assert!(matches!(admitted, InputAdmission::Accepted(_)));
            assert_eq!(record.inputs().invocations().len(), 1);
            assert_eq!(
                record.inputs().invocations()[0].request_digest(),
                request.fingerprint().expect("fingerprint").digest()
            );
            let altered = self::request("different request");
            assert!(matches!(
                store.prepare_inputs(&view, invocation, &altered),
                Err(Error::Control(ControlError::IdempotencyConflict))
            ));
        }
    }
}

#[test]
fn public_reply_is_immutable_and_enters_only_a_later_user_turn_after_restart() {
    let root = tempfile::tempdir().expect("root");
    let store_id = peritus_journal::StoreId::new([1; 16]).expect("store");
    let mut store = ControlStore::open(root.path(), store_id).expect("open");
    store
        .accept(&operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Conversation".to_owned()).expect("title"),
            },
        ))
        .expect("create");
    store
        .accept(&operation(
            2,
            1,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([5; 16]).expect("id"),
                text: ControlText::new("Original user question".to_owned()).expect("text"),
                dependencies: Vec::new(),
            }),
        ))
        .expect("enqueue");
    let start =
        operation(8, 2, ControlIntent::StartExecution { run: [9; 16], settings_digest: [10; 32] });
    store.accept(&start).expect("start");
    let captured = capture(&store);
    let invocation = InvocationId::new([6; 16]).expect("invocation");
    store
        .prepare_inputs(&captured, invocation, &request(captured.inputs().conversation()))
        .expect("bind");
    store.publish_reply(&start, "Exact public answer with a question?").expect("reply");
    store.publish_reply(&start, "Exact public answer with a question?").expect("idempotent reply");
    assert!(store.publish_reply(&start, "Attempt to rewrite the answer").is_err());
    let before = capture(&store);
    assert_eq!(before.inputs().conversation(), "User: Original user question");
    assert_eq!(before.inputs().generation(), 1);
    assert!(before.inputs().public_replies().is_empty());
    drop(store);
    let mut store = ControlStore::open(root.path(), store_id).expect("restart");
    store
        .accept(&operation(
            9,
            5,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([7; 16]).expect("id"),
                text: ControlText::new("My exact follow-up".to_owned()).expect("text"),
                dependencies: Vec::new(),
            }),
        ))
        .expect("follow-up");
    let next = capture(&store);
    assert_eq!(
        next.inputs().conversation(),
        "User: Original user question\n\nPeritus (public reply): Exact public answer with a question?\n\nUser: My exact follow-up"
    );
    assert_eq!(next.inputs().public_replies(), &[invocation]);
    store
        .prepare_inputs(
            &next,
            InvocationId::new([11; 16]).expect("next invocation"),
            &request(next.inputs().conversation()),
        )
        .expect("second binding");
    let record =
        store.load(start.conversation()).expect("verified manifest replay").expect("record");
    assert_eq!(record.replies().len(), 1);
    assert_eq!(record.inputs().invocations().len(), 2);
}
