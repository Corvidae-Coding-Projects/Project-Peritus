//! Durable, fenced custody for plugin process and invocation evidence.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_plugin_sdk::{PluginId, PluginKind, PluginVersion, RequestId};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    DiscoveredPlugin, HostError, HostFailureClass, InvocationGrant, InvocationSubject,
    RecoveryDisposition,
};

const STATE_SCHEMA: u16 = 1;
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const EXECUTION_BUFFER_BYTES: usize = 64 * 1024;

/// Stable caller identity whose successive store openings fence older writers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateOwnerId(String);

impl StateOwnerId {
    /// Validates a stable state owner identity.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-containing identities.
    pub fn new(value: impl Into<String>) -> Result<Self, HostError> {
        let value = value.into();
        validate_identity(&value, "state owner identity")?;
        Ok(Self(value))
    }

    /// Borrows the stable owner identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Caller-generated identity for exactly one launched plugin process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginInstanceId(String);

impl PluginInstanceId {
    /// Validates an identity that must never be reused for another process.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-containing identities.
    pub fn new(value: impl Into<String>) -> Result<Self, HostError> {
        let value = value.into();
        validate_identity(&value, "plugin process instance identity")?;
        Ok(Self(value))
    }

    /// Borrows the stable process identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Durable process lifecycle frontier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginInstanceFrontier {
    /// The identity was reserved before process launch.
    Starting,
    /// Initialization completed for this exact process.
    Ready,
    /// Stop admission closed before shutdown was sent.
    Stopping,
    /// The exact process was terminated and reaped.
    Stopped,
    /// Launch, protocol, or cleanup failed.
    Failed,
}

/// Durable invocation transport/result frontier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationFrontier {
    /// Exact content and fresh authority were persisted before transport began.
    Prepared,
    /// A first write was entered, so a crash could have accepted bytes.
    Dispatching,
    /// At least one request byte was accepted by the process transport.
    Accepted,
    /// A correlated response was durably fingerprinted.
    Responded,
    /// Transport established that no request byte was accepted.
    Unsent,
    /// An external authority resolved an otherwise indeterminate effect.
    Reconciled,
}

/// External resolution for an invocation whose effect could not be established locally.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationResolution {
    /// The exact effect was independently confirmed as applied.
    ConfirmedApplied,
    /// The exact effect was independently confirmed as not applied.
    ConfirmedNotApplied,
}

/// Durable identity and lifecycle evidence for one exact plugin process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginInstanceEvidence {
    /// Stable state owner.
    pub owner_id: StateOwnerId,
    /// Owner generation that created this process record.
    pub owner_fence: u64,
    /// Caller-generated process identity.
    pub instance_id: PluginInstanceId,
    /// Plugin identity.
    pub plugin_id: PluginId,
    /// Exact plugin version.
    pub plugin_version: PluginVersion,
    /// Canonical manifest SHA-256.
    pub manifest_sha256: [u8; 32],
    /// Exact executable/module SHA-256.
    pub artifact_sha256: [u8; 32],
    /// Last durable lifecycle frontier.
    pub frontier: PluginInstanceFrontier,
}

/// Durable, secret-free evidence for one caller invocation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationEvidence {
    /// Stable state owner.
    pub owner_id: StateOwnerId,
    /// Owner generation that prepared this invocation.
    pub owner_fence: u64,
    /// Exact process identity to which dispatch was admitted.
    pub instance_id: PluginInstanceId,
    /// Plugin identity.
    pub plugin_id: PluginId,
    /// Exact plugin version.
    pub plugin_version: PluginVersion,
    /// Caller-generated invocation identity.
    pub request_id: RequestId,
    /// Exact declared resource/scope name.
    pub capability: String,
    /// Canonical manifest SHA-256.
    pub manifest_sha256: [u8; 32],
    /// Exact executable/module SHA-256.
    pub artifact_sha256: [u8; 32],
    /// SHA-256 of the exact canonical request frame; request content is not retained.
    pub command_sha256: [u8; 32],
    /// SHA-256 of the exact subject, generation, grant, and deadline projection.
    pub authority_sha256: [u8; 32],
    /// Last durable transport/result frontier.
    pub frontier: InvocationFrontier,
    /// SHA-256 of the correlated response frame, when one was established.
    pub response_sha256: Option<[u8; 32]>,
    /// Independent resolution, present only at the reconciled frontier.
    pub reconciliation: Option<ReconciliationResolution>,
}

/// Concrete filesystem-backed custody opened at a caller-selected state root.
///
/// One kernel-backed exclusive lease covers the complete state root for the lifetime of all
/// clones. The lease is released automatically after process death; its persistent lock file is
/// not itself treated as proof of ownership. After obtaining the lease, reopening the same owner
/// claims a higher durable fence. Process identities cannot be reopened because this protocol has
/// no process reconnection.
#[derive(Clone)]
pub struct HostStateStore {
    inner: Arc<StateInner>,
}

