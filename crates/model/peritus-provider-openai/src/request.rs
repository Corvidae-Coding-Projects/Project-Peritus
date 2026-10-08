//! Responses request planning, pure admission, and exact bounded HTTP construction.

mod input;
mod options;
mod wire;

use peritus_model_protocol::{Capability, ModelRequest, ResponseId};
use peritus_provider_core::{
    Credential, Endpoint, Header, HeaderName, HttpHeaders, HttpMethod, HttpRequest,
    ProviderCoreError,
};

use crate::{config::OpenAiConfig, error};

pub enum RequestPlan {
    Create,
    Resume { response_id: ResponseId, sequence: u64 },
}

pub(super) struct PreparedRequest {
    method: HttpMethod,
    endpoint: Endpoint,
    body: Vec<u8>,
}

impl PreparedRequest {
    pub(super) fn len(&self) -> usize {
        self.body.len()
    }
}

pub fn validate(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(error::invalid(
            "OpenAI Responses streaming was not negotiated for this request",
        ));
    }
    let request_id = request.request_id().expose_for_wire();
    if !request_id.is_ascii() || request_id.len() > 512 {
        return Err(error::invalid(
            "OpenAI client request identity must be at most 512 ASCII characters",
        ));
    }
    let generation = request.options().generation();
    if generation.max_output_tokens() < 16 {
        return Err(error::invalid("OpenAI Responses requires at least 16 output tokens"));
    }
    if generation.seed().is_some() || !generation.stop_sequences().is_empty() {
        return Err(error::invalid(
            "OpenAI Responses does not document seed or stop-sequence request fields",
        ));
    }
    input::validate(request)?;
    options::validate(request)
}

pub fn plan(request: &ModelRequest) -> Result<RequestPlan, ProviderCoreError> {
    validate(request)?;
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

pub(super) fn prepare(
    config: &OpenAiConfig,
    request: &ModelRequest,
    plan: &RequestPlan,
) -> Result<PreparedRequest, ProviderCoreError> {
    validate(request)?;
    match plan {
        RequestPlan::Create => {
            input::validate_resolved(request)?;
            Ok(PreparedRequest {
                method: HttpMethod::Post,
                endpoint: config.responses_endpoint()?,
                body: wire::encode(request, config.http_limits().max_request_body_bytes())?,
            })
        }
        RequestPlan::Resume { response_id, sequence } => Ok(PreparedRequest {
            method: HttpMethod::Get,
            endpoint: resume_endpoint(config.endpoint(), response_id, *sequence)?,
            body: Vec::new(),
        }),
    }
}

pub(super) fn prepared_http_request(
    config: &OpenAiConfig,
    request: &ModelRequest,
    prepared: &PreparedRequest,
    credential: Credential,
) -> Result<HttpRequest, ProviderCoreError> {
    let headers = headers(config, request, credential)?;
    HttpRequest::new(
        prepared.method,
        prepared.endpoint.clone(),
        headers,
        prepared.body.clone(),
        config.http_limits(),
    )
}

pub fn http_request(
    config: &OpenAiConfig,
    request: &ModelRequest,
    plan: &RequestPlan,
    credential: Credential,
) -> Result<HttpRequest, ProviderCoreError> {
    let prepared = prepare(config, request, plan)?;
    prepared_http_request(config, request, &prepared, credential)
}

pub fn encode(request: &ModelRequest) -> Result<Vec<u8>, ProviderCoreError> {
    validate(request)?;
    input::validate_resolved(request)?;
    wire::encode(request, usize::MAX)
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
