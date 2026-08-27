//! MCP lifecycle, projection, pagination, cancellation, and cleanup acceptance tests.

use std::{future::Future, sync::Arc, time::Duration};

use peritus_mcp::{
    AuthorityBridge, BridgeContext, BridgeError, BridgeErrorClass, BridgeFuture, BridgePrompt,
    BridgePromptMessage, BridgeResource, BridgeResourceContents, BridgeTool, BridgeToolCallResult,
    McpCancellation, McpServer, McpServerInfo, ServerLimits,
};
use peritus_types::{ActorId, SessionId};
use serde_json::Value;
use tokio::io::{
    AsyncBufReadExt as _, AsyncWriteExt as _, BufReader, DuplexStream, ReadHalf, WriteHalf,
};

struct FakeBridge;

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("test JSON")
}

fn run_async<F>(future: F)
where
    F: Future<Output = ()>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future);
}

impl AuthorityBridge for FakeBridge {
    fn list_tools<'a>(
        &'a self,
        _context: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<Vec<BridgeTool>, BridgeError>> {
        Box::pin(async {
            Ok(vec![
                BridgeTool {
                    name: "fs.read".to_owned(),
                    description: "Read a file".to_owned(),
                    input_schema: json(r#"{"type":"object"}"#),
                },
                BridgeTool {
                    name: "quality.run".to_owned(),
                    description: "Run one configured check".to_owned(),
                    input_schema: json(r#"{"type":"object"}"#),
                },
            ])
        })
    }

    fn call_tool<'a>(
        &'a self,
        _context: &'a BridgeContext,
        name: &'a str,
        arguments: Value,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<BridgeToolCallResult, BridgeError>> {
        Box::pin(async move {
            if name == "slow" {
                cancellation.cancelled().await;
                return Err(BridgeError::new(
                    BridgeErrorClass::Cancelled,
                    "cancelled",
                    "request was cancelled",
                ));
            }
            if name != "fs.read" {
                return Err(BridgeError::new(
                    BridgeErrorClass::NotFound,
                    "unknown_tool",
                    "tool is not exposed",
                ));
            }
            Ok(BridgeToolCallResult {
                content: Vec::new(),
                structured_content: Some(arguments),
                is_error: false,
            })
        })
    }

    fn list_resources<'a>(
        &'a self,
        _context: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<Vec<BridgeResource>, BridgeError>> {
        Box::pin(async {
            Ok(vec![BridgeResource {
                uri: "peritus://status".to_owned(),
                name: "status".to_owned(),
                description: Some("Current status".to_owned()),
                mime_type: Some("application/json".to_owned()),
            }])
        })
    }

    fn read_resource<'a>(
        &'a self,
        _context: &'a BridgeContext,
        uri: &'a str,
        _cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgeResourceContents>, BridgeError>> {
        Box::pin(async move {
            Ok(vec![BridgeResourceContents {
                uri: uri.to_owned(),
                mime_type: Some("application/json".to_owned()),
                text: Some("{\"ready\":true}".to_owned()),
                blob: None,
            }])
        })
    }

    fn list_prompts<'a>(
        &'a self,
        _context: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<Vec<BridgePrompt>, BridgeError>> {
        Box::pin(async {
            Ok(vec![BridgePrompt {
                name: "review".to_owned(),
                description: Some("Review a change".to_owned()),
                arguments: Vec::new(),
            }])
        })
    }

    fn get_prompt<'a>(
        &'a self,
        _context: &'a BridgeContext,
        name: &'a str,
        _arguments: Value,
        _cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgePromptMessage>, BridgeError>> {
        Box::pin(async move {
            Ok(vec![BridgePromptMessage::text("user", format!("use prompt {name}"))])
        })
    }
}

struct Harness {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    server: tokio::task::JoinHandle<Result<(), peritus_mcp::McpError>>,
}

impl Harness {
    fn start(in_flight_requests: usize, page_entries: usize) -> Self {
        let context = BridgeContext::new(
            ActorId::new([1; 16]).expect("actor"),
            SessionId::new([2; 16]).expect("session"),
            3,
        );
        let server = Arc::new(
            McpServer::new(
                McpServerInfo { name: "peritus".to_owned(), version: "test".to_owned() },
                Some("Peritus test bridge".to_owned()),
                context,
                Arc::new(FakeBridge),
                ServerLimits { message_bytes: 64 * 1024, in_flight_requests, page_entries },
            )
            .expect("server"),
        );
        let (client, server_io) = tokio::io::duplex(128 * 1024);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server_io);
        let task = tokio::spawn(server.serve(BufReader::new(server_read), server_write));
        Self { reader: BufReader::new(client_read), writer: client_write, server: task }
    }

    async fn request(&mut self, value: Value) -> Value {
        self.writer.write_all(value.to_string().as_bytes()).await.expect("write request");
        self.writer.write_all(b"\n").await.expect("write delimiter");
        self.writer.flush().await.expect("flush request");
        let mut line = String::new();
        self.reader.read_line(&mut line).await.expect("read response");
        serde_json::from_str(&line).expect("JSON response")
    }

    async fn notify(&mut self, value: Value) {
        self.writer.write_all(value.to_string().as_bytes()).await.expect("write notification");
        self.writer.write_all(b"\n").await.expect("write delimiter");
        self.writer.flush().await.expect("flush notification");
    }

    async fn initialize(&mut self) {
        let response = self
            .request(json(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"tests","version":"1"}}}"#,
            ))
            .await;
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        self.notify(json(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)).await;
    }

    async fn finish(mut self) {
        self.writer.shutdown().await.expect("close client input");
        self.server.await.expect("join server").expect("clean server EOF");
    }
}

