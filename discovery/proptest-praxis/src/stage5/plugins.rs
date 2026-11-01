//! SDK value/frame contracts and actual isolated process-host histories.
use peritus_plugin_host::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, DigestTrustStore, DiscoveryLimits,
    HostCancellation, HostConfig, HostError, HostFailureClass, HostFuture, InvocationGrant,
    InvocationSubject, PluginHost, PluginInvocationResult, PluginLifecycle, RecoveryDisposition,
    discover,
};
use peritus_plugin_sdk::{
    FailureClass, JsonBounds, JsonPayload, ManifestDigest, PluginId, PluginManifest, PluginQuotas,
    PluginResponseEnvelope, PluginVersion, ProtocolRange, RequestId, decode_frame, encode_frame,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Sdk {
        depth: u8,
        width: u8,
        text: String,
        limit: u16,
        member_limit: u8,
        depth_limit: u8,
        string_limit: u8,
        fault: u8,
    },
    Host {
        operations: Vec<Operation>,
        request_limit: u8,
        trust: u8,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Start,
    Stop,
    Authority { generation: u8, enabled: bool },
    Invoke { mode: u8, value: u8, generation: u8, declared: bool, pre_cancel: bool },
    CancelAfterSend { value: u8 },
    Read,
}
fn debug(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn bump(m: &mut crate::Metrics, k: &str) {
    *m.entry(k.into()).or_default() += 1;
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let op = prop_oneof![2=>Just(Operation::Start),2=>Just(Operation::Stop),1=>Just(Operation::Read),
  2=>(0_u8..3,any::<bool>()).prop_map(|(generation,enabled)|Operation::Authority{generation,enabled}),
  9=>(0_u8..15,any::<u8>(),0_u8..3,any::<bool>(),any::<bool>()).prop_map(|(mode,value,generation,declared,pre_cancel)|Operation::Invoke{mode,value,generation,declared,pre_cancel}),
  1=>any::<u8>().prop_map(|value|Operation::CancelAfterSend{value})];
    let steps = max_steps.min(48);
    let broad = (prop::collection::vec(op.clone(), 0..=steps), 1_u8..7, 0_u8..4).prop_map(
        |(operations, request_limit, trust)| Case::Host { operations, request_limit, trust },
    );
    let warm =
        (prop::collection::vec(op, 0..=steps.saturating_sub(7)), any::<u8>(), 0_u8..15, 1_u8..7)
            .prop_map(move |(mut suffix, value, mode, request_limit)| {
                let mut operations = vec![
                    Operation::Start,
                    Operation::Invoke {
                        mode: 0,
                        value,
                        generation: 0,
                        declared: true,
                        pre_cancel: false,
                    },
                    Operation::Authority { generation: 1, enabled: true },
                    Operation::Invoke {
                        mode: 0,
                        value,
                        generation: 0,
                        declared: true,
                        pre_cancel: false,
                    },
                    Operation::Invoke {
                        mode,
                        value,
                        generation: 1,
                        declared: true,
                        pre_cancel: false,
                    },
                    Operation::Stop,
                    Operation::Start,
                ];
                operations.append(&mut suffix);
                operations.truncate(steps);
                Case::Host { operations, request_limit, trust: 1 }
            });
    let cancel = any::<u8>().prop_map(move |value| {
        let mut operations = vec![
            Operation::Start,
            Operation::CancelAfterSend { value },
            Operation::Read,
            Operation::Stop,
            Operation::Start,
            Operation::Invoke { mode: 0, value, generation: 0, declared: true, pre_cancel: false },
            Operation::Stop,
        ];
        operations.truncate(steps);
        Case::Host { operations, request_limit: 4, trust: 1 }
    });
    let sdk = (
        0_u8..5,
        0_u8..6,
        prop::collection::vec(
            prop_oneof![Just('é'), Just('🦀'), Just('"'), Just('\n'), prop::char::range('a', 'z')],
            0..12,
        ),
        0_u16..260,
        0_u8..35,
        0_u8..8,
        0_u8..32,
        0_u8..8,
    )
        .prop_map(
            |(depth, width, chars, limit, member_limit, depth_limit, string_limit, fault)| {
                Case::Sdk {
                    depth,
                    width,
                    text: chars.into_iter().collect(),
                    limit,
                    member_limit,
                    depth_limit,
                    string_limit,
                    fault,
                }
            },
        );
    prop_oneof![3=>broad,5=>warm,2=>cancel,5=>sdk].boxed()
}
fn payload(depth: u8, width: u8, text: &str) -> Value {
    let mut value = json!({"alpha":text,"omega":(0..width).collect::<Vec<_>>()});
    for _ in 0..depth {
        value = Value::Array(vec![value]);
    }
    value
}
fn payload_model(value: &Value, bounds: JsonBounds) -> bool {
    let mut queue = VecDeque::from([(value, 1_usize)]);
    let mut members = 0_usize;
    while let Some((value, depth)) = queue.pop_front() {
        if depth > bounds.max_depth {
            return false;
        }
        match value {
            Value::Number(n) if !n.is_i64() && !n.is_u64() => return false,
            Value::String(s) if s.len() > bounds.max_string_bytes => return false,
            Value::Array(a) => {
                members += a.len();
                queue.extend(a.iter().map(|v| (v, depth + 1)));
            }
            Value::Object(o) => {
                members += o.len();
                if o.keys().any(|k| k.len() > bounds.max_string_bytes) {
                    return false;
                }
                queue.extend(o.values().map(|v| (v, depth + 1)));
            }
            _ => {}
        }
        if members > bounds.max_members {
            return false;
        }
    }
    serde_json::to_vec(value).is_ok_and(|v| v.len() <= bounds.max_bytes)
}
struct SdkInput<'a> {
    depth: u8,
    width: u8,
    text: &'a str,
    limit: u16,
    member_limit: u8,
    depth_limit: u8,
    string_limit: u8,
    fault: u8,
}
fn sdk(input: SdkInput<'_>) -> Result<crate::Metrics, String> {
    let SdkInput { depth, width, text, limit, member_limit, depth_limit, string_limit, fault } =
        input;
    let value = payload(depth, width, text);
    let mut metrics = crate::Metrics::new();
    let bounds = JsonBounds {
        max_bytes: usize::from(limit),
        max_depth: usize::from(depth_limit),
        max_members: usize::from(member_limit),
        max_string_bytes: usize::from(string_limit),
    };
    let expected = payload_model(&value, bounds);
    let result = JsonPayload::new(value.clone(), bounds);
    match result {
        Ok(actual) => {
            if !expected
                || actual.value() != &value
                || actual.canonical_bytes() != serde_json::to_vec(&value).map_err(debug)?
            {
                return Err("payload acceptance/value/canonical bytes disagree with independent BFS/JSON oracle".into());
            }
            bump(&mut metrics, "accepted_bounded_payloads");
        }
        Err(_) => {
            if expected {
                return Err("valid bounded payload rejected".into());
            }
            bump(&mut metrics, "rejected_bounded_payloads");
        }
    }
    let encoded = serde_json::to_vec(&value).map_err(debug)?;
    let mut frame = Vec::new();
    frame.extend_from_slice(&u32::try_from(encoded.len()).map_err(debug)?.to_be_bytes());
    frame.extend_from_slice(&encoded);
    let frame_limit = u32::try_from(encoded.len()).map_err(debug)?;
    if encode_frame(&value, frame_limit).map_err(debug)? != frame {
        return Err("frame bytes differ from independent four-byte length and JSON".into());
    }
    if decode_frame::<Value>(&frame, frame_limit).map_err(debug)? != value {
        return Err("exact frame changed value".into());
    }
    if frame_limit > 0 && encode_frame(&value, frame_limit - 1).is_ok() {
        return Err("frame one below size accepted".into());
    }
    let mut malformed = frame.clone();
    match fault % 8 {
        0 => {
            malformed.truncate(3);
        }
        1 => {
            malformed[0..4].copy_from_slice(&0_u32.to_be_bytes());
        }
        2 => {
            malformed.push(0);
        }
        3 => {
            malformed.pop();
        }
        4 => {
            malformed[0..4].copy_from_slice(&(frame_limit + 1).to_be_bytes());
        }
        5 => {
            malformed[4] = 255;
        }
        6 => {
            malformed[0..4].copy_from_slice(&u32::MAX.to_be_bytes());
        }
        _ => {
            malformed[4] = b'!';
        }
    }
    if decode_frame::<Value>(&malformed, frame_limit).is_ok() {
        return Err("malformed/truncated/oversized frame accepted".into());
    }
    bump(&mut metrics, "frame_exact_boundary_checks");
    bump(&mut metrics, "malformed_frame_rejections");
    let parsed = PluginManifest::parse_toml(&manifest(4)).map_err(debug)?;
    let expected_manifest = json!({"manifest_version":1,"id":"fixture.probe","version":{"major":1,"minor":0,"patch":0},"kind":"process","protocol":{"minimum":1,"maximum":1},"entrypoint":{"artifact":"plugin","arguments":[]},"capabilities":[{"name":"fixture.inspect","operation":"inspection","required":true}],"quotas":{"concurrent_requests":2,"frame_bytes":4096,"output_bytes":128,"invocation_millis":300,"lifecycle_requests":4,"protocol_violations":1}});
    let manifest_bytes = serde_json::to_vec(&expected_manifest).map_err(debug)?;
    if parsed.canonical_bytes().map_err(debug)? != manifest_bytes
        || parsed.digest().map_err(debug)?.as_bytes()
            != &<[u8; 32]>::from(Sha256::digest(&manifest_bytes))
    {
        return Err(
            "manifest canonical bytes or digest differ from independent exact fields".into()
        );
    }
    let artifact = <[u8; 32]>::from(Sha256::digest(SCRIPT.as_bytes()));
    let mut preimage = b"peritus-plugin-trust-v1\0".to_vec();
    preimage.extend_from_slice(&u64::try_from(manifest_bytes.len()).map_err(debug)?.to_be_bytes());
    preimage.extend_from_slice(&manifest_bytes);
    preimage.extend_from_slice(&artifact);
    if parsed.trust_material(artifact).map_err(debug)?.signature_preimage() != preimage {
        return Err("trust preimage failed exact manifest and artifact binding".into());
    }
    for invalid in [
        format!("{}unknown=true\n", manifest(4)),
        manifest(4).replace("artifact=\"plugin\"", "artifact=\"../plugin\""),
        manifest(4).replace(
            "capabilities=[",
            "capabilities=[{name=\"fixture.inspect\",operation=\"inspection\",required=true},",
        ),
    ] {
        if PluginManifest::parse_toml(&invalid).is_ok() {
            return Err("unknown/traversing/duplicate-capability manifest accepted".into());
        }
    }
    let desired = parsed.quotas();
    let ceiling = PluginQuotas {
        concurrent_requests: 1,
        frame_bytes: 2048,
        output_bytes: 256,
        invocation_millis: 100,
        lifecycle_requests: 2,
        protocol_violations: 3,
    };
    let actual = desired.narrow(ceiling);
    if [
        u64::from(actual.concurrent_requests),
        u64::from(actual.frame_bytes),
        actual.output_bytes,
        actual.invocation_millis,
        actual.lifecycle_requests,
        u64::from(actual.protocol_violations),
    ] != [1, 2048, 128, 100, 2, 1]
    {
        return Err("host quota intersection differs from independent component table".into());
    }
    bump(&mut metrics, "manifest_identity_and_trust_preimages");
    bump(&mut metrics, "all_quota_dimension_intersections");
    // The retained wire fixture is independent JSON, decoded as a closed typed envelope.
    let wire = json!({"protocol_version":1,"request_id":"fixture-17","response":{"kind":"status","body":{"status":"ready"}}});
    let response: PluginResponseEnvelope = serde_json::from_value(wire.clone()).map_err(debug)?;
    if serde_json::to_value(response).map_err(debug)? != wire {
        return Err("typed response fixture changed identity or tag".into());
    }
    let mut unknown = wire;
    unknown["unknown"] = json!(true);
    if serde_json::from_value::<PluginResponseEnvelope>(unknown).is_ok() {
        return Err("unknown envelope field accepted".into());
    }
    for (minimum, maximum, other_min, other_max) in
        [(1, 3, 2, 4), (1, 1, 2, 2), (u16::MAX, u16::MAX, 1, u16::MAX)]
    {
        let a = ProtocolRange::new(minimum, maximum).map_err(debug)?;
        let b = ProtocolRange::new(other_min, other_max).map_err(debug)?;
        let expected = (minimum..=maximum).rev().find(|v| *v >= other_min && *v <= other_max);
        match (expected, a.negotiate(b)) {
            (Some(v), Ok(actual)) if v == actual => {}
            (None, Err(_)) => {}
            _ => return Err("protocol range intersection differs from explicit membership".into()),
        }
    }
    bump(&mut metrics, "closed_wire_fixture_checks");
    Ok(metrics)
}

// Ordinary dependency-port fixture: this table records fixture authorization, not B1/C4 permits.
#[derive(Clone)]
struct Authorization {
    generation: u8,
    enabled: bool,
    calls: u64,
}
struct Authority(Mutex<Authorization>);
impl AuthorityMediator for Authority {
    fn authorize<'a>(
        &'a self,
        r: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        Box::pin(async move {
            let mut table = self.0.lock().map_err(|_| {
                HostError::new(
                    HostFailureClass::Infrastructure,
                    RecoveryDisposition::None,
                    "fixture authorization",
                    "fixture table poisoned",
                )
            })?;
            table.calls += 1;
            let valid = table.enabled
                && r.plugin_id().as_str() == "fixture.probe"
                && r.capability().name() == "fixture.inspect"
                && r.subject().session_id() == "fixture-session"
                && r.subject().actor_id() == "fixture-actor"
                && r.subject().authority_generation() == u64::from(table.generation);
            Ok(if valid {
                AuthorityDecision::Authorized(InvocationGrant::observed(
                    vec!["fixture.inspect".into()],
                    9000,
                ))
            } else {
                AuthorityDecision::Denied {
                    code: "fixture-stale".into(),
                    detail: "Exact fixture binding is not in the reference authorization table"
                        .into(),
                }
            })
        })
    }
}
const SCRIPT: &str = r##"#!/usr/bin/python3
import json,os,struct,sys,time
root=os.path.dirname(os.path.abspath(__file__))
with open(root+'/pids.log','a') as f:f.write(str(os.getpid())+'\n');f.flush()
def readn(n):
 out=b''
 while len(out)<n:
  p=sys.stdin.buffer.read(n-len(out))
  if not p:return None
  out+=p
 return out
