use super::*;
use crate::control::{ControlIntent, ControlOperation, ConversationId, ConversationRecord};
use peritus_types::WorkspaceId;

fn operation(index: u8, revision: u64, intent: ControlIntent) -> ControlOperation {
    ControlOperation::new(
        OperationId::new([index; 16]).expect("operation"),
        ConversationId::new([2; 16]).expect("conversation"),
        ActorId::new([3; 16]).expect("owner"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        revision,
        intent,
    )
}

fn create() -> ConversationRecord {
    ConversationRecord::apply(
        None,
        &operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("Images".to_owned()).expect("title"),
            },
        ),
    )
    .expect("create")
    .0
}

// Pure transition fixtures deliberately model metadata only; daemon tests separately prove
// that this metadata cannot be accepted without decoded bytes in the atomic artifact store.
fn reference(index: u8, bytes: u64) -> ImageAttachment {
    let operation = OperationId::new([index; 16]).expect("operation");
    ImageAttachment {
        operation,
        input: source_input(operation).expect("input"),
        artifact: [index; 16],
        digest: [index; 32],
        bytes,
        format: ImageFormat::Png,
        width: 1,
        height: 1,
        frames: 1,
        label: ControlText::new("explicit source.png".to_owned()).expect("label"),
        preview_digest: None,
    }
}

fn attach(
    record: &ConversationRecord,
    index: u8,
    bytes: u64,
) -> Result<ConversationRecord, ControlError> {
    ConversationRecord::apply(
        Some(record),
        &operation(
            index,
            record.revision(),
            ControlIntent::AttachImage {
                image: reference(index, bytes),
                text: ControlText::new(format!("Reference {index}")).expect("caption"),
            },
        ),
    )
    .map(|(next, _)| next)
}

#[test]
fn count_limit_is_exact_and_held_input_cannot_be_released_past_the_limit() {
    let mut record = create();
    for index in 2..=17 {
        record = attach(&record, index, 1).expect("within count limit");
    }
    let unchanged = record.clone();
    assert_eq!(attach(&record, 18, 1), Err(ControlError::Capacity));
    assert_eq!(record, unchanged);
    let first = record.inputs().capture().expect("capture").included()[0];
    record = ConversationRecord::apply(
        Some(&record),
        &operation(
            19,
            record.revision(),
            ControlIntent::Queue(QueueIntent::Hold { selected: first, held: true }),
        ),
    )
    .expect("hold")
    .0;
    record = attach(&record, 18, 1).expect("held images do not count");
    assert_eq!(record.images().entries().len(), 17);
    assert_eq!(
        record.images().eligible(record.inputs().capture().expect("capture").included()).len(),
        MAX_IMAGE_COUNT
    );
    assert_eq!(
        ConversationRecord::apply(
            Some(&record),
            &operation(
                20,
                record.revision(),
                ControlIntent::Queue(QueueIntent::Hold { selected: first, held: false })
            )
        ),
        Err(ControlError::Capacity)
    );
    assert_eq!(
        ConversationRecord::parse(&record.canonical_bytes().expect("encode")).expect("decode"),
        record
    );
}

#[test]
fn aggregate_limit_and_invalid_metadata_reject_without_dropping_any_selection() {
    let mut record = create();
    for index in 2..=4 {
        record = attach(&record, index, MAX_IMAGE_BYTES).expect("aggregate boundary");
    }
    assert_eq!(attach(&record, 5, 1), Err(ControlError::Capacity));
    let valid = reference(5, 1);
    let mut invalid = valid.clone();
    invalid.width = MAX_IMAGE_SIDE + 1;
    assert_eq!(invalid.validate(), Err(ControlError::InvalidInput));
    invalid = valid.clone();
    invalid.frames = MAX_IMAGE_FRAMES + 1;
    assert_eq!(invalid.validate(), Err(ControlError::InvalidInput));
    invalid = valid.clone();
    invalid.input = InputId::new([99; 16]).expect("wrong source");
    assert_eq!(invalid.validate(), Err(ControlError::InvalidInput));
    invalid = valid;
    invalid.bytes = MAX_IMAGE_BYTES + 1;
    assert_eq!(invalid.validate(), Err(ControlError::InvalidInput));
}

#[test]
fn empty_image_state_preserves_legacy_bytes_and_reference_operation_cannot_be_rebound() {
    let record = create();
    let bytes = record.canonical_bytes().expect("encode");
    assert!(!String::from_utf8(bytes.clone()).expect("UTF-8").contains("\"images\""));
    assert_eq!(ConversationRecord::parse(&bytes).expect("legacy parse"), record);
    let wrong = operation(
        3,
        record.revision(),
        ControlIntent::AttachImage {
            image: reference(2, 1),
            text: ControlText::new("caption".to_owned()).expect("text"),
        },
    );
    assert_eq!(ConversationRecord::apply(Some(&record), &wrong), Err(ControlError::InvalidInput));
    let unknown = operation(
        4,
        record.revision(),
        ControlIntent::SelectImage { attachment: wrong.id(), selected: false },
    );
    assert_eq!(ConversationRecord::apply(Some(&record), &unknown), Err(ControlError::NotFound));
}
