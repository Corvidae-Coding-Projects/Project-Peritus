//! Concurrent bounded MCP JSON-RPC server lifecycle and dispatch.

use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::{
    io::{AsyncBufRead, AsyncWrite},
    sync::{Mutex, Semaphore, mpsc},
    task::JoinSet,
};

use crate::{
    AuthorityBridge, BridgeContext, BridgeError, BridgeErrorClass, JsonRpcRequest, JsonRpcResponse,
    McpCancellation, McpError, McpErrorClass, McpServerInfo, RpcId,
    framing::{read_message, write_response},
    protocol::{
        CancelParams, CursorParams, InitializeParams, MCP_PROTOCOL_VERSION, PromptGetParams,
        ResourceReadParams, ToolCallParams,
    },
};

const PARSE_ERROR: i32 = -32_700;
const INVALID_REQUEST: i32 = -32_600;
const METHOD_NOT_FOUND: i32 = -32_601;
const INVALID_PARAMS: i32 = -32_602;
const INTERNAL_ERROR: i32 = -32_603;
const REQUEST_CANCELLED: i32 = -32_800;

/// MCP transport, concurrency, and pagination ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerLimits {
    /// Maximum JSON message bytes.
    pub message_bytes: usize,
    /// Maximum concurrently executing bridge requests.
    ///
    /// Additional admitted requests wait for this execution window without being rejected.
    pub in_flight_requests: usize,
    /// Maximum entries returned on one list page.
    pub page_entries: usize,
}

impl ServerLimits {
    /// Conservative production server limits.
    pub const PRODUCTION: Self =
        Self { message_bytes: 2 * 1024 * 1024, in_flight_requests: 32, page_entries: 128 };

