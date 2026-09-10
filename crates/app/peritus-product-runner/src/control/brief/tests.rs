use super::*;
use crate::control::{
    ControlIntent, ControlOperation, ConversationId, ConversationRecord, InvocationId,
};
use peritus_types::WorkspaceId;

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
fn initial() -> ConversationRecord {
    ConversationRecord::apply(
        None,
        &operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Brief fixture".to_owned()).expect("title"),
            },
        ),
    )
    .expect("create")
    .0
}
fn set(
    record: &ConversationRecord,
    index: u8,
    field: BriefField,
    text: &str,
) -> ConversationRecord {
    ConversationRecord::apply(
        Some(record),
        &operation(
            index,
            record.revision(),
            ControlIntent::SetBrief {
                field,
                text: ControlText::new(text.to_owned()).expect("text"),
            },
        ),
    )
    .expect("set")
    .0
}
fn queue(record: &ConversationRecord, index: u8, intent: QueueIntent) -> ConversationRecord {
    ConversationRecord::apply(
        Some(record),
        &operation(index, record.revision(), ControlIntent::Queue(intent)),
    )
    .expect("queue")
    .0
}
fn capture(record: &ConversationRecord) -> crate::control::InputCapture {
    record.capture_with_replies(&std::collections::BTreeMap::new(), true).expect("capture")
}

#[test]
fn brief_edits_share_immutable_queue_sources_and_do_not_release_a_held_requirement() {
    let record = set(&initial(), 5, BriefField::Objective, "Initial objective");
    let source = record.brief().bindings()[0].selected();
    assert_eq!(record.inputs().generation(), 1);
    assert!(capture(&record).conversation().contains("Current user-confirmed brief"));
    let held = queue(&record, 6, QueueIntent::Hold { selected: source, held: true });
    let edited = set(&held, 7, BriefField::Objective, "Revised held objective");
    let selected = edited.brief().bindings()[0].selected();
    assert_eq!(selected.id(), source.id());
    assert_eq!(selected.revision(), 2);
    assert_eq!(edited.inputs().latest(source.id()).expect("source").state(), InputState::Held);
    assert_eq!(edited.inputs().revisions()[0].text(), "Initial objective");
    assert_eq!(edited.inputs().revisions()[0].state(), InputState::Superseded);
    assert!(capture(&edited).conversation().is_empty());
    let released = queue(&edited, 8, QueueIntent::Hold { selected, held: false });
    assert!(capture(&released).conversation().contains("Revised held objective"));
    assert!(!capture(&released).conversation().contains("Initial objective"));
    let withdrawn = queue(&released, 9, QueueIntent::Withdraw(selected));
    assert!(capture(&withdrawn).conversation().is_empty());
    assert_eq!(withdrawn.brief().bindings()[0].selected(), selected);
    let restored =
        ConversationRecord::parse(&withdrawn.canonical_bytes().expect("bytes")).expect("restore");
    assert_eq!(restored, withdrawn);
}

#[test]
fn incorporated_brief_edits_become_new_corrections_without_rewriting_prior_sources() {
    let record = set(&initial(), 5, BriefField::Acceptance, "Original criterion");
    let original = record.brief().bindings()[0].selected();
    let record = queue(
        &record,
        6,
        QueueIntent::Incorporate {
            invocation: InvocationId::new([10; 16]).expect("invocation"),
            request_digest: [11; 32],
            manifest_digest: [12; 32],
            items: vec![original],
        },
    );
    let revised = set(&record, 7, BriefField::Acceptance, "Revised criterion");
    let corrected = revised.brief().bindings()[0].selected();
    assert_ne!(corrected.id(), original.id());
    let correction = revised.inputs().latest(corrected.id()).expect("correction");
    assert_eq!(correction.correction_of(), Some(original));
    assert_eq!(
        revised.inputs().latest(original.id()).expect("original").text(),
        "Original criterion"
    );
    let capture = capture(&revised);
    let (_, current) =
        capture.conversation().split_once("Current user-confirmed brief").expect("brief");
    assert!(current.contains("Revised criterion"));
    assert!(!current.contains("Original criterion"));
    assert_eq!(capture.pending(), &[corrected]);
    assert_eq!(capture.generation(), 2);
}

#[test]
fn ordinary_queue_edits_refresh_exact_brief_revision_but_not_unrelated_fields() {
    let record = set(&initial(), 5, BriefField::Constraints, "Original constraint");
    let selected = record.brief().bindings()[0].selected();
    let record =
        set(&record, 6, BriefField::Assumptions, "User explicitly confirmed this assumption");
    let revised = queue(
        &record,
        7,
        QueueIntent::Edit {
            selected,
            text: ControlText::new("Edited in queue".to_owned()).expect("text"),
        },
    );
    assert_eq!(revised.brief().bindings().len(), 2);
    assert_eq!(revised.brief().bindings()[0].selected().revision(), 2);
    assert_eq!(revised.brief().bindings()[1].selected().revision(), 1);
    assert!(capture(&revised).conversation().contains("Edited in queue"));
    assert!(!capture(&revised).conversation().contains("Original constraint"));
    assert!(revised.execution().is_none());
}
