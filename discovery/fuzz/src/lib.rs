//! Coverage-guided consumers of public Peritus interfaces. Every input drives bounded,
//! deterministic operations; oracles use explicit wire contracts or independent models.
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits, decode_frame, encode_frame};
use peritus_provider_core::{FramingLimits, NdjsonParser, SseItem, SseParser};
use peritus_tool_protocol::{BoundedJson, JsonLimits, Schema, SchemaProperty};
use serde_json::{Value, json};

struct Bytes<'a> {
    rest: &'a [u8],
}
impl<'a> Bytes<'a> {
    fn new(rest: &'a [u8]) -> Self {
        Self { rest }
    }
    fn byte(&mut self) -> u8 {
        let b = self.rest.first().copied().unwrap_or(0);
        self.rest = self.rest.get(1..).unwrap_or_default();
        b
    }
    fn number(&mut self) -> u64 {
        u64::from_le_bytes(std::array::from_fn(|_| self.byte()))
    }
    fn text(&mut self) -> String {
        let n = (self.byte() as usize).min(self.rest.len()).min(96);
        let text = String::from_utf8_lossy(&self.rest[..n]).into_owned();
        self.rest = &self.rest[n..];
        text
    }
}
fn codec_limits(n: usize) -> CodecLimits {
    CodecLimits::new(n + 16, n, 64, 512, 4096, 8)
}
fn bounded(data: &[u8]) -> &[u8] {
    &data[..data.len().min(16384)]
}

/// Raw headers plus independently assembled valid headers, exact bounds and corrupt lengths.
pub fn codec_frame(data: &[u8]) {
    let data = bounded(data);
    let mut b = Bytes::new(data);
    let family = u16::from(b.byte()) + 1;
    let schema = u16::from(b.byte()) + 1;
    let payload = b.rest;
    let limits = codec_limits(payload.len());
    let mut expected = b"PRTS".to_vec();
    expected.extend_from_slice(&1u16.to_be_bytes());
    expected.extend_from_slice(&family.to_be_bytes());
    expected.extend_from_slice(&schema.to_be_bytes());
    expected.extend_from_slice(&0u16.to_be_bytes());
    expected.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    expected.extend_from_slice(payload);
    if !payload.is_empty() {
        let mut payload_ceiling = limits;
        payload_ceiling.max_payload_bytes -= 1;
        let mut frame_ceiling = limits;
        frame_ceiling.max_frame_bytes -= 1;
        for ceiling in [payload_ceiling, frame_ceiling] {
            assert!(encode_frame(family, schema, payload, ceiling).is_err());
            assert!(decode_frame(&expected, ceiling).is_err());
        }
    }
    let encoded = encode_frame(family, schema, payload, limits).unwrap();
    assert_eq!(encoded, expected);
    let decoded = decode_frame(&expected, limits).unwrap();
    assert_eq!(decoded.payload(), payload);
    assert_eq!(decoded.header().family(), family);
    assert_eq!(decoded.header().payload_len() as usize, payload.len());
    assert_eq!(decoded.header().schema_version(), schema);
    assert!(encode_frame(0, schema, payload, limits).is_err());
    assert!(encode_frame(family, 0, payload, limits).is_err());
    expected[10] = 1;
    assert!(decode_frame(&expected, limits).is_err());
    if let Ok(frame) = decode_frame(data, codec_limits(16384)) {
        assert_eq!(
            encode_frame(
                frame.header().family(),
                frame.header().schema_version(),
                frame.payload(),
                codec_limits(16384)
            )
            .unwrap(),
            data
        );
    }
}

