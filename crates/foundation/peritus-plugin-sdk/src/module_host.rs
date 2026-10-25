//! Versioned native-module host-service contracts.
//!
//! These data types describe requests and observations; possessing one never grants authority.
//! The host rechecks current user authority, package identity, scope, and lifecycle ownership before
//! performing an effect.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the module-host profile and service-envelope contract.
pub const MODULE_HOST_CONTRACT_VERSION: u16 = 1;

/// Capability discovery through [`ModuleHostRequest`].
pub const HOST_CAPABILITY_PROFILE: &str = "module.host-profile";
/// Private per-run state directory supplied to native module calls.
pub const HOST_CAPABILITY_PRIVATE_STATE: &str = "module.private-state";
/// Durable continuing module checkpoints.
pub const HOST_CAPABILITY_CHECKPOINT: &str = "module.checkpoint";
/// Host-owned provider invocation requested by a module.
pub const HOST_CAPABILITY_MODEL: &str = "module.model";
/// Explicit private artifact attachment.
pub const HOST_CAPABILITY_ATTACHMENT: &str = "module.attachment";
/// Explicit, revocable source selection and immutable source handles.
pub const HOST_CAPABILITY_SCOPED_SOURCE: &str = "module.scoped-source";
/// Pinned source discovery, local compilation, and inert installation.
pub const HOST_CAPABILITY_SOURCE_BUILD: &str = "module.source-build";
/// Module-owned worker execution in an isolated copy of explicitly scoped source.
pub const HOST_CAPABILITY_WORKER: &str = "module.worker";
/// Host-owned disposable application laboratory lifecycle and observations.
pub const HOST_CAPABILITY_APPLICATION_LABORATORY: &str = "module.application-laboratory";

/// Generic managed browser/Python runtime available to an isolated module worker.
///
/// This grants no browser operation or target authority. The module owns its driver and request
/// semantics; the host only provisions and mounts the credential-free runtime files.
pub const MODULE_WORKER_FEATURE_BROWSER_AUTOMATION: &str = "browser-automation-v1";

/// Locked Cargo registry dependencies prepared for an isolated module worker.
///
/// The host reads the disposable source copy's `Cargo.lock`, downloads checksum-pinned crates
/// during preparation, and exposes only that private vendored set to the network-denied worker.
/// The module still owns every command, target recipe, and interpretation of its output.
pub const MODULE_WORKER_FEATURE_CARGO_DEPENDENCIES: &str = "cargo-dependencies-v1";

/// One versioned host service currently available to a native module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleHostCapability {
    /// Stable lowercase capability name.
    pub name: String,
    /// Highest contract version supported for this capability.
    pub version: u16,
}

/// Additive profile supplied by an updated host on every module call.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleHostProfile {
    /// Profile schema; currently [`MODULE_HOST_CONTRACT_VERSION`].
    pub schema: u16,
    /// Available services. Absence means unsupported, not temporarily failed.
    pub capabilities: Vec<ModuleHostCapability>,
}

impl ModuleHostProfile {
    /// Returns whether the profile supports at least the requested service version.
    #[must_use]
    pub fn supports(&self, name: &str, version: u16) -> bool {
        self.schema == MODULE_HOST_CONTRACT_VERSION
            && self
                .capabilities
                .iter()
                .any(|capability| capability.name == name && capability.version >= version)
    }
}

/// One coordinator-to-host request carried by `ModuleAction::HostService`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleHostRequest {
    /// Durable module-owned request key.
    pub key: String,
    /// Capability named in the host profile.
    pub capability: String,
    /// Exact service contract version requested.
    pub version: u16,
    /// Capability-specific operation name.
    pub operation: String,
    /// Capability-specific structured input; never authority by itself.
    pub input: Value,
}

impl ModuleHostRequest {
    /// Validates the generic envelope before capability-specific input is inspected.
    ///
    /// # Errors
    ///
    /// Returns a stable detail when the request key, capability, version, or operation is not a
    /// valid host-service identity.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !key(&self.key) {
            return Err("invalid module host request key");
        }
        if !name(&self.capability, true) || self.version == 0 {
            return Err("invalid module host capability or version");
        }
        if !name(&self.operation, false) {
            return Err("invalid module host operation");
        }
        Ok(())
    }
}