struct StateInner {
    _lease: File,
    root: PathBuf,
    owner_directory: PathBuf,
    instance_directory: PathBuf,
    invocation_directory: PathBuf,
    execution_directory: PathBuf,
    owner_id: StateOwnerId,
    fence: u64,
    mutation: StdMutex<()>,
    temporary_sequence: AtomicU64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiskInstance {
    schema: u16,
    owner_id: String,
    owner_fence: u64,
    instance_id: String,
    plugin_id: String,
    plugin_version: [u64; 3],
    manifest_sha256: String,
    artifact_sha256: String,
    frontier: PluginInstanceFrontier,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiskInvocation {
    schema: u16,
    owner_id: String,
    owner_fence: u64,
    instance_id: String,
    plugin_id: String,
    plugin_version: [u64; 3],
    request_id: String,
    capability: String,
    manifest_sha256: String,
    artifact_sha256: String,
    command_sha256: String,
    authority_sha256: String,
    frontier: InvocationFrontier,
    response_sha256: Option<String>,
    reconciliation: Option<ReconciliationResolution>,
}

pub(crate) struct InvocationClaim {
    store: HostStateStore,
    key: String,
}

/// Exact immutable artifact held in caller-rooted custody through process launch and lifetime.
#[derive(Clone, Debug)]
pub(crate) struct ExecutionArtifact {
    path: PathBuf,
    sha256: [u8; 32],
    _handle: Arc<File>,
}

impl ExecutionArtifact {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) const fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

impl HostStateStore {
    /// Exclusively leases concrete custody beneath `root` and durably claims the next owner fence.
    ///
    /// Existing evidence is retained. A crashed holder leaves the lock file but releases the
    /// kernel lease, so its successor can reopen the same root and query or reconcile retained
    /// evidence. The root and every managed child must be real directories, never symbolic links.
    ///
    /// # Errors
    ///
    /// Returns a typed infrastructure error for unsafe paths, unreadable state, malformed records,
    /// or a fence claim that cannot be made durable.
    pub fn open(root: impl AsRef<Path>, owner_id: StateOwnerId) -> Result<Self, HostError> {
        let configured_root = root.as_ref();
        reject_symlink(configured_root, "plugin host state root is a symbolic link")?;
        fs::create_dir_all(configured_root).map_err(|error| {
            state_source("create plugin host state root", RecoveryDisposition::CorrectRequest, error)
        })?;
        reject_symlink(configured_root, "plugin host state root is a symbolic link")?;
        let root = fs::canonicalize(configured_root).map_err(|error| {
            state_source(
                "canonicalize plugin host state root",
                RecoveryDisposition::CorrectRequest,
                error,
            )
        })?;
        let lease = claim_root_lease(&root)?;
        let owners = checked_directory(&root, "owners")?;
        let instance_directory = checked_directory(&root, "instances")?;
        let invocation_directory = checked_directory(&root, "invocations")?;
        let execution_directory = checked_directory(&root, "execution")?;
        let owner_key = digest_key(&[owner_id.as_str().as_bytes()]);
        let owner_directory = checked_directory(&owners, &owner_key)?;
        establish_owner_identity(&owner_directory, &owner_id)?;
        let fence = claim_fence(&owner_directory)?;
        sync_directory(&root)?;
        Ok(Self {
            inner: Arc::new(StateInner {
                _lease: lease,
                root,
                owner_directory,
                instance_directory,
                invocation_directory,
                execution_directory,
                owner_id,
                fence,
                mutation: StdMutex::new(()),
                temporary_sequence: AtomicU64::new(0),
            }),
        })
    }

    /// Returns the canonical caller-selected custody root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Returns the stable owner identity.
    #[must_use]
    pub fn owner_id(&self) -> &StateOwnerId {
        &self.inner.owner_id
    }

    /// Returns this handle's durable fencing generation.
    #[must_use]
    pub fn fence(&self) -> u64 {
        self.inner.fence
    }

    /// Returns every retained invocation record for this owner in deterministic order.
    ///
    /// # Errors
    ///
    /// Fails if custody cannot be read or contains malformed records.
    pub fn recorded_invocations(&self) -> Result<Vec<InvocationEvidence>, HostError> {
        let _mutation = self.lock_mutation()?;
        let mut evidence = self.read_owner_invocations()?;
        evidence.sort_unstable_by(|left, right| {
            (left.plugin_id.as_str(), left.request_id.as_str(), left.owner_fence).cmp(&(
                right.plugin_id.as_str(),
                right.request_id.as_str(),
                right.owner_fence,
            ))
        });
        Ok(evidence)
    }

    /// Returns invocations that may have been accepted but lack a correlated durable result.
    ///
    /// # Errors
    ///
    /// Fails if custody cannot be read or contains malformed records.
    pub fn unresolved_invocations(&self) -> Result<Vec<InvocationEvidence>, HostError> {
        Ok(self
            .recorded_invocations()?
            .into_iter()
            .filter(|record| {
                matches!(record.frontier, InvocationFrontier::Dispatching | InvocationFrontier::Accepted)
            })
            .collect())
    }

    /// Returns processes whose durable frontier says an attachment could still have existed.
    ///
    /// The host cannot reconnect to them. Callers must clean up or otherwise establish their
    /// disposition before launching a replacement for the same logical work.
    ///
    /// # Errors
    ///
    /// Fails if custody cannot be read or contains malformed records.
    pub fn unresolved_instances(&self) -> Result<Vec<PluginInstanceEvidence>, HostError> {
        let _mutation = self.lock_mutation()?;
        let mut evidence = Vec::new();
        for entry in read_regular_entries(&self.inner.instance_directory)? {
            let record: DiskInstance = read_json_record(&entry)?;
            if record.owner_id == self.inner.owner_id.as_str()
                && matches!(
                    record.frontier,
                    PluginInstanceFrontier::Starting
                        | PluginInstanceFrontier::Ready
                        | PluginInstanceFrontier::Stopping
                )
            {
                evidence.push(instance_evidence(record)?);
            }
        }
        evidence.sort_unstable_by(|left, right| {
            (left.plugin_id.as_str(), left.instance_id.as_str()).cmp(&(
                right.plugin_id.as_str(),
                right.instance_id.as_str(),
            ))
        });
        Ok(evidence)
    }

    /// Records an independently established disposition for exact unresolved evidence.
    ///
    /// # Errors
    ///
    /// Rejects stale or changed evidence, resolved/unsent work, or a fenced writer.
    pub fn reconcile(
        &self,
        evidence: &InvocationEvidence,
        resolution: ReconciliationResolution,
    ) -> Result<InvocationEvidence, HostError> {
        let _mutation = self.lock_mutation()?;
        self.ensure_current_fence()?;
        if evidence.owner_id != self.inner.owner_id {
            return Err(state_request_error("reconciliation evidence belongs to another owner"));
        }
        let key = invocation_key(
            evidence.owner_id.as_str(),
            evidence.plugin_id.as_str(),
            evidence.request_id.as_str(),
        );
        let path = self.inner.invocation_directory.join(format!("{key}.json"));
        let mut record: DiskInvocation = read_json_record(&path)?;
        if invocation_evidence(record.clone())? != *evidence {
            return Err(state_request_error(
                "reconciliation evidence no longer matches the durable invocation record",
            ));
        }
        if !matches!(record.frontier, InvocationFrontier::Dispatching | InvocationFrontier::Accepted)
        {
            return Err(state_request_error(
                "only an unresolved possibly accepted invocation can be reconciled",
            ));
        }
        record.frontier = InvocationFrontier::Reconciled;
        record.reconciliation = Some(resolution);
        self.write_json_record(&path, &record)?;
        invocation_evidence(record)
    }

    pub(crate) fn stage_execution_artifact(
        &self,
        plugin: &DiscoveredPlugin,
    ) -> Result<ExecutionArtifact, HostError> {
        let _mutation = self.lock_mutation()?;
        self.ensure_current_fence()?;
        let suffix = match plugin.manifest().kind() {
            PluginKind::Process => "process.exe",
            PluginKind::WasmComponent => "component.wasm",
        };
        let digest = plugin.artifact_sha256();
        let path = self
            .inner
            .execution_directory
            .join(format!("{}-{suffix}", hex_digest(digest)));
        reject_symlink(&path, "plugin execution artifact is a symbolic link")?;
        if path.try_exists().map_err(|error| {
            state_source(
                "inspect staged plugin artifact",
                RecoveryDisposition::Reconcile,
                error,
            )
        })? {
            return open_execution_artifact(&path, plugin);
        }

        let sequence = self
            .inner
            .temporary_sequence
            .fetch_add(1, Ordering::Relaxed);
        let temporary = self.inner.execution_directory.join(format!(
            ".artifact-{:020}-{sequence:020}.tmp",
            self.inner.fence
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                state_source(
                    "create staged plugin artifact",
                    RecoveryDisposition::Reconcile,
                    error,
                )
            })?;
        if let Err(error) = plugin.copy_verified_artifact(&mut file) {
            return Err(clean_execution_temporary(&temporary, file, error));
        }
        if let Err(error) = harden_execution_artifact(&file, plugin.manifest().kind()) {
            return Err(clean_execution_temporary(&temporary, file, error));
        }
        if let Err(error) = file.sync_all().map_err(|error| {
            state_source(
                "persist staged plugin artifact",
                RecoveryDisposition::Reconcile,
                error,
            )
        }) {
            return Err(clean_execution_temporary(&temporary, file, error));
        }
        if let Err(error) = fs::rename(&temporary, &path).map_err(|error| {
            state_source(
                "commit staged plugin artifact",
                RecoveryDisposition::Reconcile,
                error,
            )
        }) {
            return Err(clean_execution_temporary(&temporary, file, error));
        }
        sync_directory(&self.inner.execution_directory)?;
        Ok(ExecutionArtifact { path, sha256: digest, _handle: Arc::new(file) })
    }

    pub(crate) fn register_instance(
        &self,
        plugin: &DiscoveredPlugin,
        execution: &ExecutionArtifact,
        instance_id: &PluginInstanceId,
    ) -> Result<(), HostError> {
        let _mutation = self.lock_mutation()?;
        self.ensure_current_fence()?;
        if execution.sha256() != plugin.artifact_sha256() {
            return Err(HostError::new(
                HostFailureClass::Trust,
                RecoveryDisposition::EstablishTrust,
                "register plugin process instance",
                "staged execution artifact differs from its discovered identity",
            ));
        }
        let key = instance_key(self.inner.owner_id.as_str(), instance_id.as_str());
        let path = self.inner.instance_directory.join(format!("{key}.json"));
        if path.try_exists().map_err(|error| {
            state_source("inspect plugin instance identity", RecoveryDisposition::Reconcile, error)
        })? {
            return Err(HostError::new(
                HostFailureClass::Infrastructure,
                RecoveryDisposition::Reconcile,
                "register plugin process instance",
                "plugin process instance identity was already used; reconcile the retained process evidence and launch a newly identified process",
            ));
        }
        let version = plugin.manifest().version();
        let record = DiskInstance {
            schema: STATE_SCHEMA,
            owner_id: self.inner.owner_id.as_str().to_owned(),
            owner_fence: self.inner.fence,
            instance_id: instance_id.as_str().to_owned(),
            plugin_id: plugin.manifest().id().as_str().to_owned(),
            plugin_version: [version.major(), version.minor(), version.patch()],
            manifest_sha256: plugin.manifest_digest().to_hex(),
            artifact_sha256: hex_digest(execution.sha256()),
            frontier: PluginInstanceFrontier::Starting,
        };
        self.write_json_record(&path, &record)
    }

    pub(crate) fn transition_instance(
        &self,
        plugin_id: &PluginId,
        instance_id: &PluginInstanceId,
        frontier: PluginInstanceFrontier,
    ) -> Result<(), HostError> {
        let _mutation = self.lock_mutation()?;
        self.ensure_current_fence()?;
        let key = instance_key(self.inner.owner_id.as_str(), instance_id.as_str());
        let path = self.inner.instance_directory.join(format!("{key}.json"));
        let mut record: DiskInstance = read_json_record(&path)?;
        if record.schema != STATE_SCHEMA
            || record.owner_id != self.inner.owner_id.as_str()
            || record.owner_fence != self.inner.fence
            || record.instance_id != instance_id.as_str()
            || record.plugin_id != plugin_id.as_str()
        {
            return Err(state_fence_error(
                "plugin instance transition does not match current fenced ownership",
            ));
        }
        if !valid_instance_transition(record.frontier, frontier) {
            return Err(state_request_error("invalid plugin instance lifecycle transition"));
        }
        record.frontier = frontier;
        self.write_json_record(&path, &record)
    }

    pub(crate) fn prepare_invocation(
        &self,
        plugin: &DiscoveredPlugin,
        instance_id: &PluginInstanceId,
        request_id: &RequestId,
        capability: &str,
        command_sha256: [u8; 32],
        authority_sha256: [u8; 32],
    ) -> Result<InvocationClaim, HostError> {
        let _mutation = self.lock_mutation()?;
        self.ensure_current_fence()?;
        let key = invocation_key(
            self.inner.owner_id.as_str(),
            plugin.manifest().id().as_str(),
            request_id.as_str(),
        );
        let path = self.inner.invocation_directory.join(format!("{key}.json"));
        let version = plugin.manifest().version();
        let candidate = DiskInvocation {
            schema: STATE_SCHEMA,
            owner_id: self.inner.owner_id.as_str().to_owned(),
            owner_fence: self.inner.fence,
            instance_id: instance_id.as_str().to_owned(),
            plugin_id: plugin.manifest().id().as_str().to_owned(),
            plugin_version: [version.major(), version.minor(), version.patch()],
            request_id: request_id.as_str().to_owned(),
            capability: capability.to_owned(),
            manifest_sha256: plugin.manifest_digest().to_hex(),
            artifact_sha256: hex_digest(plugin.artifact_sha256()),
            command_sha256: hex_digest(command_sha256),
            authority_sha256: hex_digest(authority_sha256),
            frontier: InvocationFrontier::Prepared,
            response_sha256: None,
            reconciliation: None,
        };
        if path.try_exists().map_err(|error| {
            state_source("inspect invocation evidence", RecoveryDisposition::Reconcile, error)
        })? {
            let mut existing: DiskInvocation = read_json_record(&path)?;
            if !same_invocation_identity(&existing, &candidate) {
                return Err(HostError::new(
                    HostFailureClass::Protocol,
                    RecoveryDisposition::CorrectRequest,
                    "prepare durable plugin invocation",
                    "request identity was reused with changed process, command, authority, artifact, or scope",
                ));
            }
            match existing.frontier {
                InvocationFrontier::Unsent => {
                    existing.owner_fence = self.inner.fence;
                    existing.frontier = InvocationFrontier::Prepared;
                    existing.response_sha256 = None;
                    existing.reconciliation = None;
                    self.write_json_record(&path, &existing)?;
                }
                InvocationFrontier::Dispatching | InvocationFrontier::Accepted => {
                    return Err(HostError::new(
                        HostFailureClass::Indeterminate,
                        RecoveryDisposition::Reconcile,
                        "prepare durable plugin invocation",
                        "request identity has a possibly accepted effect without a correlated result",
                    ));
                }
                InvocationFrontier::Prepared => {
                    return Err(HostError::new(
                        HostFailureClass::Infrastructure,
                        RecoveryDisposition::RetryLater,
                        "prepare durable plugin invocation",
                        "request identity is already prepared by this process instance",
                    ));
                }
                InvocationFrontier::Responded | InvocationFrontier::Reconciled => {
                    return Err(HostError::new(
                        HostFailureClass::Protocol,
                        RecoveryDisposition::CorrectRequest,
                        "prepare durable plugin invocation",
                        "completed request identity cannot be dispatched again",
                    ));
                }
            }
        } else {
            self.write_json_record(&path, &candidate)?;
        }
        Ok(InvocationClaim { store: self.clone(), key })
    }

    fn transition_invocation(
        &self,
        key: &str,
        expected: InvocationFrontier,
        frontier: InvocationFrontier,
        response_sha256: Option<[u8; 32]>,
    ) -> Result<(), HostError> {
        let _mutation = self.lock_mutation()?;
        self.ensure_current_fence()?;
        let path = self.inner.invocation_directory.join(format!("{key}.json"));
        let mut record: DiskInvocation = read_json_record(&path)?;
        if record.schema != STATE_SCHEMA
            || record.owner_id != self.inner.owner_id.as_str()
            || record.owner_fence != self.inner.fence
            || record.frontier != expected
        {
            return Err(state_fence_error(
                "invocation transition does not match current fenced evidence",
            ));
        }
        record.frontier = frontier;
        record.response_sha256 = response_sha256.map(hex_digest);
        self.write_json_record(&path, &record)
    }

    fn lock_mutation(&self) -> Result<std::sync::MutexGuard<'_, ()>, HostError> {
        self.inner.mutation.lock().map_err(|_| {
            HostError::new(
                HostFailureClass::Infrastructure,
                RecoveryDisposition::Reconcile,
                "lock plugin host state custody",
                "state custody mutex was poisoned",
            )
        })
    }

    fn ensure_current_fence(&self) -> Result<(), HostError> {
        let current = current_fence(&self.inner.owner_directory)?;
        if current == self.inner.fence {
            Ok(())
        } else {
            Err(state_fence_error(
                "a newer state owner generation fenced this host attachment",
            ))
        }
    }

    fn read_owner_invocations(&self) -> Result<Vec<InvocationEvidence>, HostError> {
        let mut evidence = Vec::new();
        for entry in read_regular_entries(&self.inner.invocation_directory)? {
            let record: DiskInvocation = read_json_record(&entry)?;
            if record.owner_id == self.inner.owner_id.as_str() {
                evidence.push(invocation_evidence(record)?);
            }
        }
        Ok(evidence)
    }

    fn write_json_record<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), HostError> {
        reject_symlink(path, "plugin host state record is a symbolic link")?;
        let bytes = serde_json::to_vec(value).map_err(|error| {
            HostError::with_source(
                HostFailureClass::Infrastructure,
                RecoveryDisposition::Reconcile,
                "encode plugin host state record",
                error.to_string(),
                error,
            )
        })?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(state_request_error("plugin host state record exceeds its size bound"));
        }
        let sequence = self.inner.temporary_sequence.fetch_add(1, Ordering::Relaxed);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| state_request_error("plugin host state record name is invalid"))?;
        let temporary = path.with_file_name(format!(
            ".{file_name}.tmp-{}-{sequence}",
            self.inner.fence
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                state_source("create plugin host state record", RecoveryDisposition::Reconcile, error)
            })?;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            if let Err(cleanup_error) = fs::remove_file(&temporary) {
                return Err(HostError::with_source(
                    HostFailureClass::Infrastructure,
                    RecoveryDisposition::Reconcile,
                    "clean failed plugin host state write",
                    format!(
                        "could not persist the temporary state record ({error}); cleanup also failed: {cleanup_error}"
                    ),
                    cleanup_error,
                ));
            }
            return Err(state_source(
                "persist plugin host state record",
                RecoveryDisposition::Reconcile,
                error,
            ));
        }
        drop(file);
        if let Err(fence_error) = self.ensure_current_fence() {
            if let Err(cleanup_error) = fs::remove_file(&temporary) {
                return Err(HostError::with_source(
                    HostFailureClass::Infrastructure,
                    RecoveryDisposition::Reconcile,
                    "clean fenced plugin host state write",
                    format!(
                        "{fence_error}; temporary state cleanup also failed: {cleanup_error}"
                    ),
                    cleanup_error,
                ));
            }
            return Err(fence_error);
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if let Err(cleanup_error) = fs::remove_file(&temporary) {
                return Err(HostError::with_source(
                    HostFailureClass::Infrastructure,
                    RecoveryDisposition::Reconcile,
                    "clean uncommitted plugin host state write",
                    format!(
                        "could not commit the temporary state record ({error}); cleanup also failed: {cleanup_error}"
                    ),
                    cleanup_error,
                ));
            }
            return Err(state_source(
                "commit plugin host state record",
                RecoveryDisposition::Reconcile,
                error,
            ));
        }
        let parent = path
            .parent()
            .ok_or_else(|| state_request_error("plugin host state record has no parent"))?;
        sync_directory(parent)?;
        self.ensure_current_fence()
    }
}

