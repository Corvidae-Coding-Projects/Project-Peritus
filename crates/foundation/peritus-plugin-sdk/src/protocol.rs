//! Typed host/plugin request, result, lifecycle, and failure protocol.

use serde::Deserialize;

use crate::{JsonPayload, PluginId, PluginQuotas, PluginVersion, RequestId};

mod request_wire;
mod response_wire;
mod wire;

/// Current plugin protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

/// Only role available to an untrusted plugin process or Wasm component.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginRole {
    /// B1 untrusted plugin role.
    Plugin,
}

/// Invocation identity and already-authorized capability projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InvocationContext {
    /// Opaque authenticated daemon session identifier.
    pub session_id: String,
    /// Opaque authenticated actor identifier.
    pub actor_id: String,
    /// Compiled untrusted extension role.
    pub role: PluginRole,
    /// Exact capability names authorized for this invocation.
    pub granted_capabilities: Vec<String>,
    /// Current daemon authority generation.
    pub authority_generation: u64,
    /// Monotonic deadline supplied by the host.
    pub deadline_millis: u64,
}

/// Closed host-to-plugin request body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostRequest {
    /// Performs one lifecycle handshake before any invocation.
    Initialize {
        /// Host-selected protocol version.
        protocol_version: u16,
        /// Plugin identity expected by discovery.
        plugin_id: PluginId,
        /// Plugin version expected by discovery.
        plugin_version: PluginVersion,
        /// Host-narrowed resource quotas.
        quotas: PluginQuotas,
    },
    /// Invokes one declared plugin capability.
    Invoke {
        /// Exact capability being invoked.
        capability: String,
        /// Bounded structured request payload.
        input: JsonPayload,
        /// Authenticated and current invocation projection.
        context: InvocationContext,
    },
    /// Requests cooperative cancellation of one active invocation.
    Cancel {
        /// Target request identifier.
        request_id: RequestId,
        /// Stable cancellation reason.
        reason: String,
    },
    /// Requests a bounded health response.
    Health,
    /// Requests an orderly plugin shutdown.
    Shutdown,
}

/// Complete versioned host request envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginRequestEnvelope {
    /// Exact protocol schema version.
    pub protocol_version: u16,
    /// Request/correlation identifier.
    pub request_id: RequestId,
    /// Typed request body.
    pub request: HostRequest,
}

/// Stable plugin failure class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureClass {
    /// Request or response violated the protocol.
    Protocol,
    /// Host-side authority mediation rejected the action.
    Authorization,
    /// Requested plugin operation is unsupported.
    Unsupported,
    /// Plugin operation rejected valid input.
    InvalidInput,
    /// Plugin resource quota was exhausted.
    Quota,
    /// Plugin reported an internal failure.
    Plugin,
    /// Host isolation/runtime infrastructure failed.
    Infrastructure,
    /// Cooperative cancellation completed.
    Cancelled,
    /// Invocation deadline elapsed.
    Timeout,
    /// Host cannot determine whether an effect completed.
    Indeterminate,
}

/// Typed plugin failure independent from rendering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginFailure {
    /// Stable failure class.
    pub class: FailureClass,
    /// Stable plugin or host code.
    pub code: String,
    /// Bounded causal detail.
    pub detail: String,
    /// Whether a new, freshly authorized action may be attempted.
    pub retryable_with_new_action: bool,
}

/// Host-observed plugin lifecycle status.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    /// Initialization handshake completed.
    Ready,
    /// Health response from a ready plugin.
    Healthy,
    /// Cancellation was observed.
    Cancelled,
    /// Shutdown was acknowledged.
    Stopped,
}

/// Closed plugin-to-host response body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginResponse {
    /// Lifecycle response.
    Status {
        /// Current status.
        status: PluginStatus,
    },
    /// Successful structured invocation result.
    Success {
        /// Bounded structured result.
        output: JsonPayload,
        /// Optional bounded human/model rendering.
        rendering: Option<String>,
    },
    /// Truthful invocation or lifecycle failure.
    Failure(PluginFailure),
}

/// Complete versioned plugin response envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginResponseEnvelope {
    /// Exact protocol schema version.
    pub protocol_version: u16,
    /// Identifier copied from the corresponding request.
    pub request_id: RequestId,
    /// Typed response body.
    pub response: PluginResponse,
}
