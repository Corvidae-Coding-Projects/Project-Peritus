//! Owned, deterministic loopback HTTP fixtures.

mod error;
mod model;
mod observation;
mod sequence;
mod server;
mod validation;
mod wire;

pub use error::{FakeHttpError, FakeHttpErrorKind};
pub use model::{
    ExpectedHttpRequest, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader, FakeHttpLimits,
    FakeHttpReleasePoint, HeaderMatchMode, ScriptedHttpResponse,
};
pub use observation::{
    CapturedHttpHeader, CapturedHttpRequest, FakeHttpExchange, FakeHttpTermination,
};
pub use sequence::FakeHttpSequenceServer;
pub use server::FakeHttpServer;
