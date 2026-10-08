//! Stable contracts for isolated Peritus plugins.
//!
//! The SDK contains data and framing only. It has no daemon handles, policy evaluator, filesystem
//! access, process launcher, or authority-minting API.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

mod canonical;
mod error;
mod framing;
mod identity;
mod manifest;
mod payload;
mod protocol;

pub use error::{SdkError, SdkErrorKind};
pub use framing::{PluginFrame, decode_frame, encode_frame};
pub use identity::{ManifestDigest, PluginId, PluginVersion, RequestId};
pub use manifest::{
    CapabilityDeclaration, CumulativeQuota, LEGACY_MANIFEST_VERSION, MANIFEST_VERSION,
    PluginEntrypoint, PluginKind, PluginManifest, PluginOperation, PluginQuotas, ProtocolRange,
    SignatureDeclaration, TrustMaterial,
};
pub use payload::{JsonBounds, JsonPayload, JsonStructure, JsonWirePolicy};
pub use protocol::{
    FailureClass, HostRequest, InvocationContext, LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION,
    PluginFailure, PluginRequestEnvelope, PluginResponse, PluginResponseEnvelope, PluginRole,
    PluginStatus,
};
