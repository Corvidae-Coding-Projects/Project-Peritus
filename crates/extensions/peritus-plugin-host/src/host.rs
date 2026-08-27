//! Plugin host lifecycle and authority-bound invocation orchestration.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use peritus_plugin_sdk::{
    FailureClass, HostRequest, InvocationContext, JsonPayload, PROTOCOL_VERSION, PluginFailure,
    PluginId, PluginKind, PluginQuotas, PluginRequestEnvelope, PluginResponse, PluginStatus,
    PluginVersion, RequestId,
};
use tokio::sync::Mutex;

use crate::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, DiscoveredPlugin, HostCancellation,
    HostError, HostFailureClass, InvocationGrant, InvocationSubject, PluginCatalog,
    RecoveryDisposition, TrustDecision, TrustVerifier,
    quota::QuotaLedger,
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
    lifecycle: Mutex<PluginLifecycle>,
    trust_anchor: String,
}

/// Bounded registry and lifecycle owner for isolated plugins.
pub struct PluginHost {
    config: HostConfig,
    catalog: PluginCatalog,
    authority: Arc<dyn AuthorityMediator>,
    trust: Arc<dyn TrustVerifier>,
    start_gate: Mutex<()>,
    instances: Mutex<BTreeMap<PluginId, Arc<PluginInstance>>>,
}

impl PluginHost {
    /// Creates a deny-unless-mediated host over an immutable discovered catalog.
    #[must_use]
    pub fn new(
        config: HostConfig,
        catalog: PluginCatalog,
        authority: Arc<dyn AuthorityMediator>,
        trust: Arc<dyn TrustVerifier>,
    ) -> Self {
        Self {
            config,
            catalog,
            authority,
            trust,
            start_gate: Mutex::new(()),
            instances: Mutex::new(BTreeMap::new()),
        }
    }

