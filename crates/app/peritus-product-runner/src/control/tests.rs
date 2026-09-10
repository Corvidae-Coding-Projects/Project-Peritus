use super::*;
use peritus_types::{ActorId, WorkspaceId};

mod restore_capacity;

fn create() -> ControlOperation {
    ControlOperation::new(
        OperationId::new([1; 16]).expect("operation"),
        ConversationId::new([2; 16]).expect("conversation"),
        ActorId::new([3; 16]).expect("actor"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        0,
        ControlIntent::CreateConversation {
            title: ControlText::new("Private project".to_owned()).expect("title"),
        },
    )
}

#[test]
fn state_and_receipt_roundtrip_but_reject_changed_owner_and_unknown_generation() {
    let create = create();
    let (record, receipt) = ConversationRecord::apply(None, &create).expect("create");
    assert_eq!(
        ControlOperation::parse(&create.canonical_bytes().expect("encode")).expect("decode"),
        create
    );
    assert_eq!(
        ConversationRecord::parse(&record.canonical_bytes().expect("encode")).expect("decode"),
        record
    );
    assert_eq!(
        ControlReceipt::resolve(&receipt.canonical_bytes().expect("encode"), &create)
            .expect("resolve"),
        receipt
    );
    let wrong_owner = ControlOperation::new(
        OperationId::new([5; 16]).expect("operation"),
        create.conversation(),
        ActorId::new([6; 16]).expect("actor"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        0,
        ControlIntent::PinConversation { pinned: true },
    );
    assert_eq!(
        ConversationRecord::apply(Some(&record), &wrong_owner),
        Err(ControlError::ScopeMismatch)
    );
    let bytes = String::from_utf8(record.canonical_bytes().expect("bytes")).expect("UTF-8");
    assert_eq!(
        ConversationRecord::parse(
            bytes.replace("\"minimum_reader\":1", "\"minimum_reader\":2").as_bytes()
        ),
        Err(ControlError::UnsupportedSchema)
    );
    assert!(
        !format!("{record:?}").contains("Private project"),
        "Debug must not expose message text"
    );
}

#[test]
fn deserialization_cannot_construct_reserved_identifiers_or_unbounded_text() {
    assert!(ConversationId::new([0; 16]).is_err());
    assert!(serde_json::from_str::<ConversationId>("[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]").is_err());
    assert!(serde_json::from_str::<ControlText<4>>("\"oversized\"").is_err());
    assert!(ControlText::<256>::new("\u{1b}]0;title\u{7}".to_owned()).is_err());
    assert!(ControlText::<256>::new(" \n\t ".to_owned()).is_err());
}
