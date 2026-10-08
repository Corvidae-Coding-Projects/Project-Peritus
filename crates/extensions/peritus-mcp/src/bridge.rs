//! Non-authoritative G0/A3/C4 bridge contract and projections.

use std::{future::Future, pin::Pin};

use peritus_policy::ActorRole;
use peritus_tool_protocol::{ResultStatus, ToolDescriptor, ToolResult};
use peritus_types::{ActorId, SessionId};
use serde_json::Value;

use crate::{BridgeError, BridgeErrorClass, JsonRpcResponse, McpCancellation, RpcId};

mod wire;

/// Sendable borrowed future returned by bridge methods.
pub type BridgeFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Authenticated daemon session projected into the MCP bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeContext {
    actor_id: ActorId,
    session_id: SessionId,
    authority_generation: u64,
}

impl BridgeContext {
    /// Creates an authenticated untrusted-extension context.
    #[must_use]
    pub const fn new(actor_id: ActorId, session_id: SessionId, authority_generation: u64) -> Self {
        Self { actor_id, session_id, authority_generation }
    }

    /// Returns the authenticated actor identity.
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }

    /// Returns the authenticated A3 session identity.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }

    /// Returns the current G0 authority generation.
    #[must_use]
    pub const fn authority_generation(self) -> u64 {
        self.authority_generation
    }

    /// Returns the compiled B1 role for MCP-originated work.
    #[must_use]
    pub const fn role(self) -> ActorRole {
        ActorRole::Plugin
    }
}

/// MCP projection of one already-exposed C4 tool descriptor.
#[derive(Clone, Debug)]
pub struct BridgeTool {
    /// Canonical C4 capability/tool name.
    pub name: String,
    /// Bounded descriptor explanation.
    pub description: String,
    /// C4 canonical JSON Schema.
    pub input_schema: Value,
}

impl BridgeTool {
    /// Projects an already-exposed C4 descriptor without changing exposure.
    ///
    /// # Errors
    ///
    /// Returns an infrastructure error only if the C4 canonical schema cannot be represented as
    /// JSON, which indicates internal descriptor corruption.
    pub fn from_exposed_descriptor(descriptor: &ToolDescriptor) -> Result<Self, BridgeError> {
        let input_schema =
            serde_json::from_slice(&descriptor.schema().canonical_bytes()).map_err(|error| {
                BridgeError::with_source(
                    BridgeErrorClass::Infrastructure,
                    "c4_schema_projection",
                    "canonical C4 schema could not be projected into MCP JSON",
                    error,
                )
            })?;
        Ok(Self {
            name: descriptor.name().as_str().to_owned(),
            description: descriptor.description().as_str().to_owned(),
            input_schema,
        })
    }
}

/// One MCP resource descriptor returned from an authority-filtered daemon query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeResource {
    /// Stable resource URI.
    pub uri: String,
    /// Human-readable resource name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Optional media type.
    pub mime_type: Option<String>,
}

/// Bounded contents returned by an authority-mediated resource read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeResourceContents {
    /// Exact requested resource URI.
    pub uri: String,
    /// Optional media type.
    pub mime_type: Option<String>,
    /// Text content when the resource is textual.
    pub text: Option<String>,
    /// Base64 content when the resource is binary.
    pub blob: Option<String>,
}

/// One MCP prompt argument declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgePromptArgument {
    /// Argument name.
    pub name: String,
    /// Optional user-facing description.
    pub description: Option<String>,
    /// Whether the argument is mandatory.
    pub required: bool,
}

/// One authority-filtered prompt template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgePrompt {
    /// Stable prompt name.
    pub name: String,
    /// Optional user-facing description.
    pub description: Option<String>,
    /// Declared arguments.
    pub arguments: Vec<BridgePromptArgument>,
}

/// Rendered MCP prompt message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgePromptMessage {
    /// MCP message role (`user` or `assistant`).
    pub role: String,
    /// Text content block.
    pub content: PromptTextContent,
}

/// Text prompt content block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptTextContent {
    /// MCP content type.
    pub content_type: &'static str,
    /// Rendered text.
    pub text: String,
}

impl BridgePromptMessage {
    /// Creates a bounded text prompt message.
    #[must_use]
    pub fn text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: PromptTextContent { content_type: "text", text: text.into() },
        }
    }
}

/// Truthful MCP tool-call result projection.
#[derive(Clone, Debug)]
pub struct BridgeToolCallResult {
    /// MCP content blocks.
    pub content: Vec<ToolTextContent>,
    /// Structured result when present.
    pub structured_content: Option<Value>,
    /// True for every non-success C4 status.
    pub is_error: bool,
}

/// Text MCP tool-result content block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolTextContent {
    /// MCP content type.
    pub content_type: &'static str,
    /// Bounded model rendering.
    pub text: String,
}

impl BridgeToolCallResult {
    /// Projects a terminal C4 result without converting failure prose into success.
    ///
    /// # Errors
    ///
    /// Returns an infrastructure error if validated C4 structured JSON cannot be decoded.
    pub fn from_c4(result: &ToolResult) -> Result<Self, BridgeError> {
        let structured_content = result
            .structured()
            .map(|value| serde_json::from_slice(value.canonical_bytes()))
            .transpose()
            .map_err(|error| {
                BridgeError::with_source(
                    BridgeErrorClass::Infrastructure,
                    "c4_result_projection",
                    "canonical C4 result could not be projected into MCP JSON",
                    error,
                )
            })?;
        Ok(Self {
            content: vec![ToolTextContent {
                content_type: "text",
                text: result.model_rendering().as_str().to_owned(),
            }],
            structured_content,
            is_error: result.status() != ResultStatus::Succeeded,
        })
    }
}