    /// Starts one exact plugin version after rechecking trust and negotiating protocol v1.
    ///
    /// # Errors
    ///
    /// Rejects unknown/already-running plugins, missing trust, launch failure, timeout, or an
    /// invalid initialization response.
    pub async fn start(&self, id: &PluginId, version: PluginVersion) -> Result<(), HostError> {
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
        let trust_anchor = match self.trust.verify(&discovered) {
            TrustDecision::Trusted { anchor } => anchor,
            TrustDecision::Unknown => {
                return Err(trust_error("plugin has no explicit trust anchor"));
            }
            TrustDecision::DigestMismatch => {
                return Err(trust_error("plugin bytes differ from the explicit trust anchor"));
            }
        };
        let quotas = discovered.manifest().quotas().narrow(self.config.quota_ceiling);
        let plan = self.launch_plan(&discovered);
        let connection = PluginConnection::spawn(plan, quotas.frame_bytes)?;
        let instance = Arc::new(PluginInstance {
            discovered,
            connection,
            quotas: QuotaLedger::new(quotas),
            lifecycle: Mutex::new(PluginLifecycle::Starting),
            trust_anchor,
        });
        let initialize = PluginRequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: internal_request_id("host.initialize")?,
            request: HostRequest::Initialize {
                protocol_version: PROTOCOL_VERSION,
                plugin_id: instance.discovered.manifest().id().clone(),
                plugin_version: instance.discovered.manifest().version(),
                quotas,
            },
        };
        let response = instance
            .connection
            .exchange(initialize, self.config.startup_timeout, &HostCancellation::new())
            .await;
        match response {
            Ok(response)
                if matches!(
                    response.response,
                    PluginResponse::Status { status: PluginStatus::Ready }
                ) =>
            {
                *instance.lifecycle.lock().await = PluginLifecycle::Ready;
                self.instances.lock().await.insert(id.clone(), instance);
                Ok(())
            }
            Ok(_) => {
                instance.connection.terminate().await;
                Err(HostError::new(
                    HostFailureClass::Protocol,
                    RecoveryDisposition::CorrectRequest,
                    "initialize plugin",
                    "plugin did not acknowledge the negotiated ready state",
                ))
            }
            Err(error) => {
                instance.connection.terminate().await;
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
        if *instance.lifecycle.lock().await != PluginLifecycle::Ready {
            return Err(unavailable("plugin is not in the ready lifecycle state"));
        }
        let capability = instance
            .discovered
            .manifest()
            .capabilities()
            .iter()
            .find(|candidate| candidate.name() == capability_name)
            .ok_or_else(|| authorization_error("plugin capability was not declared"))?;
        let decision =
            self.authority.authorize(AuthorityRequest::new(plugin_id, capability, subject)).await?;
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
        let _permit = instance.quotas.reserve()?;
        let context = InvocationContext {
            session_id: subject.session_id().to_owned(),
            actor_id: subject.actor_id().to_owned(),
            role: InvocationGrant::role(),
            granted_capabilities: grant.granted_capabilities().to_vec(),
            authority_generation: subject.authority_generation(),
            deadline_millis: grant.deadline_millis(),
        };
        let request = PluginRequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            request: HostRequest::Invoke { capability: capability_name.to_owned(), input, context },
        };
        let timeout = Duration::from_millis(
            instance
                .discovered
                .manifest()
                .quotas()
                .narrow(self.config.quota_ceiling)
                .invocation_millis,
        );
        let response = instance.connection.exchange(request, timeout, cancellation).await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                *instance.lifecycle.lock().await = PluginLifecycle::Failed;
                return Err(error);
            }
        };
        match response.response {
            PluginResponse::Success { output, rendering } => {
                let output_size = output.canonical_bytes().len() as u64
                    + rendering.as_ref().map_or(0, |text| text.len() as u64);
                if output_size > instance.quotas.limits().output_bytes {
                    *instance.lifecycle.lock().await = PluginLifecycle::Failed;
                    instance.connection.terminate().await;
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
                if failure.class == FailureClass::Cancelled {
                    Ok(PluginInvocationResult::Cancelled)
                } else {
                    Ok(PluginInvocationResult::Failed(failure))
                }
            }
            PluginResponse::Status { status: PluginStatus::Cancelled } => {
                Ok(PluginInvocationResult::Cancelled)
            }
            PluginResponse::Status { .. } => Err(HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::RestartPlugin,
                "accept plugin result",
                "plugin returned a lifecycle status for an invocation",
            )),
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
            *lifecycle = PluginLifecycle::Stopping;
        }
        let request = PluginRequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: internal_request_id("host.shutdown")?,
            request: HostRequest::Shutdown,
        };
        let result = instance
            .connection
            .exchange(request, self.config.shutdown_timeout, &HostCancellation::new())
            .await;
        instance.connection.terminate().await;
        self.instances.lock().await.remove(id);
        match result {
            Ok(response)
                if matches!(
                    response.response,
                    PluginResponse::Status { status: PluginStatus::Stopped }
                ) =>
            {
                *instance.lifecycle.lock().await = PluginLifecycle::Stopped;
                Ok(())
            }
            Ok(_) => {
                *instance.lifecycle.lock().await = PluginLifecycle::Failed;
                Err(HostError::new(
                    HostFailureClass::Protocol,
                    RecoveryDisposition::None,
                    "stop plugin",
                    "plugin did not acknowledge the stopped state",
                ))
            }
            Err(error) => {
                *instance.lifecycle.lock().await = PluginLifecycle::Failed;
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

    fn launch_plan(&self, plugin: &DiscoveredPlugin) -> LaunchPlan {
        let arguments = plugin.manifest().entrypoint().arguments().to_vec();
        match plugin.manifest().kind() {
            PluginKind::Process => LaunchPlan::Process {
                executable: plugin.artifact_path().to_path_buf(),
                arguments,
                working_directory: plugin.root().to_path_buf(),
            },
            PluginKind::WasmComponent => LaunchPlan::Wasm {
                runtime: self.config.wasm_runtime.clone(),
                module: plugin.artifact_path().to_path_buf(),
                arguments,
                working_directory: plugin.root().to_path_buf(),
            },
        }
    }
}

fn validate_grant(capability_name: &str, grant: &InvocationGrant) -> Result<(), HostError> {
    let capabilities = grant.granted_capabilities();
    if grant.deadline_millis() == 0
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
