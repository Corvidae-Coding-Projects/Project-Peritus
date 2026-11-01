//! Constructive framing investigation against the actual public C5 parsers.

use crate::Metrics;
use peritus_provider_core::{
    FramingLimits, NdjsonParser, ProviderCoreError, ProviderCoreErrorKind, SseItem, SseParser,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

/// One explicit, replayable framing case. Text is serialized independently of the parser.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Case {
    /// Valid SSE blocks, including comments, ignored fields, and the done sentinel.
    Sse { records: Vec<SseRecord>, crlf: bool, final_separator: bool, chunks: Vec<u16> },
    /// Valid JSON string records; the framing layer itself does not parse JSON.
    Ndjson {
        records: Vec<String>,
        crlf: bool,
        final_newline: bool,
        blank_lines: bool,
        chunks: Vec<u16>,
    },
    /// One record immediately below, at, or above its configured byte ceiling.
    FrameBoundary {
        format: Format,
        maximum: u16,
        relation: Relation,
        crlf: bool,
        terminated: bool,
        chunks: Vec<u16>,
    },
    /// UTF-8 corruption in either a complete line or the final unterminated line.
    InvalidUtf8 { format: Format, prefix: String, terminated: bool, chunks: Vec<u16> },
    /// A single incoming chunk exceeds the parser's independent buffer ceiling.
    BufferOverflow { format: Format, maximum: u16 },
    /// Invalid limit combinations must be rejected before constructing a parser.
    InvalidLimits { kind: InvalidLimit },
}

/// Wire family exercised by a boundary or rejection case.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Format {
    /// Server-Sent Events.
    Sse,
    /// Newline-delimited records.
    Ndjson,
}

/// Relationship between a generated logical frame's size and the configured limit.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Relation {
    /// One byte below the ceiling.
    Below,
    /// Exactly at the ceiling.
    At,
    /// One byte above the ceiling.
    Above,
}

/// Explicit invalid framing limit class.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum InvalidLimit {
    /// No logical frame bytes permitted.
    ZeroFrame,
    /// No pending bytes permitted.
    ZeroBuffer,
    /// Frame ceiling larger than pending buffer ceiling.
    FrameExceedsBuffer,
    /// Frame ceiling widened beyond the public production bound.
    WidenedFrame,
    /// Pending buffer ceiling widened beyond the public production bound.
    WidenedBuffer,
}

/// Constructed SSE records with their meaning known before serialization.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SseRecord {
    /// A data-bearing event. NUL-containing IDs are explicitly ignored.
    Event { event: Option<String>, id: Option<String>, lines: Vec<String> },
    /// An immediate keepalive/comment observation.
    Comment(String),
    /// The exact conventional done sentinel.
    Done,
    /// A block without data does not produce an event or leak its metadata.
    Ignored,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Expected {
    Event { event: Option<String>, id: Option<String>, data: String },
    Comment(String),
    Done,
    Record(String),
}

/// Builds bounded cases without filters or hidden randomness in the replay body.
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let steps = max_steps.clamp(1, 64);
    let chunks = || proptest::collection::vec(0_u16..=128, 0..=steps);
    let format = || prop_oneof![Just(Format::Sse), Just(Format::Ndjson)];
    let records = proptest::collection::vec(sse_record(), 0..=steps);
    prop_oneof![
        4 => (records, any::<bool>(), any::<bool>(), chunks()).prop_map(
            |(records, crlf, final_separator, chunks)| Case::Sse {
                records, crlf, final_separator, chunks,
            },
        ),
        4 => (
            proptest::collection::vec(text(), 0..=steps),
            any::<bool>(), any::<bool>(), any::<bool>(), chunks(),
        ).prop_map(|(records, crlf, final_newline, blank_lines, chunks)| Case::Ndjson {
            records, crlf, final_newline, blank_lines, chunks,
        }),
        3 => (
            format(), 8_u16..=128,
            prop_oneof![Just(Relation::Below), Just(Relation::At), Just(Relation::Above)],
            any::<bool>(), any::<bool>(), chunks(),
        ).prop_map(|(format, maximum, relation, crlf, terminated, chunks)| Case::FrameBoundary {
            format, maximum, relation, crlf, terminated, chunks,
        }),
        2 => (format(), text(), any::<bool>(), chunks()).prop_map(
            |(format, prefix, terminated, chunks)| Case::InvalidUtf8 {
                format, prefix, terminated, chunks,
            },
        ),
        1 => (format(), 1_u16..=128).prop_map(|(format, maximum)| Case::BufferOverflow {
            format, maximum,
        }),
        1 => proptest::sample::select(vec![
            InvalidLimit::ZeroFrame, InvalidLimit::ZeroBuffer,
            InvalidLimit::FrameExceedsBuffer, InvalidLimit::WidenedFrame,
            InvalidLimit::WidenedBuffer,
        ]).prop_map(|kind| Case::InvalidLimits { kind }),
    ]
    .boxed()
}