/// Transactional writer prefix and primitive decoding against independently encoded bytes.
pub fn codec_primitives(data: &[u8]) {
    let data = bounded(data);
    let mut b = Bytes::new(bounded(data));
    let ceiling = usize::from(b.byte()) + 1;
    let limits = codec_limits(ceiling);
    let mut writer = CanonicalWriter::new(limits);
    let mut expected = Vec::new();
    for _ in 0..64 {
        if b.rest.is_empty() {
            break;
        }
        let op = b.byte() % 5;
        let n = b.number();
        let text = b.text();
        let mut addition = Vec::new();
        let result = match op {
            0 => {
                addition.push(n as u8);
                writer.write_u8(n as u8)
            }
            1 => {
                addition.extend_from_slice(&(n as u16).to_be_bytes());
                writer.write_u16(n as u16)
            }
            2 => {
                addition.extend_from_slice(&n.to_be_bytes());
                writer.write_u64(n)
            }
            3 => {
                addition.push(u8::from(n & 1 == 1));
                writer.write_bool(n & 1 == 1)
            }
            _ => {
                addition.extend_from_slice(&(text.len() as u32).to_be_bytes());
                addition.extend_from_slice(text.as_bytes());
                writer.write_str(&text)
            }
        };
        let fits = expected.len() + addition.len() <= ceiling;
        assert_eq!(result.is_ok(), fits);
        if fits {
            expected.extend_from_slice(&addition);
        }
        assert_eq!(writer.as_slice(), expected);
    }
    let mut reader = CanonicalReader::new(data, codec_limits(16384));
    let mut pos = 0;
    while pos + 8 <= data.len() {
        assert_eq!(
            reader.read_u64().unwrap(),
            u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap())
        );
        pos += 8;
    }
    assert_eq!(reader.remaining(), data.len() - pos);
    let before = reader.offset();
    assert!(reader.read_u64().is_err());
    assert_eq!(reader.offset(), before);
}

fn sse_items(wire: &[u8], chunk: usize) -> Result<Vec<SseItem>, ()> {
    let mut parser = SseParser::new(FramingLimits::new(32768, 65536).unwrap());
    let mut out = Vec::new();
    for bytes in wire.chunks(chunk) {
        out.extend(parser.push(bytes).map_err(|_| ())?);
    }
    out.extend(parser.finish().map_err(|_| ())?);
    assert!(parser.finish().is_err());
    assert!(parser.push(b"x").is_err());
    Ok(out)
}
/// Arbitrary malformed SSE and constructed data/comments/metadata, invariant to chunking.
pub fn sse(data: &[u8]) {
    let data = bounded(data);
    let mut b = Bytes::new(data);
    let chunk = usize::from(b.byte()) + 1;
    assert_eq!(sse_items(data, chunk), sse_items(data, data.len().max(1)));
    let text = b.text().replace(['\n', '\r'], "_");
    let event = b.text().replace(['\n', '\r'], "_");
    let id = b.text().replace(['\n', '\r', '\0'], "_");
    let wire = format!(
        ": ping\r\nevent: {event}\r\nid: {id}\r\ndata: {text}\r\ndata: tail\r\n\r\ndata: [DONE]\r\n\r\n"
    );
    let out = sse_items(wire.as_bytes(), chunk).unwrap();
    assert_eq!(out.len(), 3);
    assert!(matches!(&out[0],SseItem::Comment(c) if c.as_str()=="ping"));
    match &out[1] {
        SseItem::Event(e) => {
            assert_eq!(e.data(), format!("{text}\ntail"));
            assert_eq!(e.event(), Some(event.as_str()));
            assert_eq!(e.id(), Some(id.as_str()));
        }
        _ => panic!("missing event"),
    }
    assert!(matches!(out[2], SseItem::Done));
    let line = format!("data: {text}");
    for delta in [0, 1] {
        let max = line.len() - delta;
        let mut parser = SseParser::new(FramingLimits::new(max, 65536).unwrap());
        let result = parser.push(format!("{line}\r\n\r\n").as_bytes());
        assert_eq!(result.is_ok(), delta == 0);
    }
}
fn ndjson_items(wire: &[u8], chunk: usize) -> Result<Vec<String>, ()> {
    let mut parser = NdjsonParser::new(FramingLimits::new(32768, 65536).unwrap());
    let mut out = Vec::new();
    for bytes in wire.chunks(chunk) {
        out.extend(parser.push(bytes).map_err(|_| ())?.into_iter().map(|r| r.as_str().to_owned()));
    }
    out.extend(parser.finish().map_err(|_| ())?.into_iter().map(|r| r.as_str().to_owned()));
    assert!(parser.finish().is_err());
    assert!(parser.push(b"x").is_err());
    Ok(out)
}
/// Raw bytes and valid JSON records, CRLF and unterminated EOF checked independently.
pub fn ndjson(data: &[u8]) {
    let data = bounded(data);
    let mut b = Bytes::new(data);
    let chunk = usize::from(b.byte()) + 1;
    assert_eq!(ndjson_items(data, chunk), ndjson_items(data, data.len().max(1)));
    let rows = (0..(b.byte() % 16 + 1))
        .map(|_| json!({"text":b.text(),"n":b.number()}).to_string())
        .collect::<Vec<_>>();
    let wire = rows.join("\r\n\n");
    assert_eq!(ndjson_items(wire.as_bytes(), chunk).unwrap(), rows);
    let record = &rows[0];
    for delta in [0, 1] {
        let mut parser =
            NdjsonParser::new(FramingLimits::new(record.len() - delta, 65536).unwrap());
        assert_eq!(parser.push(format!("{record}\r\n").as_bytes()).is_ok(), delta == 0);
    }
}