    /// Validates positive server limits.
    ///
    /// # Errors
    ///
    /// Rejects a zero limit or an execution window that the runtime cannot represent.
    pub fn validate(self) -> Result<Self, McpError> {
        if self.message_bytes == 0 || self.in_flight_requests == 0 || self.page_entries == 0 {
            return Err(McpError::new(
                McpErrorClass::Limit,
                "validate MCP server limits",
                "every MCP server limit must be positive",
            ));
        }
        if self.in_flight_requests > Semaphore::MAX_PERMITS {
            return Err(McpError::new(
                McpErrorClass::Limit,
                "validate MCP server limits",
                "MCP execution window exceeds the runtime semaphore representation",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
enum Lifecycle {
    Uninitialized,
    AwaitingInitialized,
    Ready,
}

/// Bounded MCP JSON-RPC server bound to one authenticated daemon session.
pub struct McpServer {
    info: McpServerInfo,
    instructions: Option<String>,
    context: BridgeContext,
    bridge: Arc<dyn AuthorityBridge>,
    limits: ServerLimits,
    lifecycle: Mutex<Lifecycle>,
    active: Mutex<HashMap<RpcId, McpCancellation>>,
    admission: Arc<Semaphore>,
}

impl McpServer {
    /// Creates a server over a non-authoritative daemon bridge.
    ///
    /// # Errors
    ///
    /// Rejects zero limits or oversized server identity/instructions.
    pub fn new(
        info: McpServerInfo,
        instructions: Option<String>,
        context: BridgeContext,
        bridge: Arc<dyn AuthorityBridge>,
        limits: ServerLimits,
    ) -> Result<Self, McpError> {
        let limits = limits.validate()?;
        if info.name.is_empty()
            || info.name.len() > 128
            || info.version.is_empty()
            || info.version.len() > 128
            || instructions.as_ref().is_some_and(|value| value.len() > 16 * 1024)
        {
            return Err(McpError::new(
                McpErrorClass::Limit,
                "construct MCP server",
                "server identity or instructions are empty or oversized",
            ));
        }
        Ok(Self {
            info,
            instructions,
            context,
            bridge,
            limits,
            lifecycle: Mutex::new(Lifecycle::Uninitialized),
            active: Mutex::new(HashMap::new()),
            admission: Arc::new(Semaphore::new(limits.in_flight_requests)),
        })
    }

    /// Serves bounded newline-delimited MCP JSON-RPC until clean EOF.
    ///
    /// Requests execute concurrently under the configured semaphore so cancellation notifications
    /// remain responsive. The writer task and every request task are owned and joined before return.
    ///
    /// # Errors
    ///
    /// Returns a transport/framing error or an observed writer/request task failure.
    pub async fn serve<R, W>(self: Arc<Self>, mut reader: R, mut writer: W) -> Result<(), McpError>
    where
        R: AsyncBufRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let (responses, mut response_receiver) = mpsc::unbounded_channel::<JsonRpcResponse>();
        let response_limit = self.limits.message_bytes;
        let writer_task = tokio::spawn(async move {
            while let Some(response) = response_receiver.recv().await {
                write_response(&mut writer, &response, response_limit).await?;
            }
            Ok::<(), McpError>(())
        });
        let mut requests = JoinSet::new();
        let read_result = self.read_requests(&mut reader, &responses, &mut requests).await;
        if read_result.is_err() {
            requests.abort_all();
        }
        drop(responses);
        let mut cleanup_error = None;
        while let Some(joined) = requests.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    if cleanup_error.is_none() {
                        cleanup_error = Some(error);
                    }
                }
                Err(error) => {
                    if !error.is_cancelled() && cleanup_error.is_none() {
                        cleanup_error = Some(McpError::new(
                            McpErrorClass::Lifecycle,
                            "join MCP request",
                            error.to_string(),
                        ));
                    }
                }
            }
        }
        let writer_result = writer_task.await.map_err(|error| {
            McpError::new(McpErrorClass::Transport, "join MCP writer", error.to_string())
        })?;
        read_result?;
        if let Some(error) = cleanup_error {
            return Err(error);
        }
        writer_result
    }

    async fn read_requests<R>(
        self: &Arc<Self>,
        reader: &mut R,
        responses: &mpsc::UnboundedSender<JsonRpcResponse>,
        requests: &mut JoinSet<Result<(), McpError>>,
    ) -> Result<(), McpError>
    where
        R: AsyncBufRead + Send + Unpin + 'static,
    {
        while let Some(message) = read_message(reader, self.limits.message_bytes).await? {
            let request = match serde_json::from_slice::<JsonRpcRequest>(&message) {
                Ok(request) if request.jsonrpc == "2.0" => request,
                Ok(request) => {
                    send_response(
                        responses,
                        JsonRpcResponse::failure(
                            request.id,
                            INVALID_REQUEST,
                            "jsonrpc must be 2.0",
                        ),
                    )?;
                    continue;
                }
                Err(error) => {
                    send_response(
                        responses,
                        JsonRpcResponse::failure(None, PARSE_ERROR, error.to_string()),
                    )?;
                    continue;
                }
            };
            if request.id.is_none() {
                self.handle_notification(request).await;
                continue;
            }
            if request.method == "initialize" {
                let Some(id) = request.id else {
                    continue;
                };
                let response = self.initialize(id, request.params).await;
                send_response(responses, response)?;
                continue;
            }
            let Some(id) = request.id.clone() else {
                continue;
            };
            let cancellation = McpCancellation::new();
            {
                let mut active = self.active.lock().await;
                if active.contains_key(&id) {
                    drop(active);
                    send_response(
                        responses,
                        JsonRpcResponse::failure(
                            Some(id),
                            INVALID_REQUEST,
                            "request id is already active",
                        ),
                    )?;
                    continue;
                }
                active.insert(id.clone(), cancellation.clone());
            }
            let server = Arc::clone(self);
            let responses = responses.clone();
            requests.spawn(async move {
                let permit = tokio::select! {
                    biased;
                    () = cancellation.cancelled() => None,
                    acquired = Arc::clone(&server.admission).acquire_owned() => {
                        match acquired {
                            Ok(permit) => Some(permit),
                            Err(_) => {
                                server.active.lock().await.remove(&id);
                                return send_response(
                                    &responses,
                                    JsonRpcResponse::failure(
                                        Some(id),
                                        INTERNAL_ERROR,
                                        "MCP request admission closed unexpectedly",
                                    ),
                                );
                            }
                        }
                    }
                };
                let response = if let Some(permit) = permit {
                    let response =
                        server.handle_admitted_request(id.clone(), request, &cancellation).await;
                    drop(permit);
                    response
                } else {
                    bridge_response(id.clone(), &cancelled())
                };
                server.active.lock().await.remove(&id);
                send_response(&responses, response)
            });
        }
        Ok(())
    }

    async fn handle_notification(&self, request: JsonRpcRequest) {
        match request.method.as_str() {
            "notifications/initialized" => {
                let mut lifecycle = self.lifecycle.lock().await;
                if matches!(*lifecycle, Lifecycle::AwaitingInitialized) {
                    *lifecycle = Lifecycle::Ready;
                }
            }
            "notifications/cancelled" => {
                if let Ok(params) = parse_params::<CancelParams>(request.params) {
                    let _ = params.reason;
                    if let Some(cancellation) = self.active.lock().await.get(&params.request_id) {
                        let _ = cancellation.cancel();
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_admitted_request(
        &self,
        id: RpcId,
        request: JsonRpcRequest,
        cancellation: &McpCancellation,
    ) -> JsonRpcResponse {
        if request.method == "ping" {
            return JsonRpcResponse::success(id, Value::Object(serde_json::Map::new()));
        }
        if !matches!(*self.lifecycle.lock().await, Lifecycle::Ready) {
            return JsonRpcResponse::failure(
                Some(id),
                INVALID_REQUEST,
                "server has not completed MCP initialization",
            );
        }
        self.dispatch(id, request, cancellation).await
    }

    async fn initialize(&self, id: RpcId, params: Option<Value>) -> JsonRpcResponse {
        let parsed = match parse_params::<InitializeParams>(params) {
            Ok(parsed) => parsed,
            Err(message) => return JsonRpcResponse::failure(Some(id), INVALID_PARAMS, message),
        };
        let mut lifecycle = self.lifecycle.lock().await;
        if !matches!(*lifecycle, Lifecycle::Uninitialized) {
            return JsonRpcResponse::failure(
                Some(id),
                INVALID_REQUEST,
                "initialize may be called exactly once",
            );
        }
        if parsed.protocol_version != MCP_PROTOCOL_VERSION {
            let mut data = serde_json::Map::new();
            data.insert("supported".to_owned(), Value::String(MCP_PROTOCOL_VERSION.to_owned()));
            return JsonRpcResponse::failure_with_data(
                Some(id),
                INVALID_PARAMS,
                "unsupported MCP protocol version",
                Value::Object(data),
            );
        }
        if parsed.client_info.name.is_empty()
            || parsed.client_info.name.len() > 128
            || parsed.client_info.version.is_empty()
            || parsed.client_info.version.len() > 128
        {
            return JsonRpcResponse::failure(
                Some(id),
                INVALID_PARAMS,
                "clientInfo is empty or oversized",
            );
        }
        let _ = parsed.capabilities;
        *lifecycle = Lifecycle::AwaitingInitialized;
        drop(lifecycle);
        JsonRpcResponse::success(id, self.initialize_result())
    }

    async fn dispatch(
        &self,
        id: RpcId,
        request: JsonRpcRequest,
        cancellation: &McpCancellation,
    ) -> JsonRpcResponse {
        let method = request.method;
        let params = request.params;
        if !matches!(
            method.as_str(),
            "tools/list"
                | "tools/call"
                | "resources/list"
                | "resources/read"
                | "prompts/list"
                | "prompts/get"
        ) {
            return JsonRpcResponse::failure(Some(id), METHOD_NOT_FOUND, "method not found");
        }
        let operation = async {
            match method.as_str() {
                "tools/list" => self.list_tools(params, cancellation).await,
                "tools/call" => self.call_tool(params, cancellation).await,
                "resources/list" => self.list_resources(params, cancellation).await,
                "resources/read" => self.read_resource(params, cancellation).await,
                "prompts/list" => self.list_prompts(params, cancellation).await,
                "prompts/get" => self.get_prompt(params, cancellation).await,
                _ => Err(BridgeError::new(
                    BridgeErrorClass::Infrastructure,
                    "mcp_dispatch",
                    "validated MCP method was unavailable",
                )),
            }
        };
        let result = tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(cancelled()),
            result = operation => result,
        };
        match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(error) => bridge_response(id, &error),
        }
    }

    async fn list_tools(
        &self,
        params: Option<Value>,
        cancellation: &McpCancellation,
    ) -> Result<Value, BridgeError> {
        let cursor = cursor(params)?;
        let tools = self.bridge.list_tools(&self.context, cancellation).await?;
        page("tools", &tools, cursor, self.limits.page_entries)
    }

    async fn call_tool(
        &self,
        params: Option<Value>,
        cancellation: &McpCancellation,
    ) -> Result<Value, BridgeError> {
        let params = bridge_params::<ToolCallParams>(params)?;
        validate_name(&params.name)?;
        let result = self
            .bridge
            .call_tool(&self.context, &params.name, params.arguments, cancellation)
            .await?;
        serde_json::to_value(result).map_err(serialization_error)
    }

    async fn list_resources(
        &self,
        params: Option<Value>,
        cancellation: &McpCancellation,
    ) -> Result<Value, BridgeError> {
        let cursor = cursor(params)?;
        let resources = self.bridge.list_resources(&self.context, cancellation).await?;
        page("resources", &resources, cursor, self.limits.page_entries)
    }

    async fn read_resource(
        &self,
        params: Option<Value>,
        cancellation: &McpCancellation,
    ) -> Result<Value, BridgeError> {
        let params = bridge_params::<ResourceReadParams>(params)?;
        if params.uri.is_empty() || params.uri.len() > 4096 {
            return Err(invalid("resource URI is empty or oversized"));
        }
        let contents = self.bridge.read_resource(&self.context, &params.uri, cancellation).await?;
        let contents = serde_json::to_value(contents).map_err(serialization_error)?;
        Ok(object("contents", contents))
    }

    async fn list_prompts(
        &self,
        params: Option<Value>,
        cancellation: &McpCancellation,
    ) -> Result<Value, BridgeError> {
        let cursor = cursor(params)?;
        let prompts = self.bridge.list_prompts(&self.context, cancellation).await?;
        page("prompts", &prompts, cursor, self.limits.page_entries)
    }

    async fn get_prompt(
        &self,
        params: Option<Value>,
        cancellation: &McpCancellation,
    ) -> Result<Value, BridgeError> {
        let params = bridge_params::<PromptGetParams>(params)?;
        validate_name(&params.name)?;
        let messages = self
            .bridge
            .get_prompt(&self.context, &params.name, params.arguments, cancellation)
            .await?;
        let messages = serde_json::to_value(messages).map_err(serialization_error)?;
        Ok(object("messages", messages))
    }

    fn initialize_result(&self) -> Value {
        let tools = object("listChanged", Value::Bool(false));
        let mut resources = serde_json::Map::new();
        resources.insert("subscribe".to_owned(), Value::Bool(false));
        resources.insert("listChanged".to_owned(), Value::Bool(false));
        let prompts = object("listChanged", Value::Bool(false));

        let mut capabilities = serde_json::Map::new();
        capabilities.insert("tools".to_owned(), tools);
        capabilities.insert("resources".to_owned(), Value::Object(resources));
        capabilities.insert("prompts".to_owned(), prompts);

        let mut server_info = serde_json::Map::new();
        server_info.insert("name".to_owned(), Value::String(self.info.name.clone()));
        server_info.insert("version".to_owned(), Value::String(self.info.version.clone()));

        let mut result = serde_json::Map::new();
        result.insert("protocolVersion".to_owned(), Value::String(MCP_PROTOCOL_VERSION.to_owned()));
        result.insert("capabilities".to_owned(), Value::Object(capabilities));
        result.insert("serverInfo".to_owned(), Value::Object(server_info));
        result.insert(
            "instructions".to_owned(),
            self.instructions.clone().map_or(Value::Null, Value::String),
        );
        Value::Object(result)
    }
}

fn cursor(params: Option<Value>) -> Result<usize, BridgeError> {
    let params = bridge_params::<CursorParams>(params)?;
    params
        .cursor
        .map_or(Ok(0), |value| value.parse::<usize>().map_err(|_| invalid("cursor is malformed")))
}

fn page<T: Serialize>(
    field: &'static str,
    values: &[T],
    cursor: usize,
    page_size: usize,
) -> Result<Value, BridgeError> {
    if cursor > values.len() {
        return Err(invalid("cursor is outside the current collection"));
    }
    let end = cursor.saturating_add(page_size).min(values.len());
    let items = serde_json::to_value(&values[cursor..end]).map_err(serialization_error)?;
    let mut result = serde_json::Map::new();
    result.insert(field.to_owned(), items);
    if end < values.len() {
        result.insert("nextCursor".to_owned(), Value::String(end.to_string()));
    }
    Ok(Value::Object(result))
}

fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, String> {
    serde_json::from_value(params.unwrap_or_else(|| Value::Object(serde_json::Map::new())))
        .map_err(|error| error.to_string())
}

fn bridge_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, BridgeError> {
    parse_params(params).map_err(|detail| {
        BridgeError::new(BridgeErrorClass::InvalidRequest, "invalid_params", detail)
    })
}

fn validate_name(name: &str) -> Result<(), BridgeError> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        Err(invalid("name is empty, oversized, or contains controls"))
    } else {
        Ok(())
    }
}

fn bridge_response(id: RpcId, error: &BridgeError) -> JsonRpcResponse {
    let code = match error.class() {
        BridgeErrorClass::InvalidRequest => INVALID_PARAMS,
        BridgeErrorClass::NotFound => METHOD_NOT_FOUND,
        BridgeErrorClass::Cancelled => REQUEST_CANCELLED,
        BridgeErrorClass::Authorization
        | BridgeErrorClass::Timeout
        | BridgeErrorClass::Infrastructure
        | BridgeErrorClass::Indeterminate => INTERNAL_ERROR,
    };
    JsonRpcResponse::failure_with_data(
        Some(id),
        code,
        error.detail(),
        object("peritusCode", Value::String(error.code().to_owned())),
    )
}

fn object(name: &'static str, value: Value) -> Value {
    let mut map = serde_json::Map::new();
    map.insert(name.to_owned(), value);
    Value::Object(map)
}

fn invalid(detail: impl Into<String>) -> BridgeError {
    BridgeError::new(BridgeErrorClass::InvalidRequest, "invalid_params", detail)
}

fn cancelled() -> BridgeError {
    BridgeError::new(BridgeErrorClass::Cancelled, "cancelled", "request was cancelled")
}

fn serialization_error(error: serde_json::Error) -> BridgeError {
    BridgeError::with_source(
        BridgeErrorClass::Infrastructure,
        "mcp_projection",
        "bridge result could not be serialized",
        error,
    )
}

fn send_response(
    sender: &mpsc::UnboundedSender<JsonRpcResponse>,
    response: JsonRpcResponse,
) -> Result<(), McpError> {
    sender.send(response).map_err(|_| {
        McpError::new(
            McpErrorClass::Transport,
            "queue MCP response",
            "response writer closed unexpectedly",
        )
    })
}
