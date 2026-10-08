//! Checked provider-neutral request to private stable-v1 Google JSON projection.

mod content;
mod generate;
mod interactions;
mod value;

use std::collections::BTreeSet;

use peritus_model_protocol::{
    CachePolicy, Capability, ContentBlock, ModelRequest, ParallelToolPolicy, RequestFingerprint,
    Role, ToolChoice, WireDialect,
};
use peritus_provider_core::{CancellationToken, Endpoint, ProviderCoreError};
use serde_json::Value;

use crate::config::GoogleConfig;

#[derive(Clone)]
pub(crate) struct EncodedRequest {
    pub(crate) endpoint: Endpoint,
    pub(crate) body: Vec<u8>,
    pub(crate) structured: bool,
    pub(crate) tool_controls: ToolControls,
}

pub(crate) fn encode(
    request: &ModelRequest,
    config: &GoogleConfig,
) -> Result<EncodedRequest, ProviderCoreError> {
    let mut projection = Projection::new(request, config)?;
    while !projection.advance(request)? {}
    projection.finish_encoding(request, config, None)?;
    projection.take_encoded()
}

pub(crate) struct Projection {
    fingerprint: RequestFingerprint,
    state: ProjectionState,
    projected: Option<Value>,
    encoded: Option<EncodedRequest>,
    controls: ToolControls,
}

enum ProjectionState {
    Interactions(interactions::Projection),
    Generate(generate::Projection),
    Finished,
}

impl Projection {
    pub(crate) fn new(
        request: &ModelRequest,
        config: &GoogleConfig,
    ) -> Result<Self, ProviderCoreError> {
        validate(request, config)?;
        let fingerprint = request
            .fingerprint()
            .map_err(|_| invalid("Google request fingerprint is unavailable"))?;
        let state = match request.dialect() {
            WireDialect::GeminiInteractionsV1 => {
                ProjectionState::Interactions(interactions::Projection::new())
            }
            WireDialect::GeminiGenerateContentV1 => {
                ProjectionState::Generate(generate::Projection::new())
            }
            _ => return Err(invalid("request selected a non-Google wire dialect")),
        };
        Ok(Self {
            fingerprint,
            state,
            projected: None,
            encoded: None,
            controls: ToolControls::new(request)?,
        })
    }

    pub(crate) fn matches(&self, request: &ModelRequest) -> Result<bool, ProviderCoreError> {
        let fingerprint = request
            .fingerprint()
            .map_err(|_| invalid("Google request fingerprint is unavailable"))?;
        Ok(self.fingerprint == fingerprint)
    }

    pub(crate) fn advance(
        &mut self,
        request: &ModelRequest,
    ) -> Result<bool, ProviderCoreError> {
        if !self.matches(request)? {
            return Err(invalid("Google projection changed request fingerprint"));
        }
        if self.projected.is_some() || self.encoded.is_some() {
            return Ok(true);
        }
        let complete = match &mut self.state {
            ProjectionState::Interactions(state) => state.advance(request)?,
            ProjectionState::Generate(state) => state.advance(request)?,
            ProjectionState::Finished => {
                return Err(invalid("Google projection finished without encoded bytes"));
            }
        };
        if !complete {
            return Ok(false);
        }
        let state = core::mem::replace(&mut self.state, ProjectionState::Finished);
        let value = match state {
            ProjectionState::Interactions(state) => state.finish(request)?,
            ProjectionState::Generate(state) => state.finish(request)?,
            ProjectionState::Finished => {
                return Err(invalid("Google projection finished more than once"));
            }
        };
        self.projected = Some(value);
        Ok(true)
    }

    pub(crate) fn finish_encoding(
        &mut self,
        request: &ModelRequest,
        config: &GoogleConfig,
        cancellation: Option<&CancellationToken>,
    ) -> Result<(), ProviderCoreError> {
        if !self.matches(request)? {
            return Err(invalid("Google projection changed request fingerprint"));
        }
        if self.encoded.is_some() {
            return Ok(());
        }
        let value = self
            .projected
            .as_ref()
            .ok_or_else(|| invalid("Google history projection has not completed"))?;
        let endpoint = config.operation_endpoint(request.dialect(), request.model().as_str())?;
        let mut writer = CancellableWriter::new(cancellation);
        if serde_json::to_writer(&mut writer, value).is_err() {
            if cancellation.is_some_and(CancellationToken::is_cancelled) {
                return Err(ProviderCoreError::cancelled("google_projection"));
            }
            return Err(ProviderCoreError::invalid_request(
                "google_encode",
                "Google request serialization failed",
            ));
        }
        self.encoded = Some(EncodedRequest {
            endpoint,
            body: writer.into_bytes(),
            structured: !matches!(
                request.options().output(),
                peritus_model_protocol::StructuredOutput::Text
            ),
            tool_controls: self.controls.clone(),
        });
        self.projected = None;
        Ok(())
    }

    pub(crate) fn take_encoded(&mut self) -> Result<EncodedRequest, ProviderCoreError> {
        self.encoded
            .take()
            .ok_or_else(|| invalid("Google projection has not completed"))
    }
}

struct CancellableWriter<'a> {
    bytes: Vec<u8>,
    cancellation: Option<&'a CancellationToken>,
}

impl<'a> CancellableWriter<'a> {
    const CHUNK_BYTES: usize = 16 * 1024;

