//! Real MCP server transports and public diagnostic constructors with independent expectations.
use peritus_mcp::{
    AuthorityBridge, BridgeContext, BridgeError, BridgeErrorClass, BridgeFuture, BridgePrompt,
    BridgePromptMessage, BridgeResource, BridgeResourceContents, BridgeTool, BridgeToolCallResult,
    JsonRpcResponse, MCP_PROTOCOL_VERSION, McpCancellation, McpError, McpErrorClass, McpServer,
    McpServerInfo, RpcId, ServerLimits,
};
use peritus_policy::ActorRole;
use peritus_types::{ActorId, SessionId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    sync::{Mutex, Semaphore},
    task::JoinHandle,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Dialogue { operations: Vec<Operation>, page: u8, entries: u8, chunk: u8 },
    Framing { fault: u8, chunk: u8 },
    ErrorText { prefix: u16, symbol: u8, repeats: u16, constructor: u8 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Initialize { supported: bool, valid: bool },
    Initialized,
    Ping,
    List { kind: u8, cursor: u8 },
    Resource { slot: u8, valid: bool },
    Tool { valid: bool, argument: u8 },
    Prompt { valid: bool, argument: u8 },
    Invalid { variant: u8 },
    Unknown,
    Notification,
    Cancellation { saturate: bool },
}
fn bump(m: &mut crate::Metrics, key: &str) {
    *m.entry(key.into()).or_default() += 1;
}
fn debug(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn text(symbol: u8) -> &'static str {
    match symbol % 4 {
        0 => "é",
        1 => "🦀",
        2 => "界",
        _ => "x",
    }
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let op = prop_oneof![
        2 => (any::<bool>(), any::<bool>()).prop_map(|(supported, valid)| Operation::Initialize{supported,valid}),
        2 => Just(Operation::Initialized), 2 => Just(Operation::Ping),
        5 => (0_u8..3,0_u8..9).prop_map(|(kind,cursor)|Operation::List{kind,cursor}),
        3 => (0_u8..9,any::<bool>()).prop_map(|(slot,valid)|Operation::Resource{slot,valid}),
        2 => (any::<bool>(),any::<u8>()).prop_map(|(valid,argument)|Operation::Tool{valid,argument}),
        2 => (any::<bool>(),any::<u8>()).prop_map(|(valid,argument)|Operation::Prompt{valid,argument}),
        3 => (0_u8..8).prop_map(|variant|Operation::Invalid{variant}),
        1 => Just(Operation::Unknown), 1 => Just(Operation::Notification),
        2 => any::<bool>().prop_map(|saturate|Operation::Cancellation{saturate}),
    ];
    let steps = max_steps.min(64);
    let broad = (prop::collection::vec(op.clone(), 0..=steps), 1_u8..5, 0_u8..8, 1_u8..16)
        .prop_map(|(operations, page, entries, chunk)| Case::Dialogue {
            operations,
            page,
            entries,
            chunk,
        });
    let warm = (
        prop::collection::vec(op, 0..=steps.saturating_sub(5)),
        1_u8..5,
        0_u8..8,
        1_u8..16,
        any::<bool>(),
    )
        .prop_map(move |(mut suffix, page, entries, chunk, saturate)| {
            let mut operations = vec![
                Operation::List { kind: 0, cursor: 0 },
                Operation::Initialize { supported: true, valid: true },
                Operation::Initialized,
                Operation::Cancellation { saturate },
                Operation::List { kind: 2, cursor: 0 },
            ];
            operations.append(&mut suffix);
            operations.truncate(steps);
            Case::Dialogue { operations, page, entries, chunk }
        });
    let diagnostics =
        (0_u8..4, 0_u8..4, 0_u8..3, 0_u16..5).prop_map(|(constructor, symbol, edge, repeats)| {
            let bound = match constructor {
                0 => 512,
                1 => 128,
                _ => 1024,
            };
            Case::ErrorText {
                prefix: bound - 1 + u16::from(edge),
                symbol,
                repeats: repeats + 2,
                constructor,
            }
        });
    prop_oneof![4=>broad,4=>warm,2=>(0_u8..8,1_u8..16).prop_map(|(fault,chunk)|Case::Framing{fault,chunk}),2=>diagnostics].boxed()
}

#[derive(Clone, Debug, PartialEq)]
struct Call {
    kind: &'static str,
    name: String,
    argument: Value,
    actor: [u8; 16],
    session: [u8; 16],
    generation: u64,
    role: ActorRole,
}
fn context_matches(call: &Call) -> bool {
    call.actor == [51; 16]
        && call.session == [52; 16]
        && call.generation == 73
        && call.role == ActorRole::Plugin
}
struct Bridge {
    entries: u8,
    calls: Mutex<Vec<Call>>,
    starts: Semaphore,
}
impl Bridge {
    async fn record(
        &self,
        context: &BridgeContext,
        kind: &'static str,
        name: &str,
        argument: Value,
    ) {
        self.calls.lock().await.push(Call {
            kind,
            name: name.into(),
            argument,
            actor: *context.actor_id().as_bytes(),
            session: *context.session_id().as_bytes(),
            generation: context.authority_generation(),
            role: context.role(),
        });
    }
}
impl AuthorityBridge for Bridge {
    fn list_tools<'a>(
        &'a self,
        c: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<Vec<BridgeTool>, BridgeError>> {
        Box::pin(async move {
            self.record(c, "tools", "", Value::Null).await;
            Ok((0..self.entries).map(|i|BridgeTool{name:format!("probe.{i}"),description:format!("tool-{i}"),input_schema:json!({"type":"object","properties":{"value":{"type":"integer"}}})}).collect())
        })
    }
    fn call_tool<'a>(
        &'a self,
        c: &'a BridgeContext,
        n: &'a str,
        a: Value,
        _: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<BridgeToolCallResult, BridgeError>> {
        Box::pin(async move {
            self.record(c, "call", n, a).await;
            Err(BridgeError::new(
                BridgeErrorClass::Authorization,
                "fixture-denied",
                "No effect authority is supplied by this transport fixture",
            ))
        })
    }
    fn list_resources<'a>(
        &'a self,
        c: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<Vec<BridgeResource>, BridgeError>> {
        Box::pin(async move {
            self.record(c, "resources", "", Value::Null).await;
            Ok((0..self.entries)
                .map(|i| BridgeResource {
                    uri: format!("fixture://{i}"),
                    name: format!("resource-{i}"),
                    description: None,
                    mime_type: Some("text/plain".into()),
                })
                .collect())
        })
    }
    fn read_resource<'a>(
        &'a self,
        c: &'a BridgeContext,
        n: &'a str,
        cancel: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgeResourceContents>, BridgeError>> {
        Box::pin(async move {
            self.record(c, "read", n, Value::Null).await;
            if n == "fixture://wait" {
                self.starts.add_permits(1);
                cancel.cancelled().await;
                return Err(BridgeError::new(
                    BridgeErrorClass::Cancelled,
                    "fixture-cancelled",
                    "cancelled after actual admission",
                ));
            }
            Ok(vec![BridgeResourceContents {
                uri: n.into(),
                mime_type: Some("text/plain".into()),
                text: Some(format!("bytes:{n}:é")),
                blob: None,
            }])
        })
    }
    fn list_prompts<'a>(
        &'a self,
        c: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<Vec<BridgePrompt>, BridgeError>> {
        Box::pin(async move {
            self.record(c, "prompts", "", Value::Null).await;
            Ok((0..self.entries)
                .map(|i| BridgePrompt {
                    name: format!("prompt.{i}"),
                    description: None,
                    arguments: vec![],
                })
                .collect())
        })
    }
    fn get_prompt<'a>(
        &'a self,
        c: &'a BridgeContext,
        n: &'a str,
        a: Value,
        _: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgePromptMessage>, BridgeError>> {
        Box::pin(async move {
            self.record(c, "get", n, a.clone()).await;
            Ok(vec![BridgePromptMessage::text("user", format!("{n}:{a}"))])
        })
    }
}
fn context() -> Result<BridgeContext, String> {
    Ok(BridgeContext::new(
        ActorId::new([51; 16]).map_err(debug)?,
        SessionId::new([52; 16]).map_err(debug)?,
        73,
    ))
}
struct Client {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    chunk: usize,
}
impl Client {
    async fn bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        for part in bytes.chunks(self.chunk) {
            self.writer.write_all(part).await.map_err(debug)?;
        }
        self.writer.flush().await.map_err(debug)
    }
    async fn send(&mut self, value: &Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(value).map_err(debug)?;
        bytes.push(b'\n');
        self.bytes(&bytes).await
    }
    async fn receive(&mut self) -> Result<Value, String> {
        let mut line = String::new();
        let count = tokio::time::timeout(Duration::from_secs(2), self.reader.read_line(&mut line))
            .await
            .map_err(debug)?
            .map_err(debug)?;
        if count == 0 {
            return Err("MCP response stream ended before expected result".into());
        }
        serde_json::from_str(&line).map_err(debug)
    }
}
fn request(id: Value, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn check_result(value: &Value, id: &Value, result: &Value) -> Result<(), String> {
    if value != &json!({"jsonrpc":"2.0","id":id,"result":result}) {
        return Err(format!("response differs: {value}, expected result {result} id{id}"));
    }
    Ok(())
}
fn check_error(value: &Value, id: &Value, code: i32) -> Result<(), String> {
    if value.get("jsonrpc") != Some(&json!("2.0"))
        || value.get("id") != Some(id)
        || value.get("result").is_some()
        || value.pointer("/error/code") != Some(&json!(code))
    {
        return Err(format!("response not expected error {code} id{id}: {value}"));
    }
    Ok(())
}
fn catalog(kind: u8, count: u8) -> Vec<Value> {
    (0..count).map(|i|match kind%3{
    0=>json!({"name":format!("probe.{i}"),"description":format!("tool-{i}"),"inputSchema":{"type":"object","properties":{"value":{"type":"integer"}}}}),
    1=>json!({"uri":format!("fixture://{i}"),"name":format!("resource-{i}"),"mimeType":"text/plain"}),
    _=>json!({"name":format!("prompt.{i}"),"arguments":[]}),}).collect()
}
async fn wait_started(bridge: &Bridge) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(2), bridge.starts.acquire())
        .await
        .map_err(debug)?
        .map_err(debug)?
        .forget();
    Ok(())
}
async fn cancellation_wave(
    client: &mut Client,
    bridge: &Bridge,
    id: &Value,
    saturate: bool,
    m: &mut crate::Metrics,
) -> Result<(), String> {
    let other = json!("held-other");
    client.send(&request(id.clone(), "resources/read", json!({"uri":"fixture://wait"}))).await?;
    wait_started(bridge).await?;
    if saturate {
        client
            .send(&request(other.clone(), "resources/read", json!({"uri":"fixture://wait"})))
            .await?;
        wait_started(bridge).await?;
        client.send(&request(json!("excess"), "ping", json!({}))).await?;
        check_error(&client.receive().await?, &json!("excess"), -32603)?;
        bump(m, "in_flight_quota_rejections");
    } else {
        client.send(&request(id.clone(), "ping", json!({}))).await?;
        check_error(&client.receive().await?, id, -32600)?;
        bump(m, "duplicate_active_id_rejections");
    }
    for target in if saturate { vec![id.clone(), other.clone()] } else { vec![id.clone()] } {
        client.send(&json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":target}})).await?;
    }
    let mut responses = BTreeMap::new();
    for _ in 0..if saturate { 2 } else { 1 } {
        let r = client.receive().await?;
        responses.insert(r["id"].to_string(), r);
    }
    for target in if saturate { vec![id.clone(), other] } else { vec![id.clone()] } {
        let result = responses.remove(&target.to_string()).ok_or("cancelled id lost")?;
        check_error(&result, &target, -32800)?;
        bump(m, "cancelled_active_requests");
    }
    client.send(&request(id.clone(), "ping", json!({}))).await?;
    check_result(&client.receive().await?, id, &json!({}))?;
    bump(m, "id_reuse_after_cancellation");
    Ok(())
}
async fn dialogue_ops(
    client: &mut Client,
    bridge: &Bridge,
    operations: &[Operation],
    page: u8,
    count: u8,
    m: &mut crate::Metrics,
) -> Result<(), String> {
    let mut phase = 0_u8;
    for (index, op) in operations.iter().enumerate() {
        let id = if index % 2 == 0 { json!(format!("rpc-{index}")) } else { json!(index) };
        let before = bridge.calls.lock().await.len();
        let mut expected_call: Option<(&str, String, Value)> = None;
        match op {
            Operation::Initialized => {
                client.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await?;
                if phase == 1 {
                    phase = 2;
                }
            }
            Operation::Notification => {
                client
                    .send(&json!({"jsonrpc":"2.0","id":null,"method":"tools/list","params":{}}))
                    .await?;
            }
            Operation::Initialize { supported, valid } => {
                let params = json!({"protocolVersion":if *supported{MCP_PROTOCOL_VERSION}else{"unsupported"},"clientInfo":{"name":if *valid{"client"}else{""},"version":"1"},"capabilities":{}});
                client.send(&request(id.clone(), "initialize", params)).await?;
                let r = client.receive().await?;
                if phase != 0 {
                    check_error(&r, &id, -32600)?;
                } else if !supported || !valid {
                    check_error(&r, &id, -32602)?;
                } else {
                    check_result(
                        &r,
                        &id,
                        &json!({
                            "protocolVersion": MCP_PROTOCOL_VERSION,
                            "serverInfo": {"name":"praxis-mcp","version":"1"},
                            "capabilities": {
                                "tools":{"listChanged":false},
                                "prompts":{"listChanged":false},
                                "resources":{"subscribe":false,"listChanged":false}
                            },
                            "instructions":null
                        }),
                    )?;
                    phase = 1;
                    bump(m, "successful_initializations");
                }
            }
            Operation::Ping => {
                client.send(&request(id.clone(), "ping", json!({}))).await?;
                check_result(&client.receive().await?, &id, &json!({}))?;
            }
            Operation::Invalid { variant } => {
                let mut value = request(id.clone(), "ping", json!({}));
                match variant % 8 {
                    0 => value["jsonrpc"] = json!("1.0"),
                    1 => value["unexpected"] = json!(1),
                    2 => value["id"] = json!(1.25),
                    3 => value["method"] = json!(3),
                    4 => value["id"] = json!(true),
                    5 => value = json!([]),
                    6 => value["jsonrpc"] = Value::Null,
                    _ => {
                        client.bytes(b"not-json\n").await?;
                    }
                }
                if variant % 8 != 7 {
                    client.send(&value).await?;
                }
                check_error(
                    &client.receive().await?,
                    if variant % 8 == 0 { &id } else { &Value::Null },
                    if variant % 8 == 0 { -32600 } else { -32700 },
                )?;
                bump(m, "malformed_request_rejections");
            }
            Operation::Cancellation { saturate } if phase == 2 => {
                cancellation_wave(client, bridge, &id, *saturate, m).await?;
                bump(m, "cancellation_waves");
            }
            _ => {
                let (method, params) = match op {
                    Operation::List { kind, cursor } => (
                        ["tools/list", "resources/list", "prompts/list"][usize::from(kind % 3)],
                        json!({"cursor":cursor.to_string()}),
                    ),
                    Operation::Resource { slot, valid } => (
                        "resources/read",
                        json!({"uri":if *valid{format!("fixture://{slot}")}else{String::new()}}),
                    ),
                    Operation::Tool { valid, argument } => (
                        "tools/call",
                        json!({"name":if *valid{"probe.0"}else{"bad\nname"},"arguments":{"value":argument}}),
                    ),
                    Operation::Prompt { valid, argument } => (
                        "prompts/get",
                        json!({"name":if *valid{"prompt.0"}else{""},"arguments":{"value":argument}}),
                    ),
                    _ => ("unknown/method", json!({})),
                };
                client.send(&request(id.clone(), method, params)).await?;
                let r = client.receive().await?;
                if phase != 2 {
                    check_error(&r, &id, -32600)?;
                    bump(m, "preinitialization_rejections");
                } else {
                    match op {
                        Operation::List { kind, cursor } => {
                            let tag = ["tools", "resources", "prompts"][usize::from(kind % 3)];
                            expected_call = Some((tag, String::new(), Value::Null));
                            if *cursor > count {
                                check_error(&r, &id, -32602)?;
                                bump(m, "invalid_cursor_rejections");
                            } else {
                                let end = usize::from(*cursor)
                                    .saturating_add(usize::from(page))
                                    .min(usize::from(count));
                                let mut result =
                                    json!({(tag):catalog(*kind,count)[usize::from(*cursor)..end]});
                                if end < usize::from(count) {
                                    result["nextCursor"] = json!(end.to_string());
                                }
                                check_result(&r, &id, &result)?;
                                bump(m, "exact_list_pages");
                            }
                        }
                        Operation::Resource { slot, valid } => {
                            if *valid {
                                let uri = format!("fixture://{slot}");
                                expected_call = Some(("read", uri.clone(), Value::Null));
                                check_result(
                                    &r,
                                    &id,
                                    &json!({"contents":[{"uri":uri,"mimeType":"text/plain","text":format!("bytes:{uri}:é")}]}),
                                )?;
                                bump(m, "resource_reads");
                            } else {
                                check_error(&r, &id, -32602)?;
                            }
                        }
                        Operation::Tool { valid, argument } => {
                            if *valid {
                                expected_call =
                                    Some(("call", "probe.0".into(), json!({"value":argument})));
                                check_error(&r, &id, -32603)?;
                                if r.pointer("/error/data/peritusCode")
                                    != Some(&json!("fixture-denied"))
                                {
                                    return Err("bridge denial identity lost".into());
                                }
                                bump(m, "authority_denials_preserved");
                            } else {
                                check_error(&r, &id, -32602)?;
                            }
                        }
                        Operation::Prompt { valid, argument } => {
                            if *valid {
                                let a = json!({"value":argument});
                                expected_call = Some(("get", "prompt.0".into(), a.clone()));
                                check_result(
                                    &r,
                                    &id,
                                    &json!({"messages":[{"role":"user","content":{"type":"text","text":format!("prompt.0:{a}")}}]}),
                                )?;
                                bump(m, "prompt_results");
                            } else {
                                check_error(&r, &id, -32602)?;
                            }
                        }
                        _ => check_error(&r, &id, -32601)?,
                    }
                }
            }
        }
        // An ordinary ping fences silent notifications without treating silence as a response.
        let barrier = json!(format!("barrier-{index}"));
        client.send(&request(barrier.clone(), "ping", json!({}))).await?;
        check_result(&client.receive().await?, &barrier, &json!({}))?;
        let calls = bridge.calls.lock().await;
        if matches!(op, Operation::Cancellation { .. }) && phase == 2 {
            let expected =
                if matches!(op, Operation::Cancellation { saturate: true }) { 2 } else { 1 };
            if calls.len() != before + expected
                || calls[before..]
                    .iter()
                    .any(|c| c.kind != "read" || c.name != "fixture://wait" || !context_matches(c))
            {
                return Err("cancellation wave bridge routing differs".into());
            }
        } else {
            let expected = expected_call.map(|(kind, name, argument)| Call {
                kind: match kind {
                    "tools" => "tools",
                    "resources" => "resources",
                    "prompts" => "prompts",
                    "read" => "read",
                    "call" => "call",
                    _ => "get",
                },
                name,
                argument,
                actor: [51; 16],
                session: [52; 16],
                generation: 73,
                role: ActorRole::Plugin,
            });
            if calls.get(before..) != Some(expected.as_slice()) {
                return Err(format!("bridge dispatch differs at {op:?}: {:?}", &calls[before..]));
            }
        }
        bump(m, "operations");
    }
    Ok(())
}
async fn finish_server(
    task: &mut JoinHandle<Result<(), McpError>>,
) -> Result<Result<(), McpError>, String> {
    match tokio::time::timeout(Duration::from_secs(2), &mut *task).await {
        Ok(joined) => joined.map_err(debug),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Err("owned MCP server failed to finish and was aborted/joined".into())
        }
    }
}
async fn dialogue(
    operations: &[Operation],
    page: u8,
    entries: u8,
    chunk: u8,
) -> Result<crate::Metrics, String> {
    let bridge = Arc::new(Bridge { entries, calls: Mutex::new(vec![]), starts: Semaphore::new(0) });
    let server = Arc::new(
        McpServer::new(
            McpServerInfo { name: "praxis-mcp".into(), version: "1".into() },
            None,
            context()?,
            bridge.clone(),
            ServerLimits {
                message_bytes: 4096,
                in_flight_requests: 2,
                page_entries: usize::from(page),
            },
        )
        .map_err(debug)?,
    );
    let (client, transport) = tokio::io::duplex(65536);
    let (cr, cw) = tokio::io::split(client);
    let (sr, sw) = tokio::io::split(transport);
    let mut client = Client { reader: BufReader::new(cr), writer: cw, chunk: usize::from(chunk) };
    let mut task = tokio::spawn(server.serve(BufReader::new(sr), sw));
    let mut metrics = crate::Metrics::new();
    let result = dialogue_ops(&mut client, &bridge, operations, page, entries, &mut metrics).await;
    // Cancel all fixture waits even on an assertion error before closing the owned input.
    for index in 0..operations.len() {
        for id in [json!(format!("rpc-{index}")), json!(index), json!("held-other")] {
            let _=client.send(&json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":id}})).await;
        }
    }
    let _ = client.writer.shutdown().await;
    let joined = finish_server(&mut task).await;
    result?;
    joined?.map_err(debug)?;
    bump(&mut metrics, "owned_server_joins");
    Ok(metrics)
}
async fn framing(fault: u8, chunk: u8) -> Result<crate::Metrics, String> {
    let bridge =
        Arc::new(Bridge { entries: 0, calls: Mutex::new(vec![]), starts: Semaphore::new(0) });
    let server = Arc::new(
        McpServer::new(
            McpServerInfo { name: "praxis-mcp".into(), version: "1".into() },
            None,
            context()?,
            bridge.clone(),
            ServerLimits { message_bytes: 4096, in_flight_requests: 2, page_entries: 2 },
        )
        .map_err(debug)?,
    );
    let (client, transport) = tokio::io::duplex(65536);
    let (cr, cw) = tokio::io::split(client);
    let (sr, sw) = tokio::io::split(transport);
    let mut client = Client { reader: BufReader::new(cr), writer: cw, chunk: usize::from(chunk) };
    let mut task = tokio::spawn(server.serve(BufReader::new(sr), sw));
    let mode = fault % 8;
    let boundary_id = json!("boundary-rpc");
    let payload = match mode {
        0 => b"{\"jsonrpc\":".to_vec(),
        1 => b"\n".to_vec(),
        2 => {
            let mut bytes = vec![b'x'; 4097];
            bytes.extend_from_slice(b"\r\n");
            bytes
        }
        3 => vec![b'x'; 4097],
        4 => b"{broken}\n".to_vec(),
        5 => vec![255, b'\n'],
        _ => {
            let size = if mode == 6 { 4096 } else { 4095 };
            let mut ping = request(boundary_id.clone(), "ping", json!({"padding":""}));
            let base = serde_json::to_vec(&ping).map_err(debug)?.len();
            ping["params"]["padding"] = json!("x".repeat(size - base));
            let mut bytes = serde_json::to_vec(&ping).map_err(debug)?;
            if bytes.len() != size {
                return Err("setup-incomplete: framing fixture padding calculation differs".into());
            }
            bytes.extend_from_slice(if mode == 6 { b"\n" } else { b"\r\n" });
            bytes
        }
    };
    let send = client.bytes(&payload).await;
    let _ = client.writer.shutdown().await;
    let observed = if mode >= 4 {
        let r = client.receive().await;
        Some(r)
    } else {
        None
    };
    let joined = finish_server(&mut task).await?;
    send?;
    let mut metrics = crate::Metrics::new();
    if let Some(r) = observed {
        let response = r?;
        if mode >= 6 {
            check_result(&response, &boundary_id, &json!({}))?;
            bump(
                &mut metrics,
                if mode == 6 { "exact_limit_lf_frames" } else { "below_limit_crlf_frames" },
            );
        } else {
            check_error(&response, &Value::Null, -32700)?;
            bump(&mut metrics, "framed_parse_error_recovery");
        }
        joined.map_err(debug)?;
    } else {
        let error = joined.err().ok_or("terminal framing input was accepted")?;
        let expected = if mode >= 2 { McpErrorClass::Limit } else { McpErrorClass::Protocol };
        if error.class() != expected {
            return Err(format!("wrong framing class {error:?}"));
        }
        bump(&mut metrics, "terminal_framing_rejections");
    }
    if !bridge.calls.lock().await.is_empty() {
        return Err("malformed framing reached authority bridge".into());
    }
    bump(&mut metrics, "owned_server_joins");
    Ok(metrics)
}
fn diagnostic(
    prefix: u16,
    symbol: u8,
    repeats: u16,
    constructor: u8,
) -> Result<crate::Metrics, String> {
    let input =
        format!("{}{}", "a".repeat(usize::from(prefix)), text(symbol).repeat(usize::from(repeats)));
    let bound = match constructor % 4 {
        0 => 512,
        1 => 128,
        _ => 1024,
    };
    let mut expected = String::new();
    for c in input.chars() {
        if expected.len() + c.len_utf8() > bound {
            break;
        }
        expected.push(c);
    }
    let actual=std::panic::catch_unwind(||match constructor%4{
        0=>JsonRpcResponse::failure(Some(RpcId::Number(19)),-32603,input.clone()).error.map(|e|e.message).unwrap_or_default(),
        1=>BridgeError::new(BridgeErrorClass::Authorization,input.clone(),"detail").code().to_owned(),
        2=>BridgeError::new(BridgeErrorClass::Infrastructure,"code",input.clone()).detail().to_owned(),
        _=>McpError::new(McpErrorClass::Protocol,"fixture",input.clone()).detail().to_owned(),}).map_err(|_|format!("public diagnostic constructor panicked at UTF-8 byte bound {bound}; prefix{prefix},symbol{symbol},repeats{repeats}"))?;
    if actual != expected {
        return Err("bounded diagnostic changed text or split Unicode scalar".into());
    }
    let mut m = crate::Metrics::new();
    bump(&mut m, "bounded_diagnostic_constructors");
    if input.len() > bound {
        bump(&mut m, "truncated_unicode_diagnostics");
    }
    Ok(m)
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    match case {
        Case::ErrorText { prefix, symbol, repeats, constructor } => {
            if *prefix > 1050 || *repeats > 128 {
                return Err("setup-incomplete: diagnostic replay exceeds bounded fixture".into());
            }
            diagnostic(*prefix, *symbol, *repeats, *constructor)
        }
        Case::Dialogue { operations, page, entries, chunk } => {
            if operations.len() > 64
                || !(1..=4).contains(page)
                || *entries > 8
                || !(1..=16).contains(chunk)
            {
                return Err("setup-incomplete: dialogue fixture bounds exceeded".into());
            }
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(debug)?
                .block_on(dialogue(operations, *page, *entries, *chunk))
        }
        Case::Framing { fault, chunk } => {
            if !(1..=16).contains(chunk) {
                return Err("setup-incomplete: zero/oversized transport chunk".into());
            }
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(debug)?
                .block_on(framing(*fault, *chunk))
        }
    }
}
pub fn calibration() -> Case {
    Case::Dialogue {
        operations: vec![
            Operation::List { kind: 0, cursor: 0 },
            Operation::Initialize { supported: true, valid: true },
            Operation::Initialized,
            Operation::List { kind: 0, cursor: 0 },
        ],
        page: 2,
        entries: 3,
        chunk: 1,
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration(),
        Case::Dialogue {
            operations: vec![
                Operation::Initialize { supported: false, valid: true },
                Operation::Initialize { supported: true, valid: true },
                Operation::List { kind: 1, cursor: 0 },
                Operation::Initialized,
                Operation::Cancellation { saturate: false },
                Operation::Cancellation { saturate: true },
                Operation::Tool { valid: true, argument: 73 },
                Operation::Prompt { valid: true, argument: 81 },
                Operation::Resource { slot: 4, valid: true },
                Operation::Initialize { supported: true, valid: true },
            ],
            page: 2,
            entries: 5,
            chunk: 3,
        },
    ];
    for fault in 0..8 {
        cases.push(Case::Framing { fault, chunk: 1 });
    }
    for constructor in 0..4 {
        let bound = match constructor {
            0 => 512,
            1 => 128,
            _ => 1024,
        };
        cases.push(Case::ErrorText { prefix: bound - 1, symbol: 0, repeats: 2, constructor });
    }
    cases
}
