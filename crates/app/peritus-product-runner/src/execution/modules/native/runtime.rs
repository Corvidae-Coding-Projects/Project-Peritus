//! Product authority adapter for the existing native Rust plugin host.
use super::super::{ProductRunInput, ProductRunnerError, error};
use peritus_modules::{Definition, Registry};
use peritus_plugin_host::{
    AuthorityDecision, AuthorityMediator, AuthorityRequest, DigestTrustStore, DiscoveryLimits,
    HostCancellation, HostConfig, HostError, HostFuture, InvocationGrant, InvocationSubject,
    PluginHost, PluginInvocationResult, discover,
};
use peritus_plugin_sdk::{
    HOST_CAPABILITY_APPLICATION_LABORATORY, HOST_CAPABILITY_ATTACHMENT, HOST_CAPABILITY_CHECKPOINT,
    HOST_CAPABILITY_MODEL, HOST_CAPABILITY_PRIVATE_STATE, HOST_CAPABILITY_PROFILE,
    HOST_CAPABILITY_SCOPED_SOURCE, HOST_CAPABILITY_SOURCE_BUILD, HOST_CAPABILITY_WORKER,
    JsonBounds, JsonPayload, MODULE_HOST_CONTRACT_VERSION, ModuleHostCapability, ModuleHostOutcome,
    ModuleHostProfile, ModuleHostRequest, ModuleHostResult, PluginId, RequestId,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

struct LiveAuthority {
    view: Arc<dyn crate::ConversationView>,
    id: String,
    digest: String,
}
impl AuthorityMediator for LiveAuthority {
    fn authorize<'a>(
        &'a self,
        request: AuthorityRequest<'a>,
    ) -> HostFuture<'a, Result<AuthorityDecision, HostError>> {
        Box::pin(async move {
            let enabled = Registry::for_user()
                .and_then(|r| r.load(&self.id, true))
                .is_ok_and(|d| d.digest == self.digest);
            let allowed = enabled
                && self.view.revision() == request.subject().authority_generation()
                && request.plugin_id().as_str() == self.id
                && request.capability().name() == "module.workflow"
                && self
                    .view
                    .effective_permissions()
                    .allows(crate::control::PermissionCapability::Read);
            Ok(if allowed {
                AuthorityDecision::Authorized(InvocationGrant::observed(
                    vec!["module.workflow".into()],
                    30000,
                ))
            } else {
                AuthorityDecision::Denied{code:"module-authority-changed".into(),detail:"The selected module, user revision or current read permission changed. No plugin invocation was admitted.".into()}
            })
        })
    }
}
pub(super) struct Runtime {
    host: Arc<PluginHost>,
    id: PluginId,
    session_id: String,
    sequence: AtomicU64,
    pub(super) state: PathBuf,
    pub(super) host_state: PathBuf,
    pub(super) module_id: String,
    pub(super) module_digest: String,
    module_executable: PathBuf,
    _owner: fs::File,
}

struct StagedPlugin {
    base: PathBuf,
    owner: fs::File,
    directory: PathBuf,
    executable: PathBuf,
}

fn stage_plugin(
    input: &ProductRunInput,
    definition: &Definition,
) -> Result<StagedPlugin, ProductRunnerError> {
    let code = definition
        .manifest
        .code
        .as_ref()
        .ok_or_else(|| error("module has no native entrypoint"))?;
    let source = definition.resource(artifact(code)?).map_err(error)?;
    let root = super::super::root(input);
    let base = root.join(format!("native-{}-{}", definition.manifest.id, definition.digest));
    if base.starts_with(&input.workspace_root)
        || input.workspace_root.starts_with(&base)
        || base.starts_with(&input.source_root)
        || input.source_root.starts_with(&base)
    {
        return Err(error(
            "native plugin private state overlaps the selected workspace; no module state or source operation was started",
        ));
    }
    fs::create_dir_all(&base).map_err(error)?;
    let owner = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(base.join("session.lock"))
        .map_err(error)?;
    fs4::FileExt::try_lock(&owner)
        .map_err(|cause| error(format!("native plugin session is already owned: {cause}")))?;
    let directory = base.join("runtime").join(&definition.manifest.id);
    fs::create_dir_all(&directory).map_err(error)?;
    let executable = directory.join(if cfg!(windows) { "plugin.exe" } else { "plugin" });
    let staged = tempfile::NamedTempFile::new_in(&directory).map_err(error)?;
    fs::copy(&source, staged.path()).map_err(error)?;
    staged.as_file().sync_all().map_err(error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staged.path(), fs::Permissions::from_mode(0o700)).map_err(error)?;
    }
    let current = Registry::for_user()
        .and_then(|registry| registry.load(&definition.manifest.id, true))
        .map_err(error)?;
    if current.digest != definition.digest || hash(&source)? != hash(staged.path())? {
        return Err(error("native package changed during launch; no executable was started"));
    }
    staged.persist(&executable).map_err(|cause| error(cause.error))?;
    Ok(StagedPlugin { base, owner, directory, executable })
}

