//! Disposable real context store + actual workspace reader shared by owner campaigns.
use crate::{
    ConversationView, DecisionViewPolicy, LocalContextConfig, ReadOnlyPlanExecution,
    control::{HostPermissions, PermissionCapability},
    local_context::{
        LocalContextHandle, MemoryTools,
        memory::{LocalMemory, environment::WorkspaceScope},
    },
};
use peritus_agent::{
    DeveloperLoopLimits, DeveloperLoopRequest, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_context::{ContextNodeId, working::WorkingBinding};
use peritus_model_protocol::*;
use peritus_provider_core::CancellationToken;
use peritus_role::HarnessRole;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub(in crate::local_context) fn text(role: Role, body: &str) -> Result<Message, String> {
    Message::new(
        role,
        vec![ContentBlock::Text(
            BoundedText::new(body.to_owned(), ProtocolLimits::PRODUCTION)
                .map_err(|e| e.to_string())?,
        )],
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|e| e.to_string())
}
pub(in crate::local_context) fn call(
    id: &str,
    name: &str,
    args: &Value,
) -> Result<CompletedToolCall, String> {
    CompletedToolCall::new(
        ToolCallId::new(id.into()).map_err(|e| e.to_string())?,
        ToolName::new(name.into()).map_err(|e| e.to_string())?,
        CanonicalJson::parse(&args.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub(in crate::local_context) fn result_message(
    call: &CompletedToolCall,
    output: &DeveloperToolObservation,
) -> Result<Message, String> {
    Message::new(
        Role::Tool,
        vec![ContentBlock::ToolResult(ToolResult::new(
            call.id().clone(),
            output.output.clone(),
            output.is_error,
        ))],
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|e| e.to_string())
}
pub(in crate::local_context) fn profile(
    input: u64,
    output: u64,
) -> Result<ProviderProfile, String> {
    ProviderProfile::new(
        ProviderProfileId::new([75; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new("praxis".into()).map_err(|e| e.to_string())?,
        ModelName::new("fixture".into()).map_err(|e| e.to_string())?,
        WireDialect::OpenAiResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::ParallelToolCalls], &[])
            .map_err(|e| e.to_string())?,
        CapabilityProvenance::Probed,
        ModelLimits::new(input, output, 64, 8, 1).map_err(|e| e.to_string())?,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|e| e.to_string())
}

pub(in crate::local_context) struct Control {
    pub read: AtomicBool,
    pub opaque: AtomicBool,
    pub protected: Mutex<Vec<PathBuf>>,
}
impl ConversationView for Control {
    fn revision(&self) -> u64 {
        7
    }
    fn render(&self) -> String {
        if self.opaque.load(Ordering::Acquire) {
            "black-box query import call `file.txt` `file-0.txt` `file-1.txt` `file-2.txt`".into()
        } else {
            "Inspect disposable fixture files.".into()
        }
    }
    fn effective_permissions(&self) -> HostPermissions {
        if self.read.load(Ordering::Acquire) {
            HostPermissions::all()
        } else {
            HostPermissions::all().without(PermissionCapability::Read)
        }
    }
    fn protected_paths(&self) -> Vec<PathBuf> {
        self.protected.lock().expect("protected").clone()
    }
}
pub(in crate::local_context) struct Fixture {
    pub workspace: PathBuf,
    root: PathBuf,
    trace: PathBuf,
    binding: WorkingBinding,
    config: LocalContextConfig,
    pub view: Arc<Control>,
    pub handle: Option<LocalContextHandle>,
    serial: u64,
    _temp: tempfile::TempDir,
}
impl Fixture {
    pub fn new(conservative: bool, plans: bool, role: HarnessRole) -> Result<Self, String> {
        let temp = tempfile::Builder::new()
            .prefix("peritus-praxis-context-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).map_err(|e| e.to_string())?;
        let root = temp.path().join("memory");
        let trace = temp.path().join("trace.bin");
        let binding = WorkingBinding::new(
            RunId::new([71; 16]).map_err(|e| format!("{e:?}"))?,
            WorkspaceId::new([72; 16]).map_err(|e| format!("{e:?}"))?,
            ContextNodeId::new([73; 16]).map_err(|e| format!("{e:?}"))?,
            role,
            7,
        );
        let config = LocalContextConfig {
            decision_view: if conservative {
                DecisionViewPolicy::Conservative
            } else {
                DecisionViewPolicy::History
            },
            read_only_plans: if plans {
                ReadOnlyPlanExecution::Sequential
            } else {
                ReadOnlyPlanExecution::Disabled
            },
            ..LocalContextConfig::default()
        };
        let mut this = Self {
            _temp: temp,
            workspace,
            root,
            trace,
            binding,
            config,
            view: Arc::new(Control {
                read: AtomicBool::new(true),
                opaque: AtomicBool::new(false),
                protected: Mutex::new(Vec::new()),
            }),
            handle: None,
            serial: 0,
        };
        let mut memory = this.load()?;
        let request = DeveloperLoopRequest {
            request_prefix: "praxis-context".into(),
            system: "Fixture policy.".into(),
            prompt: "Inspect disposable fixture files.".into(),
            cyber_access_program: None,
            attachments: Vec::new(),
            tools: Vec::new(),
            limits: DeveloperLoopLimits::new(2, 64).map_err(|e| e.to_string())?,
            cancellation: CancellationToken::default(),
        };
        memory
            .begin(
                &request,
                &[
                    text(Role::System, "Fixture policy.")?,
                    text(Role::User, "Inspect disposable fixture files.")?,
                ],
            )
            .map_err(|e| e.to_string())?;
        this.handle = Some(LocalContextHandle::praxis_from_memory(memory, this.view.clone()));
        Ok(this)
    }
    fn load(&self) -> Result<LocalMemory, String> {
        let mut memory = LocalMemory::load_scoped(
            &self.root,
            &self.workspace,
            &self.trace,
            self.binding,
            self.config.clone(),
            WorkspaceScope { direct: true, protected: Vec::new() },
        )
        .map_err(|e| e.to_string())?;
        memory.conversation = Some(self.view.clone());
        Ok(memory)
    }
    pub fn memory(&self) -> Result<&LocalContextHandle, String> {
        self.handle.as_ref().ok_or_else(|| "memory closed".into())
    }
    pub fn reopen(&mut self) -> Result<(), String> {
        drop(self.handle.take());
        let memory = self.load()?;
        self.handle = Some(LocalContextHandle::praxis_from_memory(memory, self.view.clone()));
        Ok(())
    }
    pub fn execute(
        &mut self,
        name: &str,
        args: &Value,
        reuse: bool,
    ) -> Result<(Value, bool, u64), String> {
        self.serial += 1;
        let call = call(&format!("call_{}", self.serial), name, args)?;
        self.memory()?
            .lock()
            .map_err(|e| e.to_string())?
            .observe_message(
                &Message::new(
                    Role::Assistant,
                    vec![ContentBlock::ToolCall(call.clone())],
                    ProtocolLimits::PRODUCTION,
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let mut base =
            crate::developer_tools::WorkspaceDeveloperTools::read_only(self.workspace.clone())
                .with_task_contract(&self.view.render())
                .with_protection_view(self.view.clone());
        let output = if reuse {
            MemoryTools::new(&mut base, self.memory()?.clone()).execute(&call)
        } else {
            base.execute(&call)
        }
        .map_err(|e| e.to_string())?;
        let mut memory = self.memory()?.lock().map_err(|e| e.to_string())?;
        let id = memory.observe_tool(&call, &output).map_err(|e| e.to_string())?;
        memory.observe_message(&result_message(&call, &output)?).map_err(|e| e.to_string())?;
        drop(memory);
        Ok((
            serde_json::from_slice(output.output.canonical_bytes()).map_err(|e| e.to_string())?,
            output.is_error,
            id,
        ))
    }
}
