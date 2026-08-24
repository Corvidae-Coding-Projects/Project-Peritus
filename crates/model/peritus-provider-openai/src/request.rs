//! Responses request planning, validation, and HTTP construction.

mod input;
mod options;
mod value;

use peritus_model_protocol::{Capability, ModelRequest, ResponseId};
use peritus_provider_core::{
    Credential, Endpoint, Header, HeaderName, HttpHeaders, HttpMethod, HttpRequest,
    ProviderCoreError,
};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;

use crate::{config::OpenAiConfig, error};

pub enum RequestPlan {
    Create,
    Resume { response_id: ResponseId, sequence: u64 },
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "these booleans are independent fields in the fixed Responses wire contract"
)]
struct WireRequest<'a> {
    model: &'a str,
    input: Vec<Value>,
    stream: bool,
    store: bool,
    background: bool,
    max_output_tokens: u64,
    previous_response_id: Option<&'a str>,
    reasoning_includes: Vec<&'static str>,
    tools: Vec<Value>,
    tool_choice: Option<Value>,
    parallel_tool_calls: bool,
    text: Value,
    reasoning: Option<Value>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    prompt_cache_key: Option<&'a str>,
    prompt_cache_options: Option<Value>,
}

impl Serialize for WireRequest<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("model", self.model)?;
        map.serialize_entry("input", &self.input)?;
        map.serialize_entry("stream", &self.stream)?;
        map.serialize_entry("store", &self.store)?;
        map.serialize_entry("background", &self.background)?;
        map.serialize_entry("max_output_tokens", &self.max_output_tokens)?;
        if let Some(value) = self.previous_response_id {
            map.serialize_entry("previous_response_id", value)?;
        }
        if !self.reasoning_includes.is_empty() {
            map.serialize_entry("include", &self.reasoning_includes)?;
        }
        if !self.tools.is_empty() {
            map.serialize_entry("tools", &self.tools)?;
        }
        if let Some(value) = &self.tool_choice {
            map.serialize_entry("tool_choice", value)?;
        }
        map.serialize_entry("parallel_tool_calls", &self.parallel_tool_calls)?;
        map.serialize_entry("text", &self.text)?;
        if let Some(value) = &self.reasoning {
            map.serialize_entry("reasoning", value)?;
        }
        if let Some(value) = self.temperature {
            map.serialize_entry("temperature", &value)?;
        }
        if let Some(value) = self.top_p {
            map.serialize_entry("top_p", &value)?;
        }
        if let Some(value) = self.prompt_cache_key {
            map.serialize_entry("prompt_cache_key", value)?;
        }
        if let Some(value) = &self.prompt_cache_options {
            map.serialize_entry("prompt_cache_options", value)?;
        }
        map.end()
    }
}

pub fn plan(request: &ModelRequest) -> Result<RequestPlan, ProviderCoreError> {
    validate_common(request)?;
    match request.options().continuation() {
        Some(continuation) if continuation.sequence().is_some() => {
            if !request.options().persistence().background() {
                return Err(error::invalid(
                    "exact cursor continuation requires background persistence",
                ));
            }
            Ok(RequestPlan::Resume {
                response_id: continuation.response_id().clone(),
                sequence: continuation.sequence().unwrap_or(0),
            })
        }
        _ => Ok(RequestPlan::Create),
    }
}

fn validate_common(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(error::invalid(
            "OpenAI Responses streaming was not negotiated for this request",
        ));
    }
    if !request.request_id().expose_for_wire().is_ascii() {
        return Err(error::invalid("OpenAI client request identity must be ASCII"));
    }
    let generation = request.options().generation();
    if generation.seed().is_some() || !generation.stop_sequences().is_empty() {
        return Err(error::invalid(
            "OpenAI Responses does not document seed or stop-sequence request fields",
        ));
    }
    input::validate(request)?;
    options::validate(request)
}

pub fn http_request(
    config: &OpenAiConfig,
    request: &ModelRequest,
    plan: &RequestPlan,
    credential: Credential,
) -> Result<HttpRequest, ProviderCoreError> {
    let (method, endpoint, body) = match plan {
        RequestPlan::Create => {
            (HttpMethod::Post, config.endpoint().with_path("/v1/responses")?, encode(request)?)
        }
        RequestPlan::Resume { response_id, sequence } => (
            HttpMethod::Get,
            resume_endpoint(config.endpoint(), response_id, *sequence)?,
            Vec::new(),
        ),
    };
    let headers = headers(config, request, credential)?;
    HttpRequest::new(method, endpoint, headers, body, config.http_limits())
}

pub fn encode(request: &ModelRequest) -> Result<Vec<u8>, ProviderCoreError> {
    validate_common(request)?;
    let projected = WireRequest {
        model: request.model().as_str(),
        input: input::messages(request)?,
        stream: true,
        store: request.options().persistence().store(),
        background: request.options().persistence().background(),
        max_output_tokens: request.options().generation().max_output_tokens(),
        previous_response_id: request
            .options()
            .continuation()
            .filter(|continuation| continuation.sequence().is_none())
            .map(|continuation| continuation.response_id().expose_for_wire()),
        reasoning_includes: input::reasoning_includes(request),
        tools: options::tools(request)?,
        tool_choice: options::tool_choice(request),
        parallel_tool_calls: options::parallel(request),
        text: options::text(request)?,
        reasoning: options::reasoning(request),
        temperature: request
            .options()
            .generation()
            .temperature_millionths()
            .map(|value| f64::from(value) / 1_000_000.0),
        top_p: request
            .options()
            .generation()
            .top_p_millionths()
            .map(|value| f64::from(value) / 1_000_000.0),
        prompt_cache_key: options::cache_key(request),
        prompt_cache_options: options::cache_options(request),
    };
    serde_json::to_vec(&projected)
        .map_err(|_| error::invalid("OpenAI request serialization failed"))
}

fn headers(
    config: &OpenAiConfig,
    request: &ModelRequest,
    credential: Credential,
) -> Result<HttpHeaders, ProviderCoreError> {
    let mut headers = vec![
        credential.into_header(HeaderName::new("authorization".to_owned())?, Some("Bearer "))?,
        Header::new(HeaderName::new("content-type".to_owned())?, b"application/json".to_vec())?,
        Header::new(HeaderName::new("accept".to_owned())?, b"text/event-stream".to_vec())?,
        Header::new(
            HeaderName::new("x-client-request-id".to_owned())?,
            request.request_id().expose_for_wire().as_bytes().to_vec(),
        )?,
    ];
    if let Some(value) = config.organization() {
        headers
            .push(Header::new(HeaderName::new("openai-organization".to_owned())?, value.to_vec())?);
    }
    if let Some(value) = config.project() {
        headers.push(Header::new(HeaderName::new("openai-project".to_owned())?, value.to_vec())?);
    }
    HttpHeaders::new(headers, config.http_limits())
}

fn resume_endpoint(
    base: &Endpoint,
    response_id: &ResponseId,
    sequence: u64,
) -> Result<Endpoint, ProviderCoreError> {
    let identity = response_id.expose_for_wire();
    if !identity.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
        return Err(error::invalid("OpenAI response identity is unsafe in a retrieval path"));
    }
    Endpoint::new(format!(
        "{}/v1/responses/{identity}?stream=true&starting_after={sequence}",
        base.as_str().trim_end_matches('/')
    ))
}