def send(x):
 p=json.dumps(x,ensure_ascii=False,separators=(',',':')).encode();frame=struct.pack('>I',len(p))+p;chunk=1+len(x['request_id'])%9
 for offset in range(0,len(frame),chunk):sys.stdout.buffer.write(frame[offset:offset+chunk]);sys.stdout.buffer.flush()
while True:
 h=readn(4)
 if h is None:break
 body=readn(struct.unpack('>I',h)[0])
 if body is None:break
 r=json.loads(body)
 with open(root+'/requests.log','a') as f:f.write(json.dumps(r,ensure_ascii=False)+'\n');f.flush()
 method=r['request']['method'];params=r['request'].get('params',{})
 response={'protocol_version':1,'request_id':r['request_id'],'response':{'kind':'status','body':{'status':'ready'}}}
 if method=='shutdown':response['response']['body']['status']='stopped'
 if method=='invoke':
  value=params['input'];mode=value['mode']
  response['response']={'kind':'success','body':{'output':value,'rendering':'fixture'}}
  if mode==1:response['response']={'kind':'failure','body':{'class':'invalid-input','code':'fixture-input','detail':'controlled rejection','retryable_with_new_action':False}}
  if mode==2:response['response']={'kind':'status','body':{'status':'cancelled'}}
  if mode==3:response['request_id']='wrong-identity'
  if mode==4:response['protocol_version']=2
  if mode==5:response['response']={'kind':'status','body':{'status':'healthy'}}
  if mode==6:sys.stdout.buffer.write(struct.pack('>I',4097));sys.stdout.buffer.flush();continue
  if mode==7:sys.stdout.buffer.write(struct.pack('>I',100)+b'{');sys.stdout.buffer.flush();sys.exit(0)
  if mode==8:sys.exit(7)
  if mode==9:sys.stdin.buffer.read(1);sys.exit(0)
  if mode==10:response['response']['body']['rendering']='x'*300
  if mode in (12,13,14):response['response']['body']['rendering']='x'*(128-len(json.dumps(value,separators=(',',':')).encode())+({12:0,13:1,14:-1}[mode]))
 send(response)
 if method=='shutdown':break