impl InvocationClaim {
    pub(crate) fn mark_prepared_unsent(&self) -> Result<(), HostError> {
        self.store.transition_invocation(
            &self.key,
            InvocationFrontier::Prepared,
            InvocationFrontier::Unsent,
            None,
        )
    }

    pub(crate) fn mark_dispatching(&self) -> Result<(), HostError> {
        self.store.transition_invocation(
            &self.key,
            InvocationFrontier::Prepared,
            InvocationFrontier::Dispatching,
            None,
        )
    }

    pub(crate) fn mark_unsent(&self) -> Result<(), HostError> {
        self.store.transition_invocation(
            &self.key,
            InvocationFrontier::Dispatching,
            InvocationFrontier::Unsent,
            None,
        )
    }

    pub(crate) fn mark_accepted(&self) -> Result<(), HostError> {
        self.store.transition_invocation(
            &self.key,
            InvocationFrontier::Dispatching,
            InvocationFrontier::Accepted,
            None,
        )
    }

    pub(crate) fn mark_responded(&self, response_sha256: [u8; 32]) -> Result<(), HostError> {
        self.store.transition_invocation(
            &self.key,
            InvocationFrontier::Accepted,
            InvocationFrontier::Responded,
            Some(response_sha256),
        )
    }
}

pub(crate) fn authority_fingerprint(
    subject: &InvocationSubject,
    grant: &InvocationGrant,
    capability: &str,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"peritus-plugin-authority-v1\0");
    hash_field(&mut digest, subject.session_id().as_bytes());
    hash_field(&mut digest, subject.actor_id().as_bytes());
    digest.update(subject.authority_generation().to_be_bytes());
    hash_field(&mut digest, capability.as_bytes());
    digest.update((grant.granted_capabilities().len() as u64).to_be_bytes());
    for granted in grant.granted_capabilities() {
        hash_field(&mut digest, granted.as_bytes());
    }
    match grant.deadline_millis() {
        Some(deadline) => {
            digest.update([1]);
            digest.update(deadline.to_be_bytes());
        }
        None => digest.update([0]),
    }
    digest.finalize().into()
}

