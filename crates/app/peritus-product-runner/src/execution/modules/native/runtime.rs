//! Product authority adapter for the existing native Rust plugin host.
use super::super::{ProductRunInput, ProductRunnerError, error};
use peritus_modules::{Definition, Registry};
use peritus_plugin_host::*;
use peritus_plugin_sdk::*;
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
    revision: u64,
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
                && self.view.revision() == self.revision
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
    subject: InvocationSubject,
    sequence: AtomicU64,
    pub(super) state: PathBuf,
    pub(super) module_id: String,
    pub(super) module_digest: String,
    _owner: fs::File,
}
impl Runtime {
    pub(super) async fn start(
        input: &ProductRunInput,
        definition: &Definition,
    ) -> Result<Self, ProductRunnerError> {
        let code = definition
            .manifest
            .code
            .as_ref()
            .ok_or_else(|| error("module has no native entrypoint"))?;
        let artifact = artifact(code)?;
        let source = definition.resource(artifact).map_err(error)?;
        let base = super::super::root(input)
            .join(format!("native-{}-{}", definition.manifest.id, definition.digest));
        fs::create_dir_all(&base).map_err(error)?;
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(base.join("session.lock"))
            .map_err(error)?;
        fs4::FileExt::try_lock(&owner)
            .map_err(|e| error(format!("native plugin session is already owned: {e}")))?;
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
            .and_then(|r| r.load(&definition.manifest.id, true))
            .map_err(error)?;
        if current.digest != definition.digest || hash(&source)? != hash(staged.path())? {
            return Err(error("native package changed during launch; no executable was started"));
        }
        staged.persist(&executable).map_err(|e| error(e.error))?;
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
                revision: input.conversation.revision(),
            }),
            Arc::new(trust),
        ));
        host.start(&id, version).await.map_err(error)?;
        let state = base.join("state");
        fs::create_dir_all(&state).map_err(error)?;
        Ok(Self {
            host,
            id,
            subject: InvocationSubject::new(
                hex(input.run_id.as_bytes()),
                "interactive-module-user",
                input.conversation.revision(),
            ),
            sequence: AtomicU64::new(0),
            state,
            module_id: definition.manifest.id.clone(),
            module_digest: definition.digest.clone(),
            _owner: owner,
        })
    }
    pub(super) async fn call(
        &self,
        input: &ProductRunInput,
        method: &str,
        mut params: Value,
    ) -> Result<Value, ProductRunnerError> {
        params["state_directory"] = json!(self.state);
        let payload = JsonPayload::new(
            json!({"method":method,"params_json":params.to_string()}),
            JsonBounds::PRODUCTION,
        )
        .map_err(error)?;
        let cancellation = HostCancellation::new();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let request = RequestId::new(format!("module-native-{sequence}")).map_err(error)?;
        let invocation = self.host.invoke(
            &self.id,
            request,
            "module.workflow",
            payload,
            &self.subject,
            &cancellation,
        );
        tokio::pin!(invocation);
        let result = loop {
            tokio::select! {result=&mut invocation=>break result.map_err(error)?,()=tokio::time::sleep(Duration::from_millis(100))=>{if input.provider_cancellation.is_cancelled()||input.cancelled.load(Ordering::Acquire)||input.conversation.revision()!=self.subject.authority_generation(){let _=cancellation.cancel();}}}
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
    pub(super) fn call_sync(
        &self,
        input: &ProductRunInput,
        method: &str,
        params: Value,
    ) -> Result<Value, ProductRunnerError> {
        let handle = tokio::runtime::Handle::try_current().map_err(error)?;
        let operation = || {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| handle.block_on(self.call(input, method, params)))
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
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash(path: &Path) -> Result<Vec<u8>, ProductRunnerError> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 32768];
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