/// Host observation returned through the coordinator's `host_result` method.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleHostResult {
    /// Exact request key being answered.
    pub key: String,
    /// Exact requested capability.
    pub capability: String,
    /// Exact requested service version.
    pub version: u16,
    /// Typed observed outcome.
    pub outcome: ModuleHostOutcome,
}

/// Result of one host-service request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModuleHostOutcome {
    /// The host completed the operation.
    Success {
        /// Capability-specific result.
        output: Value,
    },
    /// The capability, version, or operation is unavailable.
    Unsupported {
        /// Actionable compatibility detail.
        detail: String,
    },
    /// Current authority or input validation rejected the operation before its effect.
    Rejected {
        /// Stable rejection code.
        code: String,
        /// Actionable rejection detail.
        detail: String,
    },
    /// Host infrastructure attempted but did not complete the operation.
    Failed {
        /// Stable failure code.
        code: String,
        /// Causal failure detail.
        detail: String,
        /// Whether a fresh authorized action may retry.
        retryable_with_new_action: bool,
    },
}

/// Version-one request to let the user explicitly select application source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceRequest {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Human-readable reason displayed during selection.
    pub purpose: String,
    /// Whether an immutable content snapshot is required.
    pub immutable_snapshot: bool,
    /// Explicit source-relative paths omitted from the capture.
    pub exclusions: Vec<String>,
    /// Prior authority to refresh. A successful refresh revokes this exact revision.
    pub refresh: Option<ScopedSourceAuthority>,
}

/// Exact opaque source authority supplied back to the host for revalidation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceAuthority {
    /// Opaque host-issued source handle.
    pub handle: String,
    /// Exact authority revision issued with the handle.
    pub scope_revision: String,
}

/// Opaque, revocable source identity returned by the host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceHandle {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Opaque host handle; it is not a filesystem path.
    pub handle: String,
    /// Authority revision against which access is rechecked.
    pub scope_revision: String,
    /// Digest of captured source bytes and exclusions.
    pub snapshot_digest: String,
    /// Safe host-selected display label.
    pub display_name: String,
    /// Content-addressed identity of the complete original-project byte manifest.
    pub manifest_artifact: String,
    /// Number of included regular files.
    pub file_count: u64,
    /// Number of included source bytes.
    pub total_bytes: u64,
    /// Included files whose verified artifact was reused without rereading source bytes.
    pub reused_files: u64,
    /// Included bytes reused without rereading source bytes.
    pub reused_bytes: u64,
    /// Paths whose content identity differs from the refreshed snapshot.
    pub changed_paths: Vec<String>,
    /// Paths present in the refreshed snapshot's predecessor but no longer included.
    pub removed_paths: Vec<String>,
    /// Effective source-relative exclusions bound into the snapshot identity.
    pub exclusions: Vec<String>,
}

/// Request for one bounded page of an immutable source manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceManifestRequest {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Exact source authority.
    pub authority: ScopedSourceAuthority,
    /// Return entries lexically after this path, or from the beginning when absent.
    pub after: Option<String>,
    /// Requested entry count; the host applies its own upper bound.
    pub maximum_entries: u16,
}

/// One immutable regular-file entry in an original-project byte manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceEntry {
    /// Canonical source-relative path.
    pub path: String,
    /// SHA-256 of the complete file bytes.
    pub digest: String,
    /// Exact byte length.
    pub bytes: u64,
    /// Whether any executable mode bit was observed on supported Unix hosts.
    pub executable: bool,
}

/// Bounded manifest observation returned by the scoped-source broker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceManifestPage {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Snapshot identity whose entries were returned.
    pub snapshot_digest: String,
    /// Complete manifest artifact identity.
    pub manifest_artifact: String,
    /// Entries in canonical path order.
    pub entries: Vec<ScopedSourceEntry>,
    /// Last returned path when another page exists.
    pub next_after: Option<String>,
    /// Effective exclusions bound into the snapshot.
    pub exclusions: Vec<String>,
}

/// Request for one bounded byte range from an immutable source snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceReadRequest {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Exact source authority.
    pub authority: ScopedSourceAuthority,
    /// Canonical source-relative regular-file path.
    pub path: String,
    /// Zero-based starting byte offset.
    pub offset: u64,
    /// Maximum bytes to return.
    pub maximum_bytes: u64,
}

