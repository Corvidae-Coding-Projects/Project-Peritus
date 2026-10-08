//! Typed host/plugin request, result, lifecycle, and failure protocol.

use serde::Deserialize;

use crate::{
    JsonBounds, JsonPayload, PluginId, PluginQuotas, PluginVersion, RequestId, SdkError,
    SdkErrorKind,
};
use crate::framing::PluginFrame;

mod request_wire;
mod response_wire;

/// Historical plugin protocol with numeric finite quotas and compulsory deadlines.
pub const LEGACY_PROTOCOL_VERSION: u16 = 1;
/// Current plugin protocol version.
pub const PROTOCOL_VERSION: u16 = 2;

/// Only role available to an untrusted plugin process or Wasm component.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginRole {
    /// B1 untrusted plugin role.
    Plugin,
}

/// Invocation identity and already-authorized capability projection.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Optional host-enforced duration for this invocation; `None` is explicitly untimed.
    pub deadline_millis: Option<u64>,
}

impl InvocationContext {
    /// Creates a validated authenticated invocation projection.
    ///
    /// # Errors
    ///
    /// Rejects empty/control-containing identities, noncanonical capabilities, or a zero deadline.
    pub fn new(
        session_id: impl Into<String>,
        actor_id: impl Into<String>,
        role: PluginRole,
        granted_capabilities: Vec<String>,
        authority_generation: u64,
        deadline_millis: Option<u64>,
    ) -> Result<Self, SdkError> {
        let value = Self {
            session_id: session_id.into(),
            actor_id: actor_id.into(),
            role,
            granted_capabilities,
            authority_generation,
            deadline_millis,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), SdkError> {
        validate_opaque_identity(&self.session_id, "session id")?;
        validate_opaque_identity(&self.actor_id, "actor id")?;
        if self.deadline_millis == Some(0) {
            return Err(protocol_identity_error("invocation deadline must be positive"));
        }
        for capability in &self.granted_capabilities {
            crate::manifest::validate_capability_name(capability)?;
        }
        if self.granted_capabilities.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(SdkError::new(
                SdkErrorKind::NonCanonical,
                "validate invocation context",
                "granted capabilities must be strictly ordered without duplicates",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for InvocationContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            session_id: String,
            actor_id: String,
            role: PluginRole,
            granted_capabilities: Vec<String>,
            authority_generation: u64,
            deadline_millis: Option<u64>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.session_id,
            wire.actor_id,
            wire.role,
            wire.granted_capabilities,
            wire.authority_generation,
            wire.deadline_millis,
        )
        .map_err(serde::de::Error::custom)
    }
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginFailure {
    /// Stable failure class.
    class: FailureClass,
    /// Stable plugin or host code.
    code: PluginFailureCode,
    /// Causal diagnostic detail.
    detail: PluginFailureDetail,
    /// Whether a new, freshly authorized action may be attempted.
    retryable_with_new_action: bool,
}

/// Validated stable plugin failure code.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PluginFailureCode(String);

impl PluginFailureCode {
    /// Creates a stable ASCII failure code.
    ///
    /// # Errors
    ///
    /// Rejects empty values and characters outside the stable code alphabet.
    pub fn new(value: impl Into<String>) -> Result<Self, SdkError> {
        let value = value.into();
        if value.is_empty()
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
            })
        {
            return Err(protocol_identity_error("plugin failure code is not canonical ASCII"));
        }
        Ok(Self(value))
    }

    /// Borrows the stable code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated plugin diagnostic text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginFailureDetail(String);

impl PluginFailureDetail {
    /// Creates diagnostic text without unsafe control characters.
    ///
    /// # Errors
    ///
    /// Rejects empty text and controls other than line-feed or tab.
    pub fn new(value: impl Into<String>) -> Result<Self, SdkError> {
        let value = value.into();
        if value.is_empty()
            || value
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        {
            return Err(protocol_identity_error("plugin failure detail is empty or unsafe"));
        }
        Ok(Self(value))
    }

    /// Borrows diagnostic text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PluginFailure {
    /// Creates a typed, validated plugin failure.
    ///
    /// # Errors
    ///
    /// Rejects a malformed stable code or unsafe diagnostic text.
    pub fn new(
        class: FailureClass,
        code: impl Into<String>,
        detail: impl Into<String>,
        retryable_with_new_action: bool,
    ) -> Result<Self, SdkError> {
        Ok(Self {
            class,
            code: PluginFailureCode::new(code)?,
            detail: PluginFailureDetail::new(detail)?,
            retryable_with_new_action,
        })
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn class(&self) -> FailureClass {
        self.class
    }

    /// Borrows the stable failure code.
    #[must_use]
    pub const fn code(&self) -> &PluginFailureCode {
        &self.code
    }

    /// Borrows causal diagnostic text.
    #[must_use]
    pub const fn detail(&self) -> &PluginFailureDetail {
        &self.detail
    }

    /// Returns whether a freshly authorized action may be attempted.
    #[must_use]
    pub const fn retryable_with_new_action(&self) -> bool {
        self.retryable_with_new_action
    }
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginResponseEnvelope {
    /// Exact protocol schema version.
    pub protocol_version: u16,
    /// Identifier copied from the corresponding request.
    pub request_id: RequestId,
    /// Typed response body.
    pub response: PluginResponse,
}

impl PluginFrame for PluginRequestEnvelope {
    fn validate_payloads(&self, bounds: JsonBounds) -> Result<(), crate::SdkError> {
        if let HostRequest::Invoke { input, .. } = &self.request {
            input.validate_bounds(bounds)?;
        }
        Ok(())
    }
}

impl PluginFrame for PluginResponseEnvelope {
    fn validate_payloads(&self, bounds: JsonBounds) -> Result<(), crate::SdkError> {
        if let PluginResponse::Success { output, .. } = &self.response {
            output.validate_bounds(bounds)?;
        }
        Ok(())
    }
}

fn validate_opaque_identity(value: &str, label: &'static str) -> Result<(), SdkError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(SdkError::new(
            SdkErrorKind::InvalidIdentity,
            "validate invocation context",
            format!("{label} must contain non-control UTF-8 text"),
        ))
    } else {
        Ok(())
    }
}

fn protocol_identity_error(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::InvalidIdentity, "validate plugin protocol identity", detail)
}
