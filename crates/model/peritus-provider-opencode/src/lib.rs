//! Unmodified `OpenCode` CLI as a tool-disabled Zen/Go inference transport.
//!
//! Peritus owns credentials, conversation replay, and host tools. Each invocation uses a private
//! disposable configuration; `OpenCode` owns provider SDK selection and gateway request headers.

mod catalog;
mod dsml;
mod executable;
mod isolation;
mod output;
mod provider;
mod recovery;
mod request;
mod stream;

pub use catalog::discover_models;
pub use executable::OpenCodeExecutable;
pub use provider::OpenCodeRuntimeProvider;
pub use request::OpenCodeRequestLayout;

use peritus_provider_core::{ProviderCoreError, hosted::HostedService};

const fn service_id(service: HostedService) -> Result<&'static str, ProviderCoreError> {
    match service {
        HostedService::OpenCodeZen => Ok("opencode"),
        HostedService::OpenCodeGo => Ok("opencode-go"),
        _ => Err(ProviderCoreError::configuration(
            "opencode_runtime",
            "OpenCode transport accepts only Zen or Go",
        )),
    }
}

const fn malformed() -> ProviderCoreError {
    ProviderCoreError::malformed_stream(
        "opencode_runtime_output",
        "OpenCode did not return a complete inert Peritus turn",
    )
}
