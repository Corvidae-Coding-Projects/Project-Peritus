//! Plugin host lifecycle and authority-bound invocation orchestration.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use peritus_plugin_sdk::{
    FailureClass, HostRequest, InvocationContext, JsonPayload, LEGACY_PROTOCOL_VERSION,
    PROTOCOL_VERSION, PluginFailure, PluginId, PluginKind, PluginQuotas, PluginRequestEnvelope,
    PluginResponse, PluginStatus, PluginVersion, ProtocolRange, RequestId,
};
use tokio::sync::Mutex;

use crate::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, DiscoveredPlugin, HostCancellation,
    HostError, HostFailureClass, HostStateStore, InvocationGrant, InvocationSubject, PluginCatalog,
    PluginInstanceFrontier, PluginInstanceId, RecoveryDisposition, TrustDecision, TrustVerifier,
    quota::QuotaLedger,
    state::{ExecutionArtifact, authority_fingerprint},
    transport::{LaunchPlan, PluginConnection, internal_request_id},
};

/// Host-wide runtime, protocol, and quota ceilings.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Executable used for Wasm components.
    pub wasm_runtime: PathBuf,
    /// Host maximum quotas intersected with every manifest.
    pub quota_ceiling: PluginQuotas,
    /// Maximum lifecycle handshake duration.
    pub startup_timeout: Duration,
    /// Maximum graceful shutdown duration.
    pub shutdown_timeout: Duration,
}

/// Owned plugin lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginLifecycle {
    /// Artifact is known but not executing.
    Discovered,
    /// Isolation runtime has been launched and is negotiating.
    Starting,
    /// Version negotiation succeeded and requests are accepted.
    Ready,
    /// Shutdown is in progress and admission is closed.
    Stopping,
    /// Isolated runtime exited after an observed shutdown.
    Stopped,
    /// Runtime failed and must be restarted before new work.
    Failed,
}

/// Read-only plugin state snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginSnapshot {
    /// Plugin identity.
    pub id: PluginId,
    /// Exact plugin version.
    pub version: PluginVersion,
    /// Caller-generated identity of the exact owned process.
    pub instance_id: PluginInstanceId,
    /// Current host-owned lifecycle.
    pub lifecycle: PluginLifecycle,
    /// Active invocation count.
    pub active_requests: usize,
    /// Total admitted invocation count.
    pub lifecycle_requests: u64,
    /// User-visible trust anchor used at startup.
    pub trust_anchor: String,
}

/// Truthful terminal plugin invocation outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginInvocationResult {
    /// Structured successful result.
    Succeeded {
        /// Bounded plugin output.
        output: JsonPayload,
        /// Optional bounded rendering.
        rendering: Option<String>,
    },
    /// Plugin returned a typed failure.
    Failed(PluginFailure),
    /// Plugin observed cancellation.
    Cancelled,
}

struct PluginInstance {
    discovered: DiscoveredPlugin,
    connection: Arc<PluginConnection>,
    quotas: QuotaLedger,
    protocol_version: u16,
    lifecycle: Mutex<PluginLifecycle>,
    trust_anchor: String,
    instance_id: PluginInstanceId,
}

/// Bounded registry and lifecycle owner for isolated plugins.
pub struct PluginHost {
    config: HostConfig,
    catalog: PluginCatalog,
    authority: Arc<dyn AuthorityMediator>,
    trust: Arc<dyn TrustVerifier>,
    start_gate: Mutex<()>,
    instances: Mutex<BTreeMap<PluginId, Arc<PluginInstance>>>,
    state: HostStateStore,
}

impl PluginHost {
    /// Creates a deny-unless-mediated host over an immutable discovered catalog.
    #[must_use]
    pub fn new(
        config: HostConfig,
        catalog: PluginCatalog,
        authority: Arc<dyn AuthorityMediator>,
        trust: Arc<dyn TrustVerifier>,
        state: HostStateStore,
    ) -> Self {
        Self {
            config,
            catalog,
            authority,
            trust,
            start_gate: Mutex::new(()),
            instances: Mutex::new(BTreeMap::new()),
            state,
        }
    }

