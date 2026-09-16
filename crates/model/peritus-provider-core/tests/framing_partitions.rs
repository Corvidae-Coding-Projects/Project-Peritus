//! Chunk boundaries must not change admission of an otherwise bounded frame.

use peritus_provider_core::{
    FramingLimits, NdjsonParser, ProviderCoreErrorKind, SseItem, SseParser,
};

#[test]
fn ndjson_exact_limit_crlf_survives_every_split() {
    for record in ["0", "{}", "{\"text\":\"é\"}"] {
        let source = format!("{record}\r\n");
        let limits = FramingLimits::new(record.len(), source.len()).expect("limits");
        for split in 0..=source.len() {
            let mut parser = NdjsonParser::new(limits);
            let mut frames = parser.push(&source.as_bytes()[..split]).expect("first chunk");
            frames.extend(parser.push(&source.as_bytes()[split..]).expect("second chunk"));
            frames.extend(parser.finish().expect("finish"));
            assert_eq!(frames.len(), 1, "record={record:?}, split={split}");
            assert_eq!(frames[0].as_str(), record);
        }
    }
}

#[test]
fn sse_exact_limit_crlf_survives_every_split() {
    for data in ["", "x", "é"] {
        let line = format!("data: {data}");
        let source = format!("{line}\r\n\r\n");
        let limits = FramingLimits::new(line.len(), source.len()).expect("limits");
        for split in 0..=source.len() {
            let mut parser = SseParser::new(limits);
            let mut items = parser.push(&source.as_bytes()[..split]).expect("first chunk");
            items.extend(parser.push(&source.as_bytes()[split..]).expect("second chunk"));
            items.extend(parser.finish().expect("finish"));
            assert_eq!(items.len(), 1, "data={data:?}, split={split}");
            let SseItem::Event(frame) = &items[0] else { panic!("expected data event") };
            assert_eq!(frame.data(), data);
        }
    }
}

#[test]
fn pending_carriage_return_does_not_admit_over_limit_content() {
    let limits = FramingLimits::new(2, 8).expect("limits");
    let mut parser = NdjsonParser::new(limits);
    assert!(parser.push(b"{}\r").expect("possible delimiter").is_empty());
    let error = parser.push(b"x\n").expect_err("interior CR is content");
    assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);

    let limits = FramingLimits::new(7, 16).expect("limits");
    let mut parser = SseParser::new(limits);
    assert!(parser.push(b"data: x\r").expect("possible delimiter").is_empty());
    let error = parser.push(b"x\n\n").expect_err("interior CR is content");
    assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);
}

#[test]
fn carriage_return_allowance_does_not_widen_buffer_limit() {
    let limits = FramingLimits::new(2, 2).expect("limits");
    let error = NdjsonParser::new(limits).push(b"{}\r").expect_err("hard buffer limit");
    assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);
    let limits = FramingLimits::new(7, 7).expect("limits");
    let error = SseParser::new(limits).push(b"data: x\r").expect_err("hard buffer limit");
    assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);
}

#[test]
fn final_carriage_return_is_trimmed_consistently() {
    let mut parser = NdjsonParser::new(FramingLimits::new(2, 3).expect("limits"));
    assert!(parser.push(b"{}\r").expect("terminal chunk").is_empty());
    assert_eq!(parser.finish().expect("finish")[0].as_str(), "{}");
    let mut parser = SseParser::new(FramingLimits::new(7, 8).expect("limits"));
    assert!(parser.push(b"data: x\r").expect("terminal chunk").is_empty());
    let items = parser.finish().expect("finish");
    let SseItem::Event(frame) = &items[0] else { panic!("expected data event") };
    assert_eq!(frame.data(), "x");
}