fn text() -> BoxedStrategy<String> {
    proptest::collection::vec(
        proptest::sample::select(vec![
            'a', 'z', '0', ' ', ':', '"', '\\', '~', '\t', 'é', '中', '🙂', '\u{0301}', '\0',
        ]),
        0..=24,
    )
    .prop_map(|characters| characters.into_iter().collect())
    .boxed()
}

fn sse_record() -> BoxedStrategy<SseRecord> {
    prop_oneof![
        5 => (
            proptest::option::of(text()),
            proptest::option::of(text()),
            proptest::collection::vec(text(), 1..=4),
        ).prop_map(|(event, id, lines)| SseRecord::Event { event, id, lines }),
        2 => text().prop_map(SseRecord::Comment),
        1 => Just(SseRecord::Done),
        1 => Just(SseRecord::Ignored),
    ]
    .boxed()
}

/// Exercises a case and returns observed operation/partition counters.
pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    add(&mut metrics, "cases", 1);
    match case {
        Case::Sse { records, crlf, final_separator, chunks } => {
            let (bytes, expected) = sse_bytes(records, *crlf, *final_separator)?;
            let limits = roomy_limits(bytes.len())?;
            partitions(&mut metrics, Format::Sse, *crlf, !final_separator);
            exercise(Format::Sse, &bytes, limits, chunks, Some(&expected), None, &mut metrics)?;
            add(&mut metrics, "constructed_sse_blocks", records.len() as u64);
        }
        Case::Ndjson { records, crlf, final_newline, blank_lines, chunks } => {
            let delimiter = if *crlf { "\r\n" } else { "\n" };
            let mut wire = String::new();
            let mut expected = Vec::new();
            for (index, record) in records.iter().enumerate() {
                if *blank_lines {
                    wire.push_str(delimiter);
                }
                let encoded = serde_json::to_string(record).map_err(|error| error.to_string())?;
                expected.push(Expected::Record(encoded.clone()));
                wire.push_str(&encoded);
                if index + 1 < records.len() || *final_newline {
                    wire.push_str(delimiter);
                }
            }
            let limits = roomy_limits(wire.len())?;
            partitions(&mut metrics, Format::Ndjson, *crlf, !final_newline);
            exercise(
                Format::Ndjson,
                wire.as_bytes(),
                limits,
                chunks,
                Some(&expected),
                None,
                &mut metrics,
            )?;
            if *blank_lines {
                add(&mut metrics, "blank_line_cases", 1);
            }
        }
        Case::FrameBoundary { format, maximum, relation, crlf, terminated, chunks } => {
            if !(8..=4096).contains(maximum) {
                return Err("invalid replay: boundary maximum must be 8 through 4096".into());
            }
            let maximum = usize::from(*maximum);
            let length = match relation {
                Relation::Below => maximum - 1,
                Relation::At => maximum,
                Relation::Above => maximum + 1,
            };
            let payload = "a".repeat(length - if matches!(format, Format::Sse) { 5 } else { 0 });
            let mut wire = match format {
                Format::Sse => format!("data:{payload}"),
                Format::Ndjson => payload.clone(),
            };
            if *terminated {
                wire.push_str(if *crlf { "\r\n" } else { "\n" });
                if matches!(format, Format::Sse) {
                    wire.push_str(if *crlf { "\r\n" } else { "\n" });
                }
            } else if *crlf {
                // An EOF after CR exercises the same optional terminal CR rule as CRLF.
                wire.push('\r');
            }
            let limits = FramingLimits::new(maximum, wire.len().max(maximum) + 16)
                .map_err(|error| error.to_string())?;
            let expected = match format {
                Format::Sse => vec![Expected::Event { event: None, id: None, data: payload }],
                Format::Ndjson => vec![Expected::Record(payload)],
            };
            let rejected = matches!(relation, Relation::Above);
            partitions(&mut metrics, *format, *crlf, !terminated);
            add(
                &mut metrics,
                match relation {
                    Relation::Below => "frame_limit_below",
                    Relation::At => "frame_limit_at",
                    Relation::Above => "frame_limit_above",
                },
                1,
            );
            exercise(
                *format,
                wire.as_bytes(),
                limits,
                chunks,
                (!rejected).then_some(expected.as_slice()),
                rejected.then_some(ProviderCoreErrorKind::LimitExceeded),
                &mut metrics,
            )?;
        }
        Case::InvalidUtf8 { format, prefix, terminated, chunks } => {
            if prefix.contains(['\r', '\n']) {
                return Err(
                    "invalid replay: malformed-line prefix contains a line delimiter".into()
                );
            }
            let mut bytes =
                if matches!(format, Format::Sse) { b"data: ".to_vec() } else { Vec::new() };
            bytes.extend_from_slice(prefix.as_bytes());
            bytes.push(0xc3); // A leading UTF-8 byte with its required continuation absent.
            if *terminated {
                bytes.push(b'\n');
            }
            let limits = roomy_limits(bytes.len())?;
            partitions(&mut metrics, *format, false, !terminated);
            add(&mut metrics, "invalid_utf8_cases", 1);
            exercise(
                *format,
                &bytes,
                limits,
                chunks,
                None,
                Some(ProviderCoreErrorKind::MalformedStream),
                &mut metrics,
            )?;
        }
        Case::BufferOverflow { format, maximum } => {
            let maximum = usize::from(*maximum);
            let limits = FramingLimits::new(maximum, maximum).map_err(|error| error.to_string())?;
            // Newlines avoid accidentally testing a logical frame ceiling instead.
            let bytes = vec![b'\n'; maximum + 1];
            let mut parser = Parser::new(*format, limits);
            expect_kind(
                parser.push(&bytes),
                ProviderCoreErrorKind::LimitExceeded,
                "buffer overflow",
            )?;
            add(&mut metrics, "buffer_overflow_rejections", 1);
            let mut parser = Parser::new(*format, limits);
            if !parser.push(&bytes[..maximum]).map_err(|error| error.to_string())?.is_empty()
                || !parser.finish().map_err(|error| error.to_string())?.is_empty()
            {
                return Err("empty lines at the buffer ceiling produced output".into());
            }
            add(&mut metrics, "buffer_exact_acceptances", 1);
        }
        Case::InvalidLimits { kind } => {
            let production = FramingLimits::PRODUCTION;
            let (frame, buffer) = match kind {
                InvalidLimit::ZeroFrame => (0, 1),
                InvalidLimit::ZeroBuffer => (1, 0),
                InvalidLimit::FrameExceedsBuffer => (2, 1),
                InvalidLimit::WidenedFrame => {
                    (production.max_frame_bytes() + 1, production.max_buffer_bytes())
                }
                InvalidLimit::WidenedBuffer => (1, production.max_buffer_bytes() + 1),
            };
            expect_kind(
                FramingLimits::new(frame, buffer),
                ProviderCoreErrorKind::LimitExceeded,
                "invalid limits",
            )?;
            add(&mut metrics, "invalid_limits_rejections", 1);
        }
    }
    Ok(metrics)
}