    const fn new(cancellation: Option<&'a CancellationToken>) -> Self {
        Self { bytes: Vec::new(), cancellation }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl std::io::Write for CancellableWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
        }
        let length = bytes.len().min(Self::CHUNK_BYTES);
        self.bytes.extend_from_slice(&bytes[..length]);
        Ok(length)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct ToolControls {
    choice: ToolChoice,
    allowed: BTreeSet<String>,
    maximum_calls: u32,
}

impl ToolControls {
    fn new(request: &ModelRequest) -> Result<Self, ProviderCoreError> {
        let allowed = request
            .tools()
            .iter()
            .map(|tool| tool.name().as_str().to_owned())
            .collect::<BTreeSet<_>>();
        if let ToolChoice::Specific(name) = request.tool_choice()
            && !allowed.contains(name.as_str())
        {
            return Err(invalid("Google specific tool choice is not declared"));
        }
        let maximum_calls = match (request.tool_choice(), request.parallel_tool_policy()) {
            (ToolChoice::None, _) => 0,
            (_, ParallelToolPolicy::Disabled) => 1,
            (_, ParallelToolPolicy::Allowed(maximum)) => maximum,
        };
        Ok(Self { choice: request.tool_choice().clone(), allowed, maximum_calls })
    }

    pub(crate) fn terminal() -> Self {
        Self {
            choice: ToolChoice::Auto,
            allowed: BTreeSet::new(),
            maximum_calls: 0,
        }
    }

    pub(crate) fn admit_call(
        &self,
        name: &str,
        previous_calls: u32,
    ) -> Result<u32, ProviderCoreError> {
        if !self.allowed.contains(name) {
            return Err(invalid("Google returned an undeclared function call"));
        }
        if let ToolChoice::Specific(selected) = &self.choice
            && selected.as_str() != name
        {
            return Err(invalid("Google returned a function outside the specific tool choice"));
        }
        let calls = previous_calls
            .checked_add(1)
            .ok_or_else(|| invalid("Google function-call count overflowed"))?;
        if calls > self.maximum_calls {
            return Err(invalid("Google exceeded the selected parallel function-call policy"));
        }
        Ok(calls)
    }

    pub(crate) fn validate_completion(&self, calls: u32) -> Result<(), ProviderCoreError> {
        let valid = match &self.choice {
            ToolChoice::Required | ToolChoice::Specific(_) => calls > 0,
            ToolChoice::None => calls == 0,
            ToolChoice::Auto => true,
        };
        if !valid {
            return Err(invalid("Google response contradicted the selected tool choice"));
        }
        Ok(())
    }
}

pub(crate) fn validate(
    request: &ModelRequest,
    config: &GoogleConfig,
) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(invalid(
            "Google streaming must be selected because the adapter opens a streaming response",
        ));
    }
    if request.options().persistence().background() {
        return Err(invalid(
            "foreground Google streams do not expose background retrieval or server cancellation",
        ));
    }
    if !request.options().extensions().is_empty() {
        return Err(invalid("Google provider extensions are not profile-authorized"));
    }
    if request.dialect() == WireDialect::GeminiGenerateContentV1
        && request.options().generation().stop_sequences().len() > 5
    {
        return Err(invalid("Google Generate Content accepts at most five stop sequences"));
    }
    if request.tools().iter().any(|tool| !valid_function_name(tool.name().as_str()))
        || request.messages().iter().flat_map(|message| message.content()).any(|block| {
            matches!(block, ContentBlock::ToolCall(call) if !valid_function_name(call.name().as_str()))
        })
    {
        return Err(invalid(
            "Google function names exceed the stable-v1 128-character grammar",
        ));
    }
    let model = request.model().as_str();
    if request.dialect() == WireDialect::GeminiGenerateContentV1
        && (model.is_empty()
            || model.len() > 256
            || !model
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
    {
        return Err(invalid(
            "Google Generate Content model name is not one safe resource-name segment",
        ));
    }
    match request.dialect() {
        WireDialect::GeminiGenerateContentV1 => {
            if request.options().continuation().is_some()
                || request.options().persistence().store()
            {
                return Err(invalid(
                    "Generate Content is stateless and has no response continuation",
                ));
            }
            if matches!(request.options().cache(), CachePolicy::Ephemeral { .. }) {
                return Err(invalid(
                    "Generate Content can reuse cachedContent but cannot create cache resources",
                ));
            }
        }
        WireDialect::GeminiInteractionsV1 => {
            let continuation = request.options().continuation();
            if let Some(continuation) = continuation
                && (!request.options().persistence().store()
                    || continuation.event_id().is_some()
                    || continuation.sequence().is_some())
            {
                return Err(invalid(
                    "Google Interactions continuation requires storage and permits no exact cursor",
                ));
            }
            let has_input = request
                .messages()
                .iter()
                .any(|message| !matches!(message.role(), Role::System | Role::Developer));
            if !has_input && continuation.is_none() {
                return Err(invalid(
                    "Google Interactions requires non-system input or continuation",
                ));
            }
            if matches!(
                request.options().cache(),
                CachePolicy::Explicit(_) | CachePolicy::Ephemeral { .. }
            ) {
                return Err(invalid(
                    "Google Interactions exposes implicit/state caching, not cachedContent resources",
                ));
            }
        }
        _ => return Err(invalid("request selected a non-Google wire dialect")),
    }
    let _ = ToolControls::new(request)?;
    let _ = config.operation_endpoint(request.dialect(), model)?;
    Ok(())
}

fn valid_function_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'.' | b'-')
        })
}

pub const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::invalid_request("google_request", detail)
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
