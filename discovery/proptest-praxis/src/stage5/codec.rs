//! Public canonical codec with independent byte lists, cursors and boundary arithmetic.
use crate::Metrics;
use peritus_codec::{
    CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind as Kind, CodecLimit as Limit,
    CodecLimits, decode_frame, encode_frame,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Frame {
        family: u16,
        schema: u16,
        payload: Vec<u8>,
        frame_limit: u16,
        payload_limit: u16,
        fault: u8,
    },
    Read {
        input: Vec<u8>,
        operations: Vec<Read>,
        bound: u8,
    },
    Write {
        operations: Vec<Value>,
        bound: u8,
    },
    ReadValues {
        values: Vec<Value>,
        bound: u8,
        cut: u16,
    },
    Nested {
        depth: u8,
        limit: u8,
        fail: bool,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Read {
    U8,
    U16,
    U32,
    U64,
    Fixed,
    Bool,
    Option,
    Bytes,
    String,
    Collection,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Value {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Fixed([u8; 3]),
    Bool(bool),
    Option(bool),
    Bytes(Vec<u8>),
    String(String),
    Collection(u16),
}
fn bump(m: &mut Metrics, key: &str) {
    *m.entry(key.into()).or_default() += 1;
}
fn text() -> BoxedStrategy<String> {
    prop_oneof![
        Just(String::new()),
        Just("é👩‍💻\r\n".into()),
        prop::collection::vec(any::<char>(), 0..12).prop_map(|v| v.into_iter().collect())
    ]
    .boxed()
}
fn read_strategy() -> BoxedStrategy<Read> {
    prop::sample::select(vec![
        Read::U8,
        Read::U16,
        Read::U32,
        Read::U64,
        Read::Fixed,
        Read::Bool,
        Read::Option,
        Read::Bytes,
        Read::String,
        Read::Collection,
    ])
    .boxed()
}
fn value_strategy() -> BoxedStrategy<Value> {
    prop_oneof![
        any::<u8>().prop_map(Value::U8),
        any::<u16>().prop_map(Value::U16),
        any::<u32>().prop_map(Value::U32),
        any::<u64>().prop_map(Value::U64),
        any::<[u8; 3]>().prop_map(Value::Fixed),
        any::<bool>().prop_map(Value::Bool),
        any::<bool>().prop_map(Value::Option),
        prop::collection::vec(any::<u8>(), 0..40).prop_map(Value::Bytes),
        text().prop_map(Value::String),
        (0_u16..80).prop_map(Value::Collection)
    ]
    .boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let steps = max_steps.min(64);
    prop_oneof![
        4=>(any::<u16>(),any::<u16>(),prop::collection::vec(any::<u8>(),0..96),0_u16..160,0_u16..112,0_u8..9).prop_map(|(family,schema,payload,frame_limit,payload_limit,fault)|Case::Frame{family,schema,payload,frame_limit,payload_limit,fault}),
        4=>(prop::collection::vec(any::<u8>(),0..128),prop::collection::vec(read_strategy(),0..=steps),0_u8..80).prop_map(|(input,operations,bound)|Case::Read{input,operations,bound}),
        5=>(prop::collection::vec(value_strategy(),0..=steps),0_u8..80).prop_map(|(operations,bound)|Case::Write{operations,bound}),
        4=>(prop::collection::vec(value_strategy(),0..=steps.min(8)),0_u8..80,prop_oneof![3=>Just(0_u16),1=>1_u16..80]).prop_map(|(values,bound,cut)|Case::ReadValues{values,bound,cut}),
        2=>(0_u8..12,0_u8..10,any::<bool>()).prop_map(|(depth,limit,fail)|Case::Nested{depth,limit,fail}),
        2=>(any::<u8>(),0_u8..80).prop_map(|(tag,bound)|Case::Read{input:vec![tag,1,0,255],operations:vec![Read::Bool,Read::Option,Read::U8],bound}),
    ].boxed()
}
fn limits(bound: usize) -> CodecLimits {
    CodecLimits::new(bound + 16, bound, bound, bound, bound, 8)
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Failure {
    kind: Kind,
    offset: usize,
    limit: Option<Limit>,
}
impl Failure {
    fn simple(kind: Kind, offset: usize) -> Self {
        Self { kind, offset, limit: None }
    }
    fn limited(limit: Limit, offset: usize) -> Self {
        Self { kind: Kind::LimitExceeded, offset, limit: Some(limit) }
    }
    fn actual(value: CodecError) -> Self {
        Self { kind: value.kind(), offset: value.offset(), limit: value.limit() }
    }
}
fn require_error<T: std::fmt::Debug>(
    actual: Result<T, CodecError>,
    expected: Failure,
) -> Result<(), String> {
    match actual {
        Err(error) if Failure::actual(error) == expected => Ok(()),
        other => Err(format!("expected {expected:?}, observed {other:?}")),
    }
}
fn raw_frame(family: u16, schema: u16, payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = b"PRTS".to_vec();
    for value in [1_u16, family, schema, 0] {
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend(u32::try_from(payload.len()).map_err(|e| e.to_string())?.to_be_bytes());
    bytes.extend(payload);
    Ok(bytes)
}
fn frame(case: &Case, m: &mut Metrics) -> Result<(), String> {
    let Case::Frame { family, schema, payload, frame_limit, payload_limit, fault } = case else {
        return Err("frame case mismatch".into());
    };
    if payload.len() > 128 {
        return Err("setup-incomplete: codec payload bound".into());
    }
    let mut policy = limits(128);
    policy.max_frame_bytes = usize::from(*frame_limit);
    policy.max_payload_bytes = usize::from(*payload_limit);
    let expected = if *family == 0 {
        Some(Failure::simple(Kind::InvalidFamily, 6))
    } else if *schema == 0 {
        Some(Failure::simple(Kind::InvalidSchemaVersion, 8))
    } else if payload.len() > policy.max_payload_bytes {
        Some(Failure::limited(Limit::PayloadBytes, 12))
    } else if payload.len() + 16 > policy.max_frame_bytes {
        Some(Failure::limited(Limit::FrameBytes, 0))
    } else {
        None
    };
    let encoded = encode_frame(*family, *schema, payload, policy);
    if let Some(expected) = expected {
        require_error(encoded, expected)?;
        bump(m, "frame_encoding_rejections");
    } else if encoded.map_err(|e| e.to_string())? != raw_frame(*family, *schema, payload)? {
        return Err("frame bytes differ from independent header/payload".into());
    } else {
        bump(m, "exact_frame_encodings");
    }
    // Isolate malformed grammar from configured-capacity failures with a generous decode policy.
    let mut bytes = raw_frame((*family).max(1), (*schema).max(1), payload)?;
    let expected = match fault % 9 {
        0 => None,
        1 => {
            bytes[0] ^= 1;
            Some(Failure::simple(Kind::InvalidMagic, 0))
        }
        2 => {
            bytes[4..6].copy_from_slice(&2_u16.to_be_bytes());
            Some(Failure::simple(Kind::UnsupportedFormatVersion, 4))
        }
        3 => {
            bytes[6..8].fill(0);
            Some(Failure::simple(Kind::InvalidFamily, 6))
        }
        4 => {
            bytes[8..10].fill(0);
            Some(Failure::simple(Kind::InvalidSchemaVersion, 8))
        }
        5 => {
            bytes[10] = 1;
            Some(Failure::simple(Kind::NonzeroFlags, 10))
        }
        6 => {
            let cut = usize::from(*family) % bytes.len();
            bytes.truncate(cut);
            Some(Failure::simple(Kind::Truncated, cut))
        }
        7 => {
            let length = bytes.len();
            bytes.push(17);
            Some(Failure::simple(Kind::TrailingBytes, length))
        }
        _ => {
            bytes[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
            Some(Failure::limited(Limit::PayloadBytes, 12))
        }
    };
    let decoded = decode_frame(&bytes, limits(256));
    if let Some(expected) = &expected {
        require_error(decoded, expected.clone())?;
        bump(m, &format!("malformed_frame_partition_{}", fault % 9));
    } else {
        let actual = decoded.map_err(|e| e.to_string())?;
        if actual.header().family() != (*family).max(1)
            || actual.header().schema_version() != (*schema).max(1)
            || actual.payload() != payload
        {
            return Err("decoded header/payload lost independent input".into());
        }
        bump(m, "exact_frame_decodings");
    }
    for (capacity, good) in
        [(bytes.len().saturating_sub(1), false), (bytes.len(), true), (bytes.len() + 1, true)]
    {
        let mut bounded = limits(256);
        bounded.max_frame_bytes = capacity;
        let actual = decode_frame(&bytes, bounded);
        if !good && bytes.is_empty() {
            // Zero has no representable one-byte-short ceiling; the empty header still rejects.
            require_error(actual, Failure::simple(Kind::Truncated, 0))?;
            bump(m, "empty_frame_header_rejections");
        } else if !good {
            require_error(actual, Failure::limited(Limit::FrameBytes, 0))?;
            bump(m, "frame_capacity_one_short");
        } else if let Some(expected) = &expected {
            require_error(actual, expected.clone())?;
        } else {
            let actual = actual.map_err(|e| e.to_string())?;
            if actual.payload() != payload {
                return Err("frame exact/spare capacity lost payload".into());
            }
            bump(m, "frame_capacity_exact_or_spare");
        }
        // Malformed inputs retain their grammar error when complete-frame capacity is sufficient.
    }
    Ok(())
}
#[derive(Debug, Eq, PartialEq)]
enum Observed {
    Number(u64),
    Boolean(bool),
    Bytes(Vec<u8>),
    Text(String),
}
fn observed(reader: &mut CanonicalReader<'_>, op: Read) -> Result<Observed, CodecError> {
    Ok(match op {
        Read::U8 => Observed::Number(u64::from(reader.read_u8()?)),
        Read::U16 => Observed::Number(u64::from(reader.read_u16()?)),
        Read::U32 => Observed::Number(u64::from(reader.read_u32()?)),
        Read::U64 => Observed::Number(reader.read_u64()?),
        Read::Fixed => Observed::Bytes(reader.read_fixed::<3>()?.to_vec()),
        Read::Bool => Observed::Boolean(reader.read_bool()?),
        Read::Option => Observed::Boolean(reader.read_option_tag()?),
        Read::Bytes => Observed::Bytes(reader.read_bytes_owned()?),
        Read::String => Observed::Text(reader.read_str()?.into()),
        Read::Collection => Observed::Number(
            u64::try_from(reader.read_collection_len()?)
                .map_err(|_| CodecError::at(Kind::LengthOverflow, reader.offset()))?,
        ),
    })
}
fn take<'a>(input: &'a [u8], offset: &mut usize, width: usize) -> Result<&'a [u8], Failure> {
    if input.len().saturating_sub(*offset) < width {
        return Err(Failure::simple(Kind::Truncated, *offset));
    }
    let start = *offset;
    *offset += width;
    Ok(&input[start..*offset])
}
fn number(input: &[u8], offset: &mut usize, width: usize) -> Result<u64, Failure> {
    Ok(take(input, offset, width)?.iter().fold(0_u64, |n, byte| (n << 8) | u64::from(*byte)))
}
fn expected_read(
    input: &[u8],
    offset: &mut usize,
    op: Read,
    bound: usize,
) -> Result<Observed, Failure> {
    let start = *offset;
    match op {
        Read::U8 => number(input, offset, 1).map(Observed::Number),
        Read::U16 => number(input, offset, 2).map(Observed::Number),
        Read::U32 => number(input, offset, 4).map(Observed::Number),
        Read::U64 => number(input, offset, 8).map(Observed::Number),
        Read::Fixed => take(input, offset, 3).map(|v| Observed::Bytes(v.to_vec())),
        Read::Bool | Read::Option => match number(input, offset, 1)? {
            0 => Ok(Observed::Boolean(false)),
            1 => Ok(Observed::Boolean(true)),
            _ => Err(Failure::simple(
                if matches!(op, Read::Bool) { Kind::InvalidBoolean } else { Kind::InvalidOption },
                start,
            )),
        },
        Read::Collection => {
            let n = number(input, offset, 4)?;
            if n > u64::try_from(bound).map_err(|_| Failure::simple(Kind::LengthOverflow, start))? {
                Err(Failure::limited(Limit::CollectionItems, start))
            } else {
                Ok(Observed::Number(n))
            }
        }
        Read::Bytes | Read::String => {
            let n = usize::try_from(number(input, offset, 4)?)
                .map_err(|_| Failure::simple(Kind::LengthOverflow, start))?;
            if n > bound {
                return Err(Failure::limited(
                    if matches!(op, Read::Bytes) { Limit::OpaqueBytes } else { Limit::StringBytes },
                    start,
                ));
            }
            let bytes = take(input, offset, n)?;
            if matches!(op, Read::Bytes) {
                Ok(Observed::Bytes(bytes.to_vec()))
            } else {
                std::str::from_utf8(bytes)
                    .map(|s| Observed::Text(s.into()))
                    .map_err(|_| Failure::simple(Kind::InvalidUtf8, start + 4))
            }
        }
    }
}
fn reads(input: &[u8], operations: &[Read], bound: u8, m: &mut Metrics) -> Result<(), String> {
    if input.len() > 1024 || operations.len() > 64 {
        return Err("setup-incomplete: codec reader bounds".into());
    }
    let mut reader = CanonicalReader::new(input, limits(usize::from(bound)));
    let mut offset = 0;
    for op in operations {
        let expected = expected_read(input, &mut offset, *op, usize::from(bound));
        let actual = observed(&mut reader, *op).map_err(Failure::actual);
        if actual != expected
            || reader.offset() != offset
            || reader.remaining() != input.len() - offset
        {
            return Err(format!(
                "independent cursor/value differs at {op:?}: expected {expected:?} offset{offset}, actual{actual:?} offset{}",
                reader.offset()
            ));
        }
        bump(m, if actual.is_ok() { "primitive_reads" } else { "primitive_read_rejections" });
    }
    if offset == input.len() {
        reader.finish().map_err(|e| e.to_string())?;
    } else {
        require_error(reader.finish(), Failure::simple(Kind::TrailingBytes, offset))?;
        bump(m, "unconsumed_primitive_tails");
    }
    Ok(())
}
fn value_bytes(value: &Value) -> Result<Vec<u8>, String> {
    if matches!(value,Value::Bytes(bytes) if bytes.len()>1024)
        || matches!(value,Value::String(text) if text.len()>1024)
    {
        return Err("setup-incomplete: primitive field representation bound".into());
    }
    Ok(match value {
        Value::U8(v) => vec![*v],
        Value::U16(v) => v.to_be_bytes().to_vec(),
        Value::U32(v) => v.to_be_bytes().to_vec(),
        Value::U64(v) => v.to_be_bytes().to_vec(),
        Value::Fixed(v) => v.to_vec(),
        Value::Bool(v) | Value::Option(v) => vec![u8::from(*v)],
        Value::Bytes(v) => {
            [u32::try_from(v.len()).map_err(|e| e.to_string())?.to_be_bytes().as_slice(), v]
                .concat()
        }
        Value::String(v) => [
            u32::try_from(v.len()).map_err(|e| e.to_string())?.to_be_bytes().as_slice(),
            v.as_bytes(),
        ]
        .concat(),
        Value::Collection(v) => u32::from(*v).to_be_bytes().to_vec(),
    })
}
fn write(writer: &mut CanonicalWriter, value: &Value) -> Result<(), CodecError> {
    match value {
        Value::U8(v) => writer.write_u8(*v),
        Value::U16(v) => writer.write_u16(*v),
        Value::U32(v) => writer.write_u32(*v),
        Value::U64(v) => writer.write_u64(*v),
        Value::Fixed(v) => writer.write_fixed(v),
        Value::Bool(v) => writer.write_bool(*v),
        Value::Option(v) => writer.write_option_tag(*v),
        Value::Bytes(v) => writer.write_bytes(v),
        Value::String(v) => writer.write_str(v),
        Value::Collection(v) => writer.write_collection_len(usize::from(*v)),
    }
}
fn writes(operations: &[Value], bound: u8, m: &mut Metrics) -> Result<(), String> {
    if operations.len() > 64 {
        return Err("setup-incomplete: codec writer operation bound".into());
    }
    let bound = usize::from(bound);
    let mut writer = CanonicalWriter::new(limits(bound));
    let mut expected = Vec::new();
    for value in operations {
        let bytes = value_bytes(value)?;
        if bytes.len() > 1024 {
            return Err("setup-incomplete: codec primitive byte bound".into());
        }
        let special = match value {
            Value::Bytes(v) if v.len() > bound => Some(Limit::OpaqueBytes),
            Value::String(v) if v.len() > bound => Some(Limit::StringBytes),
            Value::Collection(v) if usize::from(*v) > bound => Some(Limit::CollectionItems),
            _ => None,
        };
        let rejection = special
            .or_else(|| (expected.len() + bytes.len() > bound).then_some(Limit::PayloadBytes));
        let actual = write(&mut writer, value);
        if let Some(limit) = rejection {
            require_error(actual, Failure::limited(limit, expected.len()))?;
            bump(m, "atomic_primitive_write_rejections");
        } else {
            actual.map_err(|e| e.to_string())?;
            expected.extend(bytes);
            bump(m, "primitive_writes");
        }
        if writer.as_slice() != expected
            || writer.len() != expected.len()
            || writer.is_empty() != expected.is_empty()
        {
            return Err(
                "writer prefix changed after rejection or differs from exact primitive bytes"
                    .into(),
            );
        }
    }
    if writer.into_bytes() != expected {
        return Err("finished primitive bytes differ".into());
    }
    Ok(())
}
fn nested_write(writer: &mut CanonicalWriter, depth: u8, fail: bool) -> Result<(), CodecError> {
    if depth == 0 {
        return if fail {
            Err(CodecError::at(Kind::InvalidDomainValue, writer.len()))
        } else {
            Ok(())
        };
    }
    writer.nested(|writer| {
        writer.write_u8(depth)?;
        nested_write(writer, depth - 1, fail)
    })
}
fn nested_read(reader: &mut CanonicalReader<'_>, depth: u8, fail: bool) -> Result<(), CodecError> {
    if depth == 0 {
        return if fail {
            Err(CodecError::at(Kind::InvalidDomainValue, reader.offset()))
        } else {
            Ok(())
        };
    }
    reader.nested(|reader| {
        reader.read_u8()?;
        nested_read(reader, depth - 1, fail)
    })
}
fn nested(depth: u8, limit: u8, fail: bool, m: &mut Metrics) -> Result<(), String> {
    if depth > 16 || limit > 16 {
        return Err("setup-incomplete: nesting depth bound".into());
    }
    let mut policy = limits(64);
    policy.max_nesting_depth = u16::from(limit);
    let mut writer = CanonicalWriter::new(policy);
    let input = (1..=depth).rev().collect::<Vec<_>>();
    let mut reader = CanonicalReader::new(&input, policy);
    let first = nested_write(&mut writer, depth, fail);
    let second = nested_read(&mut reader, depth, fail);
    let consumed = usize::from(depth.min(limit));
    let expected = if depth > limit {
        Some(Failure::limited(Limit::NestingDepth, consumed))
    } else if fail {
        Some(Failure::simple(Kind::InvalidDomainValue, consumed))
    } else {
        None
    };
    if let Some(expected) = expected {
        require_error(first, expected.clone())?;
        require_error(second, expected)?;
        bump(m, "nested_error_unwinds");
    } else {
        first.map_err(|e| e.to_string())?;
        second.map_err(|e| e.to_string())?;
        bump(m, "nested_successes");
    }
    if writer.as_slice() != &input[..consumed] || reader.offset() != consumed {
        return Err("nested callback prefix/cursor differs".into());
    }
    // A new aggregate sees reset depth after success or failure; callbacks are not global rollback.
    if limit > 0 {
        writer.nested(|w| w.write_u8(99)).map_err(|e| e.to_string())?;
        let tail = &input[consumed..];
        if tail.is_empty() {
            require_error(
                reader.nested(CanonicalReader::read_u8),
                Failure::simple(Kind::Truncated, consumed),
            )?;
        } else {
            let actual = reader.nested(CanonicalReader::read_u8).map_err(|e| e.to_string())?;
            if actual != tail[0] {
                return Err("nested reader continuation changed byte".into());
            }
        }
        bump(m, "continued_after_nested_result");
    }
    Ok(())
}
pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    match case {
        Case::Frame { .. } => frame(case, &mut metrics)?,
        Case::Read { input, operations, bound } => reads(input, operations, *bound, &mut metrics)?,
        Case::Write { operations, bound } => writes(operations, *bound, &mut metrics)?,
        Case::ReadValues { values, bound, cut } => {
            if values.len() > 8 {
                return Err("setup-incomplete: generated primitive tuple bound".into());
            }
            let mut input = Vec::new();
            let mut operations = Vec::new();
            for value in values {
                input.extend(value_bytes(value)?);
                operations.push(match value {
                    Value::U8(_) => Read::U8,
                    Value::U16(_) => Read::U16,
                    Value::U32(_) => Read::U32,
                    Value::U64(_) => Read::U64,
                    Value::Fixed(_) => Read::Fixed,
                    Value::Bool(_) => Read::Bool,
                    Value::Option(_) => Read::Option,
                    Value::Bytes(_) => Read::Bytes,
                    Value::String(_) => Read::String,
                    Value::Collection(_) => Read::Collection,
                });
            }
            if *cut != 0 && !input.is_empty() {
                let retained = usize::from(*cut) % input.len();
                input.truncate(retained);
                bump(&mut metrics, "constructed_primitive_truncations");
            }
            reads(&input, &operations, *bound, &mut metrics)?;
            bump(&mut metrics, "constructed_primitive_tuples");
        }
        Case::Nested { depth, limit, fail } => nested(*depth, *limit, *fail, &mut metrics)?,
    }
    Ok(metrics)
}
pub fn calibration() -> Case {
    Case::Read { input: vec![2], operations: vec![Read::Bool], bound: 8 }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration(),
        Case::Write {
            operations: vec![
                Value::String("é".into()),
                Value::Bytes(vec![1]),
                Value::U64(u64::MAX),
                Value::U8(7),
            ],
            bound: 7,
        },
        Case::Read {
            input: vec![0, 0, 0, 2, 0xff, 0xfe, 1],
            operations: vec![Read::String, Read::U8],
            bound: 2,
        },
    ];
    let values = vec![
        Value::U8(u8::MAX),
        Value::U16(u16::MAX),
        Value::U32(u32::MAX),
        Value::U64(u64::MAX),
        Value::Fixed([0, 255, 17]),
        Value::Bool(true),
        Value::Option(false),
        Value::Bytes(vec![0, 128, 255]),
        Value::String("é\r\n👩‍💻".into()),
        Value::Collection(3),
    ];
    cases.push(Case::Write { operations: values.clone(), bound: 80 });
    for chunk in values.chunks(8) {
        cases.push(Case::ReadValues { values: chunk.to_vec(), bound: 80, cut: 0 });
    }
    for (family, schema, payload_limit, frame_limit) in
        [(0, 1, 3, 19), (1, 0, 3, 19), (1, 1, 2, 19), (1, 1, 3, 18), (1, 1, 4, 20)]
    {
        cases.push(Case::Frame {
            family,
            schema,
            payload: vec![0, 1, 255],
            frame_limit,
            payload_limit,
            fault: 0,
        });
    }
    for depth in [0, 1, 3] {
        for limit in [0, 1, 3] {
            for fail in [false, true] {
                cases.push(Case::Nested { depth, limit, fail });
            }
        }
    }
    for fault in 0..9 {
        cases.push(Case::Frame {
            family: 94,
            schema: 1,
            payload: vec![0, 1, 255],
            frame_limit: 19,
            payload_limit: 3,
            fault,
        });
    }
    cases
}