/// Transport observation that caused one MCP connection to relinquish local ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeConnectionCloseReason {
    /// The client input reached a clean end of stream.
    InputClosed,
    /// Reading or decoding the client input failed.
    InputFailed,
    /// The response writer failed or stopped unexpectedly.
    OutputFailed,
    /// An owned request task failed or panicked.
    RequestFailed,
}

/// Exact per-request ownership visible when an MCP connection closes.
#[derive(Clone, Debug)]
pub struct BridgeRequestOwnership {
    request_id: RpcId,
    local_task_active: bool,
    daemon_dispatched: bool,
    cancellation_requested: bool,
    response: Option<JsonRpcResponse>,
}

impl BridgeRequestOwnership {
    pub(crate) const fn new(
        request_id: RpcId,
        local_task_active: bool,
        daemon_dispatched: bool,
        cancellation_requested: bool,
        response: Option<JsonRpcResponse>,
    ) -> Self {
        Self {
            request_id,
            local_task_active,
            daemon_dispatched,
            cancellation_requested,
            response,
        }
    }

    /// Borrows the exact JSON-RPC request identity.
    #[must_use]
    pub const fn request_id(&self) -> &RpcId {
        &self.request_id
    }

    /// Reports whether the connection still owned a live local request task at handoff.
    #[must_use]
    pub const fn local_task_active(&self) -> bool {
        self.local_task_active
    }

    /// Reports whether dispatch crossed into the daemon-owned bridge operation.
    #[must_use]
    pub const fn daemon_dispatched(&self) -> bool {
        self.daemon_dispatched
    }

    /// Reports explicit client cancellation independently of connection closure.
    #[must_use]
    pub const fn cancellation_requested(&self) -> bool {
        self.cancellation_requested
    }

    /// Borrows the completed response when local delivery remained unresolved.
    #[must_use]
    pub const fn response(&self) -> Option<&JsonRpcResponse> {
        self.response.as_ref()
    }
}

/// Ordered ownership handoff from a closing MCP connection to durable daemon authority.
#[derive(Clone, Debug)]
pub struct BridgeConnectionClose {
    reason: BridgeConnectionCloseReason,
    requests: Vec<BridgeRequestOwnership>,
}

impl BridgeConnectionClose {
    pub(crate) const fn new(
        reason: BridgeConnectionCloseReason,
        requests: Vec<BridgeRequestOwnership>,
    ) -> Self {
        Self { reason, requests }
    }

    /// Returns the observed connection-close reason.
    #[must_use]
    pub const fn reason(&self) -> BridgeConnectionCloseReason {
        self.reason
    }

    /// Borrows unresolved requests in their original admission order.
    #[must_use]
    pub fn requests(&self) -> &[BridgeRequestOwnership] {
        &self.requests
    }
}

/// Adapter implemented at the daemon boundary over existing A3/G0/C4 authority owners.
///
/// Implementations must compute tool exposure through the C4 registry and current B1 scope, route
/// calls through C4 preparation/authorization/dispatch, and apply A3 session/revision binding to
/// resources and prompts. Each returned future owns its daemon operation: implementations must
/// propagate the supplied cancellation and must not detach accepted effects when the future is
/// dropped. Returning a value is an observation; this trait has no grant API.
pub trait AuthorityBridge: Send + Sync {
    /// Reconciles a new connection with durable authority for the exact session and generation.
    ///
    /// Implementations must recover prior close handoffs before new requests can be admitted. A
    /// recovered operation is deduplicated or observed through daemon-owned state; it is never
    /// silently redispatched by the MCP transport.
    fn reconcile_connection<'a>(
        &'a self,
        context: &'a BridgeContext,
    ) -> BridgeFuture<'a, Result<(), BridgeError>>;

    /// Durably assumes ownership of unresolved work from a closing connection.
    ///
    /// This is an observation and reconciliation handoff, not cancellation intent. Implementations
    /// retain dispatched-operation and response facts by request identity until a later exact
    /// session reconciliation resolves them.
    fn close_connection<'a>(
        &'a self,
        context: &'a BridgeContext,
        close: BridgeConnectionClose,
    ) -> BridgeFuture<'a, Result<(), BridgeError>>;

    /// Lists tools already exposed to the exact authenticated context.
    fn list_tools<'a>(
        &'a self,
        context: &'a BridgeContext,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgeTool>, BridgeError>>;

    /// Routes one tool call through the authoritative C4/G0 lifecycle.
    fn call_tool<'a>(
        &'a self,
        context: &'a BridgeContext,
        name: &'a str,
        arguments: Value,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<BridgeToolCallResult, BridgeError>>;

    /// Lists resources visible through the exact authenticated A3 session.
    fn list_resources<'a>(
        &'a self,
        context: &'a BridgeContext,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgeResource>, BridgeError>>;

    /// Reads one exact authority-filtered resource.
    fn read_resource<'a>(
        &'a self,
        context: &'a BridgeContext,
        uri: &'a str,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgeResourceContents>, BridgeError>>;

    /// Lists prompts visible through the exact authenticated A3 session.
    fn list_prompts<'a>(
        &'a self,
        context: &'a BridgeContext,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgePrompt>, BridgeError>>;

    /// Resolves one prompt template through current daemon state.
    fn get_prompt<'a>(
        &'a self,
        context: &'a BridgeContext,
        name: &'a str,
        arguments: Value,
        cancellation: &'a McpCancellation,
    ) -> BridgeFuture<'a, Result<Vec<BridgePromptMessage>, BridgeError>>;
}