fn sse_bytes(
    records: &[SseRecord],
    crlf: bool,
    final_separator: bool,
) -> Result<(Vec<u8>, Vec<Expected>), String> {
    let delimiter = if crlf { "\r\n" } else { "\n" };
    let mut wire = String::new();
    let mut expected = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let mut lines = Vec::new();
        match record {
            SseRecord::Event { event, id, lines: data } => {
                if data.is_empty()
                    || data
                        .iter()
                        .chain(event.iter())
                        .chain(id.iter())
                        .any(|line| line.contains(['\n', '\r']))
                {
                    return Err(
                        "invalid replay: SSE event needs data lines without line delimiters".into(),
                    );
                }
                if let Some(event) = event {
                    lines.push(format!("event: {event}"));
                }
                if let Some(id) = id {
                    lines.push(format!("id: {id}"));
                }
                lines.extend(data.iter().map(|line| format!("data: {line}")));
                expected.push(Expected::Event {
                    event: event.clone(),
                    id: id.clone().filter(|value| !value.contains('\0')),
                    data: data.join("\n"),
                });
            }
            SseRecord::Comment(comment) => {
                if comment.contains(['\n', '\r']) {
                    return Err("invalid replay: comment contains a line delimiter".into());
                }
                lines.push(format!(": {comment}"));
                expected.push(Expected::Comment(comment.clone()));
            }
            SseRecord::Done => {
                lines.push("data: [DONE]".into());
                expected.push(Expected::Done);
            }
            SseRecord::Ignored => {
                lines.extend([
                    "event: discarded".into(),
                    "id: discarded".into(),
                    "retry: 123".into(),
                ]);
            }
        }
        wire.push_str(&lines.join(delimiter));
        if index + 1 < records.len() || final_separator {
            wire.push_str(delimiter);
            wire.push_str(delimiter);
        }
    }
    Ok((wire.into_bytes(), expected))
}

