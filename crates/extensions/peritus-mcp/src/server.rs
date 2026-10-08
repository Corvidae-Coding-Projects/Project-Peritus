//! Concurrent bounded MCP JSON-RPC server lifecycle and dispatch.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard},
};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::{
    io::{AsyncBufRead, AsyncWrite},
    sync::{Mutex, Semaphore, mpsc},
    task::JoinSet,
};

use crate::{
    AuthorityBridge, BridgeConnectionClose, BridgeConnectionCloseReason, BridgeContext,
    BridgeError, BridgeErrorClass, BridgeRequestOwnership, JsonRpcRequest, JsonRpcResponse,
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

#[derive(Default)]
struct RequestLedger {
    connection_open: bool,
    closing: bool,
    order: Vec<RpcId>,
    entries: HashMap<RpcId, RequestOwnership>,
}

struct RequestOwnership {
    cancellation: McpCancellation,
    task_active: bool,
    daemon_dispatched: bool,
    response: Option<Arc<JsonRpcResponse>>,
}

#[derive(Clone)]
struct ResponseOwner {
    request_id: RpcId,
    cancellation: McpCancellation,
}

struct QueuedResponse {
    response: Arc<JsonRpcResponse>,
    owner: Option<ResponseOwner>,
}

struct ActiveRequestGuard {
    server: Arc<McpServer>,
    request_id: RpcId,
    cancellation: McpCancellation,
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.server.release_unanswered_request(&self.request_id, &self.cancellation);
    }
}

struct ConnectionEnd {
    reason: BridgeConnectionCloseReason,
    error: Option<McpError>,
    writer_finished: bool,
}

/// Bounded MCP JSON-RPC server bound to one authenticated daemon session.
pub struct McpServer {
    info: McpServerInfo,
    instructions: Option<String>,
    context: BridgeContext,
    bridge: Arc<dyn AuthorityBridge>,
    limits: ServerLimits,
    ownership: StdMutex<RequestLedger>,
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
            ownership: StdMutex::new(RequestLedger::default()),
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
        self.begin_connection()?;
        if let Err(error) = self.bridge.reconcile_connection(&self.context).await {
            self.finish_connection();
            return Err(bridge_lifecycle_error("reconcile MCP connection", error));
        }
        let lifecycle = Arc::new(Mutex::new(Lifecycle::Uninitialized));
        let (responses, mut response_receiver) = mpsc::unbounded_channel::<QueuedResponse>();
        let response_limit = self.limits.message_bytes;
        let writer_server = Arc::clone(&self);
        let writer_task = tokio::spawn(async move {
            while let Some(queued) = response_receiver.recv().await {
                write_response(&mut writer, queued.response.as_ref(), response_limit).await?;
                if let Some(owner) = queued.owner {
                    writer_server.response_delivered(&owner.request_id, &owner.cancellation);
                }
            }
            Ok::<(), McpError>(())
        });
        let mut writer_task = writer_task;
        let mut requests = JoinSet::new();
        let end = self
            .drive_connection(
                &mut reader,
                &responses,
                &mut requests,
                &lifecycle,
                &mut writer_task,
            )
            .await;
        let close = self.close_snapshot(end.reason);
        let close_result = self
            .bridge
            .close_connection(&self.context, close)
            .await
            .map_err(|error| bridge_lifecycle_error("close MCP connection", error));