/// Exact retained observation of one immutable source byte range.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceReadResult {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Snapshot identity used for the read.
    pub snapshot_digest: String,
    /// Canonical source-relative path.
    pub path: String,
    /// Complete source-file digest.
    pub source_digest: String,
    /// Complete source-file byte length.
    pub source_bytes: u64,
    /// Inclusive selected start offset.
    pub range_start: u64,
    /// Exclusive selected end offset.
    pub range_end: u64,
    /// Content-addressed artifact identity for exactly the returned bytes.
    pub range_artifact: String,
    /// Exactly the selected bytes, represented by the enclosing JSON codec.
    pub bytes: Vec<u8>,
    /// Whether an identical durable range receipt already existed.
    pub reused: bool,
}

/// Request to revoke one exact source authority revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceRevokeRequest {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Exact source authority to revoke.
    pub authority: ScopedSourceAuthority,
}

/// Durable revocation observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceRevocation {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Revoked opaque handle.
    pub handle: String,
    /// Revoked scope revision.
    pub scope_revision: String,
    /// Whether this request changed retained authority state.
    pub newly_revoked: bool,
}

/// Request for an exact byte comparison between a retained snapshot and the selected workspace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceVerifyRequest {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Exact retained authority whose manifest is the comparison baseline.
    pub authority: ScopedSourceAuthority,
}

/// Host-observed before/after proof for the selected original workspace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceVerification {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Retained source snapshot used as the baseline.
    pub snapshot_digest: String,
    /// Content identity of the newly observed path/digest/mode manifest.
    pub observed_manifest_digest: String,
    /// Whether every included original-project byte and executable bit is unchanged.
    pub unchanged: bool,
    /// Included paths whose bytes or executable bit changed.
    pub changed_paths: Vec<String>,
    /// Newly present, non-excluded regular-file paths.
    pub added_paths: Vec<String>,
    /// Baseline paths no longer present as regular files.
    pub removed_paths: Vec<String>,
}

/// Pinned source identity for one locally built market package.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePackageRequest {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Stable module identifier.
    pub module_id: String,
    /// Immutable source commit.
    pub source_commit: String,
    /// Digest of the selected source tree.
    pub source_digest: String,
    /// Safe repository-relative module path.
    pub module_path: String,
    /// Digest of the exact dependency lockfile.
    pub lockfile_digest: String,
    /// Pinned toolchain identity.
    pub toolchain: String,
    /// Requested compilation target.
    pub target: String,
}

/// Distinct source, recipe, executable, and package identities from a local source build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalPackageIdentity {
    /// Contract schema, currently one.
    pub schema: u16,
    /// Stable module identifier.
    pub module_id: String,
    /// Digest of the source inputs.
    pub source_digest: String,
    /// Digest of the validated build recipe.
    pub recipe_digest: String,
    /// Actual compilation target.
    pub target: String,
    /// Digest of the produced executable.
    pub executable_digest: String,
    /// Digest of the installed local package.
    pub package_digest: String,
}

/// Host platform identity for a portable module service.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleHostPlatform {
    /// Rust-style operating-system name such as `linux`.
    pub os: String,
    /// Rust-style architecture name such as `x86_64`.
    pub architecture: String,
}

/// Resource ceilings requested for one module-owned worker; the host may only narrow them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleWorkerLimits {
    /// Maximum elapsed execution time in milliseconds.
    pub wall_millis: u64,
    /// Maximum aggregate memory in bytes.
    pub memory_bytes: u64,
    /// Maximum disposable storage in bytes.
    pub storage_bytes: u64,
    /// Maximum owned process count.
    pub processes: u32,
    /// Maximum simultaneously open file descriptors across each worker process.
    pub open_handles: u32,
}

/// Generic request to execute the selected module's own worker against disposable source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleWorkerRequest {
    /// Request schema, currently one.
    pub schema: u16,
    /// Exact source authority selected by the user.
    pub source: ScopedSourceAuthority,
    /// Module-defined worker name; the host does not interpret its meaning.
    pub worker: String,
    /// Opaque module-owned JSON input serialized to the worker's standard input.
    pub input: Value,
    /// Optional named host runtime features. Names are generic execution resources rather than
    /// module, adapter, or vulnerability identities. Unknown features are rejected.
    #[serde(default)]
    pub runtime_features: Vec<String>,
    /// Requested ceilings narrowed by the host.
    pub limits: ModuleWorkerLimits,
}

