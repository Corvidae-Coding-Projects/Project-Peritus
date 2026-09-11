//! Metamorphic checks retain production admission and terminal semantics.

use peritus_provider_core::{FramingLimits, NdjsonParser, ProviderCoreErrorKind, SseParser};

fn input(bytes: &[u8]) -> (FramingLimits, usize, &[u8]) {
    let frame = usize::from(bytes.first().copied().unwrap_or(0) % 64) + 1;
    let width = usize::from(bytes.get(1).copied().unwrap_or(0) % 64) + 1;
    let wire = bytes.get(2..).unwrap_or_default();
    (FramingLimits::new(frame, super::MAX_INPUT_BYTES).expect("bounded limits"), width, wire)
}

/// Checks SSE parsing against alternate chunk partitions.
pub fn sse(bytes: &[u8]) {
    sse_regression_seed();
    let (limits, width, wire) = input(bytes);
    let mut whole = SseParser::new(limits);
    let expected = whole.push(wire).and_then(|mut items| {
        items.extend(whole.finish()?);
        Ok(items)
    });
    let mut chunked = SseParser::new(limits);
    let observed = wire
        .chunks(width)
        .try_fold(Vec::new(), |mut items, chunk| {
            items.extend(chunked.push(chunk)?);
            Ok::<_, peritus_provider_core::ProviderCoreError>(items)
        })
        .and_then(|mut items| {
            items.extend(chunked.finish()?);
            Ok(items)
        });
    let expected = expected.map_err(|error| error.kind());
    let observed = observed.map_err(|error| error.kind());
    assert_eq!(observed, expected, "SSE result changed across chunk partition");
    if expected.is_ok() {
        assert_eq!(
            chunked.finish().expect_err("terminal is single-use").kind(),
            ProviderCoreErrorKind::MalformedStream
        );
        assert_eq!(
            chunked.push(b"data: late\n\n").expect_err("closed stream rejects input").kind(),
            ProviderCoreErrorKind::MalformedStream
        );
    }
}

/// Checks NDJSON parsing against alternate chunk partitions.
pub fn ndjson(bytes: &[u8]) {
    ndjson_regression_seed();
    let (limits, width, wire) = input(bytes);
    let mut whole = NdjsonParser::new(limits);
    let expected = whole.push(wire).and_then(|mut items| {
        items.extend(whole.finish()?);
        Ok(items)
    });
    let mut chunked = NdjsonParser::new(limits);
    let observed = wire
        .chunks(width)
        .try_fold(Vec::new(), |mut items, chunk| {
            items.extend(chunked.push(chunk)?);
            Ok::<_, peritus_provider_core::ProviderCoreError>(items)
        })
        .and_then(|mut items| {
            items.extend(chunked.finish()?);
            Ok(items)
        });
    let expected = expected.map_err(|error| error.kind());
    let observed = observed.map_err(|error| error.kind());
    assert_eq!(observed, expected, "NDJSON result changed across chunk partition");
    if expected.is_ok() {
        assert_eq!(
            chunked.finish().expect_err("terminal is single-use").kind(),
            ProviderCoreErrorKind::MalformedStream
        );
        assert_eq!(
            chunked.push(b"{}\n").expect_err("closed stream rejects input").kind(),
            ProviderCoreErrorKind::MalformedStream
        );
    }
}

fn sse_regression_seed() {
    let limits = FramingLimits::new(7, 16).expect("regression limits");
    let mut parser = SseParser::new(limits);
    assert!(parser.push(b"data: x\r").expect("split CRLF prefix").is_empty());
    let items = parser.push(b"\n\r\n").expect("split CRLF suffix");
    assert_eq!(items.len(), 1, "exact-limit SSE was lost across split CRLF");
}

fn ndjson_regression_seed() {
    let limits = FramingLimits::new(2, 8).expect("regression limits");
    let mut parser = NdjsonParser::new(limits);
    assert!(parser.push(b"{}\r").expect("split CRLF prefix").is_empty());
    let frames = parser.push(b"\n").expect("split CRLF suffix");
    assert_eq!(frames.len(), 1, "exact-limit NDJSON was lost across split CRLF");
}