fn hash_field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

fn validate_identity(value: &str, label: &'static str) -> Result<(), HostError> {
    if value.is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || value.chars().any(char::is_control)
    {
        Err(HostError::new(
            HostFailureClass::Protocol,
            RecoveryDisposition::CorrectRequest,
            "validate plugin host state identity",
            format!("{label} is empty, oversized, or contains control characters"),
        ))
    } else {
        Ok(())
    }
}

fn valid_instance_transition(
    current: PluginInstanceFrontier,
    next: PluginInstanceFrontier,
) -> bool {
    matches!(
        (current, next),
        (PluginInstanceFrontier::Starting, PluginInstanceFrontier::Ready)
            | (PluginInstanceFrontier::Starting, PluginInstanceFrontier::Failed)
            | (PluginInstanceFrontier::Ready, PluginInstanceFrontier::Stopping)
            | (PluginInstanceFrontier::Ready, PluginInstanceFrontier::Failed)
            | (PluginInstanceFrontier::Stopping, PluginInstanceFrontier::Stopped)
            | (PluginInstanceFrontier::Stopping, PluginInstanceFrontier::Failed)
    )
}

fn same_invocation_identity(left: &DiskInvocation, right: &DiskInvocation) -> bool {
    left.schema == right.schema
        && left.owner_id == right.owner_id
        && left.instance_id == right.instance_id
        && left.plugin_id == right.plugin_id
        && left.plugin_version == right.plugin_version
        && left.request_id == right.request_id
        && left.capability == right.capability
        && left.manifest_sha256 == right.manifest_sha256
        && left.artifact_sha256 == right.artifact_sha256
        && left.command_sha256 == right.command_sha256
        && left.authority_sha256 == right.authority_sha256
}