impl Runtime {
    pub(super) async fn start(
        input: &ProductRunInput,
        definition: &Definition,
    ) -> Result<Self, ProductRunnerError> {
        let StagedPlugin { base, owner, directory, executable } = stage_plugin(input, definition)?;
        let version = definition
            .manifest
            .version
            .split('.')
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        let manifest = format!(
            "manifest_version=1\nid=\"{}\"\nversion={{major={},minor={},patch={}}}\nkind=\"process\"\nprotocol={{minimum=1,maximum=1}}\nentrypoint={{artifact=\"{}\",arguments=[]}}\ncapabilities=[{{name=\"module.workflow\",operation=\"inspection\",required=true}}]\nquotas={{concurrent_requests=1,frame_bytes=33554432,output_bytes=25165824,invocation_millis=30000,lifecycle_requests=9223372036854775807,protocol_violations=1}}\n",
            definition.manifest.id,
            version[0],
            version[1],
            version[2],
            if cfg!(windows) { "plugin.exe" } else { "plugin" }
        );
        fs::write(directory.join("peritus-plugin.toml"), manifest).map_err(error)?;
        let catalog =
            discover(&[base.join("runtime")], DiscoveryLimits::PRODUCTION).map_err(error)?;
        let discovered = catalog
            .iter()
            .next()
            .ok_or_else(|| error("native plugin discovery returned no executable"))?;
        let id = discovered.manifest().id().clone();
        let version = discovered.manifest().version();
        let quotas = discovered.manifest().quotas();
        let trust = DigestTrustStore::new().with_anchor(
            id.clone(),
            version,
            discovered.manifest_digest(),
            discovered.artifact_sha256(),
            format!("explicitly enabled module {}", definition.digest),
        );
        let host = Arc::new(PluginHost::new(
            HostConfig {
                wasm_runtime: PathBuf::new(),
                quota_ceiling: quotas,
                startup_timeout: Duration::from_secs(10),
                shutdown_timeout: Duration::from_secs(3),
            },
            catalog,
            Arc::new(LiveAuthority {
                view: Arc::clone(&input.conversation),
                id: definition.manifest.id.clone(),
                digest: definition.digest.clone(),
            }),
            Arc::new(trust),
        ));
        host.start(&id, version).await.map_err(error)?;
        let state = base.join("state");
        fs::create_dir_all(&state).map_err(error)?;
        let host_state = base.join("host-state");
        fs::create_dir_all(&host_state).map_err(error)?;
        Ok(Self {
            host,
            id,
            session_id: hex(input.run_id.as_bytes()),
            sequence: AtomicU64::new(0),
            state,
            host_state,
            module_id: definition.manifest.id.clone(),
            module_digest: definition.digest.clone(),
            module_executable: executable,
            _owner: owner,
        })
    }
    pub(super) async fn call(
        &self,
        input: &ProductRunInput,
        method: &str,
        mut params: Value,
        revision: u64,
    ) -> Result<Value, ProductRunnerError> {
        params["state_directory"] = json!(self.state);
        params["host_profile"] = serde_json::to_value(host_profile(input)).map_err(error)?;
        params["provider_routes"] = json!([
            {
                "role":"writer",
                "provider":input.providers.writer.profile().provider().as_str(),
                "model":input.providers.writer.profile().model().as_str(),
            },
            {
                "role":"reviewer",
                "provider":input.providers.reviewer.profile().provider().as_str(),
                "model":input.providers.reviewer.profile().model().as_str(),
            },
        ]);
        let payload = JsonPayload::new(
            json!({"method":method,"params_json":params.to_string()}),
            JsonBounds::PRODUCTION,
        )
        .map_err(error)?;
        let cancellation = HostCancellation::new();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let request = RequestId::new(format!("module-native-{sequence}")).map_err(error)?;
        let subject = InvocationSubject::new(&self.session_id, "interactive-module-user", revision);
        let invocation = self.host.invoke(
            &self.id,
            request,
            "module.workflow",
            payload,
            &subject,
            &cancellation,
        );
        tokio::pin!(invocation);
        let result = loop {
            tokio::select! {result=&mut invocation=>break result.map_err(error)?,()=tokio::time::sleep(Duration::from_millis(100))=>{if input.provider_cancellation.is_cancelled()||input.cancelled.load(Ordering::Acquire)||input.conversation.revision()!=revision{let _=cancellation.cancel();}}}
        };
        match result {
            PluginInvocationResult::Succeeded { output, .. } => serde_json::from_str(
                output.value()["json"]
                    .as_str()
                    .ok_or_else(|| error("native plugin response has no JSON service result"))?,
            )
            .map_err(error),
            PluginInvocationResult::Failed(failure) => {
                Err(error(format!("native plugin {}: {}", failure.code, failure.detail)))
            }
            PluginInvocationResult::Cancelled => {
                Err(error("native plugin invocation was cancelled; private state retained"))
            }
        }
    }
    pub(super) fn host_service(
        &self,
        input: &ProductRunInput,
        request: &ModuleHostRequest,
    ) -> Result<ModuleHostResult, ProductRunnerError> {
        let outcome = self.host_outcome(input, request)?;
        Ok(ModuleHostResult {
            key: request.key.clone(),
            capability: request.capability.clone(),
            version: request.version,
            outcome,
        })
    }