/// Raw host observation from one isolated module-owned worker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleWorkerResult {
    /// Result schema, currently one.
    pub schema: u16,
    /// Exact module-defined worker name.
    pub worker: String,
    /// Stable terminal disposition observed by the host process owner.
    pub terminal_disposition: String,
    /// Numeric exit code when the platform supplied one.
    pub exit_code: Option<i32>,
    /// Bounded raw standard output.
    pub stdout: Vec<u8>,
    /// Bounded raw standard error.
    pub stderr: Vec<u8>,
    /// Digest of the checked execution plan.
    pub execution_plan_digest: String,
    /// Whether the complete owned process tree and support tasks stopped.
    pub cleanup_complete: bool,
    /// Whether the selected original source still matches its retained snapshot.
    pub original_source_unchanged: bool,
}

/// One exact application shape supported by the host-owned laboratory controller.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryRecipe {
    /// Recipe schema, currently one.
    pub schema: u16,
    /// Stable recipe identity interpreted by the host.
    pub recipe_id: String,
    /// Exact recipe version.
    pub version: String,
    /// Supported host platform.
    pub platform: ModuleHostPlatform,
    /// Required source roles, in canonical order.
    pub source_roles: Vec<String>,
    /// Host-owned isolation backend required by this recipe.
    pub isolation_backend: String,
    /// Human-readable interface shape exposed through opaque handles.
    pub interface: String,
}

/// Host projection of currently supported laboratory recipes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryCatalog {
    /// Catalog schema, currently one.
    pub schema: u16,
    /// Recipes available on this exact host platform.
    pub recipes: Vec<ModuleLaboratoryRecipe>,
}

/// Request to create a laboratory from the complete immutable source snapshot.
///
/// Unlike an exact compatibility recipe, this request does not require the target to implement a
/// Peritus protocol or place code in specially named files. The host copies the complete admitted
/// tree and exposes only generic, sandboxed process operations over it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratorySourceCreateRequest {
    /// Request schema, currently one.
    pub schema: u16,
    /// Exact immutable source authority selected by the user.
    pub source: ScopedSourceAuthority,
}

/// One generic process invocation against a complete disposable source copy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryProcessInput {
    /// Input schema, currently one.
    pub schema: u16,
    /// Host-resolved runtime family, for example `direct`, `python3`, `node`, or `cargo`.
    pub runtime: String,
    /// Canonical source-relative entry point or project manifest.
    pub entrypoint: String,
    /// Literal argument vector. No shell parsing or expansion is performed.
    pub arguments: Vec<String>,
    /// Bounded standard-input bytes supplied to the process.
    pub stdin: Vec<u8>,
    /// Credential-free literal environment entries selected for the copied target.
    pub environment: std::collections::BTreeMap<String, String>,
    /// Per-invocation wall-clock ceiling in milliseconds.
    pub wall_millis: u64,
    /// Exit codes accepted by the caller's predeclared oracle.
    pub expected_exit_codes: Vec<i32>,
    /// Literal stdout fragments required by the caller's predeclared oracle.
    pub stdout_contains: Vec<String>,
    /// Literal stderr fragments required by the caller's predeclared oracle.
    pub stderr_contains: Vec<String>,
}

/// Opaque identities returned for one laboratory. No resolved path, address, or credential appears.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryHandle {
    /// Handle schema, currently one.
    pub schema: u16,
    /// Content-bound laboratory identity.
    pub laboratory_id: String,
    /// Exact source revision.
    pub scope_revision: String,
    /// Immutable source snapshot.
    pub source_snapshot: String,
    /// Exact host recipe.
    pub recipe_id: String,
    /// Opaque application service identity.
    pub application_service: String,
    /// Opaque backing service identity.
    pub backing_service: String,
    /// Opaque endpoint resolved only by the host.
    pub endpoint_handle: String,
    /// Opaque test identity; this is not credential material.
    pub identity_handle: String,
    /// Opaque secret lease identity; this is not secret material.
    pub secret_handle: String,
}