"##;
fn manifest(request_limit: u8) -> String {
    format!(
        r#"manifest_version=1
id="fixture.probe"
version={{major=1,minor=0,patch=0}}
kind="process"
protocol={{minimum=1,maximum=1}}
entrypoint={{artifact="plugin",arguments=[]}}
capabilities=[{{name="fixture.inspect",operation="inspection",required=true}}]
quotas={{concurrent_requests=2,frame_bytes=4096,output_bytes=128,invocation_millis=300,lifecycle_requests={request_limit},protocol_violations=1}}
"#
    )
}
fn log(path: &Path) -> Result<Vec<Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(debug(e)),
    };
    text.lines().map(|line| serde_json::from_str(line).map_err(debug)).collect()
}
fn start_token(pid: u32) -> Option<String> {
    let raw = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    raw.rsplit_once(')')?.1.split_whitespace().nth(19).map(str::to_owned)
}
#[derive(Default)]
struct Model {
    authority_calls: u64,
    phase: u8,
    used: u64,
    limit: u64,
    generation: u8,
    enabled: bool,
    pids: Vec<(u32, String)>,
}
async fn compare(host: &PluginHost, model: &Model, m: &mut crate::Metrics) -> Result<(), String> {
    let snapshots = host.snapshots().await;
    if model.phase == 0 {
        if !snapshots.is_empty() {
            return Err("absent model has a live host instance".into());
        }
    } else {
        if snapshots.len() != 1 {
            return Err("host snapshot cardinality differs".into());
        }
        let s = &snapshots[0];
        let expected =
            if model.phase == 1 { PluginLifecycle::Ready } else { PluginLifecycle::Failed };
        if s.lifecycle != expected
            || s.active_requests != 0
            || s.lifecycle_requests != model.used
            || s.id.as_str() != "fixture.probe"
            || s.version != PluginVersion::new(1, 0, 0)
            || s.trust_anchor != "explicit disposable fixture"
        {
            return Err(format!("host lifecycle/quota/identity differs:{s:?}"));
        }
    }
    bump(m, "independent_host_snapshots");
    Ok(())
}
async fn invoke(
    host: &PluginHost,
    id: &PluginId,
    root: &Path,
    model: &mut Model,
    operation: &Operation,
    index: usize,
    m: &mut crate::Metrics,
) -> Result<(), String> {
    let (mode, value, generation, declared, pre_cancel, after_send) = match operation {
        Operation::Invoke { mode, value, generation, declared, pre_cancel } => {
            (*mode, *value, *generation, *declared, *pre_cancel, false)
        }
        Operation::CancelAfterSend { value } => (9, *value, model.generation, true, false, true),
        _ => return Err("non-invocation entered adapter".into()),
    };
    let cancellation = HostCancellation::new();
    if pre_cancel {
        let _ = cancellation.cancel();
    }
    let before = log(&root.join("requests.log"))?.len();
    let input = json!({"mode":mode,"value":value});
    let subject = InvocationSubject::new("fixture-session", "fixture-actor", u64::from(generation));
    let capability = if declared { "fixture.inspect" } else { "fixture.undeclared" };
    let payload = JsonPayload::new(input.clone(), JsonBounds::PRODUCTION).map_err(debug)?;
    let request_id = RequestId::new(format!("fixture-{index}")).map_err(debug)?;
    let future = host.invoke(id, request_id.clone(), capability, payload, &subject, &cancellation);
    tokio::pin!(future);
    let allowed = model.phase == 1
        && !pre_cancel
        && declared
        && model.enabled
        && generation == model.generation;
    if model.phase == 1 && !pre_cancel && declared {
        model.authority_calls += 1;
    }
    let should_send = allowed && model.used < model.limit;
    let actual = if after_send && should_send {
        let watch = async {
            for _ in 0..100 {
                if log(&root.join("requests.log"))?.len() > before {
                    let snapshots = host.snapshots().await;
                    if snapshots.len() != 1 || snapshots[0].active_requests != 1 {
                        return Err::<(), String>(
                            "blocked invocation lacks one actual active permit".into(),
                        );
                    }
                    let _ = cancellation.cancel();
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let _ = cancellation.cancel();
            Err("setup-incomplete: plugin admission marker not observed".into())
        };
        let (result, marker) = tokio::join!(&mut future, watch);
        marker?;
        result
    } else {
        future.await
    };
    let rows = log(&root.join("requests.log"))?;
    let sent = rows.len() > before;
    if sent != should_send {
        return Err(format!(
            "child frame admission differs from independent lifecycle/authority/quota model: sent{sent} expected{should_send}"
        ));
    }
    if sent {
        if rows.len() != before + 1 {
            return Err("one invocation produced multiple child input frames".into());
        }
        let row = &rows[before];
        let expected = json!({"protocol_version":1,"request_id":request_id.as_str(),"request":{"method":"invoke","params":{"capability":"fixture.inspect","input":input,"context":{"session_id":"fixture-session","actor_id":"fixture-actor","role":"plugin","granted_capabilities":["fixture.inspect"],"authority_generation":generation,"deadline_millis":9000}}}});
        if row != &expected {
            return Err(format!("child received a different identity/authority/payload: {row}"));
        }
        bump(m, "exact_child_invocation_frames");
    }
    match actual {
        Ok(result) => {
            if !allowed {
                return Err("host succeeded without exact current fixture mediation".into());
            }
            model.used += 1;
            match (mode, result) {
                (0 | 11 | 12 | 14, PluginInvocationResult::Succeeded { output, rendering }) => {
                    let expected_rendering = if mode == 12 || mode == 14 {
                        "x".repeat(
                            128 - serde_json::to_vec(&input).map_err(debug)?.len()
                                - usize::from(mode == 14),
                        )
                    } else {
                        "fixture".into()
                    };
                    if output.value() != &input
                        || rendering.as_deref() != Some(expected_rendering.as_str())
                    {
                        return Err("successful output changed exact bytes or rendering".into());
                    }
                    bump(m, "successful_invocations");
                    if mode == 12 {
                        bump(m, "output_quota_exact");
                    }
                    if mode == 14 {
                        bump(m, "output_quota_one_below");
                    }
                }
                (1, PluginInvocationResult::Failed(e))
                    if e.code == "fixture-input"
                        && e.class == FailureClass::InvalidInput
                        && e.detail == "controlled rejection"
                        && !e.retryable_with_new_action =>
                {
                    bump(m, "typed_plugin_failures");
                }
                (2, PluginInvocationResult::Cancelled) => bump(m, "plugin_cancelled_results"),
                _ => return Err("wrong truthful plugin result classification".into()),
            }
            if !sent {
                return Err("successful invocation lacked a real child input frame".into());
            }
        }
        Err(error) => {
            let expected = if model.phase == 0 {
                HostFailureClass::Infrastructure
            } else if pre_cancel {
                HostFailureClass::Cancelled
            } else if model.phase == 2 {
                HostFailureClass::Infrastructure
            } else if !declared || !model.enabled || generation != model.generation {
                HostFailureClass::Authorization
            } else if !sent {
                HostFailureClass::Quota
            } else if after_send {
                HostFailureClass::Cancelled
            } else {
                match mode {
                    3..=6 => HostFailureClass::Protocol,
                    7..=8 => HostFailureClass::Infrastructure,
                    9 => HostFailureClass::Timeout,
                    10 | 13 => HostFailureClass::Quota,
                    _ => return Err(format!("valid echo rejected:{error}")),
                }
            };
            if error.class() != expected {
                return Err(format!("wrong host error {error:?}; expected{expected:?}"));
            }
            if sent {
                model.used += 1;
                model.phase = 2;
                if error.recovery() != RecoveryDisposition::RestartPlugin {
                    return Err("possibly-sent failure lacks explicit restart requirement".into());
                }
                bump(m, "failed_sent_invocations");
                if mode == 13 {
                    bump(m, "output_quota_one_above");
                }
                if after_send {
                    bump(m, "cancelled_after_actual_send");
                }
                if mode == 9 && !after_send {
                    bump(m, "actual_wall_timeouts");
                }
            } else {
                bump(m, "unsent_rejections");
            }
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
struct HostSetup<'a> {
    trust: u8,
    request_limit: u8,
    authority: &'a Authority,
}
async fn host_operations(
    host: &PluginHost,
    id: &PluginId,
    root: &Path,
    operations: &[Operation],
    setup: HostSetup<'_>,
    m: &mut crate::Metrics,
) -> Result<Model, String> {
    let HostSetup { trust, request_limit, authority } = setup;
    let mut model = Model { enabled: true, limit: u64::from(request_limit), ..Model::default() };
    for (index, op) in operations.iter().enumerate() {
        match op {
            Operation::Authority { generation, enabled } => {
                model.generation = *generation;
                model.enabled = *enabled;
                let mut table = authority.0.lock().map_err(debug)?;
                table.generation = *generation;
                table.enabled = *enabled;
                bump(m, "fixture_authority_changes");
            }
            Operation::Start => {
                let result = host.start(id, PluginVersion::new(1, 0, 0)).await;
                if model.phase != 0 {
                    if !result.is_err_and(|e| e.class() == HostFailureClass::Protocol) {
                        return Err("duplicate start did not preserve ownership".into());
                    }
                } else if trust != 1 {
                    if !result.is_err_and(|e| e.class() == HostFailureClass::Trust) {
                        return Err("untrusted start accepted".into());
                    }
                    bump(m, "trust_rejections");
                } else {
                    result.map_err(debug)?;
                    model.phase = 1;
                    model.used = 0;
                    let pids = fs::read_to_string(root.join("pids.log")).map_err(debug)?;
                    let pid = pids
                        .lines()
                        .last()
                        .ok_or("child PID marker absent")?
                        .parse::<u32>()
                        .map_err(debug)?;
                    let token = start_token(pid).ok_or("owned child missing after startup")?;
                    model.pids.push((pid, token));
                    bump(m, "real_process_starts");
                }
            }
            Operation::Stop => {
                let result = host.stop(id).await;
                if model.phase == 0 {
                    if result.is_ok() {
                        return Err("stop accepted unknown instance".into());
                    }
                } else {
                    if model.phase == 1 {
                        result.map_err(debug)?;
                    }
                    model.phase = 0;
                    model.used = 0;
                    bump(m, "explicit_stops");
                }
            }
            Operation::Invoke { .. } | Operation::CancelAfterSend { .. } => {
                invoke(host, id, root, &mut model, op, index, m).await?
            }
            Operation::Read => {}
        }
        if authority.0.lock().map_err(debug)?.calls != model.authority_calls {
            return Err("fresh mediator call count differs from admission model".into());
        }
        compare(host, &model, m).await?;
    }
    Ok(model)
}
async fn host_case(
    operations: &[Operation],
    request_limit: u8,
    trust: u8,
) -> Result<crate::Metrics, String> {
    let directory = tempfile::tempdir().map_err(debug)?;
    let package = directory.path().join("fixture");
    fs::create_dir(&package).map_err(debug)?;
    fs::write(package.join("plugin"), SCRIPT).map_err(debug)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(package.join("plugin"), fs::Permissions::from_mode(0o700))
            .map_err(debug)?;
    }
    fs::write(package.join("peritus-plugin.toml"), manifest(request_limit)).map_err(debug)?;
    let catalog =
        discover(&[directory.path().to_path_buf()], DiscoveryLimits::PRODUCTION).map_err(debug)?;
    let discovered = catalog.iter().next().ok_or("fixture not discovered")?;
    let id = discovered.manifest().id().clone();
    let quotas = discovered.manifest().quotas();
    if discovered.artifact_sha256() != <[u8; 32]>::from(Sha256::digest(SCRIPT.as_bytes())) {
        return Err("discovery artifact digest differs from exact input bytes".into());
    }
    let trust_store = if trust == 0 {
        DigestTrustStore::new()
    } else {
        DigestTrustStore::new().with_anchor(
            id.clone(),
            PluginVersion::new(1, 0, 0),
            if trust == 3 { ManifestDigest::new([255; 32]) } else { discovered.manifest_digest() },
            if trust == 2 { [254; 32] } else { discovered.artifact_sha256() },
            "explicit disposable fixture",
        )
    };
    let authority =
        Arc::new(Authority(Mutex::new(Authorization { generation: 0, enabled: true, calls: 0 })));
    // Update the dependency table at each explicit operation, retaining the exact same host owner.
    let host = PluginHost::new(
        HostConfig {
            wasm_runtime: "/nonexistent-owned-fixture-runtime".into(),
            quota_ceiling: PluginQuotas { lifecycle_requests: u64::from(request_limit), ..quotas },
            startup_timeout: Duration::from_secs(3),
            shutdown_timeout: Duration::from_millis(300),
        },
        catalog,
        authority.clone(),
        Arc::new(trust_store),
    );
    let mut metrics = crate::Metrics::new();
    // host_operations owns semantic history; the fixture table is synchronized by its explicit adapter.
    let result = host_operations(
        &host,
        &id,
        &package,
        operations,
        HostSetup { trust, request_limit, authority: &authority },
        &mut metrics,
    )
    .await;
    let _ = host.stop(&id).await;
    let model = result?;
    if !host.snapshots().await.is_empty() {
        return Err("cleanup retained an owned plugin".into());
    }
    for (pid, token) in model.pids {
        if start_token(pid).as_ref() == Some(&token) {
            return Err("owned plugin process remains after explicit host cleanup".into());
        }
        bump(&mut metrics, "observed_reaped_processes");
    }
    Ok(metrics)
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    match case {
        Case::Sdk { depth, width, text, limit, member_limit, depth_limit, string_limit, fault } => {
            if *depth > 4 || *width > 8 || text.len() > 64 || *limit > 1024 {
                return Err("setup-incomplete: SDK fixture bounds exceeded".into());
            }
            sdk(SdkInput {
                depth: *depth,
                width: *width,
                text,
                limit: *limit,
                member_limit: *member_limit,
                depth_limit: *depth_limit,
                string_limit: *string_limit,
                fault: *fault,
            })
        }
        Case::Host { operations, request_limit, trust } => {
            if !cfg!(target_os = "linux") {
                return Err("setup-incomplete: native host fixture requires Linux /proc identity observations".into());
            }
            if operations.len() > 48
                || !(1..=6).contains(request_limit)
                || *trust > 3
                || operations.iter().any(|op| matches!(op,Operation::Invoke{mode,..} if *mode>14))
            {
                return Err("setup-incomplete: host fixture bounds exceeded".into());
            }
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(debug)?
                .block_on(host_case(operations, *request_limit, *trust))
        }
    }
}
pub fn calibration() -> Case {
    Case::Host {
        operations: vec![
            Operation::Start,
            Operation::Invoke {
                mode: 3,
                value: 11,
                generation: 0,
                declared: true,
                pre_cancel: false,
            },
            Operation::Read,
            Operation::Stop,
        ],
        request_limit: 4,
        trust: 1,
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![calibration()];
    for mode in 0..15 {
        cases.push(Case::Host {
            operations: vec![
                Operation::Start,
                Operation::Invoke {
                    mode,
                    value: 47,
                    generation: 0,
                    declared: true,
                    pre_cancel: false,
                },
                Operation::Read,
                Operation::Stop,
                Operation::Start,
                Operation::Stop,
            ],
            request_limit: 4,
            trust: 1,
        });
    }
    cases.push(Case::Host {
        operations: vec![
            Operation::Start,
            Operation::CancelAfterSend { value: 71 },
            Operation::Stop,
            Operation::Start,
            Operation::Invoke {
                mode: 0,
                value: 9,
                generation: 0,
                declared: true,
                pre_cancel: false,
            },
        ],
        request_limit: 4,
        trust: 1,
    });
    for trust in [0, 2, 3] {
        cases.push(Case::Host {
            operations: vec![Operation::Start, Operation::Read, Operation::Stop],
            request_limit: 1,
            trust,
        });
    }
    cases
}