#[test]
fn lifecycle_pagination_tool_resource_and_prompt_methods_work() {
    run_async(async {
        let mut harness = Harness::start(4, 1);
        let before = harness
            .request(json(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#))
            .await;
        assert_eq!(before["error"]["code"], -32_600);
        harness.initialize().await;

        let first = harness
            .request(json(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#))
            .await;
        assert_eq!(first["result"]["tools"][0]["name"], "fs.read");
        assert_eq!(first["result"]["nextCursor"], "1");
        let second = harness
            .request(json(
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/list","params":{"cursor":"1"}}"#,
            ))
            .await;
        assert_eq!(second["result"]["tools"][0]["name"], "quality.run");

        let called = harness
        .request(json(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"fs.read","arguments":{"path":"README.md"}}}"#,
        ))
        .await;
        assert_eq!(called["result"]["structuredContent"]["path"], "README.md");
        assert_eq!(called["result"]["isError"], false);

        let resource = harness
        .request(json(
            r#"{"jsonrpc":"2.0","id":6,"method":"resources/read","params":{"uri":"peritus://status"}}"#,
        ))
        .await;
        assert_eq!(resource["result"]["contents"][0]["uri"], "peritus://status");

        let prompt = harness
        .request(json(
            r#"{"jsonrpc":"2.0","id":7,"method":"prompts/get","params":{"name":"review","arguments":{}}}"#,
        ))
        .await;
        assert_eq!(prompt["result"]["messages"][0]["role"], "user");
        harness.finish().await;
    });
}

#[test]
fn cancellation_remains_responsive_while_a_call_is_active() {
    run_async(async {
        let mut harness = Harness::start(1, 8);
        harness.initialize().await;
        harness
        .notify(json(
            r#"{"jsonrpc":"2.0","id":"slow-request","method":"tools/call","params":{"name":"slow","arguments":{}}}"#,
        ))
        .await;
        harness
        .notify(json(
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"slow-request","reason":"test"}}"#,
        ))
        .await;

        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(1), harness.reader.read_line(&mut line))
            .await
            .expect("cancellation response deadline")
            .expect("read cancellation response");
        let response: Value = serde_json::from_str(&line).expect("JSON response");
        assert_eq!(response["id"], "slow-request");
        assert_eq!(response["error"]["code"], -32_800);
        harness.finish().await;
    });
}

#[test]
fn malformed_framing_is_reported_after_owned_tasks_are_joined() {
    run_async(async {
        let mut harness = Harness::start(1, 8);
        harness.writer.write_all(b"{\"jsonrpc\":").await.expect("write partial frame");
        harness.writer.shutdown().await.expect("close partial frame");
        let error =
            harness.server.await.expect("join server").expect_err("partial message rejected");
        assert_eq!(error.operation(), "read MCP input");
    });
}