    /// Starts one exact plugin version after rechecking trust and negotiating protocol v1.
    ///
    /// # Errors
    ///
    /// Rejects unknown/already-running plugins, missing trust, launch failure, timeout, or an
    /// invalid initialization response.
    pub async fn start(
        &self,
        id: &PluginId,
        version: PluginVersion,
        instance_id: PluginInstanceId,
    ) -> Result<(), HostError> {
        let _start = self.start_gate.lock().await;
        if self.instances.lock().await.contains_key(id) {
            return Err(HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::CorrectRequest,
                "start plugin",
                "a plugin with this identity is already owned by the host",
            ));
        }
        let discovered = self.catalog.get(id, version).cloned().ok_or_else(|| {
            HostError::new(
                HostFailureClass::Discovery,
                RecoveryDisposition::CorrectRequest,
                "start plugin",
                "requested plugin identity and version were not discovered",
            )
        })?;
        let execution = self.state.stage_execution_artifact(&discovered)?;
        let trust_anchor = match self.trust.verify(&discovered, execution.sha256()) {
            TrustDecision::Trusted { anchor } => anchor,
            TrustDecision::Unknown => {
                return Err(trust_error("plugin has no explicit trust anchor"));
            }
            TrustDecision::DigestMismatch => {
                return Err(trust_error("plugin bytes differ from the explicit trust anchor"));
            }
        };
        let host_protocol = ProtocolRange::new(LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION)
            .map_err(plugin_protocol_error)?;
        let protocol_version = discovered
            .manifest()
            .protocol()
            .negotiate(host_protocol)
            .map_err(plugin_protocol_error)?;
        let quotas = discovered
            .manifest()
            .quotas()
            .narrow(self.config.quota_ceiling)
            .validate()
            .map_err(plugin_protocol_error)?;
        if protocol_version == LEGACY_PROTOCOL_VERSION && !quotas.is_v1_compatible() {
            return Err(HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::CorrectRequest,
                "negotiate plugin policy",
                "selected host ceilings are not representable by plugin protocol version one",
            ));
        }
        self.state.register_instance(&discovered, &execution, &instance_id)?;
        let plan = self.launch_plan(&discovered, execution);
        let connection = match PluginConnection::spawn(plan, quotas, protocol_version) {
            Ok(connection) => connection,
            Err(error) => {
                self.state.transition_instance(id, &instance_id, PluginInstanceFrontier::Failed)?;
                return Err(error);
            }
        };
        let instance = Arc::new(PluginInstance {
            discovered,
            connection,
            quotas: QuotaLedger::new(quotas),
            protocol_version,
            lifecycle: Mutex::new(PluginLifecycle::Starting),
            trust_anchor,
            instance_id,
        });
        let initialize = PluginRequestEnvelope {
            protocol_version,
            request_id: internal_request_id("host.initialize")?,
            request: HostRequest::Initialize {
                protocol_version,
                plugin_id: instance.discovered.manifest().id().clone(),
                plugin_version: instance.discovered.manifest().version(),
                quotas,
            },
        };
        let response = instance
            .connection
            .exchange(
                initialize,
                Some(self.config.startup_timeout),
                &HostCancellation::new(),
            )
            .await;
        match response {
            Ok(response)
                if matches!(
                    response.response,
                    PluginResponse::Status { status: PluginStatus::Ready }
                ) =>
            {
                if let Err(error) = self.state.transition_instance(
                    id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Ready,
                ) {
                    let termination = instance.connection.terminate().await;
                    let failed = self.state.transition_instance(
                        id,
                        &instance.instance_id,
                        PluginInstanceFrontier::Failed,
                    );
                    if let Err(termination) = termination {
                        return Err(termination);
                    }
                    failed?;
                    return Err(error);
                }
                *instance.lifecycle.lock().await = PluginLifecycle::Ready;
                self.instances.lock().await.insert(id.clone(), instance);
                Ok(())
            }
            Ok(_) => {
                let termination = instance.connection.terminate().await;
                let failed = self.state.transition_instance(
                    id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Failed,
                );
                termination?;
                failed?;
                Err(HostError::new(
                    HostFailureClass::Protocol,
                    RecoveryDisposition::CorrectRequest,
                    "initialize plugin",
                    "plugin did not acknowledge the negotiated ready state",
                ))
            }
            Err(error) => {
                let termination = instance.connection.terminate().await;
                let failed = self.state.transition_instance(
                    id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Failed,
                );
                termination?;
                failed?;
                Err(error)
            }
        }
    }

    /// Invokes one declared capability after current authority mediation and quota admission.
    ///
    /// # Errors
    ///
    /// Rejects unavailable lifecycle, undeclared capability, authority denial, malformed grants,
    /// quota exhaustion, transport failure, cancellation, timeout, or invalid response shape.
    pub async fn invoke(
        &self,
        plugin_id: &PluginId,
        request_id: RequestId,
        capability_name: &str,
        input: JsonPayload,
        subject: &InvocationSubject,
        cancellation: &HostCancellation,
    ) -> Result<PluginInvocationResult, HostError> {
        let instance = self.instance(plugin_id).await?;
        let capability = instance
            .discovered
            .manifest()
            .capabilities()
            .iter()
            .find(|candidate| candidate.name() == capability_name)
            .ok_or_else(|| authorization_error("plugin capability was not declared"))?;
        let mut permit = instance.quotas.reserve()?;
        let local_timeout = instance
            .quotas
            .limits()
            .invocation_millis
            .map(Duration::from_millis);
        let mut dispatch = instance
            .connection
            .admit_dispatch(local_timeout, cancellation)
            .await?;
        let mut lifecycle = instance.lifecycle.lock().await;
        if *lifecycle != PluginLifecycle::Ready {
            return Err(unavailable("plugin is not in the ready lifecycle state at dispatch"));
        }
        let decision = dispatch
            .await_before_send(
                self.authority.authorize(AuthorityRequest::new(plugin_id, capability, subject)),
                cancellation,
            )
            .await?;
        let grant = match decision {
            AuthorityDecision::Authorized(grant) => grant,
            AuthorityDecision::Denied { code, detail } => {
                return Err(HostError::new(
                    HostFailureClass::Authorization,
                    RecoveryDisposition::Reauthorize,
                    "authorize plugin invocation",
                    format!("{code}: {detail}"),
                ));
            }
        };
        validate_grant(capability_name, &grant)?;
        dispatch.narrow_timeout(grant.deadline_millis().map(Duration::from_millis));
        let deadline_millis = min_optional_millis(
            instance.quotas.limits().invocation_millis,
            grant.deadline_millis(),
        );
        let context = InvocationContext::new(
            subject.session_id(),
            subject.actor_id(),
            InvocationGrant::role(),
            grant.granted_capabilities().to_vec(),
            subject.authority_generation(),
            deadline_millis,
        )
        .map_err(plugin_protocol_error)?;
        let request = PluginRequestEnvelope {
            protocol_version: instance.protocol_version,
            request_id,
            request: HostRequest::Invoke { capability: capability_name.to_owned(), input, context },
        };
        let command_sha256 = dispatch.request_sha256(&request)?;
        let authority_sha256 = authority_fingerprint(subject, &grant, capability_name);
        let claim = self.state.prepare_invocation(
            &instance.discovered,
            &instance.instance_id,
            &request.request_id,
            capability_name,
            command_sha256,
            authority_sha256,
        )?;
        let exchange = dispatch
            .exchange_admitted(request, cancellation, &mut permit, &claim)
            .await;
        let exchange = match exchange {
            Ok(exchange) => exchange,
            Err(error) => {
                if permit.is_admitted() || error.class() == HostFailureClass::Infrastructure {
                    *lifecycle = PluginLifecycle::Failed;
                    if let Err(state_error) = self.state.transition_instance(
                        plugin_id,
                        &instance.instance_id,
                        PluginInstanceFrontier::Failed,
                    ) {
                        return Err(HostError::with_source(
                            error.class(),
                            error.recovery(),
                            "record failed plugin process",
                            format!(
                                "{error}; the durable failed-process transition also failed: {state_error}"
                            ),
                            error,
                        ));
                    }
                }
                return Err(error);
            }
        };
        match exchange.response.response {
            PluginResponse::Success { output, rendering } => {
                let output_size = output.canonical_bytes().len() as u64
                    + rendering.as_ref().map_or(0, |text| text.len() as u64);
                if output_size > instance.quotas.limits().output_bytes {
                    *lifecycle = PluginLifecycle::Failed;
                    let termination = instance.connection.terminate().await;
                    let failed = self.state.transition_instance(
                        plugin_id,
                        &instance.instance_id,
                        PluginInstanceFrontier::Failed,
                    );
                    termination?;
                    failed?;
                    return Err(HostError::new(
                        HostFailureClass::Quota,
                        RecoveryDisposition::RestartPlugin,
                        "accept plugin result",
                        "plugin result exceeds its output quota",
                    ));
                }
                Ok(PluginInvocationResult::Succeeded { output, rendering })
            }
            PluginResponse::Failure(failure) => {
                if failure.class() == FailureClass::Cancelled {
                    Ok(PluginInvocationResult::Cancelled)
                } else {
                    Ok(PluginInvocationResult::Failed(failure))
                }
            }
            PluginResponse::Status { status: PluginStatus::Cancelled } => {
                Ok(PluginInvocationResult::Cancelled)
            }
            PluginResponse::Status { .. } => {
                *lifecycle = PluginLifecycle::Failed;
                let termination = instance.connection.terminate().await;
                let failed = self.state.transition_instance(
                    plugin_id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Failed,
                );
                termination?;
                failed?;
                Err(HostError::new(
                    HostFailureClass::Protocol,
                    RecoveryDisposition::RestartPlugin,
                    "accept plugin result",
                    "plugin returned a lifecycle status for an invocation",
                ))
            }
        }
    }

    /// Gracefully shuts down one owned plugin and always terminates the child afterward.
    ///
    /// # Errors
    ///
    /// Returns a typed error when no instance exists or the shutdown acknowledgement is invalid.
    pub async fn stop(&self, id: &PluginId) -> Result<(), HostError> {
        let instance = self.instance(id).await?;
        {
            let mut lifecycle = instance.lifecycle.lock().await;
            if matches!(*lifecycle, PluginLifecycle::Stopping | PluginLifecycle::Stopped) {
                return Ok(());
            }
            self.state.transition_instance(
                id,
                &instance.instance_id,
                PluginInstanceFrontier::Stopping,
            )?;
            *lifecycle = PluginLifecycle::Stopping;
        }
        let request = PluginRequestEnvelope {
            protocol_version: instance.protocol_version,
            request_id: internal_request_id("host.shutdown")?,
            request: HostRequest::Shutdown,
        };
        let result = instance
            .connection
            .exchange(
                request,
                Some(self.config.shutdown_timeout),
                &HostCancellation::new(),
            )
            .await;
        if let Err(error) = instance.connection.terminate().await {
            *instance.lifecycle.lock().await = PluginLifecycle::Failed;
            if let Err(state_error) = self.state.transition_instance(
                id,
                &instance.instance_id,
                PluginInstanceFrontier::Failed,
            ) {
                return Err(HostError::with_source(
                    HostFailureClass::Infrastructure,
                    RecoveryDisposition::Reconcile,
                    "terminate owned plugin",
                    format!(
                        "{error}; the durable failed-process transition also failed: {state_error}"
                    ),
                    error,
                ));
            }
            return Err(error);
        }
        self.instances.lock().await.remove(id);
        match result {
            Ok(response)
                if matches!(
                    response.response,
                    PluginResponse::Status { status: PluginStatus::Stopped }
                ) =>
            {
                self.state.transition_instance(
                    id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Stopped,
                )?;
                *instance.lifecycle.lock().await = PluginLifecycle::Stopped;
                Ok(())
            }
            Ok(_) => {
                *instance.lifecycle.lock().await = PluginLifecycle::Failed;
                self.state.transition_instance(
                    id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Failed,
                )?;
                Err(HostError::new(
                    HostFailureClass::Protocol,
                    RecoveryDisposition::None,
                    "stop plugin",
                    "plugin did not acknowledge the stopped state",
                ))
            }
            Err(error) => {
                *instance.lifecycle.lock().await = PluginLifecycle::Failed;
                self.state.transition_instance(
                    id,
                    &instance.instance_id,
                    PluginInstanceFrontier::Failed,
                )?;
                Err(error)
            }
        }
    }

    /// Returns canonical snapshots for all currently owned plugin instances.
    pub async fn snapshots(&self) -> Vec<PluginSnapshot> {
        let instances = self.instances.lock().await.values().cloned().collect::<Vec<_>>();
        let mut snapshots = Vec::with_capacity(instances.len());
        for instance in instances {
            snapshots.push(PluginSnapshot {
                id: instance.discovered.manifest().id().clone(),
                version: instance.discovered.manifest().version(),
                instance_id: instance.instance_id.clone(),
                lifecycle: *instance.lifecycle.lock().await,
                active_requests: instance.quotas.active(),
                lifecycle_requests: instance.quotas.used(),
                trust_anchor: instance.trust_anchor.clone(),
            });
        }
        snapshots.sort_unstable_by(|left, right| {
            (left.id.as_str(), left.version).cmp(&(right.id.as_str(), right.version))
        });
        snapshots
    }

    async fn instance(&self, id: &PluginId) -> Result<Arc<PluginInstance>, HostError> {
        self.instances
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| unavailable("plugin is not owned by this host"))
    }

    fn launch_plan(
        &self,
        plugin: &DiscoveredPlugin,
        execution: ExecutionArtifact,
    ) -> LaunchPlan {
        let arguments = plugin.manifest().entrypoint().arguments().to_vec();
        match plugin.manifest().kind() {
            PluginKind::Process => LaunchPlan::Process {
                executable: execution,
                arguments,
                working_directory: plugin.root().to_path_buf(),
            },
            PluginKind::WasmComponent => LaunchPlan::Wasm {
                runtime: self.config.wasm_runtime.clone(),
                module: execution,
                arguments,
                working_directory: plugin.root().to_path_buf(),
            },
        }
    }
}