    fn host_outcome(
        &self,
        input: &ProductRunInput,
        request: &ModuleHostRequest,
    ) -> Result<ModuleHostOutcome, ProductRunnerError> {
        let outcome = if request.capability == HOST_CAPABILITY_PROFILE
            && request.version == MODULE_HOST_CONTRACT_VERSION
            && request.operation == "inspect"
        {
            ModuleHostOutcome::Success {
                output: serde_json::to_value(host_profile(input)).map_err(error)?,
            }
        } else if request.capability == HOST_CAPABILITY_SCOPED_SOURCE
            && request.version == MODULE_HOST_CONTRACT_VERSION
        {
            if input
                .conversation
                .effective_permissions()
                .allows(crate::control::PermissionCapability::Read)
            {
                super::scoped_source::Broker::new(input, &self.state, &self.module_digest)
                    .dispatch(&request.operation, request.input.clone())
            } else {
                ModuleHostOutcome::Rejected {
                    code: "source_read_permission".into(),
                    detail: "current conversation authority does not permit source reads".into(),
                }
            }
        } else if request.capability == HOST_CAPABILITY_SOURCE_BUILD
            && request.version == MODULE_HOST_CONTRACT_VERSION
            && request.operation == "inspect"
        {
            #[cfg(target_os = "linux")]
            {
                match Registry::for_user()
                    .and_then(|registry| registry.source_build_status(&self.module_id))
                {
                    Ok(output) => ModuleHostOutcome::Success { output },
                    Err(error) => ModuleHostOutcome::Rejected {
                        code: "source_build_record".into(),
                        detail: error.to_string(),
                    },
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                ModuleHostOutcome::Unsupported {
                    detail: "source-built module installation currently requires Linux".into(),
                }
            }
        } else if request.capability == HOST_CAPABILITY_APPLICATION_LABORATORY
            && request.version == MODULE_HOST_CONTRACT_VERSION
        {
            self.application_laboratory_outcome(input, request)
        } else if request.capability == HOST_CAPABILITY_WORKER
            && request.version == MODULE_HOST_CONTRACT_VERSION
            && request.operation == "execute"
        {
            self.worker_outcome(input, request)
        } else {
            ModuleHostOutcome::Unsupported {
                detail: format!(
                    "host capability {} version {} operation {} is unavailable in this Peritus build",
                    request.capability, request.version, request.operation
                ),
            }
        };
        Ok(outcome)
    }

    fn worker_outcome(
        &self,
        input: &ProductRunInput,
        request: &ModuleHostRequest,
    ) -> ModuleHostOutcome {
        #[cfg(target_os = "linux")]
        {
            super::application_laboratory::Broker::new(
                input,
                &self.host_state,
                &self.state,
                &self.module_id,
                &self.module_digest,
            )
            .execute_worker(&self.module_executable, request.input.clone())
        }
        #[cfg(not(target_os = "linux"))]
        {
            ModuleHostOutcome::Unsupported {
                detail: "isolated native module workers currently require Linux".into(),
            }
        }
    }

    fn application_laboratory_outcome(
        &self,
        input: &ProductRunInput,
        request: &ModuleHostRequest,
    ) -> ModuleHostOutcome {
        #[cfg(target_os = "linux")]
        {
            super::application_laboratory::Broker::new(
                input,
                &self.host_state,
                &self.state,
                &self.module_id,
                &self.module_digest,
            )
            .dispatch(&request.key, &request.operation, request.input.clone())
        }
        #[cfg(not(target_os = "linux"))]
        {
            ModuleHostOutcome::Unsupported {
                detail: "host-owned application laboratories currently require Linux".into(),
            }
        }
    }
    pub(super) fn call_sync(
        &self,
        input: &ProductRunInput,
        method: &str,
        params: Value,
        revision: u64,
    ) -> Result<Value, ProductRunnerError> {
        let handle = tokio::runtime::Handle::try_current().map_err(error)?;
        let operation = || {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| handle.block_on(self.call(input, method, params, revision)))
                    .join()
                    .map_err(|_| error("native plugin invocation worker failed"))?
            })
        };
        if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
            tokio::task::block_in_place(operation)
        } else {
            operation()
        }
    }
    pub(super) async fn stop(&self) -> Result<(), ProductRunnerError> {
        self.host.stop(&self.id).await.map_err(error)
    }
}
fn host_profile(input: &ProductRunInput) -> ModuleHostProfile {
    let mut capabilities = vec![
        ModuleHostCapability { name: HOST_CAPABILITY_ATTACHMENT.into(), version: 1 },
        ModuleHostCapability { name: HOST_CAPABILITY_CHECKPOINT.into(), version: 1 },
        ModuleHostCapability { name: HOST_CAPABILITY_MODEL.into(), version: 1 },
        ModuleHostCapability { name: HOST_CAPABILITY_PRIVATE_STATE.into(), version: 1 },
        ModuleHostCapability { name: HOST_CAPABILITY_SCOPED_SOURCE.into(), version: 1 },
        ModuleHostCapability {
            name: HOST_CAPABILITY_PROFILE.into(),
            version: MODULE_HOST_CONTRACT_VERSION,
        },
    ];
    if cfg!(target_os = "linux") {
        capabilities.push(ModuleHostCapability {
            name: HOST_CAPABILITY_SOURCE_BUILD.into(),
            version: MODULE_HOST_CONTRACT_VERSION,
        });
    }
    if cfg!(target_os = "linux") && input.module_jobs.is_some() {
        capabilities.push(ModuleHostCapability {
            name: HOST_CAPABILITY_APPLICATION_LABORATORY.into(),
            version: MODULE_HOST_CONTRACT_VERSION,
        });
        capabilities.push(ModuleHostCapability {
            name: HOST_CAPABILITY_WORKER.into(),
            version: MODULE_HOST_CONTRACT_VERSION,
        });
    }
    ModuleHostProfile { schema: MODULE_HOST_CONTRACT_VERSION, capabilities }
}
fn hex(bytes: &[u8]) -> String {
    crate::encode_hex(bytes)
}
fn hash(path: &Path) -> Result<Vec<u8>, ProductRunnerError> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut bytes = vec![0; 32_768];
    loop {
        let n = f.read(&mut bytes).map_err(error)?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(hash.finalize().to_vec())
}
fn artifact(code: &peritus_modules::Code) -> Result<&str, ProductRunnerError> {
    let arch = std::env::consts::ARCH;
    let targets = if cfg!(target_os = "linux") {
        vec![
            format!(
                "{arch}-unknown-linux-{}",
                if cfg!(target_env = "musl") { "musl" } else { "gnu" }
            ),
            format!("{arch}-unknown-linux-musl"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![format!("{arch}-apple-darwin")]
    } else if cfg!(windows) {
        vec![format!("{arch}-pc-windows-msvc"), format!("{arch}-pc-windows-gnu")]
    } else {
        vec![]
    };
    targets.iter().find_map(|target| code.artifacts.get(target).map(String::as_str)).ok_or_else(
        || {
            error(format!(
                "this native Rust plugin has no binary for {}/{}",
                std::env::consts::OS,
                arch
            ))
        },
    )
}