/// Durable lifecycle phase for one disposable application laboratory.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleLaboratoryPhase {
    /// Identity exists but no source bytes have been materialized.
    Created,
    /// Exact copied source and private support data are prepared.
    Prepared,
    /// The copied application and backing service passed real readiness checks.
    Ready,
    /// The owned process tree stopped and retained state remains available.
    Stopped,
    /// The controller observed a recoverable or terminal failure.
    Failed,
    /// Cleanup, identity, or secret-output integrity could not be established.
    Quarantined,
    /// Disposable bytes were removed while identities and receipts remain readable.
    TornDown,
}

/// Bounded resource and activity facts authored by the laboratory controller.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryTelemetry {
    /// Requests admitted through the opaque endpoint.
    pub requests: u64,
    /// Bytes written through the host control channel.
    pub input_bytes: u64,
    /// Bytes admitted from target responses after secret filtering.
    pub output_bytes: u64,
    /// Latest host-observed copied-store digest.
    pub state_digest: String,
    /// Whether the host-loopback escape canary remained untouched.
    pub host_loopback_blocked: bool,
    /// Whether no credential value was admitted to a module-visible result.
    pub credential_output_blocked: bool,
}

/// Host-authored receipt for a laboratory lifecycle or application operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryReceipt {
    /// Receipt schema, currently one.
    pub schema: u16,
    /// Exact laboratory identity.
    pub laboratory_id: String,
    /// Stable operation name.
    pub operation: String,
    /// Digest of the exact admitted request.
    pub request_digest: String,
    /// Exact immutable source snapshot.
    pub source_snapshot: String,
    /// Digest of the copied application entry point.
    pub application_digest: String,
    /// Digest of the copied backing service entry point.
    pub service_digest: String,
    /// Host-owned isolation backend identity.
    pub isolation_backend: String,
    /// Network and service-routing policy identity.
    pub network_policy: String,
    /// Typed observation with secrets and resolved host paths excluded.
    pub observation: Value,
    /// Telemetry observed at this operation boundary.
    pub telemetry: ModuleLaboratoryTelemetry,
    /// Whether the complete owned target process tree was observed stopped.
    pub process_cleanup_complete: bool,
    /// Whether the selected original source still matches its retained snapshot.
    pub original_source_unchanged: Option<bool>,
}

/// Current durable laboratory status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryStatus {
    /// Status schema, currently one.
    pub schema: u16,
    /// Opaque handle set.
    pub handle: ModuleLaboratoryHandle,
    /// Current lifecycle phase.
    pub phase: ModuleLaboratoryPhase,
    /// Latest controller telemetry.
    pub telemetry: ModuleLaboratoryTelemetry,
    /// Number of retained snapshots.
    pub snapshots: u64,
    /// Number of retained host-authored receipts.
    pub receipts: u64,
    /// Actionable lifecycle detail.
    pub detail: String,
    /// Retained causal failure, if one exists.
    pub retained_error: Option<String>,
    /// Whether disposable source, state, process, socket, and secret bytes were removed.
    pub cleanup_complete: bool,
}

/// Combined current status and optional new receipt returned by a laboratory operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryResult {
    /// Result schema, currently one.
    pub schema: u16,
    /// Current durable lifecycle state.
    pub status: ModuleLaboratoryStatus,
    /// New host-authored operation receipt, absent for a pure status read.
    pub receipt: Option<ModuleLaboratoryReceipt>,
}

/// Request for one exact laboratory lifecycle operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryLifecycleRequest {
    /// Request schema, currently one.
    pub schema: u16,
    /// Exact opaque laboratory identity.
    pub laboratory_id: String,
}

/// Request to operate one live application through its host-resolved endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLaboratoryInvokeRequest {
    /// Request schema, currently one.
    pub schema: u16,
    /// Exact opaque laboratory identity.
    pub laboratory_id: String,
    /// Exact opaque endpoint returned at creation.
    pub endpoint_handle: String,
    /// Registered operation, currently `state.round-trip.v1`.
    pub operation: String,
    /// Bounded application input.
    pub input: Value,
}

fn key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn name(value: &str, dot: bool) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'_'
                || (dot && byte == b'.')
        })
}
