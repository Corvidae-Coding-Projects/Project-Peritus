use super::{Case, provider};
use peritus_app_client::Client;
use peritus_app_protocol::*;
use peritus_types::{RunId, WorkspaceId};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn workspace() -> WorkspaceId {
    WorkspaceId::new([78; 16]).expect("fixture id")
}
pub(super) fn run() -> RunId {
    RunId::new([79; 16]).expect("fixture id")
}
pub(super) fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(ConversationId::new([80; 16]).expect("fixture id"), workspace())
}
pub(super) fn models(model: &str, high: bool) -> Result<ProductRoleModels, String> {
    let choice = ProductModelChoice::new(model.into(), true)
        .map_err(|e| format!("{e:?}"))?
        .with_effort(if high { ProductModelEffort::High } else { ProductModelEffort::Low });
    Ok(ProductRoleModels::new(choice.clone(), choice.clone(), choice))
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}
pub(super) fn save(path: &Path, v: &impl serde::Serialize) -> Result<(), String> {
    let temporary = path.with_extension("praxis-tmp");
    let mut f = fs::File::create(&temporary).map_err(|e| format!("{e:?}"))?;
    f.write_all(&serde_json::to_vec(v).map_err(|e| format!("{e:?}"))?)
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("{e:?}"))?;
    drop(f);
    fs::rename(temporary, path).map_err(|e| format!("{e:?}"))
}
pub(super) struct Fixture {
    pub root: PathBuf,
    pub client: Option<Client>,
    child: Option<Child>,
    serial: u8,
    _temporary: tempfile::TempDir,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
impl Fixture {
    pub async fn new(case: &Case) -> Result<Self, String> {
        let temp =
            tempfile::Builder::new().prefix("pt3-").tempdir().map_err(|e| format!("{e:?}"))?;
        let root = temp.path().to_path_buf();
        fs::create_dir(root.join("workspace")).map_err(|e| format!("{e:?}"))?;
        fs::write(root.join("workspace/source.txt"), "exact fixture μ🙂\n")
            .map_err(|e| format!("{e:?}"))?;
        save(&root.join("case.json"), case)?;
        let mut this = Self { root, client: None, child: None, serial: 0, _temporary: temp };
        this.start().await?;
        Ok(this)
    }
    pub async fn start(&mut self) -> Result<(), String> {
        let ready = self.root.join("ready.json");
        if ready.exists() {
            fs::remove_file(&ready).map_err(|e| format!("{e:?}"))?;
        }
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("child.log"))
            .map_err(|e| format!("{e:?}"))?;
        let child = Command::new(std::env::current_exe().map_err(|e| format!("{e:?}"))?)
            .args(["praxis_daemon::praxis_stage3_daemon_child", "--exact", "--nocapture"])
            .env("PRAXIS_DAEMON_ROOT", &self.root)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| format!("{e:?}"))?)
            .stderr(log)
            .spawn()
            .map_err(|e| format!("{e:?}"))?;
        self.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if ready.exists() {
                break;
            }
            if self
                .child
                .as_mut()
                .ok_or("child missing")?
                .try_wait()
                .map_err(|e| format!("{e:?}"))?
                .is_some()
            {
                return Err(format!(
                    "daemon child exited before readiness: {}",
                    fs::read_to_string(self.root.join("child.log")).unwrap_or_default()
                ));
            }
            if Instant::now() > deadline {
                return Err("daemon startup timed out".into());
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.reconnect().await
    }
    pub async fn reconnect(&mut self) -> Result<(), String> {
        let ready: Value = serde_json::from_slice(
            &fs::read(self.root.join("ready.json")).map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| format!("{e:?}"))?;
        self.client = Some(
            Client::connect(
                std::ffi::OsStr::new(ready["endpoint"].as_str().ok_or("endpoint")?),
                None,
                Duration::from_secs(8),
                &[
                    WellKnownProtocolFeature::WorkbenchControl,
                    WellKnownProtocolFeature::WorkbenchInputs,
                    WellKnownProtocolFeature::WorkbenchExecution,
                    WellKnownProtocolFeature::WorkbenchConversation,
                    WellKnownProtocolFeature::WorkbenchBriefPages,
                ],
            )
            .await
            .map_err(|e| format!("{e:?}"))?,
        );
        Ok(())
    }
    pub fn kill(&mut self) -> Result<(), String> {
        drop(self.client.take());
        let mut c = self.child.take().ok_or("missing daemon child")?;
        c.kill().map_err(|e| format!("{e:?}"))?;
        c.wait().map_err(|e| format!("{e:?}"))?;
        Ok(())
    }
    pub async fn request(
        &mut self,
        payload: AppRequestPayload,
    ) -> Result<AppResponsePayload, String> {
        let response = self
            .client
            .as_mut()
            .ok_or("not connected")?
            .request(Client::new_request_identity().map_err(|e| format!("{e:?}"))?, payload)
            .await
            .map_err(|e| format!("{e:?}"))?;
        Ok(response.payload().clone())
    }
    pub async fn revision(&mut self) -> Result<u64, String> {
        match self.request(AppRequestPayload::QueryWorkbench(query())).await? {
            AppResponsePayload::Workbench(s) => Ok(s.revision()),
            other => Err(format!("query revision: {other:?}")),
        }
    }
    pub async fn command(&mut self, intent: WorkbenchIntent) -> Result<WorkbenchReceipt, String> {
        let revision = if matches!(intent, WorkbenchIntent::CreateConversation(_)) {
            0
        } else {
            self.revision().await?
        };
        self.serial = self.serial.checked_add(1).ok_or("operation ids exhausted")?;
        let command = WorkbenchCommand::new(
            ControlOperationId::new([self.serial; 16]).map_err(|e| format!("{e:?}"))?,
            query(),
            revision,
            intent,
        );
        match self.request(AppRequestPayload::WorkbenchCommand(command)).await? {
            AppResponsePayload::WorkbenchReceipt(r) => Ok(r),
            other => Err(format!("command: {other:?}")),
        }
    }
    pub async fn enqueue(&mut self, id: u8, text: &str) -> Result<WorkbenchReceipt, String> {
        let input = WorkbenchNewInput::new(
            WorkbenchInputId::new([id; 16]).map_err(|e| format!("{e:?}"))?,
            WorkbenchInputText::new(text.into()).map_err(|e| format!("{e:?}"))?,
            WorkbenchInputOrder::new(Vec::new()).map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| format!("{e:?}"))?;
        self.command(WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(input))).await
    }
    pub async fn begin(&mut self) -> Result<(), String> {
        self.command(WorkbenchIntent::CreateConversation(
            ConversationTitle::new("Fixture conversation".into()).map_err(|e| format!("{e:?}"))?,
        ))
        .await?;
        self.enqueue(
            81,
            "Inspect disposable fixture source files and follow all later instructions.",
        )
        .await?;
        let id = provider::profile("fixture-a")?.profile_id();
        let settings = WorkbenchExecutionSettings::new(
            run(),
            ProductProviderSelection::new(id, id, id),
            ProductInteractionMode::Chat,
            models("fixture-a", false)?,
        );
        self.command(WorkbenchIntent::StartExecution(settings)).await?;
        Ok(())
    }
    pub fn script(&self, turn: u64, value: &Value) -> Result<(), String> {
        save(&self.root.join(format!("turn-{turn}.json")), value)
    }
    pub fn release(&self, turn: u64) -> Result<(), String> {
        fs::write(self.root.join(format!("release-{turn}")), b"release")
            .map_err(|e| format!("{e:?}"))
    }
    pub fn requests(&self) -> Result<Vec<Value>, String> {
        requests_at(&self.root)
    }
    pub fn wait_requests(
        &self,
        count: usize,
    ) -> impl Future<Output = Result<Vec<Value>, String>> + Send + 'static {
        let root = self.root.clone();
        async move {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let rows = requests_at(&root)?;
                if rows.len() >= count {
                    return Ok(rows);
                }
                if Instant::now() > deadline {
                    return Err(format!(
                        "no request {count}; daemon log {}",
                        fs::read_to_string(root.join("child.log")).unwrap_or_default()
                    ));
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }
    pub async fn interaction(&mut self) -> Result<ProductInteractionSnapshot, String> {
        match self
            .request(AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run())))
            .await?
        {
            AppResponsePayload::Interaction(v) => Ok(v),
            other => Err(format!("interaction: {other:?}")),
        }
    }
    pub async fn wait_phase(
        &mut self,
        phase: ProductRunPhase,
    ) -> Result<ProductInteractionSnapshot, String> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let snapshot = self.interaction().await?;
            if snapshot.snapshot().phase() == phase {
                return Ok(snapshot);
            }
            if Instant::now() > deadline {
                return Err(format!(
                    "expected {phase:?}, got {:?}: {} {}",
                    snapshot.snapshot().phase(),
                    snapshot.snapshot().summary(),
                    fs::read_to_string(self.root.join("child.log")).unwrap_or_default()
                ));
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}
pub(super) fn child(root: &Path) -> Result<(), String> {
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e|e.to_string())?.block_on(async{
        let registry=peritus_approval::CredentialRegistrySnapshot::new(peritus_types::RevisionNumber::new(1).map_err(|e|format!("{e:?}"))?,Vec::new()).map_err(|e|format!("{e:?}"))?;
        fs::write(root.join("registry.bin"),registry.canonical_bytes().map_err(|e|format!("{e:?}"))?).map_err(|e|e.to_string())?;
        let folder=peritus_workspace::FolderIdentity::observe(&root.join("workspace")).map_err(|e|e.to_string())?;
        let paths=["state_root","artifact_root","evidence_root","workspace_root","process_root","transaction_root","backup_root"].into_iter().zip(["state","artifacts","evidence","managed","processes","transactions","backup"]).map(|(key,folder)|(key.to_owned(),json!(if key=="state_root"{root.join("state")}else{root.join("state").join(folder)}))).collect::<serde_json::Map<_,_>>();
        let value=json!({"version":1,"store_id":hex(&[77;16]),"paths":paths,"approval_registry":{"payload_file":root.join("registry.bin"),"generation":1},"human":{"actor_id":hex(&[82;16])},"folders":[{"workspace_id":hex(workspace().as_bytes()),"root":root.join("workspace"),"identity":hex(folder.digest().as_bytes()),"writable":true,"protected_paths":[]}],"telemetry":{"mode":"disabled"},"product":{"automatic_provider_failover":false}});
        let config=crate::DaemonConfig::parse(&toml::to_string(&value).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let runtime=crate::DaemonRuntime::start(config).await.map_err(|e|e.to_string())?;
        let crate::LocalEndpointAddress::Unix(endpoint)=runtime.endpoint_address();
        save(&root.join("ready.json"),&json!({"endpoint":endpoint,"pid":std::process::id()}))?;
        let deadline=Instant::now()+Duration::from_mins(3);
        while !root.join("shutdown").exists() && Instant::now()<deadline{tokio::time::sleep(Duration::from_millis(10)).await;}
        runtime.shutdown().await.map_err(|e|e.to_string())?;Ok(())
    })
}

fn requests_at(root: &Path) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    for turn in 0..64 {
        let path = root.join(format!("request-{turn}.json"));
        if !path.exists() {
            return Ok(rows);
        }
        rows.push(
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        );
    }
    Err("fixture model turn bound exceeded".into())
}
