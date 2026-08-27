//! Bounded MCP server and non-authoritative bridge for Peritus.
//!
//! MCP input is untrusted protocol data. The crate projects already-exposed tools and forwards
//! calls to a daemon-owned [`AuthorityBridge`]; it cannot construct C4 invocation permits.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

mod bridge;
mod cancellation;
mod error;
mod framing;
mod jsonrpc;
mod protocol;
mod server;

pub use bridge::{
    AuthorityBridge, BridgeContext, BridgeFuture, BridgePrompt, BridgePromptArgument,
    BridgePromptMessage, BridgeResource, BridgeResourceContents, BridgeTool, BridgeToolCallResult,
};
pub use cancellation::McpCancellation;
pub use error::{BridgeError, BridgeErrorClass, McpError, McpErrorClass};
pub use jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, RpcId};
pub use protocol::{MCP_PROTOCOL_VERSION, McpClientInfo, McpServerInfo};
pub use server::{McpServer, ServerLimits};