fn roomy_limits(bytes: usize) -> Result<FramingLimits, String> {
    let maximum = bytes.checked_add(32).ok_or("case byte bound overflow")?.max(32);
    FramingLimits::new(maximum, maximum).map_err(|error| error.to_string())
}

fn exercise(
    format: Format,
    bytes: &[u8],
    limits: FramingLimits,
    chunks: &[u16],
    expected: Option<&[Expected]>,
    rejection: Option<ProviderCoreErrorKind>,
    metrics: &mut Metrics,
) -> Result<(), String> {
    // Every schedule has a buffer allowance at least as large as this entire wire.
    // The explicit buffer-overflow family tests the different pre-drain admission contract.
    let mut schedules = vec![vec![bytes.len()], vec![1; bytes.len()]];
    schedules.push(chunks.iter().map(|size| usize::from(*size)).collect());
    if bytes.len() <= 128 {
        schedules.extend((0..=bytes.len()).map(|split| vec![split, bytes.len() - split]));
    }
    for (schedule_index, schedule) in schedules.iter().enumerate() {
        let mut parser = Parser::new(format, limits);
        let mut actual = Vec::new();
        let mut position: usize = 0;
        let mut failed = None;
        for size in schedule.iter().copied().chain(std::iter::once(bytes.len())) {
            let end = position.saturating_add(size).min(bytes.len());
            if position < end && end < bytes.len() {
                if bytes.get(end.wrapping_sub(1)) == Some(&b'\r') && bytes.get(end) == Some(&b'\n')
                {
                    add(metrics, "crlf_split_points", 1);
                }
                if bytes[end] & 0xc0 == 0x80 {
                    add(metrics, "utf8_split_points", 1);
                }
            }
            add(metrics, "pushes", 1);
            add(metrics, "bytes_fed", (end - position) as u64);
            if position == end {
                add(metrics, "empty_pushes", 1);
            }
            match parser.push(&bytes[position..end]) {
                Ok(items) => actual.extend(items),
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
            position = end;
        }
        if failed.is_none() {
            add(metrics, "finishes", 1);
            match parser.finish() {
                Ok(items) => actual.extend(items),
                Err(error) => failed = Some(error),
            }
        }
        add(metrics, "schedules", 1);
        match (failed, rejection) {
            (Some(error), Some(kind)) if error.kind() == kind => {
                add(metrics, "expected_rejections", 1);
            }
            (Some(error), _) => {
                return Err(format!("schedule {schedule_index} unexpectedly rejected: {error}"));
            }
            (None, Some(kind)) => {
                return Err(format!(
                    "schedule {schedule_index} accepted a case requiring {kind:?}"
                ));
            }
            (None, None) => {
                let expected = expected.ok_or("harness omitted its constructed oracle")?;
                if actual != expected {
                    return Err(format!(
                        "schedule {schedule_index}: expected {expected:?}; actual {actual:?}"
                    ));
                }
                add(metrics, "emitted_items", actual.len() as u64);
                for item in &actual {
                    add(
                        metrics,
                        match item {
                            Expected::Event { .. } => "emitted_sse_events",
                            Expected::Comment(_) => "emitted_sse_comments",
                            Expected::Done => "emitted_sse_done",
                            Expected::Record(_) => "emitted_ndjson_records",
                        },
                        1,
                    );
                }
                add(metrics, "successful_schedules", 1);
                expect_kind(
                    parser.finish(),
                    ProviderCoreErrorKind::MalformedStream,
                    "repeat finish",
                )?;
                expect_kind(
                    parser.push(&[]),
                    ProviderCoreErrorKind::MalformedStream,
                    "push after finish",
                )?;
                add(metrics, "post_finish_rejections", 2);
            }
        }
    }
    Ok(())
}

enum Parser {
    Sse(SseParser),
    Ndjson(NdjsonParser),
}
impl Parser {
    fn new(format: Format, limits: FramingLimits) -> Self {
        match format {
            Format::Sse => Self::Sse(SseParser::new(limits)),
            Format::Ndjson => Self::Ndjson(NdjsonParser::new(limits)),
        }
    }
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Expected>, ProviderCoreError> {
        match self {
            Self::Sse(parser) => parser.push(bytes).map(sse_items),
            Self::Ndjson(parser) => parser.push(bytes).map(|items| {
                items.into_iter().map(|item| Expected::Record(item.as_str().into())).collect()
            }),
        }
    }
    fn finish(&mut self) -> Result<Vec<Expected>, ProviderCoreError> {
        match self {
            Self::Sse(parser) => parser.finish().map(sse_items),
            Self::Ndjson(parser) => parser.finish().map(|items| {
                items.into_iter().map(|item| Expected::Record(item.as_str().into())).collect()
            }),
        }
    }
}
fn sse_items(items: Vec<SseItem>) -> Vec<Expected> {
    items
        .into_iter()
        .map(|item| match item {
            SseItem::Event(frame) => Expected::Event {
                event: frame.event().map(str::to_owned),
                id: frame.id().map(str::to_owned),
                data: frame.data().into(),
            },
            SseItem::Comment(comment) => Expected::Comment(comment.as_str().into()),
            SseItem::Done => Expected::Done,
        })
        .collect()
}
fn expect_kind<T>(
    result: Result<T, ProviderCoreError>,
    kind: ProviderCoreErrorKind,
    label: &str,
) -> Result<(), String> {
    match result {
        Err(error) if error.kind() == kind => Ok(()),
        Err(error) => Err(format!("{label}: expected {kind:?}, got {error}")),
        Ok(_) => Err(format!("{label}: expected {kind:?}, got success")),
    }
}
fn partitions(metrics: &mut Metrics, format: Format, crlf: bool, unterminated: bool) {
    add(
        metrics,
        match format {
            Format::Sse => "sse_cases",
            Format::Ndjson => "ndjson_cases",
        },
        1,
    );
    add(metrics, if crlf { "crlf_cases" } else { "lf_cases" }, 1);
    if unterminated {
        add(metrics, "unterminated_cases", 1);
    }
}
fn add(metrics: &mut Metrics, name: &str, value: u64) {
    *metrics.entry(name.into()).or_default() += value;
}

/// Deterministic examples force every operation and boundary class into a smoke run.
pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        Case::Sse {
            records: vec![
                SseRecord::Ignored,
                SseRecord::Comment("alive🙂".into()),
                SseRecord::Event {
                    event: Some("message".into()),
                    id: Some("nul\0ignored".into()),
                    lines: vec!["héllo".into(), "中🙂".into()],
                },
                SseRecord::Done,
            ],
            crlf: true,
            final_separator: false,
            chunks: vec![1, 0, 2, 3, 0, 1],
        },
        Case::Sse { records: Vec::new(), crlf: false, final_separator: true, chunks: vec![0] },
        Case::Ndjson {
            records: vec!["quote\" slash\\ é🙂".into(), String::new()],
            crlf: true,
            final_newline: false,
            blank_lines: true,
            chunks: vec![0, 1, 2, 3],
        },
    ];
    for format in [Format::Sse, Format::Ndjson] {
        for relation in [Relation::Below, Relation::At, Relation::Above] {
            for crlf in [false, true] {
                for terminated in [false, true] {
                    cases.push(Case::FrameBoundary {
                        format,
                        maximum: 8,
                        relation,
                        crlf,
                        terminated,
                        chunks: vec![1, 0, 2],
                    });
                }
            }
        }
        for terminated in [false, true] {
            cases.push(Case::InvalidUtf8 {
                format,
                prefix: "🙂".into(),
                terminated,
                chunks: vec![1, 2, 0, 1],
            });
        }
        cases.push(Case::BufferOverflow { format, maximum: 8 });
    }
    for kind in [
        InvalidLimit::ZeroFrame,
        InvalidLimit::ZeroBuffer,
        InvalidLimit::FrameExceedsBuffer,
        InvalidLimit::WidenedFrame,
        InvalidLimit::WidenedBuffer,
    ] {
        cases.push(Case::InvalidLimits { kind });
    }
    cases
}

/// Witness for the retained NDJSON frame-limit mutation canary.
pub fn calibration_case() -> Case {
    Case::FrameBoundary {
        format: Format::Ndjson,
        maximum: 8,
        relation: Relation::Above,
        crlf: true,
        terminated: true,
        chunks: vec![],
    }
}