fn invocation_evidence(record: DiskInvocation) -> Result<InvocationEvidence, HostError> {
    validate_schema(record.schema)?;
    Ok(InvocationEvidence {
        owner_id: StateOwnerId::new(record.owner_id)?,
        owner_fence: record.owner_fence,
        instance_id: PluginInstanceId::new(record.instance_id)?,
        plugin_id: plugin_id(record.plugin_id)?,
        plugin_version: plugin_version(record.plugin_version),
        request_id: request_id(record.request_id)?,
        capability: record.capability,
        manifest_sha256: parse_digest(&record.manifest_sha256)?,
        artifact_sha256: parse_digest(&record.artifact_sha256)?,
        command_sha256: parse_digest(&record.command_sha256)?,
        authority_sha256: parse_digest(&record.authority_sha256)?,
        frontier: record.frontier,
        response_sha256: record.response_sha256.as_deref().map(parse_digest).transpose()?,
        reconciliation: record.reconciliation,
    })
}

fn instance_evidence(record: DiskInstance) -> Result<PluginInstanceEvidence, HostError> {
    validate_schema(record.schema)?;
    Ok(PluginInstanceEvidence {
        owner_id: StateOwnerId::new(record.owner_id)?,
        owner_fence: record.owner_fence,
        instance_id: PluginInstanceId::new(record.instance_id)?,
        plugin_id: plugin_id(record.plugin_id)?,
        plugin_version: plugin_version(record.plugin_version),
        manifest_sha256: parse_digest(&record.manifest_sha256)?,
        artifact_sha256: parse_digest(&record.artifact_sha256)?,
        frontier: record.frontier,
    })
}

