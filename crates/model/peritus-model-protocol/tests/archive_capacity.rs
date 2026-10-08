//! Durable message archives preserve complete content independently of provider view size.

use peritus_model_protocol::{
    BoundedText, ContentBlock, Message, ProtocolLimits, Role, decode_messages, encode_messages,
};

#[test]
fn message_archive_round_trips_above_the_former_aggregate_byte_ceiling() {
    let limits = ProtocolLimits::PRODUCTION;
    let content = (0..5)
        .map(|_| {
            ContentBlock::Text(
                BoundedText::new("x".repeat(16 * 1024 * 1024), limits).expect("valid text"),
            )
        })
        .collect();
    let messages = vec![Message::new(Role::User, content, limits).expect("valid message")];
    let bytes = encode_messages(&messages, limits).expect("complete archive");
    assert!(bytes.len() > 64 * 1024 * 1024);
    assert_eq!(decode_messages(&bytes, limits).expect("restore archive"), messages);
}

#[test]
fn archive_field_and_history_admission_is_separate_from_provider_view_admission() {
    let limits = ProtocolLimits::ARCHIVE;
    let content = "x".repeat(ProtocolLimits::PRODUCTION.max_text_bytes() + 1);
    let message = Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new(content, limits).expect("archive text"))],
        limits,
    )
    .expect("archive message");
    let bytes = encode_messages(std::slice::from_ref(&message), limits).expect("encode");
    assert_eq!(decode_messages(&bytes, limits).expect("restore"), [message]);
    assert!(decode_messages(&bytes, ProtocolLimits::PRODUCTION).is_err());

    let small = Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new("x".to_owned(), limits).expect("text"))],
        limits,
    )
    .expect("message");
    let history = vec![small; ProtocolLimits::PRODUCTION.max_messages() + 1];
    let bytes = encode_messages(&history, limits).expect("encode full history");
    assert_eq!(decode_messages(&bytes, limits).expect("restore history"), history);
    assert!(encode_messages(&history, ProtocolLimits::PRODUCTION).is_err());
}

#[test]
fn archive_decoder_rejects_unbacked_counts_before_reserving_collection_storage() {
    let limits = ProtocolLimits::ARCHIVE;
    let mut bytes = encode_messages(&[], limits).expect("empty archive");
    bytes[6..10].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_messages(&bytes, limits).is_err());

    bytes[6..10].copy_from_slice(&1_u32.to_be_bytes());
    bytes.push(3); // User role.
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_messages(&bytes, limits).is_err());
}
