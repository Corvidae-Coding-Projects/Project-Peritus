//! Independent JSON/schema interpreter and exact C4 call/control/progress/result envelope bytes.
use peritus_policy::{AuthorityInstant, OperationClass, OperationDescriptor, RiskClass, RiskSet};
use peritus_tool_protocol::*;
use peritus_types::*;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Shape {
    Null,
    Bool,
    Int(Option<i64>, Option<i64>),
    Text(u32, u32),
    Array(Box<Shape>, u32, u32),
    Object(Vec<(String, bool, Shape)>, bool),
    Enum(Box<Shape>, Vec<Node>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Json {
        node: Node,
        bounds: [u16; 4],
        whitespace: bool,
    },
    Raw {
        input: String,
    },
    Schema {
        shape: Shape,
        node: Node,
    },
    Compatibility {
        before: Shape,
        after: Shape,
    },
    Envelope {
        seed: u8,
        fault: u8,
        descriptor_timeout: bool,
        call_timeout: bool,
        deadline: bool,
        control: u8,
        progress: u8,
        status: u8,
    },
    Header {
        family: u16,
        version: u16,
        payload: Vec<u8>,
        ceiling: u16,
        corrupt: bool,
    },
}
fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn bump(m: &mut crate::Metrics, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}
fn node_strategy() -> BoxedStrategy<Node> {
    let text = prop::sample::select(vec!["x", "é🦀", "\n\t\"\\", "", "e\u{301}", "/~"]);
    prop_oneof![
        Just(Node::Null),
        any::<bool>().prop_map(Node::Bool),
        any::<i64>().prop_map(Node::Int),
        text.prop_map(|s| Node::Text(s.into()))
    ]
    .prop_recursive(4, 48, 5, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(Node::Array),
            prop::collection::vec(
                (prop::sample::select(vec!["a", "b", "x/~", "é", "\""]), inner),
                0..5
            )
            .prop_map(|pairs| Node::Object(
                pairs.into_iter().map(|(k, v)| (k.into(), v)).collect()
            ))
        ]
    })
    .boxed()
}
fn shape_strategy() -> BoxedStrategy<Shape> {
    prop_oneof![
        Just(Shape::Null),
        Just(Shape::Bool),
        (prop::option::of(any::<i16>()), prop::option::of(any::<i16>()))
            .prop_map(|(a, b)| Shape::Int(a.map(i64::from), b.map(i64::from))),
        (0_u32..6, 0_u32..10).prop_map(|(a, b)| Shape::Text(a, b)),
        prop::collection::vec(any::<bool>(), 0..4).prop_map(|v| Shape::Enum(
            Box::new(Shape::Bool),
            v.into_iter().map(Node::Bool).collect()
        ))
    ]
    .prop_recursive(3, 32, 4, |s| {
        prop_oneof![
            (s.clone(), 0_u32..3, 0_u32..6).prop_map(|(s, a, b)| Shape::Array(Box::new(s), a, b)),
            prop::collection::vec(
                (prop::sample::select(vec!["a", "b", "x/~"]), any::<bool>(), s),
                0..4
            )
            .prop_map(|p| Shape::Object(
                p.into_iter().map(|(k, r, s)| (k.into(), r, s)).collect(),
                false
            ))
        ]
    })
    .boxed()
}
fn matching(n: &Node) -> Shape {
    match n {
        Node::Null => Shape::Null,
        Node::Bool(_) => Shape::Bool,
        Node::Int(x) => Shape::Int(Some(*x), Some(*x)),
        Node::Text(x) => Shape::Text(0, x.len() as u32),
        Node::Array(_) => Shape::Array(Box::new(Shape::Bool), 0, 4),
        Node::Object(p) => {
            let mut p = p.iter().map(|(n, v)| (n.clone(), true, matching(v))).collect::<Vec<_>>();
            p.sort_by(|a, b| a.0.cmp(&b.0));
            Shape::Object(p, false)
        }
    }
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let nodes = node_strategy();
    let schemas = shape_strategy();
    prop_oneof![
 5=>(nodes.clone(),prop::array::uniform4(1_u16..128),any::<bool>()).prop_map(|(node,bounds,whitespace)|Case::Json{node,bounds,whitespace}),
 5=>(schemas.clone(),nodes.clone()).prop_map(|(shape,node)|Case::Schema{shape,node}),
 2=>(prop::sample::select(vec!["a","required","x/~"]),any::<bool>()).prop_map(|(name,extra)|Case::Schema{shape:Shape::Object(vec![(name.into(),true,Shape::Bool)],extra),node:Node::Object(vec![])}),
 4=>nodes.prop_map(|node|Case::Schema{shape:matching(&node),node}),
 2=>(schemas.clone(),schemas).prop_map(|(before,after)|Case::Compatibility{before,after}),
 6=>(any::<u8>(),0_u8..10,any::<bool>(),any::<bool>(),any::<bool>(),0_u8..20,0_u8..10,0_u8..5).prop_map(|(seed,fault,descriptor_timeout,call_timeout,deadline,control,progress,status)|Case::Envelope{seed,fault,descriptor_timeout,call_timeout,deadline,control,progress,status}),
 2=>(any::<u16>(),0_u16..4,prop::collection::vec(any::<u8>(),0..=max_steps.min(64)),0_u16..80,any::<bool>()).prop_map(|(family,version,payload,ceiling,corrupt)|Case::Header{family,version,payload,ceiling,corrupt}),
 1=>prop::sample::select(vec!["{\"x\":0,\"\\u0078\":1}","[1,{\"a\":1,\"a\":2}]","1.0","1e3","18446744073709551615","-9223372036854775809","{\"x\":" ]).prop_map(|s|Case::Raw{input:s.into()})].boxed()
}
fn value(n: &Node) -> Result<Value, String> {
    Ok(match n {
        Node::Null => Value::Null,
        Node::Bool(x) => json!(x),
        Node::Int(x) => json!(x),
        Node::Text(x) => json!(x),
        Node::Array(a) => Value::Array(a.iter().map(value).collect::<Result<_, _>>()?),
        Node::Object(a) => {
            let mut m = BTreeMap::new();
            for (k, v) in a {
                if m.insert(k.clone(), value(v)?).is_some() {
                    return Err("duplicate".into());
                }
            }
            Value::Object(m.into_iter().collect())
        }
    })
}
fn raw(n: &Node) -> String {
    match n {
        Node::Object(p) => format!(
            "{{{}}}",
            p.iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), raw(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Node::Array(a) => format!("[{}]", a.iter().map(raw).collect::<Vec<_>>().join(",")),
        _ => serde_json::to_string(&value(n).unwrap()).unwrap(),
    }
}
fn fits(v: &Value, b: [u16; 4]) -> bool {
    let encoded = serde_json::to_vec(v).unwrap();
    if encoded.len() > usize::from(b[0]) {
        return false;
    }
    let mut queue = VecDeque::from([(v, 1_usize)]);
    let mut members = 0;
    while let Some((v, d)) = queue.pop_front() {
        if d > usize::from(b[1]) {
            return false;
        }
        match v {
            Value::String(s) => {
                if s.len() > usize::from(b[3]) {
                    return false;
                }
            }
            Value::Array(a) => {
                members += a.len();
                for n in a {
                    queue.push_back((n, d + 1));
                }
            }
            Value::Object(a) => {
                members += a.len();
                for (k, n) in a {
                    if k.len() > usize::from(b[3]) {
                        return false;
                    }
                    queue.push_back((n, d + 1));
                }
            }
            _ => {}
        }
        if members > usize::from(b[2]) {
            return false;
        }
    }
    true
}
fn construct(n: &Node, l: JsonLimits) -> Result<BoundedJson, ProtocolError> {
    match n {
        Node::Null => Ok(BoundedJson::null()),
        Node::Bool(x) => Ok(BoundedJson::boolean(*x)),
        Node::Int(x) => Ok(BoundedJson::integer(*x)),
        Node::Text(s) => BoundedJson::string(s.clone(), l),
        Node::Array(a) => BoundedJson::array(
            a.iter().map(|v| construct(v, JsonLimits::PRODUCTION)).collect::<Result<_, _>>()?,
            l,
        ),
        Node::Object(a) => BoundedJson::object(
            a.iter()
                .map(|(k, v)| Ok((k.clone(), construct(v, JsonLimits::PRODUCTION)?)))
                .collect::<Result<_, ProtocolError>>()?,
            l,
        ),
    }
}
fn json_case(n: &Node, b: [u16; 4], whitespace: bool) -> Result<crate::Metrics, String> {
    if b.contains(&0) {
        return Err("setup-incomplete: zero tool JSON limit".into());
    }
    let l =
        JsonLimits::new(b[0] as usize, b[1] as usize, b[2] as usize, b[3] as usize).map_err(err)?;
    let source = if whitespace { format!(" \n{}\t", raw(n)) } else { raw(n) };
    let expected = value(n);
    let accept = expected.as_ref().is_ok_and(|v| fits(v, b)) && source.len() <= usize::from(b[0]);
    let actual = BoundedJson::parse(&source, l);
    if actual.is_ok() != accept {
        return Err(format!(
            "tool JSON byte/depth/member/string/duplicate oracle mismatch {n:?} {b:?}: {actual:?}"
        ));
    }
    if let Ok(actual) = actual {
        let expected = expected.as_ref().map_err(err)?;
        if actual.canonical_bytes() != serde_json::to_vec(expected).map_err(err)? {
            return Err("tool JSON canonical bytes differ from independent input value".into());
        }
    }
    // Scalar convenience constructors intentionally bypass byte limits; aggregates/string accept
    // explicit limits. Do not invent a global-limit promise for null()/integer()/boolean().
    if matches!(n, Node::Array(_) | Node::Object(_) | Node::Text(_)) {
        let built = construct(n, l);
        let accepted = expected.as_ref().is_ok_and(|v| fits(v, b));
        if built.is_ok() != accepted {
            return Err(
                "constructed tool JSON differs from full independent tree measurements".into()
            );
        }
        if let Ok(built) = built
            && built.canonical_bytes()
                != serde_json::to_vec(&expected.map_err(err)?).map_err(err)?
        {
            return Err("constructed tool JSON lost bytes".into());
        }
    }
    Ok(crate::Metrics::from([
        (if accept { "accepted_json" } else { "rejected_json" }.into(), 1),
        ("json_constructor_comparisons".into(), 1),
    ]))
}
fn native_shape(s: &Shape) -> Result<Schema, ProtocolError> {
    match s {
        Shape::Null => Ok(Schema::null()),
        Shape::Bool => Ok(Schema::boolean()),
        Shape::Int(a, b) => Schema::integer(*a, *b),
        Shape::Text(a, b) => Schema::string(*a, *b),
        Shape::Array(s, a, b) => Schema::array(native_shape(s)?, *a, *b),
        Shape::Object(p, x) => Schema::object(
            p.iter()
                .map(|(n, r, s)| SchemaProperty::new(n.clone(), native_shape(s)?, *r))
                .collect::<Result<_, _>>()?,
            *x,
        ),
        Shape::Enum(s, v) => native_shape(s)?.with_enum(
            v.iter().map(|n| construct(n, JsonLimits::PRODUCTION)).collect::<Result<_, _>>()?,
        ),
    }
}
fn shape_valid(s: &Shape) -> bool {
    match s {
        Shape::Null | Shape::Bool => true,
        Shape::Int(a, b) => !matches!((a,b),(Some(a),Some(b)) if a>b),
        Shape::Text(a, b) => a <= b,
        Shape::Array(s, a, b) => a <= b && shape_valid(s),
        Shape::Object(p, _) => {
            p.len() <= 256
                && p.windows(2).all(|w| w[0].0.as_bytes() < w[1].0.as_bytes())
                && p.iter().all(|(n, _, s)| {
                    !n.is_empty()
                        && n.len() <= 256
                        && !n.chars().any(char::is_control)
                        && shape_valid(s)
                })
        }
        Shape::Enum(s, v) => {
            shape_valid(s)
                && !v.is_empty()
                && v.len() <= 256
                && v.iter().all(|v| value(v).is_ok_and(|v| accepts(s, &v, 1)))
                && v.windows(2).all(|w| raw_canon(&w[0]) < raw_canon(&w[1]))
        }
    }
}
fn raw_canon(n: &Node) -> Vec<u8> {
    value(n).ok().and_then(|v| serde_json::to_vec(&v).ok()).unwrap_or_default()
}
fn accepts(s: &Shape, v: &Value, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match (s, v) {
        (Shape::Null, Value::Null) | (Shape::Bool, Value::Bool(_)) => true,
        (Shape::Int(a, b), Value::Number(n)) => {
            n.as_i64().is_some_and(|v| a.is_none_or(|a| v >= a) && b.is_none_or(|b| v <= b))
        }
        (Shape::Text(a, b), Value::String(v)) => v.len() >= *a as usize && v.len() <= *b as usize,
        (Shape::Array(s, a, b), Value::Array(v)) => {
            v.len() >= *a as usize
                && v.len() <= *b as usize
                && v.iter().all(|v| accepts(s, v, depth + 1))
        }
        (Shape::Object(p, x), Value::Object(v)) => {
            p.iter().all(|(n, r, _)| !*r || v.contains_key(n))
                && v.iter().all(|(n, v)| match p.iter().find(|(k, _, _)| k == n) {
                    Some((_, _, s)) => accepts(s, v, depth + 1),
                    None => *x,
                })
        }
        (Shape::Enum(s, vs), v) => {
            accepts(s, v, depth) && vs.iter().any(|n| value(n).is_ok_and(|n| n == *v))
        }
        _ => false,
    }
}
fn schema_value(s: &Shape) -> Value {
    match s {
        Shape::Null => json!({"type":"null"}),
        Shape::Bool => json!({"type":"boolean"}),
        Shape::Int(a, b) => {
            let mut v = json!({"type":"integer"});
            if let Some(a) = a {
                v["minimum"] = json!(a);
            }
            if let Some(b) = b {
                v["maximum"] = json!(b);
            }
            v
        }
        Shape::Text(a, b) => json!({"type":"string","minLength":a,"maxLength":b}),
        Shape::Array(s, a, b) => {
            json!({"type":"array","items":schema_value(s),"minItems":a,"maxItems":b})
        }
        Shape::Object(p, x) => {
            json!({"type":"object","additionalProperties":x,"properties":p.iter().map(|(n,_,s)|(n.clone(),schema_value(s))).collect::<BTreeMap<_,_>>(),"required":p.iter().filter(|(_,r,_)|*r).map(|(n,_,_)|n).collect::<Vec<_>>()})
        }
        Shape::Enum(s, values) => {
            let mut v = schema_value(s);
            v["enum"] = Value::Array(values.iter().map(|v| value(v).unwrap()).collect());
            v
        }
    }
}
fn schema_case(s: &Shape, n: &Node) -> Result<crate::Metrics, String> {
    let native = native_shape(s);
    if native.is_ok() != shape_valid(s) {
        return Err(format!(
            "schema constructor disagrees with independent type/range/order/enum predicates: {s:?}"
        ));
    }
    let mut m = crate::Metrics::new();
    let Ok(native) = native else {
        bump(&mut m, "rejected_schema_constructors");
        return Ok(m);
    };
    if native.canonical_bytes() != serde_json::to_vec(&schema_value(s)).map_err(err)? {
        return Err("schema canonical bytes differ from independent AST".into());
    }
    let expected = value(n);
    let input = BoundedJson::parse(&raw(n), JsonLimits::PRODUCTION);
    if expected.is_err() {
        if input.is_ok() {
            return Err("duplicate-key schema input was accepted".into());
        }
        bump(&mut m, "duplicate_schema_inputs");
        return Ok(m);
    }
    let expected = accepts(s, &expected.map_err(err)?, 1);
    let actual = native.validate(&input.map_err(err)?);
    if actual.is_ok() != expected {
        return Err(format!("schema semantic interpreter diverged s={s:?}, n={n:?}: {actual:?}"));
    }
    bump(&mut m, if expected { "accepted_schema_values" } else { "rejected_schema_values" });
    bump(&mut m, "independent_schema_byte_encodings");
    Ok(m)
}
#[path = "tools_compatibility.rs"]
mod compatibility_ref;

fn compatibility(a: &Shape, b: &Shape) -> Result<crate::Metrics, String> {
    compatibility_ref::check(a, b)
}
fn text(s: &str) -> Result<BoundedText, String> {
    BoundedText::new(s.into()).map_err(err)
}
fn hash(b: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(b).into())
}
fn header(f: u16, v: u16) -> Vec<u8> {
    [b"PTL1".as_slice(), &f.to_be_bytes(), &v.to_be_bytes()].concat()
}
fn sized(b: &mut Vec<u8>, v: &[u8]) {
    b.extend_from_slice(&(v.len() as u64).to_be_bytes());
    b.extend_from_slice(v);
}
fn revision() -> Result<RevisionTuple, String> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).map_err(err)?,
        HarnessId::new([2; 16]).map_err(err)?,
        WorkspaceId::new([3; 16]).map_err(err)?,
        Generation::new(2).map_err(err)?,
        RevisionNumber::new(3).map_err(err)?,
        PolicyId::new([4; 16]).map_err(err)?,
        ProviderProfileId::new([5; 16]).map_err(err)?,
    ))
}
fn revision_bytes() -> Vec<u8> {
    [
        vec![1; 16],
        vec![2; 16],
        vec![3; 16],
        2_u64.to_be_bytes().to_vec(),
        3_u64.to_be_bytes().to_vec(),
        vec![4; 16],
        vec![5; 16],
    ]
    .concat()
}
#[allow(clippy::too_many_arguments)]
fn envelope(
    seed: u8,
    fault: u8,
    descriptor_timeout: bool,
    call_timeout: bool,
    deadline: bool,
    control: u8,
    progress: u8,
    status: u8,
) -> Result<crate::Metrics, String> {
    let shape = Shape::Object(vec![("v".into(), true, Shape::Int(Some(0), Some(255)))], false);
    let schema = native_shape(&shape).map_err(err)?;
    let schema_bytes = serde_json::to_vec(&schema_value(&shape)).map_err(err)?;
    let schema_digest = hash(&schema_bytes);
    let name = CapabilityName::new("workspace.read".into()).map_err(err)?;
    let op = OperationDescriptor::new(
        name.clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).map_err(err)?,
    )
    .map_err(err)?;
    let mut max = ToolLimits::new(100, 128, 32, 32, 4, 2, 64).map_err(err)?;
    if !descriptor_timeout {
        max = max.without_timeout();
    }
    let control_mask = seed % 32;
    let controls = ControlSet::new(
        control_mask & 1 != 0,
        control_mask & 2 != 0,
        control_mask & 4 != 0,
        control_mask & 8 != 0,
        control_mask & 16 != 0,
    );
    if [controls.stdin(), controls.resize(), controls.signal(), controls.cancel(), controls.poll()]
        != std::array::from_fn(|bit| control_mask & (1 << bit) != 0)
    {
        return Err("control capability projection drift".into());
    }
    let descriptor = Arc::new(
        ToolDescriptor::new(
            name.clone(),
            SemanticVersion::new(1, 2, 3).map_err(err)?,
            schema,
            op,
            SideEffectClass::None,
            LeaseRequirement::None,
            IdempotencySemantics::ReplayTerminal,
            ImplementationIdentity::new("fixture.v1".into()).map_err(err)?,
            max,
            controls,
            ProtocolCompatibility::V1,
            text("fixture")?,
        )
        .map_err(err)?,
    );
    let mut d = if descriptor_timeout {
        b"peritus.tool-descriptor.v1\0".to_vec()
    } else {
        b"peritus.tool-descriptor.v2\0".to_vec()
    };
    sized(&mut d, b"workspace.read");
    for n in [1_u16, 2, 3] {
        d.extend_from_slice(&n.to_be_bytes());
    }
    sized(&mut d, &schema_bytes);
    d.extend_from_slice(schema_digest.as_bytes());
    d.extend_from_slice(&[1, 1, 255, 1, 1, 1]);
    sized(&mut d, b"fixture.v1");
    d.extend_from_slice(&(if descriptor_timeout { 100_u64 } else { 0 }).to_be_bytes());
    d.extend_from_slice(&128_u64.to_be_bytes());
    for n in [32_u32, 32, 4] {
        d.extend_from_slice(&n.to_be_bytes());
    }
    d.extend_from_slice(&2_u16.to_be_bytes());
    d.extend_from_slice(&64_u32.to_be_bytes());
    d.push(control_mask);
    d.extend_from_slice(&[0, 1, 0, 1]);
    sized(&mut d, b"fixture");
    if descriptor.canonical_bytes() != d
        || descriptor.descriptor_digest().as_bytes() != hash(&d).as_bytes()
    {
        return Err("descriptor bytes/digest differ from independent bound metadata".into());
    }
    let timeout = if call_timeout { Some(if fault == 1 { 101 } else { 50 }) } else { None };
    let out = if fault == 2 { 129 } else { 64 };
    let model = if fault == 3 { 33 } else { 16 };
    let human = if fault == 4 { 33 } else { 16 };
    let events = if fault == 5 { 5 } else { 3 };
    let artifacts = if fault == 6 { 3 } else { 1 };
    let limits =
        CallLimits::new_optional(timeout, out, model, human, events, artifacts).map_err(err)?;
    let arg = if fault == 7 { json!({"v":"wrong"}) } else { json!({"v":seed}) };
    let arg_bytes = serde_json::to_vec(&arg).map_err(err)?;
    let args =
        BoundedJson::parse(std::str::from_utf8(&arg_bytes).map_err(err)?, JsonLimits::PRODUCTION)
            .map_err(err)?;
    let epoch = Generation::new(2).map_err(err)?;
    let time = AuthorityInstant::new(epoch, 100);
    let action = ActionId::new([9; 16]).map_err(err)?;
    let call = ToolCall::new_with_lifetime(
        action,
        if fault == 8 { CapabilityName::new("other".into()).map_err(err)? } else { name },
        SemanticVersion::new(if fault == 9 { 2 } else { 1 }, 2, 3).map_err(err)?,
        args,
        limits,
        revision()?,
        if deadline {
            CallLifetime::Deadline(time)
        } else {
            CallLifetime::UntilCancelled { epoch }
        },
        IdempotencyKey::new(format!("key-{seed}")).map_err(err)?,
    );
    let valid = (fault == 0 || (fault == 1 && (!descriptor_timeout || !call_timeout)))
        && (!descriptor_timeout || call_timeout)
        && (deadline || !call_timeout);
    let prepared = prepare_call(descriptor, call.clone());
    if prepared.is_ok() != valid {
        return Err(format!(
            "prepared-call identity/schema/ceilings mismatch fault{fault}: {prepared:?}"
        ));
    }
    let mut m = crate::Metrics::new();
    if !valid {
        bump(&mut m, "rejected_prepared_bindings");
        return Ok(m);
    }
    let p = prepared.map_err(err)?;
    let mut lb = Vec::new();
    lb.extend_from_slice(&timeout.unwrap_or(0).to_be_bytes());
    lb.extend_from_slice(&out.to_be_bytes());
    for n in [model, human, events] {
        lb.extend_from_slice(&n.to_be_bytes());
    }
    lb.extend_from_slice(&artifacts.to_be_bytes());
    let legacy = deadline && call_timeout;
    let mut c = header(2, if legacy { 1 } else { 2 });
    c.extend_from_slice(&[9; 16]);
    sized(&mut c, b"workspace.read");
    for n in [1_u16, 2, 3] {
        c.extend_from_slice(&n.to_be_bytes());
    }
    sized(&mut c, &arg_bytes);
    c.extend_from_slice(&lb);
    c.extend_from_slice(&revision_bytes());
    if !legacy {
        c.push(u8::from(deadline));
    }
    c.extend_from_slice(&2_u64.to_be_bytes());
    if deadline {
        c.extend_from_slice(&100_u64.to_be_bytes());
    }
    sized(&mut c, format!("key-{seed}").as_bytes());
    if call.canonical_bytes() != c {
        return Err("exact C4 v1/v2 call encoding drift".into());
    }
    let mut pre = if legacy {
        b"peritus.prepared-tool-call.v1\0".to_vec()
    } else {
        b"peritus.prepared-tool-call.v2\0".to_vec()
    };
    pre.extend_from_slice(&[9; 16]);
    pre.extend_from_slice(hash(&d).as_bytes());
    pre.extend_from_slice(hash(&arg_bytes).as_bytes());
    pre.extend_from_slice(&lb);
    pre.extend_from_slice(&revision_bytes());
    if !legacy {
        pre.push(u8::from(deadline));
    }
    pre.extend_from_slice(&2_u64.to_be_bytes());
    if deadline {
        pre.extend_from_slice(&100_u64.to_be_bytes());
    }
    sized(&mut pre, format!("key-{seed}").as_bytes());
    let pre_hash = hash(&pre);
    let mut replay = b"peritus.tool-replay.v1\0".to_vec();
    replay.extend_from_slice(&[9; 16]);
    sized(&mut replay, b"workspace.read");
    for n in [1_u16, 2, 3] {
        replay.extend_from_slice(&n.to_be_bytes());
    }
    replay.extend_from_slice(hash(&d).as_bytes());
    replay.extend_from_slice(hash(&arg_bytes).as_bytes());
    replay.extend_from_slice(pre_hash.as_bytes());
    replay.extend_from_slice(&lb);
    replay.extend_from_slice(&revision_bytes());
    sized(&mut replay, format!("key-{seed}").as_bytes());
    if p.prepared_digest() != pre_hash || p.replay_identity().digest() != hash(&replay) {
        return Err("independent prepared/replay preimage digest drift".into());
    }
    let mut pb = header(3, 1);
    sized(&mut pb, &c);
    pb.extend_from_slice(hash(&d).as_bytes());
    pb.extend_from_slice(hash(&arg_bytes).as_bytes());
    pb.extend_from_slice(pre_hash.as_bytes());
    pb.extend_from_slice(hash(&replay).as_bytes());
    if p.canonical_bytes() != pb {
        return Err("prepared envelope lost exact bindings".into());
    }
    let (kind, tag) = match progress % 5 {
        0 => (ProgressKind::Started, 1),
        1 => (ProgressKind::Update, 2),
        2 => (ProgressKind::Control, 3),
        3 => (ProgressKind::Stopping, 4),
        _ => (ProgressKind::Recovery, 5),
    };
    let observed = AuthorityInstant::new(epoch, 77);
    let sequence = u32::from(progress % 4);
    let prog = ToolProgress::new(
        &p,
        sequence,
        kind,
        observed,
        Some(BoundedJson::boolean(true)),
        text("p")?,
    );
    if prog.is_ok() != (sequence < events) {
        return Err("progress accepted outside exact sequence capacity".into());
    }
    if let Ok(prog) = prog {
        let mut b = header(4, 1);
        b.extend_from_slice(&[9; 16]);
        b.extend_from_slice(pre_hash.as_bytes());
        b.extend_from_slice(&sequence.to_be_bytes());
        b.push(tag);
        b.extend_from_slice(&2_u64.to_be_bytes());
        b.extend_from_slice(&77_u64.to_be_bytes());
        b.push(1);
        sized(&mut b, b"true");
        sized(&mut b, b"p");
        if prog.canonical_bytes() != b {
            return Err("progress encoding drift".into());
        }
        bump(&mut m, "exact_progress_envelopes");
        bump(&mut m, format!("progress_kind_{tag}"));
    }
    if ToolProgress::new(
        &p,
        0,
        kind,
        AuthorityInstant::new(Generation::new(3).map_err(err)?, 77),
        None,
        text("p")?,
    )
    .is_ok()
    {
        return Err("progress lost authority epoch binding".into());
    }
    let (reason, rtag) = match (control / 5) % 4 {
        0 => (CancellationReason::Requested, 1),
        1 => (CancellationReason::Deadline, 2),
        2 => (CancellationReason::Shutdown, 3),
        _ => (CancellationReason::Recovery, 4),
    };
    let (ctrl, body) = match control % 5 {
        0 => (ToolControl::Poll, vec![1]),
        1 => (
            ToolControl::stdin(vec![0, 255], 64).map_err(err)?,
            vec![2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 255],
        ),
        2 => (ToolControl::resize(17, 29).map_err(err)?, vec![3, 0, 17, 0, 29]),
        3 => {
            let mut b = vec![4];
            sized(&mut b, b"TERM");
            (ToolControl::Signal(text("TERM")?), b)
        }
        _ => (ToolControl::Cancel(reason), vec![5, rtag]),
    };
    let mut cb = header(7, 1);
    cb.extend_from_slice(&body);
    if ctrl.canonical_bytes() != cb
        || ToolControl::stdin(vec![], 64).is_ok()
        || ToolControl::stdin(vec![1; 65], 64).is_ok()
        || ToolControl::stdin(vec![1; 64], 64).is_err()
        || ToolControl::resize(0, 1).is_ok()
    {
        return Err("control variant/bound encoding drift".into());
    }
    bump(&mut m, format!("control_variant_{}", control % 5));
    if control % 5 == 4 {
        bump(&mut m, format!("cancel_reason_{rtag}"));
    }
    let categories = [
        FailureCategory::Protocol,
        FailureCategory::Authorization,
        FailureCategory::Workspace,
        FailureCategory::Execution,
        FailureCategory::Artifact,
        FailureCategory::Infrastructure,
        FailureCategory::Cancelled,
        FailureCategory::Timeout,
        FailureCategory::Indeterminate,
    ];
    let subsystems = [
        ResponsibleSubsystem::Protocol,
        ResponsibleSubsystem::Router,
        ResponsibleSubsystem::Workspace,
        ResponsibleSubsystem::Process,
        ResponsibleSubsystem::Sandbox,
        ResponsibleSubsystem::ArtifactStore,
        ResponsibleSubsystem::Tool,
    ];
    let retries = [Retryability::Never, Retryability::NewAction, Retryability::AfterRecovery];
    let routes = [
        RecoveryRoute::None,
        RecoveryRoute::Reauthorize,
        RecoveryRoute::ReconcileWorkspace,
        RecoveryRoute::ReconcileProcess,
        RecoveryRoute::RepublishArtifact,
        RecoveryRoute::HumanReview,
    ];
    let category_tag = seed % 9 + 1;
    let subsystem_tag = (seed / 9) % 7 + 1;
    let retry_tag = (seed / 7) % 3 + 1;
    let route_tag = (seed / 3) % 6 + 1;
    let failure = ToolFailure::new(
        categories[usize::from(category_tag - 1)],
        text("code")?,
        subsystems[usize::from(subsystem_tag - 1)],
        retries[usize::from(retry_tag - 1)],
        routes[usize::from(route_tag - 1)],
        text("detail")?,
    );
    let mut fb = header(9, 1);
    fb.push(category_tag);
    sized(&mut fb, b"code");
    fb.extend_from_slice(&[subsystem_tag, retry_tag, route_tag]);
    sized(&mut fb, b"detail");
    if failure.canonical_bytes() != fb {
        return Err("failure envelope exact bytes drift".into());
    }
    let status = match status % 5 {
        0 => ResultStatus::Succeeded,
        1 => ResultStatus::Failed,
        2 => ResultStatus::Cancelled,
        3 => ResultStatus::TimedOut,
        _ => ResultStatus::Indeterminate,
    };
    let timing =
        ToolTiming::new(AuthorityInstant::new(epoch, 20), AuthorityInstant::new(epoch, 21))
            .map_err(err)?;
    let truncations = [
        Truncation::Complete,
        Truncation::TailDropped,
        Truncation::HeadDropped,
        Truncation::Windowed,
        Truncation::Indeterminate,
    ];
    let truncation_tags = [seed % 5 + 1, (seed / 5) % 5 + 1, (seed / 25) % 5 + 1];
    let truncation = TruncationMetadata {
        output: truncations[usize::from(truncation_tags[0] - 1)],
        model: truncations[usize::from(truncation_tags[1] - 1)],
        human: truncations[usize::from(truncation_tags[2] - 1)],
    };
    let complete = match seed % 3 {
        0 => ArtifactCompleteness::Complete,
        1 => ArtifactCompleteness::Truncated,
        _ => ArtifactCompleteness::Indeterminate,
    };
    let artifact = ArtifactReference::new(
        Sha256Digest::new([7; 32]),
        5,
        text("application/octet-stream")?,
        text("artifact")?,
        complete,
        ArtifactProvenance::new(action, pre_hash),
    )
    .map_err(err)?;
    let mut ab = header(5, 1);
    ab.extend_from_slice(&[7; 32]);
    ab.extend_from_slice(&5_u64.to_be_bytes());
    sized(&mut ab, b"application/octet-stream");
    sized(&mut ab, b"artifact");
    ab.push(seed % 3 + 1);
    ab.extend_from_slice(&[9; 16]);
    ab.extend_from_slice(pre_hash.as_bytes());
    if artifact.canonical_bytes() != ab {
        return Err("artifact reference lost independent bytes/provenance".into());
    }
    let bad = ArtifactReference::new(
        Sha256Digest::new([7; 32]),
        5,
        text("application/octet-stream")?,
        text("artifact")?,
        complete,
        ArtifactProvenance::new(ActionId::new([8; 16]).map_err(err)?, pre_hash),
    )
    .map_err(err)?;
    if ToolResult::success(
        &p,
        BoundedJson::boolean(true),
        text("h")?,
        text("m")?,
        vec![bad],
        timing,
        truncation,
        0,
    )
    .is_ok()
    {
        return Err("terminal artifact accepted another action provenance".into());
    }
    let wrong_digest = ArtifactReference::new(
        Sha256Digest::new([7; 32]),
        5,
        text("application/octet-stream")?,
        text("artifact")?,
        complete,
        ArtifactProvenance::new(action, Sha256Digest::new([6; 32])),
    )
    .map_err(err)?;
    if ToolResult::success(
        &p,
        BoundedJson::boolean(true),
        text("h")?,
        text("m")?,
        vec![wrong_digest],
        timing,
        truncation,
        0,
    )
    .is_ok()
    {
        return Err("terminal artifact accepted another prepared-call digest".into());
    }
    if ArtifactReference::new(
        Sha256Digest::new([7; 32]),
        0,
        text("text/plain")?,
        text("empty")?,
        complete,
        ArtifactProvenance::new(action, pre_hash),
    )
    .is_ok()
    {
        return Err("zero-size artifact accepted".into());
    }
    let result = if status == ResultStatus::Succeeded {
        ToolResult::success(
            &p,
            BoundedJson::boolean(true),
            text("h")?,
            text("m")?,
            vec![artifact.clone()],
            timing,
            truncation,
            events,
        )
    } else {
        ToolResult::failure(
            &p,
            status,
            failure,
            Some(BoundedJson::boolean(true)),
            text("h")?,
            text("m")?,
            vec![artifact.clone()],
            timing,
            truncation,
            events,
        )
    }
    .map_err(err)?;
    let mut rb = header(6, 1);
    rb.extend_from_slice(&[9; 16]);
    rb.extend_from_slice(hash(&d).as_bytes());
    rb.extend_from_slice(pre_hash.as_bytes());
    rb.extend_from_slice(hash(&replay).as_bytes());
    rb.push(match status {
        ResultStatus::Succeeded => 1,
        ResultStatus::Failed => 2,
        ResultStatus::Cancelled => 3,
        ResultStatus::TimedOut => 4,
        ResultStatus::Indeterminate => 5,
    });
    rb.push(1);
    sized(&mut rb, b"true");
    if status == ResultStatus::Succeeded {
        rb.push(0);
    } else {
        rb.push(1);
        sized(&mut rb, &fb);
    }
    sized(&mut rb, b"h");
    sized(&mut rb, b"m");
    rb.extend_from_slice(&1_u16.to_be_bytes());
    sized(&mut rb, &ab);
    for n in [2_u64, 20, 2, 21] {
        rb.extend_from_slice(&n.to_be_bytes());
    }
    rb.extend_from_slice(&truncation_tags);
    rb.extend_from_slice(&events.to_be_bytes());
    if result.canonical_bytes() != rb
        || result.action_id() != action
        || result.prepared_digest() != pre_hash
        || result.replay_identity().digest() != hash(&replay)
    {
        return Err("terminal result lost exact independent bytes or action identity".into());
    }
    for bytes in [&c, &pb, &rb, &cb, &fb, &ab] {
        let parsed = CanonicalEnvelope::parse(bytes, bytes.len()).map_err(err)?;
        if parsed.canonical_bytes() != bytes
            || CanonicalEnvelope::parse(bytes, bytes.len() - 1).is_ok()
        {
            return Err("canonical envelope framing/capacity drift".into());
        }
    }
    bump(&mut m, "exact_prepared_calls");
    bump(&mut m, format!("call_wire_v{}", if legacy { 1 } else { 2 }));
    bump(&mut m, format!("result_status_{status:?}"));
    bump(&mut m, "independent_envelope_preimages");
    bump(&mut m, format!("artifact_completeness_{}", seed % 3));
    bump(&mut m, format!("failure_category_{category_tag}"));
    bump(&mut m, format!("failure_subsystem_{subsystem_tag}"));
    for (channel, tag) in ["output", "model", "human"].into_iter().zip(truncation_tags) {
        bump(&mut m, format!("truncation_{channel}_{tag}"));
    }
    Ok(m)
}
pub fn check(c: &Case) -> Result<crate::Metrics, String> {
    match c {
        Case::Json { node, bounds, whitespace } => json_case(node, *bounds, *whitespace),
        Case::Raw { input } => {
            if BoundedJson::parse(input, JsonLimits::PRODUCTION).is_ok() {
                Err("unsupported duplicate/numeric/malformed tool JSON accepted".into())
            } else {
                Ok(crate::Metrics::from([("rejected_raw_json".into(), 1)]))
            }
        }
        Case::Schema { shape, node } => schema_case(shape, node),
        Case::Compatibility { before, after } => compatibility(before, after),
        Case::Envelope {
            seed,
            fault,
            descriptor_timeout,
            call_timeout,
            deadline,
            control,
            progress,
            status,
        } => envelope(
            *seed,
            *fault,
            *descriptor_timeout,
            *call_timeout,
            *deadline,
            *control,
            *progress,
            *status,
        ),
        Case::Header { family, version, payload, ceiling, corrupt } => {
            let mut b = header(*family, *version);
            b.extend_from_slice(payload);
            if *corrupt {
                b[0] ^= 1;
            }
            let expected = !*corrupt
                && *family != 0
                && (*version == 1 || (*family == 2 && *version == 2))
                && *ceiling != 0
                && b.len() <= usize::from(*ceiling);
            let actual = CanonicalEnvelope::parse(&b, usize::from(*ceiling));
            if actual.is_ok() != expected {
                return Err("open-family canonical header grammar drift".into());
            }
            if let Ok(v) = actual
                && (v.payload() != payload || v.family() != *family || v.version() != *version)
            {
                return Err("canonical envelope lost opaque family payload".into());
            }
            Ok(crate::Metrics::from([("canonical_header_checks".into(), 1)]))
        }
    }
}
pub fn calibration() -> Case {
    Case::Schema {
        shape: Shape::Object(vec![("a".into(), true, Shape::Bool)], false),
        node: Node::Object(vec![]),
    }
}
pub fn examples() -> Vec<Case> {
    let mut v = vec![calibration()];
    for s in [
        Shape::Null,
        Shape::Bool,
        Shape::Int(Some(i64::MIN), Some(i64::MAX)),
        Shape::Text(1, 4),
        Shape::Array(Box::new(Shape::Bool), 1, 2),
        Shape::Enum(Box::new(Shape::Bool), vec![Node::Bool(false), Node::Bool(true)]),
    ] {
        for n in [
            Node::Null,
            Node::Bool(true),
            Node::Int(i64::MAX),
            Node::Text("é🦀".into()),
            Node::Array(vec![Node::Bool(false)]),
        ] {
            v.push(Case::Schema { shape: s.clone(), node: n });
        }
    }
    for fault in 0..10 {
        for (descriptor_timeout, call_timeout, deadline) in
            [(true, true, true), (false, false, false), (false, false, true), (false, true, true)]
        {
            v.push(Case::Envelope {
                seed: 7,
                fault,
                descriptor_timeout,
                call_timeout,
                deadline,
                control: fault,
                progress: 1,
                status: fault % 5,
            });
        }
    }
    for seed in 0..=125 {
        v.push(Case::Envelope {
            seed,
            fault: 0,
            descriptor_timeout: true,
            call_timeout: true,
            deadline: true,
            control: seed % 20,
            progress: seed % 10,
            status: seed % 5,
        });
    }
    let before = Shape::Object(vec![("a".into(), true, Shape::Bool)], false);
    let after = Shape::Object(
        vec![("a".into(), true, Shape::Bool), ("b".into(), false, Shape::Text(0, 8))],
        false,
    );
    for (a, b) in
        [(before.clone(), before.clone()), (before.clone(), after.clone()), (after, before)]
    {
        v.push(Case::Compatibility { before: a, after: b });
    }
    for bound in 1..8 {
        v.push(Case::Json {
            node: Node::Object(vec![("é".into(), Node::Array(vec![Node::Text("🦀".into())]))]),
            bounds: [bound * 8, bound, bound, bound],
            whitespace: bound % 2 == 0,
        });
    }
    for input in ["{\"x\":0,\"\\u0078\":1}", "1.0", "1e3", "18446744073709551615"] {
        v.push(Case::Raw { input: input.into() });
    }
    v.extend(compatibility_ref::examples());
    v
}
