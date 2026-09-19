//! Actionable JSON failures, shared by the local HTTP boundary.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub struct Error(pub(crate) String, pub(crate) bool);
pub type Result<T> = std::result::Result<T, Error>;
pub fn problem(value: impl std::fmt::Display) -> Error {
    Error(value.to_string(), false)
}
pub fn uncertain(value: impl std::fmt::Display) -> Error {
    Error(value.to_string(), true)
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        problem(value)
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        problem(value)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(json!({"error": self.0,"uncertain":self.1}))).into_response()
    }
}
