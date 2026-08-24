//! Incremental SSE and NDJSON framing boundary tests.

use peritus_provider_core::{
    FramingLimits, NdjsonParser, ProviderCoreErrorKind, SseItem, SseParser,
};

#[test]
fn sse_handles_fragmented_utf8_multiline_data_comments_and_done() {
    let mut parser = SseParser::new(FramingLimits::new(1024, 2048).expect("limits"));
    let source =
        "event: message\nid: provider-1\n: heartbeat\ndata: hé\ndata: llo\n\ndata: [DONE]\n\n";
    let bytes = source.as_bytes();
    let split_inside_utf8 = source.find('é').expect("unicode") + 1;
    let mut items = parser.push(&bytes[..split_inside_utf8]).expect("fragment one");
    items.extend(parser.push(&bytes[split_inside_utf8..]).expect("fragment two"));
    items.extend(parser.finish().expect("finish"));

    assert_eq!(items.len(), 3);
    match &items[0] {
        SseItem::Comment(comment) => assert_eq!(comment.as_str(), "heartbeat"),
        item => panic!("unexpected item: {item:?}"),
    }
    match &items[1] {
        SseItem::Event(frame) => {
            assert_eq!(frame.event(), Some("message"));
            assert_eq!(frame.id(), Some("provider-1"));
            assert_eq!(frame.data(), "hé\nllo");
            assert!(!format!("{frame:?}").contains("hé"));
        }
        item => panic!("unexpected item: {item:?}"),
    }
    assert_eq!(items[2], SseItem::Done);
}

#[test]
fn sse_dispatches_final_unterminated_event_and_rejects_invalid_utf8() {
    let limits = FramingLimits::new(64, 128).expect("limits");
    let mut parser = SseParser::new(limits);
    assert!(parser.push(b"data: final").expect("partial").is_empty());
    let items = parser.finish().expect("final frame");
    assert_eq!(items.len(), 1);
    match &items[0] {
        SseItem::Event(frame) => assert_eq!(frame.data(), "final"),
        item => panic!("unexpected item: {item:?}"),
    }

    let mut invalid = SseParser::new(limits);
    let error = invalid.push(b"data: \xff\n").expect_err("invalid UTF-8");
    assert_eq!(error.kind(), ProviderCoreErrorKind::MalformedStream);
}

#[test]
fn framing_limits_unterminated_lines_and_disallows_use_after_finish() {
    let limits = FramingLimits::new(8, 16).expect("limits");
    let mut parser = SseParser::new(limits);
    let error = parser.push(b"123456789").expect_err("line bound");
    assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);

    let mut parser = NdjsonParser::new(limits);
    parser.finish().expect("first finish");
    assert_eq!(
        parser.push(b"{}\n").expect_err("push after finish").kind(),
        ProviderCoreErrorKind::MalformedStream
    );
    assert_eq!(
        parser.finish().expect_err("second finish").kind(),
        ProviderCoreErrorKind::MalformedStream
    );
}

#[test]
fn ndjson_handles_crlf_empty_lines_utf8_fragments_and_final_record() {
    let mut parser = NdjsonParser::new(FramingLimits::new(128, 256).expect("limits"));
    let source = "{\"text\":\"é\"}\r\n\n{\"final\":true}";
    let bytes = source.as_bytes();
    let split_inside_utf8 = source.find('é').expect("unicode") + 1;
    let mut frames = parser.push(&bytes[..split_inside_utf8]).expect("fragment one");
    frames.extend(parser.push(&bytes[split_inside_utf8..]).expect("fragment two"));
    frames.extend(parser.finish().expect("finish"));
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].as_str(), "{\"text\":\"é\"}");
    assert_eq!(frames[1].as_str(), "{\"final\":true}");
    assert!(!format!("{:?}", frames[0]).contains("text"));
}