fn tree(b: &mut Bytes<'_>, depth: u8) -> Value {
    match if depth == 0 { b.byte() % 4 } else { b.byte() % 6 } {
        0 => Value::Null,
        1 => Value::Bool(b.byte() & 1 != 0),
        2 => Value::from(b.number() as i64),
        3 => Value::String(b.text()),
        4 => Value::Array((0..b.byte() % 5).map(|_| tree(b, depth - 1)).collect()),
        _ => Value::Object(
            (0..b.byte() % 5).map(|i| (format!("k{i}_{}", b.text()), tree(b, depth - 1))).collect(),
        ),
    }
}
// Independent measurements from the generated JSON tree, not the production validator.
fn dimensions(value: &Value) -> (usize, usize, usize) {
    match value {
        Value::Array(values) => {
            let children = values.iter().map(dimensions).collect::<Vec<_>>();
            (
                1 + children.iter().map(|v| v.0).max().unwrap_or(0),
                values.len() + children.iter().map(|v| v.1).sum::<usize>(),
                children.iter().map(|v| v.2).max().unwrap_or(0),
            )
        }
        Value::Object(values) => {
            let children = values.values().map(dimensions).collect::<Vec<_>>();
            (
                1 + children.iter().map(|v| v.0).max().unwrap_or(0),
                values.len() + children.iter().map(|v| v.1).sum::<usize>(),
                children
                    .iter()
                    .map(|v| v.2)
                    .chain(values.keys().map(String::len))
                    .max()
                    .unwrap_or(0),
            )
        }
        Value::String(value) => (1, 0, value.len()),
        _ => (1, 0, 0),
    }
}
fn check_json_bounds(value: &Value, selector: u8) {
    let text = value.to_string();
    let (depth, members, string) = dimensions(value);
    let mut limits = [text.len().max(1), depth.max(1), members.max(1), string.max(1)];
    let index = usize::from(selector % 4);
    if selector & 4 != 0 {
        limits[index] = limits[index].saturating_sub(1).max(1);
    }
    if selector & 8 != 0 {
        limits[index] += 1;
    }
    let expected = text.len() <= limits[0]
        && depth <= limits[1]
        && members <= limits[2]
        && string <= limits[3];
    let bounds = JsonLimits::new(limits[0], limits[1], limits[2], limits[3]).unwrap();
    assert_eq!(BoundedJson::parse(&text, bounds).is_ok(), expected);
    let bounds =
        peritus_model_protocol::JsonBounds::new(limits[0], limits[1], limits[2], limits[3])
            .unwrap();
    assert_eq!(peritus_model_protocol::CanonicalJson::parse(&text, bounds).is_ok(), expected);
}
fn json_limits() -> JsonLimits {
    JsonLimits::new(65536, 32, 4096, 32768).unwrap()
}
/// Canonical semantics against serde plus constructor-valid recursive values and duplicates.
pub fn tool_json(data: &[u8]) {
    let mut b = Bytes::new(&data[..data.len().min(4096)]);
    let node = tree(&mut b, 4);
    check_json_bounds(&node, data.first().copied().unwrap_or(0));
    let text = serde_json::to_string_pretty(&node).unwrap();
    let parsed = BoundedJson::parse(&text, json_limits()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(parsed.canonical_bytes()).unwrap(), node);
    assert_eq!(
        BoundedJson::parse(std::str::from_utf8(parsed.canonical_bytes()).unwrap(), json_limits())
            .unwrap(),
        parsed
    );
    assert!(BoundedJson::parse("{\"a\":1,\"a\":2}", json_limits()).is_err());
    if let Ok(raw) = std::str::from_utf8(bounded(data)) {
        if let Ok(parsed) = BoundedJson::parse(raw, json_limits()) {
            assert_eq!(
                serde_json::from_slice::<Value>(parsed.canonical_bytes()).unwrap(),
                serde_json::from_str::<Value>(raw).unwrap()
            );
        }
    }
}
/// Related valid and one-defect nested schema inputs reach required-property/type/range checks.
pub fn tool_schema(data: &[u8]) {
    let mut b = Bytes::new(bounded(data));
    let min = b.byte() as i64 - 128;
    let max = min + i64::from(b.byte());
    let n = b.number() as i64;
    let text = b.text();
    let lo = b.byte() as u32;
    let hi = lo + u32::from(b.byte());
    let ints = Schema::integer(Some(min), Some(max)).unwrap();
    assert_eq!(ints.validate(&BoundedJson::integer(n)).is_ok(), min <= n && n <= max);
    let strings = Schema::string(lo, hi).unwrap();
    assert_eq!(
        strings.validate(&BoundedJson::string(text.clone(), json_limits()).unwrap()).is_ok(),
        (lo as usize..=hi as usize).contains(&text.len())
    );
    for length in [lo, hi, hi + 1] {
        let value = BoundedJson::string("x".repeat(length as usize), json_limits()).unwrap();
        assert_eq!(strings.validate(&value).is_ok(), length <= hi);
    }
    if lo > 0 {
        let value = BoundedJson::string("x".repeat(lo as usize - 1), json_limits()).unwrap();
        assert!(strings.validate(&value).is_err());
    }
    let schema = Schema::object(
        vec![SchemaProperty::new("a".into(), Schema::array(ints, 1, 4).unwrap(), true).unwrap()],
        false,
    )
    .unwrap();
    let valid = BoundedJson::parse(&json!({"a":[min,max]}).to_string(), json_limits()).unwrap();
    assert!(schema.validate(&valid).is_ok());
    for length in 1..=4 {
        let values = (0..length).map(|i| if i % 2 == 0 { min } else { max }).collect::<Vec<_>>();
        let valid = BoundedJson::parse(&json!({"a":values}).to_string(), json_limits()).unwrap();
        assert!(schema.validate(&valid).is_ok());
    }
    for invalid in [
        json!({}),
        json!({"a":[min,max],"extra":1}),
        json!({"a":[min.to_string()]}),
        json!({"a":[]}),
        json!({"a":[min,min,min,min,min]}),
    ] {
        assert!(
            schema
                .validate(&BoundedJson::parse(&invalid.to_string(), json_limits()).unwrap())
                .is_err()
        );
    }
}
/// Model JSON semantic preservation and a strict duplicate-key rejection positive control.
pub fn model_json(data: &[u8]) {
    use peritus_model_protocol::{CanonicalJson, JsonBounds};
    let bounds = JsonBounds::new(65536, 32, 4096, 32768).unwrap();
    let mut b = Bytes::new(&data[..data.len().min(4096)]);
    let value = tree(&mut b, 4);
    let text = serde_json::to_string_pretty(&value).unwrap();
    let got = CanonicalJson::parse(&text, bounds).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(got.canonical_bytes()).unwrap(), value);
    assert_eq!(CanonicalJson::parse(&got.to_wire_string(), bounds).unwrap(), got);
    assert!(CanonicalJson::parse("{\"x\":1,\"x\":2}", bounds).is_err());
    if let Ok(raw) = std::str::from_utf8(bounded(data)) {
        if let Ok(parsed) = CanonicalJson::parse(raw, bounds) {
            assert_eq!(
                serde_json::from_slice::<Value>(parsed.canonical_bytes()).unwrap(),
                serde_json::from_str::<Value>(raw).unwrap()
            );
        }
    }
}
/// Actual JSON healing, exact preservation of valid objects and accepted formatting repairs.
pub fn json_healing(data: &[u8]) {
    use peritus_model_protocol::ProtocolLimits;
    use peritus_provider_core::healing;
    let mut b = Bytes::new(bounded(data));
    let value = json!({"a":b.text(),"n":b.number() as i64,"nested":[true,null]});
    let plain = value.to_string();
    for input in
        [plain.clone(), format!("```json\n{plain}\n```"), serde_json::to_string(&plain).unwrap()]
    {
        let result = healing::object(&input, "fuzz", ProtocolLimits::PRODUCTION).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(result.value().canonical_bytes()).unwrap(),
            value
        );
    }
    if let Ok(raw) = std::str::from_utf8(bounded(data)) {
        for result in [
            healing::object(raw, "fuzz", ProtocolLimits::PRODUCTION),
            healing::private_envelope(raw, "fuzz", ProtocolLimits::PRODUCTION),
        ] {
            if let Ok(result) = result {
                assert!(
                    serde_json::from_slice::<Value>(result.value().canonical_bytes())
                        .unwrap()
                        .is_object()
                );
            }
        }
    }
}
/// Canonical POSIX relative paths against a separate component predicate, plus protected names.
pub fn workspace_path(data: &[u8]) {
    use peritus_patch::WorkspacePath;
    if let Ok(raw) = std::str::from_utf8(bounded(data)) {
        let expected = !raw.is_empty()
            && raw.len() <= 4096
            && !raw.contains('\0')
            && raw.split('/').all(|c| {
                !c.is_empty()
                    && c != "."
                    && c != ".."
                    && c.len() <= 255
                    && !c.eq_ignore_ascii_case(".git")
                    && !c.eq_ignore_ascii_case(".peritus")
                    && !c.to_ascii_lowercase().starts_with(".peritus-txn-")
            });
        let got = WorkspacePath::new(raw);
        assert_eq!(got.is_ok(), expected);
        if let Ok(got) = got {
            assert_eq!(got.as_str(), raw);
            assert!(!got.as_path().is_absolute());
        }
    }
    let mut b = Bytes::new(bounded(data));
    let safe = format!("src/f{}_{}.rs", b.byte(), b.number());
    assert!(WorkspacePath::new(safe).is_ok());
    for protected in [".git", ".peritus", ".PeRiTuS-TXN-any"] {
        assert!(WorkspacePath::new(format!("a/{protected}/b")).is_err());
    }
}
/// Permission set construction, exact identity/name membership and subset against integer sets.
pub fn policy_sets(data: &[u8]) {
    use peritus_policy::{Permission, PermissionSet};
    use peritus_types::{CapabilityName, ResourceId};
    fn permission(v: u8) -> Permission {
        Permission::new(
            ResourceId::new((u128::from(v / 4) + 1).to_be_bytes()).unwrap(),
            CapabilityName::new(format!("op{}", v % 4)).unwrap(),
        )
    }
    let input = bounded(data).iter().take(64).map(|x| x % 32).collect::<Vec<_>>();
    let raw = PermissionSet::new(input.iter().map(|v| permission(*v)).collect());
    assert_eq!(raw.is_ok(), !input.is_empty() && input.windows(2).all(|w| w[0] < w[1]));
    let mut canonical = input;
    canonical.sort_unstable();
    canonical.dedup();
    if canonical.is_empty() {
        canonical.push(0);
    }
    let set = PermissionSet::new(canonical.iter().map(|v| permission(*v)).collect()).unwrap();
    assert_eq!(set.as_slice().len(), canonical.len());
    for v in 0..36 {
        assert_eq!(set.contains(&permission(v)), canonical.contains(&v));
    }
    let other_values =
        canonical.iter().copied().filter(|v| v % 2 == 0).chain([34]).collect::<Vec<_>>();
    let other = PermissionSet::new(other_values.iter().map(|v| permission(*v)).collect()).unwrap();
    assert_eq!(set.is_subset_of(&other), canonical.iter().all(|v| other_values.contains(v)));
    assert!(set.is_subset_of(&set));
}

