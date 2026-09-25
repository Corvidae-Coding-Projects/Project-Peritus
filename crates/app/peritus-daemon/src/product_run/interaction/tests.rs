//! Public text and display-summary stream boundaries.

use super::*;

#[test]
fn thinking_summary_streams_separately_from_answer_with_split_utf8() {
    let mut options =
        InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default());
    options.summary(b"Checking ").unwrap();
    options.summary(&[0xce]).unwrap();
    options.text(b"Answer ").unwrap();
    options.summary(&[0xbb]).unwrap();
    options.summary(b" now").unwrap();
    options.text(b"complete.").unwrap();
    let summaries: String = options
        .activities
        .iter()
        .filter(|activity| activity.detail() == SUMMARY_DETAIL)
        .map(ProductActivity::text)
        .collect();
    let answer: String = options
        .activities
        .iter()
        .filter(|activity| activity.kind() == ProductActivityKind::Assistant)
        .map(ProductActivity::text)
        .collect();
    assert_eq!(summaries, "Checking λ now");
    assert_eq!(answer, "Answer complete.");
    assert!(options.pending_summary_utf8.is_empty());
}

#[test]
fn standalone_leading_whitespace_waits_for_real_assistant_text() {
    let mut options =
        InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default());

    options.text(b"\n").expect("leading whitespace");
    assert!(options.activities.is_empty());
    options.text(b"The values match.").expect("assistant text");

    assert_eq!(options.activities.len(), 1);
    assert_eq!(options.activities[0].kind(), ProductActivityKind::Assistant);
    assert_eq!(options.activities[0].text(), "\nThe values match.");
}

#[test]
fn whitespace_only_stream_is_bounded_without_a_public_empty_activity() {
    let mut options =
        InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default());

    options.text(&vec![b' '; MAX_PRODUCT_ACTIVITY_BYTES * 2]).expect("whitespace stream");

    assert!(options.activities.is_empty());
    assert!(options.pending_utf8.len() <= 3);
}

#[test]
fn invalid_provider_utf8_retains_provider_classification() {
    let mut options =
        InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default());

    let error = options.text(b"valid\xff").unwrap_err();
    let ProductRunServiceError::Context { code, retry, subsystem, operation, detail } = error
    else {
        panic!("classified provider error");
    };
    assert_eq!(code, peritus_app_protocol::AppErrorCode::MalformedFrame);
    assert_eq!(retry, peritus_app_protocol::RetryDisposition::AfterRecovery);
    assert_eq!(subsystem, peritus_app_protocol::ResponsibleSubsystem::Provider);
    assert_eq!(operation, "decode streamed assistant text");
    assert!(detail.contains("invalid UTF-8"));
}
