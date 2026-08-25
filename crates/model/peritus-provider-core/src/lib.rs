//! Provider-independent owned transport primitives for Peritus.
//!
//! This crate is the dependency-private effect shell shared by C5 provider adapters. It exposes
//! bounded Peritus-owned HTTP values, credential isolation, cancellation, streaming framing,
//! deterministic retry planning, and a Reqwest/Rustls transport without leaking implementation
//! dependencies through its public API.

mod adapter;
mod backoff;
mod cancellation;
mod continuation;
mod credential;
mod diagnostic;
mod endpoint;
mod error;
mod framing;
mod http;
mod process;
mod redaction;
mod reqwest_transport;
mod retry;
mod stream;
mod transport;

pub use adapter::{ModelProvider, ResponseCancellationOutcome, validate_request_profile};
pub use backoff::wait_for_backoff;
pub use cancellation::{CancellationFuture, CancellationToken};
pub use continuation::{ContinuationRestoreOutcome, PersistedContinuation};
pub use credential::{Credential, CredentialReference, CredentialSource};
pub use diagnostic::{Diagnostic, DiagnosticValue, TransportPhase};
pub use endpoint::Endpoint;
pub use error::{ProviderCoreError, ProviderCoreErrorKind};
pub use framing::{
    FramingLimits, NdjsonFrame, NdjsonParser, SseComment, SseFrame, SseItem, SseParser,
};
pub use http::{
    Header, HeaderName, HeaderValue, HttpHeaders, HttpLimits, HttpMethod, HttpRequest,
    HttpResponse, StatusCode,
};
pub use process::{
    EnvironmentName, ProcessExecutable, ProcessExit, ProcessLimits, ProcessOutput, ProcessRequest,
    ProcessTransport, TokioProcessTransport,
};
pub use redaction::RedactedValue;
pub use reqwest_transport::ReqwestTransport;
pub use retry::{
    RetryAction, RetryFailure, RetryObservation, RetryPlan, RetryPolicy, RetryProtection,
    SubmissionState,
};
pub use stream::{ModelStream, OwnedModelStream};
pub use transport::{BoxFuture, ByteStream, HttpTransport, MemoryByteStream};