/// Context graph cycle admission checked with independent transitive closure; clone preserves graph.
pub fn context_graph(data: &[u8]) {
    use peritus_context::*;
    use peritus_policy::ActorRole;
    use peritus_role::ContextClass;
    let mut b = Bytes::new(bounded(data));
    let n = usize::from(b.byte() % 12) + 1;
    let constructive_dag = b.byte() & 1 == 0;
    let limits = ContextLimits::new(16, 1024, 16, 4).unwrap();
    let id = |i: usize| ContextNodeId::new((i as u128 + 1).to_be_bytes()).unwrap();
    let mut reach = vec![vec![false; n]; n];
    let mut nodes = Vec::new();
    for (i, row) in reach.iter_mut().enumerate() {
        let deps = (0..n)
            .filter(|j| *j != i && (!constructive_dag || *j < i) && b.byte() % 4 == 0)
            .collect::<Vec<_>>();
        for j in &deps {
            row[*j] = true;
        }
        let metadata = ContextNodeMetadata::new(
            id(i),
            Provenance::Repository,
            AuthorityClass::NonAuthoritative,
            TrustClass::Untrusted,
            ContextClass::RepositorySource,
            ContentKind::RepositorySource,
            1,
            1,
            RequirementMode::Optional,
            0,
            RoleVisibility::new(vec![ActorRole::Writer], limits).unwrap(),
            deps.iter().map(|j| id(*j)).collect(),
            limits,
        )
        .unwrap();
        let bytes = vec![i as u8 + 1];
        let content =
            bind_context_content(bytes.clone(), peritus_codec::sha256(&bytes), limits).unwrap();
        nodes.push(ContextNode::new(metadata, content));
    }
    let direct = reach.clone();
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                reach[i][j] |= reach[i][k] && reach[k][j];
            }
        }
    }
    let acyclic = (0..n).all(|i| !reach[i][i]);
    let graph = ContextGraph::new(nodes, limits);
    assert_eq!(graph.is_ok(), acyclic);
    if let Ok(graph) = graph {
        assert_eq!(graph.nodes().len(), n);
        assert!(graph.node(id(n)).is_none());
        let cloned = graph.clone();
        assert_eq!(cloned, graph);
        for i in 0..n {
            let node = graph.node(id(i)).unwrap();
            assert_eq!(node.id(), id(i));
            assert_eq!(
                node.dependencies(),
                (0..n).filter(|j| direct[i][*j]).map(id).collect::<Vec<_>>()
            );
            assert_eq!(node.content().bytes(), &[i as u8 + 1]);
        }
    }
}
/// Statistics against u128 summation and direct order-statistic ranks, including u64 overflow.
pub fn statistics(data: &[u8]) {
    use peritus_eval::{DistributionSummary, pass_at_k};
    let mut b = Bytes::new(bounded(data));
    let missing = b.byte() as u32;
    let count = usize::from(b.byte() % 32);
    let extremes = b.byte() & 1 != 0;
    let values = (0..count)
        .map(|_| {
            let n = b.number();
            if extremes { n } else { n % 1_000_000 }
        })
        .collect::<Vec<_>>();
    let sum = values.iter().map(|v| u128::from(*v)).sum::<u128>();
    let result = DistributionSummary::new(values.clone(), missing);
    assert_eq!(result.is_ok(), !values.is_empty() && sum <= u128::from(u64::MAX));
    if let Ok(got) = result {
        let mut sorted = values.clone();
        sorted.sort();
        assert_eq!(got.total() as u128, sum);
        assert_eq!(got.count(), count as u32);
        assert_eq!(got.missing(), missing);
        assert_eq!(got.minimum(), sorted[0]);
        assert_eq!(got.maximum(), sorted[count - 1]);
        assert_eq!(got.mean() as u128, sum / count as u128);
        let rank = |p: usize| sorted[(count * p - 1) / 100];
        assert_eq!((got.p50(), got.p95(), got.p99()), (rank(50), rank(95), rank(99)));
    }
    let total = u32::from(b.byte()) + 1;
    let successes = u32::from(b.byte()) % (total + 1);
    let k = u16::from(b.byte()) % total.min(8) as u16 + 1;
    fn choose(n: u32, k: u16) -> u128 {
        if u32::from(k) > n {
            return 0;
        }
        (1..=u32::from(k))
            .fold(1u128, |count, i| count * u128::from(n - u32::from(k) + i) / u128::from(i))
    }
    let estimate = pass_at_k(total, successes, k).unwrap().estimate().get();
    let denominator = choose(total, k);
    let exact = (denominator - choose(total - successes, k)) * 1_000_000 / denominator;
    assert!((i64::from(estimate) - exact as i64).abs() <= 1);
    assert!(pass_at_k(0, 0, 1).is_err());
    assert!(pass_at_k(total, total + 1, 1).is_err());
    assert!(pass_at_k(total, successes, 0).is_err());
}
