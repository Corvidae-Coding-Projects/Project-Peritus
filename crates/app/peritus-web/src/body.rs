//! Authenticated request collection without a cumulative work allowance.

use crate::error::{Error, Result, problem};
use axum::{extract::{FromRequest, Request}, http::header};
use futures_util::StreamExt;
use serde_json::Value;

pub struct Input(pub Vec<u8>);

impl<S: Send + Sync> FromRequest<S> for Input {
    type Rejection = Error;

    async fn from_request(request: Request, _state: &S) -> Result<Self> {
        let mut stream = request.into_body().into_data_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(problem)?;
            bytes.try_reserve(chunk.len()).map_err(problem)?;
            bytes.extend_from_slice(&chunk);
        }
        Ok(Self(bytes))
    }
}

pub struct JsonInput(pub Value);

impl<S: Send + Sync> FromRequest<S> for JsonInput {
    type Rejection = Error;

    async fn from_request(request: Request, state: &S) -> Result<Self> {
        let media = request.headers().get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .and_then(|value| value.trim().split_once('/'));
        if !media.is_some_and(|(kind, subtype)| {
            kind.eq_ignore_ascii_case("application") &&
                (subtype.eq_ignore_ascii_case("json") || subtype.to_ascii_lowercase().ends_with("+json"))
        }) {
            return Err(problem("Use an application/json request body"));
        }
        let Input(bytes) = Input::from_request(request, state).await?;
        Ok(Self(serde_json::from_slice(&bytes)?))
    }
}