fn plugin_id(value: String) -> Result<PluginId, HostError> {
    PluginId::new(value).map_err(|error| {
        HostError::with_source(
            HostFailureClass::Infrastructure,
            RecoveryDisposition::Reconcile,
            "decode plugin host state identity",
            error.to_string(),
            error,
        )
    })
}

fn request_id(value: String) -> Result<RequestId, HostError> {
    RequestId::new(value).map_err(|error| {
        HostError::with_source(
            HostFailureClass::Infrastructure,
            RecoveryDisposition::Reconcile,
            "decode plugin host invocation identity",
            error.to_string(),
            error,
        )
    })
}

const fn plugin_version(parts: [u64; 3]) -> PluginVersion {
    PluginVersion::new(parts[0], parts[1], parts[2])
}

fn validate_schema(schema: u16) -> Result<(), HostError> {
    if schema == STATE_SCHEMA {
        Ok(())
    } else {
        Err(state_fence_error("unsupported plugin host state schema"))
    }
}

fn digest_key(fields: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"peritus-plugin-state-key-v1\0");
    for field in fields {
        hash_field(&mut digest, field);
    }
    hex_digest(digest.finalize().into())
}

fn instance_key(owner_id: &str, instance_id: &str) -> String {
    digest_key(&[owner_id.as_bytes(), instance_id.as_bytes()])
}

fn invocation_key(owner_id: &str, plugin_id: &str, request_id: &str) -> String {
    digest_key(&[owner_id.as_bytes(), plugin_id.as_bytes(), request_id.as_bytes()])
}

fn hex_digest(bytes: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn parse_digest(value: &str) -> Result<[u8; 32], HostError> {
    if value.len() != 64 {
        return Err(state_fence_error("durable SHA-256 has an invalid length"));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair)
            .map_err(|_| state_fence_error("durable SHA-256 is not ASCII"))?;
        digest[index] = u8::from_str_radix(text, 16)
            .map_err(|_| state_fence_error("durable SHA-256 is not lowercase hexadecimal"))?;
    }
    if hex_digest(digest) != value {
        return Err(state_fence_error("durable SHA-256 is not canonical lowercase hexadecimal"));
    }
    Ok(digest)
}

