//! Non-authoritative G0/A3/C4 bridge contract and projections.

use std::{future::Future, pin::Pin};

use peritus_policy::ActorRole;
use peritus_tool_protocol::{ResultStatus, ToolDescriptor, ToolResult};
use peritus_types::{ActorId, SessionId};
use serde_json::Value;

use crate::{BridgeError, BridgeErrorClass, McpCancellation};

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

/// Adapter implemented at the daemon boundary over existing A3/G0/C4 authority owners.
///
/// Implementations must compute tool exposure through the C4 registry and current B1 scope, route
/// calls through C4 preparation/authorization/dispatch, and apply A3 session/revision binding to
/// resources and prompts. Returning a value is an observation; this trait has no grant API.
pub trait AuthorityBridge: Send + Sync {
    /// Lists tools already exposed to the exact authenticated context.
    fn list_tools<'a>(
        &'a self,
        context: &'a BridgeContext,
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