        if !end.writer_finished {
            writer_task.abort();
        }
        requests.abort_all();
        drop(responses);
        let mut cleanup_error = None;
        if !end.writer_finished {
            match writer_task.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => cleanup_error = Some(error),
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    cleanup_error = Some(McpError::new(
                        McpErrorClass::Transport,
                        "join MCP writer",
                        error.to_string(),
                    ));
                }
            }
        }
        while let Some(joined) = requests.join_next().await {
            if cleanup_error.is_none() {
                cleanup_error = request_failure(joined);
            }
        }
        self.finish_connection();
        if let Some(error) = end.error {
            return Err(error);
        }
        close_result?;
        if let Some(error) = cleanup_error {
            return Err(error);
        }
        Ok(())
    }

    async fn drive_connection<R>(
        self: &Arc<Self>,
        reader: &mut R,
        responses: &mpsc::UnboundedSender<QueuedResponse>,
        requests: &mut JoinSet<Result<(), McpError>>,
        lifecycle: &Arc<Mutex<Lifecycle>>,
        writer_task: &mut tokio::task::JoinHandle<Result<(), McpError>>,
    ) -> ConnectionEnd
    where
        R: AsyncBufRead + Send + Unpin + 'static,
    {
        loop {
            tokio::select! {
                biased;
                writer = &mut *writer_task => {
                    let error = match writer {
                        Ok(Ok(())) => McpError::new(
                            McpErrorClass::Transport,
                            "write MCP response",
                            "MCP response writer stopped while the connection remained open",
                        ),
                        Ok(Err(error)) => error,
                        Err(error) => McpError::new(
                            McpErrorClass::Transport,
                            "join MCP writer",
                            error.to_string(),
                        ),
                    };
                    return ConnectionEnd {
                        reason: BridgeConnectionCloseReason::OutputFailed,
                        error: Some(error),
                        writer_finished: true,
                    };
                }
                joined = requests.join_next(), if !requests.is_empty() => {
                    if let Some(joined) = joined
                        && let Some(error) = request_failure(joined)
                    {
                        return ConnectionEnd {
                            reason: BridgeConnectionCloseReason::RequestFailed,
                            error: Some(error),
                            writer_finished: false,
                        };
                    }
                }
                message = read_message(reader, self.limits.message_bytes) => {
                    match message {
                        Ok(Some(message)) => {
                            if let Err(error) = self
                                .handle_message(&message, responses, requests, lifecycle)
                                .await
                            {
                                return ConnectionEnd {
                                    reason: BridgeConnectionCloseReason::RequestFailed,
                                    error: Some(error),
                                    writer_finished: false,
                                };
                            }
                        }
                        Ok(None) => {
                            return ConnectionEnd {
                                reason: BridgeConnectionCloseReason::InputClosed,
                                error: None,
                                writer_finished: false,
                            };
                        }
                        Err(error) => {
                            return ConnectionEnd {
                                reason: BridgeConnectionCloseReason::InputFailed,
                                error: Some(error),
                                writer_finished: false,
                            };
                        }
                    }
                }
            }
        }
    }

    async fn handle_message(
        self: &Arc<Self>,
        message: &[u8],
        responses: &mpsc::UnboundedSender<QueuedResponse>,
        requests: &mut JoinSet<Result<(), McpError>>,
        lifecycle: &Arc<Mutex<Lifecycle>>,
    ) -> Result<(), McpError> {
        let request = match serde_json::from_slice::<JsonRpcRequest>(message) {
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
                return Ok(());
            }
            Err(error) => {
                send_response(
                    responses,
                    JsonRpcResponse::failure(None, PARSE_ERROR, error.to_string()),
                )?;
                return Ok(());
            }
        };
        if request.id.is_none() {
            self.handle_notification(request, lifecycle).await;
            return Ok(());
        }
        if request.method == "initialize" {
            let Some(id) = request.id else { return Ok(()) };
            let response = self.initialize(id, request.params, lifecycle).await;
            send_response(responses, response)?;
            return Ok(());
        }
        let Some(id) = request.id.clone() else { return Ok(()) };
        let cancellation = McpCancellation::new();
        if !self.register_request(id.clone(), cancellation.clone())? {
            send_response(
                responses,
                JsonRpcResponse::failure(
                    Some(id),
                    INVALID_REQUEST,
                    "request id is already active or awaiting response delivery",
                ),
            )?;
            return Ok(());
        }
        let server = Arc::clone(self);
        let responses = responses.clone();
        let lifecycle = Arc::clone(lifecycle);
        let guard = ActiveRequestGuard {
            server: Arc::clone(self),
            request_id: id.clone(),
            cancellation: cancellation.clone(),
        };
        requests.spawn(async move {
            let _guard = guard;
            let permit = tokio::select! {
                biased;
                () = cancellation.cancelled() => None,
                acquired = Arc::clone(&server.admission).acquire_owned() => acquired.ok(),
            };
            let response = if let Some(permit) = permit {
                let response = server
                    .handle_admitted_request(
                        id.clone(),
                        request,
                        &cancellation,
                        &lifecycle,
                    )
                    .await;
                drop(permit);
                response
            } else if cancellation.is_cancelled() {
                Some(bridge_response(id.clone(), &cancelled()))
            } else {
                Some(JsonRpcResponse::failure(
                    Some(id.clone()),
                    INTERNAL_ERROR,
                    "MCP request admission closed unexpectedly",
                ))
            };
            if let Some(response) = response {
                server.queue_owned_response(&responses, &id, &cancellation, response)?;
            }
            Ok(())
        });
        Ok(())
    }

    async fn handle_notification(
        &self,
        request: JsonRpcRequest,
        lifecycle: &Mutex<Lifecycle>,
    ) {
        match request.method.as_str() {
            "notifications/initialized" => {
                let mut lifecycle = lifecycle.lock().await;
                if matches!(*lifecycle, Lifecycle::AwaitingInitialized) {
                    *lifecycle = Lifecycle::Ready;
                }
            }
            "notifications/cancelled" => {
                if let Ok(params) = parse_params::<CancelParams>(request.params) {
                    let _ = params.reason;
                    if let Some(cancellation) = self.request_cancellation(&params.request_id) {
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
        lifecycle: &Mutex<Lifecycle>,
    ) -> Option<JsonRpcResponse> {
        if request.method == "ping" {
            return Some(JsonRpcResponse::success(id, Value::Object(serde_json::Map::new())));
        }
        if !matches!(*lifecycle.lock().await, Lifecycle::Ready) {
            return Some(JsonRpcResponse::failure(
                Some(id),
                INVALID_REQUEST,
                "server has not completed MCP initialization",
            ));
        }
        self.dispatch(id, request, cancellation).await
    }

    async fn initialize(
        &self,
        id: RpcId,
        params: Option<Value>,
        lifecycle: &Mutex<Lifecycle>,
    ) -> JsonRpcResponse {
        let parsed = match parse_params::<InitializeParams>(params) {
            Ok(parsed) => parsed,
            Err(message) => return JsonRpcResponse::failure(Some(id), INVALID_PARAMS, message),
        };
        let mut lifecycle = lifecycle.lock().await;
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
    ) -> Option<JsonRpcResponse> {
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
            return Some(JsonRpcResponse::failure(
                Some(id),
                METHOD_NOT_FOUND,
                "method not found",
            ));
        }
        if !self.mark_daemon_dispatched(&id, cancellation) {
            return None;
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
        Some(match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(error) => bridge_response(id, &error),
        })
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

    fn request_ledger(&self) -> StdMutexGuard<'_, RequestLedger> {
        self.ownership.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn begin_connection(&self) -> Result<(), McpError> {
        let mut ledger = self.request_ledger();
        if ledger.connection_open || !ledger.entries.is_empty() || !ledger.order.is_empty() {
            return Err(McpError::new(
                McpErrorClass::Lifecycle,
                "open MCP connection",
                "MCP server already owns a connection or unresolved local requests",
            ));
        }
        ledger.connection_open = true;
        ledger.closing = false;
        Ok(())
    }

    fn finish_connection(&self) {
        let mut ledger = self.request_ledger();
        ledger.entries.clear();
        ledger.order.clear();
        ledger.closing = false;
        ledger.connection_open = false;
    }

    fn register_request(
        &self,
        request_id: RpcId,
        cancellation: McpCancellation,
    ) -> Result<bool, McpError> {
        let mut ledger = self.request_ledger();
        if !ledger.connection_open || ledger.closing {
            return Err(McpError::new(
                McpErrorClass::Lifecycle,
                "admit MCP request",
                "MCP connection is not accepting requests",
            ));
        }
        if ledger.entries.contains_key(&request_id) {
            return Ok(false);
        }
        ledger.order.push(request_id.clone());
        ledger.entries.insert(
            request_id,
            RequestOwnership {
                cancellation,
                task_active: true,
                daemon_dispatched: false,
                response: None,
            },
        );
        Ok(true)
    }

    fn request_cancellation(&self, request_id: &RpcId) -> Option<McpCancellation> {
        self.request_ledger()
            .entries
            .get(request_id)
            .filter(|ownership| ownership.task_active && ownership.response.is_none())
            .map(|ownership| ownership.cancellation.clone())
    }

    fn mark_daemon_dispatched(
        &self,
        request_id: &RpcId,
        cancellation: &McpCancellation,
    ) -> bool {
        let mut ledger = self.request_ledger();
        if ledger.closing {
            return false;
        }
        let Some(ownership) = ledger.entries.get_mut(request_id) else { return false };
        if !ownership.task_active
            || !ownership.cancellation.same_request(cancellation)
            || ownership.response.is_some()
        {
            return false;
        }
        ownership.daemon_dispatched = true;
        true
    }

    fn queue_owned_response(
        &self,
        sender: &mpsc::UnboundedSender<QueuedResponse>,
        request_id: &RpcId,
        cancellation: &McpCancellation,
        response: JsonRpcResponse,
    ) -> Result<(), McpError> {
        let response = Arc::new(response);
        {
            let mut ledger = self.request_ledger();
            let Some(ownership) = ledger.entries.get_mut(request_id) else { return Ok(()) };
            if !ownership.task_active
                || !ownership.cancellation.same_request(cancellation)
                || ownership.response.is_some()
            {
                return Ok(());
            }
            ownership.response = Some(Arc::clone(&response));
        }
        send_queued_response(
            sender,
            QueuedResponse {
                response,
                owner: Some(ResponseOwner {
                    request_id: request_id.clone(),
                    cancellation: cancellation.clone(),
                }),
            },
        )
    }

    fn response_delivered(&self, request_id: &RpcId, cancellation: &McpCancellation) {
        let mut ledger = self.request_ledger();
        let delivered = ledger.entries.get(request_id).is_some_and(|ownership| {
            ownership.cancellation.same_request(cancellation) && ownership.response.is_some()
        });
        if delivered {
            ledger.entries.remove(request_id);
            ledger.order.retain(|candidate| candidate != request_id);
        }
    }

    fn release_unanswered_request(&self, request_id: &RpcId, cancellation: &McpCancellation) {
        let mut ledger = self.request_ledger();
        let Some(ownership) = ledger.entries.get_mut(request_id) else { return };
        if !ownership.cancellation.same_request(cancellation) || ownership.response.is_some() {
            return;
        }
        if ownership.daemon_dispatched {
            ownership.task_active = false;
        } else {
            ledger.entries.remove(request_id);
            ledger.order.retain(|candidate| candidate != request_id);
        }
    }

    fn close_snapshot(&self, reason: BridgeConnectionCloseReason) -> BridgeConnectionClose {
        let mut ledger = self.request_ledger();
        ledger.closing = true;
        let requests = ledger
            .order
            .iter()
            .filter_map(|request_id| {
                ledger.entries.get(request_id).map(|ownership| {
                    BridgeRequestOwnership::new(
                        request_id.clone(),
                        ownership.task_active,
                        ownership.daemon_dispatched,
                        ownership.cancellation.is_cancelled(),
                        ownership.response.as_deref().cloned(),
                    )
                })
            })
            .collect();
        BridgeConnectionClose::new(reason, requests)
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

fn request_failure(
    joined: Result<Result<(), McpError>, tokio::task::JoinError>,
) -> Option<McpError> {
    match joined {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(error) if error.is_cancelled() => None,
        Err(error) => Some(McpError::new(
            McpErrorClass::Lifecycle,
            "join MCP request",
            error.to_string(),
        )),
    }
}

fn bridge_lifecycle_error(operation: &'static str, error: BridgeError) -> McpError {
    McpError::with_source(McpErrorClass::Bridge, operation, error.to_string(), error)
}

fn send_response(
    sender: &mpsc::UnboundedSender<QueuedResponse>,
    response: JsonRpcResponse,
) -> Result<(), McpError> {
    send_queued_response(sender, QueuedResponse { response: Arc::new(response), owner: None })
}

fn send_queued_response(
    sender: &mpsc::UnboundedSender<QueuedResponse>,
    response: QueuedResponse,
) -> Result<(), McpError> {
    sender.send(response).map_err(|_| {
        McpError::new(
            McpErrorClass::Transport,
            "queue MCP response",
            "response writer closed unexpectedly",
        )
    })
}