fn open_execution_artifact(
    path: &Path,
    plugin: &DiscoveredPlugin,
) -> Result<ExecutionArtifact, HostError> {
    reject_symlink(path, "plugin execution artifact is a symbolic link")?;
    let mut file = OpenOptions::new().read(true).open(path).map_err(|error| {
        state_source(
            "open staged plugin artifact",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        state_source(
            "inspect staged plugin artifact",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?;
    if !metadata.is_file() || metadata.len() != plugin.artifact_bytes() {
        return Err(execution_state_error(
            "staged plugin artifact is not the expected bounded regular file",
        ));
    }
    verify_execution_artifact(&mut file, plugin)?;
    harden_execution_artifact(&file, plugin.manifest().kind())?;
    file.sync_all().map_err(|error| {
        state_source(
            "persist staged plugin artifact permissions",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?;
    Ok(ExecutionArtifact {
        path: path.to_path_buf(),
        sha256: plugin.artifact_sha256(),
        _handle: Arc::new(file),
    })
}

fn verify_execution_artifact(
    file: &mut File,
    plugin: &DiscoveredPlugin,
) -> Result<(), HostError> {
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        state_source(
            "rewind staged plugin artifact",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; EXECUTION_BUFFER_BYTES].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            state_source(
                "verify staged plugin artifact",
                RecoveryDisposition::Reconcile,
                error,
            )
        })?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| {
            execution_state_error("staged plugin artifact read size is not representable")
        })?;
        total = total.checked_add(count_u64).ok_or_else(|| {
            execution_state_error("staged plugin artifact size overflowed")
        })?;
        if total > plugin.artifact_bytes() {
            return Err(execution_state_error(
                "staged plugin artifact exceeds its discovered size",
            ));
        }
        hasher.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        state_source(
            "restore staged plugin artifact",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?;
    let digest: [u8; 32] = hasher.finalize().into();
    if total != plugin.artifact_bytes() || digest != plugin.artifact_sha256() {
        return Err(execution_state_error(
            "staged plugin artifact differs from its discovered identity",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn harden_execution_artifact(file: &File, kind: PluginKind) -> Result<(), HostError> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = file.metadata().map_err(|error| {
        state_source(
            "inspect staged plugin artifact permissions",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?.permissions();
    permissions.set_mode(match kind {
        PluginKind::Process => 0o500,
        PluginKind::WasmComponent => 0o400,
    });
    file.set_permissions(permissions).map_err(|error| {
        state_source(
            "harden staged plugin artifact permissions",
            RecoveryDisposition::Reconcile,
            error,
        )
    })
}

#[cfg(not(unix))]
fn harden_execution_artifact(file: &File, _kind: PluginKind) -> Result<(), HostError> {
    let mut permissions = file.metadata().map_err(|error| {
        state_source(
            "inspect staged plugin artifact permissions",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?.permissions();
    permissions.set_readonly(true);
    file.set_permissions(permissions).map_err(|error| {
        state_source(
            "harden staged plugin artifact permissions",
            RecoveryDisposition::Reconcile,
            error,
        )
    })
}

fn clean_execution_temporary(path: &Path, file: File, error: HostError) -> HostError {
    drop(file);
    match fs::remove_file(path) {
        Ok(()) => error,
        Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => error,
        Err(cleanup) => HostError::with_source(
            HostFailureClass::Infrastructure,
            RecoveryDisposition::Reconcile,
            "clean failed staged plugin artifact",
            format!("staging failed ({error}); temporary cleanup also failed: {cleanup}"),
            cleanup,
        ),
    }
}

fn execution_state_error(detail: impl Into<String>) -> HostError {
    HostError::new(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::Reconcile,
        "verify staged plugin artifact",
        detail,
    )
}

fn checked_directory(parent: &Path, name: &str) -> Result<PathBuf, HostError> {
    let directory = parent.join(name);
    reject_symlink(&directory, "plugin host state directory is a symbolic link")?;
    fs::create_dir_all(&directory).map_err(|error| {
        state_source("create plugin host state directory", RecoveryDisposition::CorrectRequest, error)
    })?;
    reject_symlink(&directory, "plugin host state directory is a symbolic link")?;
    let canonical = fs::canonicalize(&directory).map_err(|error| {
        state_source("canonicalize plugin host state directory", RecoveryDisposition::Reconcile, error)
    })?;
    if !canonical.starts_with(parent) {
        return Err(state_request_error("plugin host state directory escapes its custody root"));
    }
    Ok(canonical)
}

fn claim_root_lease(root: &Path) -> Result<File, HostError> {
    let path = root.join("custody.lock");
    reject_symlink(&path, "plugin host custody lease is a symbolic link")?;
    let lease = match OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
        Ok(file) => {
            file.sync_all().map_err(|error| {
                state_source(
                    "persist plugin host custody lease file",
                    RecoveryDisposition::Reconcile,
                    error,
                )
            })?;
            sync_directory(root)?;
            file
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            reject_symlink(&path, "plugin host custody lease is a symbolic link")?;
            OpenOptions::new().read(true).write(true).open(&path).map_err(|error| {
                state_source(
                    "open plugin host custody lease",
                    RecoveryDisposition::Reconcile,
                    error,
                )
            })?
        }
        Err(error) => {
            return Err(state_source(
                "create plugin host custody lease",
                RecoveryDisposition::Reconcile,
                error,
            ));
        }
    };
    let metadata = lease.metadata().map_err(|error| {
        state_source(
            "inspect plugin host custody lease",
            RecoveryDisposition::Reconcile,
            error,
        )
    })?;
    if !metadata.is_file() {
        return Err(state_fence_error(
            "plugin host custody lease is not a regular file",
        ));
    }
    match fs4::FileExt::try_lock(&lease) {
        Ok(()) => Ok(lease),
        Err(fs4::TryLockError::WouldBlock) => Err(HostError::new(
            HostFailureClass::Infrastructure,
            RecoveryDisposition::RetryLater,
            "claim plugin host state custody",
            "another live owner holds the exclusive state-root lease",
        )),
        Err(fs4::TryLockError::Error(error)) => Err(state_source(
            "lock plugin host state custody",
            RecoveryDisposition::Reconcile,
            error,
        )),
    }
}

fn establish_owner_identity(directory: &Path, owner_id: &StateOwnerId) -> Result<(), HostError> {
    let path = directory.join("identity");
    reject_symlink(&path, "plugin host state owner identity is a symbolic link")?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            if let Err(error) = file
                .write_all(owner_id.as_str().as_bytes())
                .and_then(|()| file.sync_all())
            {
                drop(file);
                if let Err(cleanup_error) = fs::remove_file(&path) {
                    return Err(HostError::with_source(
                        HostFailureClass::Infrastructure,
                        RecoveryDisposition::Reconcile,
                        "clean failed plugin host owner claim",
                        format!(
                            "could not persist the owner identity ({error}); cleanup also failed: {cleanup_error}"
                        ),
                        cleanup_error,
                    ));
                }
                return Err(state_source(
                    "persist plugin host state owner identity",
                    RecoveryDisposition::Reconcile,
                    error,
                ));
            }
            sync_directory(directory)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut bytes = Vec::new();
            File::open(&path)
                .and_then(|mut file| file.take((MAX_IDENTITY_BYTES + 1) as u64).read_to_end(&mut bytes))
                .map_err(|error| {
                    state_source(
                        "read plugin host state owner identity",
                        RecoveryDisposition::Reconcile,
                        error,
                    )
                })?;
            if bytes == owner_id.as_str().as_bytes() {
                Ok(())
            } else {
                Err(state_fence_error("state owner digest collides with another identity"))
            }
        }
        Err(error) => Err(state_source(
            "create plugin host state owner identity",
            RecoveryDisposition::Reconcile,
            error,
        )),
    }
}

fn claim_fence(directory: &Path) -> Result<u64, HostError> {
    loop {
        let next = current_fence(directory)?.checked_add(1).ok_or_else(|| {
            state_fence_error("plugin host state owner generation is exhausted")
        })?;
        let path = directory.join(format!("fence-{next:020}"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                if let Err(error) = file.sync_all() {
                    drop(file);
                    if let Err(cleanup_error) = fs::remove_file(&path) {
                        return Err(HostError::with_source(
                            HostFailureClass::Infrastructure,
                            RecoveryDisposition::Reconcile,
                            "clean failed plugin host owner fence",
                            format!(
                                "could not persist the owner fence ({error}); cleanup also failed: {cleanup_error}"
                            ),
                            cleanup_error,
                        ));
                    }
                    return Err(state_source(
                        "persist plugin host state owner fence",
                        RecoveryDisposition::Reconcile,
                        error,
                    ));
                }
                sync_directory(directory)?;
                return Ok(next);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(state_source(
                    "claim plugin host state owner fence",
                    RecoveryDisposition::Reconcile,
                    error,
                ));
            }
        }
    }
}

fn current_fence(directory: &Path) -> Result<u64, HostError> {
    let mut maximum = 0_u64;
    for entry in fs::read_dir(directory)
        .map_err(|error| state_source("read state owner fences", RecoveryDisposition::Reconcile, error))?
    {
        let entry = entry.map_err(|error| {
            state_source("enumerate state owner fences", RecoveryDisposition::Reconcile, error)
        })?;
        let file_type = entry.file_type().map_err(|error| {
            state_source("inspect state owner fence", RecoveryDisposition::Reconcile, error)
        })?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(value) = name.strip_prefix("fence-") else { continue };
        let fence = value.parse::<u64>().map_err(|_| {
            state_fence_error("plugin host state contains a malformed owner fence")
        })?;
        maximum = maximum.max(fence);
    }
    Ok(maximum)
}

fn read_regular_entries(directory: &Path) -> Result<Vec<PathBuf>, HostError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| {
        state_source("read plugin host state records", RecoveryDisposition::Reconcile, error)
    })? {
        let entry = entry.map_err(|error| {
            state_source("enumerate plugin host state records", RecoveryDisposition::Reconcile, error)
        })?;
        let file_type = entry.file_type().map_err(|error| {
            state_source("inspect plugin host state record", RecoveryDisposition::Reconcile, error)
        })?;
        if file_type.is_symlink() {
            return Err(state_fence_error("plugin host state record is a symbolic link"));
        }
        if file_type.is_file()
            && entry.path().extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            entries.push(entry.path());
        }
    }
    entries.sort_unstable();
    Ok(entries)
}

fn read_json_record<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, HostError> {
    reject_symlink(path, "plugin host state record is a symbolic link")?;
    let metadata = fs::metadata(path).map_err(|error| {
        state_source("inspect plugin host state record", RecoveryDisposition::Reconcile, error)
    })?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(state_fence_error("plugin host state record is not a bounded regular file"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| {
            state_source("read plugin host state record", RecoveryDisposition::Reconcile, error)
        })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        HostError::with_source(
            HostFailureClass::Infrastructure,
            RecoveryDisposition::Reconcile,
            "decode plugin host state record",
            error.to_string(),
            error,
        )
    })
}

fn reject_symlink(path: &Path, detail: &'static str) -> Result<(), HostError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(state_request_error(detail)),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(state_source(
            "inspect plugin host state path",
            RecoveryDisposition::CorrectRequest,
            error,
        )),
    }
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), HostError> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            state_source("sync plugin host state directory", RecoveryDisposition::Reconcile, error)
        })
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), HostError> {
    Ok(())
}

fn state_request_error(detail: impl Into<String>) -> HostError {
    HostError::new(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::CorrectRequest,
        "access plugin host state custody",
        detail,
    )
}

fn state_fence_error(detail: impl Into<String>) -> HostError {
    HostError::new(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::Reconcile,
        "access plugin host state custody",
        detail,
    )
}

fn state_source(
    operation: &'static str,
    recovery: RecoveryDisposition,
    error: std::io::Error,
) -> HostError {
    HostError::with_source(
        HostFailureClass::Infrastructure,
        recovery,
        operation,
        error.to_string(),
        error,
    )
}