fn validate_grant(capability_name: &str, grant: &InvocationGrant) -> Result<(), HostError> {
    let capabilities = grant.granted_capabilities();
    if grant.deadline_millis() == Some(0)
        || !capabilities.iter().any(|name| name == capability_name)
        || capabilities.windows(2).any(|pair| pair[0] >= pair[1])
    {
        Err(HostError::new(
            HostFailureClass::Authorization,
            RecoveryDisposition::Reauthorize,
            "validate plugin authority grant",
            "mediator grant is stale, noncanonical, or does not include the requested capability",
        ))
    } else {
        Ok(())
    }
}

const fn min_optional_millis(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left < right { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn plugin_protocol_error(error: peritus_plugin_sdk::SdkError) -> HostError {
    HostError::with_source(
        HostFailureClass::Protocol,
        RecoveryDisposition::CorrectRequest,
        "negotiate plugin policy",
        error.to_string(),
        error,
    )
}

fn trust_error(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Trust,
        RecoveryDisposition::EstablishTrust,
        "verify plugin trust",
        detail,
    )
}

fn authorization_error(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Authorization,
        RecoveryDisposition::Reauthorize,
        "authorize plugin invocation",
        detail,
    )
}

fn unavailable(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::RestartPlugin,
        "access plugin instance",
        detail,
    )
}
