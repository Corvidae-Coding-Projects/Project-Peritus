//! Real durable inspection-plan execution with an independent small interpreter.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use peritus_agent::{
    DeveloperActivity, DeveloperControlFlow, DeveloperInput, DeveloperInteraction,
    DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopRequest, DeveloperModelRole,
    DeveloperRequestAdmission, DeveloperToolEffect, DeveloperToolExecutor,
    DeveloperToolObservation,
};
use peritus_context::{ContextNodeId, working::WorkingBinding};
use peritus_model_protocol::{
    BoundedText, CancellationKind, CanonicalJson, Capability, CapabilityMatrix,
    CapabilityProvenance, CompletedToolCall, ContentBlock, JsonBounds, Message, ModelLimits,
    ModelName, ModelRequest, OutputLimitEnforcement, ProtocolLimits, ProviderName, ProviderProfile,
    ResumeKind, Role, StateMode, ToolCallId, ToolName, WireDialect,
};
use peritus_provider_core::CancellationToken;
use peritus_role::HarnessRole;
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    ConversationView, DecisionViewPolicy, LocalContextConfig, ReadOnlyPlanExecution,
    control::{HostPermissions, PermissionCapability},
    local_context::{
        LocalContextHandle,
        memory::{LocalMemory, environment::WorkspaceScope},
        storage::StoredArtifact,
    },
};

pub type Metrics = BTreeMap<String, u64>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum Case {
    Plan {
        enabled: bool,
        nodes: Vec<NodeSpec>,
        page_bytes: u16,
        payload_units: u16,
        reopen: bool,
        stop: Stop,
        after: u8,
    },
    Invalid {
        defect: Defect,
    },
    Cursor {
        defect: CursorDefect,
        reopen: bool,
    },
    Archive {
        fault: ArchiveFault,
        disable_execution: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeSpec {
    pub tool: Tool,
    pub reference: Reference,
    pub source: u8,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Read,
    List,
    Search,
}
impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Read => "workspace_read",
            Self::List => "workspace_list",
            Self::Search => "workspace_search",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reference {
    Literal,
    Direct,
    Array,
    Escaped,
    Missing,
    WrongType,
}
impl Reference {
    fn pointer(self) -> Option<&'static str> {
        match self {
            Self::Literal => None,
            Self::Direct => Some("/next_path"),
            Self::Array => Some("/items/0/path"),
            Self::Escaped => Some("/a~1b/~0key"),
            Self::Missing => Some("/absent"),
            Self::WrongType => Some("/typed"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopCondition {
    None,
    ChildError,
    ExecutorError,
    CancelRun,
    CancelProvider,
    InputChange,
    RevokeRead,
    AdmissionYield,
    CompleteYield,
    CompleteStop,
}
pub type Stop = StopCondition;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Defect {
    DuplicateId,
    ForwardDependency,
    MissingDependency,
    LiteralConflict,
    NullConflict,
    EmptyConflict,
    InvalidPointer,
    ForbiddenTool,
    UnknownArgument,
    EmptyPlan,
    TooManyNodes,
    AmbiguousRequest,
    WrongScope,
    WrongRevision,
    MissingProposal,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorDefect {
    Malformed,
    ForeignScope,
    ProposalHandle,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFault {
    Missing,
    Oversized,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let count = max_steps.clamp(1, 8);
    let node = (prop_oneof![Just(Tool::Read), Just(Tool::List), Just(Tool::Search)], prop_oneof![4 => Just(Reference::Literal), 3 => Just(Reference::Direct), 2 => Just(Reference::Array), 2 => Just(Reference::Escaped), 1 => Just(Reference::Missing), 1 => Just(Reference::WrongType)], 0_u8..16).prop_map(|(tool, reference, source)| NodeSpec {tool, reference, source});
    let stops = proptest::sample::select(vec![
        Stop::None,
        Stop::None,
        Stop::None,
        Stop::ChildError,
        Stop::ExecutorError,
        Stop::CancelRun,
        Stop::CancelProvider,
        Stop::InputChange,
        Stop::RevokeRead,
        Stop::AdmissionYield,
        Stop::CompleteYield,
        Stop::CompleteStop,
    ]);
    prop_oneof![
        10 => (any::<bool>(), proptest::collection::vec(node, 1..=count), 2048_u16..=4096, 0_u16..=1000, any::<bool>(), stops, 0_u8..8).prop_map(|(enabled,nodes,page_bytes,payload_units,reopen,stop,after)| Case::Plan{enabled,nodes,page_bytes,payload_units,reopen,stop,after}),
        2 => proptest::sample::select(vec![Defect::DuplicateId,Defect::ForwardDependency,Defect::MissingDependency,Defect::LiteralConflict,Defect::NullConflict,Defect::EmptyConflict,Defect::InvalidPointer,Defect::ForbiddenTool,Defect::UnknownArgument,Defect::EmptyPlan,Defect::TooManyNodes,Defect::AmbiguousRequest,Defect::WrongScope,Defect::WrongRevision,Defect::MissingProposal]).prop_map(|defect|Case::Invalid{defect}),
        1 => (proptest::sample::select(vec![CursorDefect::Malformed, CursorDefect::ForeignScope, CursorDefect::ProposalHandle]),any::<bool>()).prop_map(|(defect,reopen)|Case::Cursor{defect,reopen}),
        1 => (any::<bool>(),any::<bool>()).prop_map(|(missing,disable_execution)|Case::Archive{fault:if missing{ArchiveFault::Missing}else{ArchiveFault::Oversized},disable_execution}),
    ].boxed()
}

fn fail(message: impl Into<String>) -> DeveloperLoopError {
    DeveloperLoopError::Tool(message.into())
}
fn text(role: Role, body: &str) -> Result<Message, String> {
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
fn call(id: &str, name: &str, arguments: &Value) -> Result<CompletedToolCall, String> {
    CompletedToolCall::new(
        ToolCallId::new(id.to_owned()).map_err(|e| e.to_string())?,
        ToolName::new(name.to_owned()).map_err(|e| e.to_string())?,
        CanonicalJson::parse(&arguments.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn observe(value: &Value, is_error: bool) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    Ok(DeveloperToolObservation {
        output: CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?,
        is_error,
    })
}
fn profile() -> Result<ProviderProfile, String> {
    ProviderProfile::new(
        ProviderProfileId::new([73; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new("praxis".into()).map_err(|e| e.to_string())?,
        ModelName::new("local-plan-fixture".into()).map_err(|e| e.to_string())?,
        WireDialect::OpenAiResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::ParallelToolCalls], &[])
            .map_err(|e| e.to_string())?,
        CapabilityProvenance::Probed,
        ModelLimits::new(200_000, 8192, 64, 8, 1).map_err(|e| e.to_string())?,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|e| e.to_string())
}

#[derive(Default)]
struct Log {
    admitted: Vec<u32>,
    completed: Vec<u32>,
    bindings: BTreeMap<u32, String>,
    events: Vec<String>,
    executed: Vec<(String, Value)>,
    activated_stops: Vec<Stop>,
}
struct Control {
    revision: AtomicU64,
    read: AtomicBool,
    cancelled: Arc<AtomicBool>,
    provider: CancellationToken,
    stop: Stop,
    trigger: usize,
    log: Mutex<Log>,
}
impl Control {
    fn new(stop: Stop, trigger: usize) -> Self {
        Self {
            revision: AtomicU64::new(7),
            read: AtomicBool::new(true),
            cancelled: Arc::new(AtomicBool::new(false)),
            provider: CancellationToken::new(),
            stop,
            trigger,
            log: Mutex::new(Log::default()),
        }
    }
    fn after_execute(&self, index: usize) -> Result<(), DeveloperLoopError> {
        if index != self.trigger {
            return Ok(());
        }
        match self.stop {
            Stop::CancelRun => self.cancelled.store(true, Ordering::Release),
            Stop::CancelProvider => {
                let _ = self.provider.cancel();
            }
            Stop::InputChange => {
                self.revision.fetch_add(1, Ordering::AcqRel);
            }
            Stop::RevokeRead => self.read.store(false, Ordering::Release),
            _ => {}
        }
        if matches!(
            self.stop,
            Stop::CancelRun | Stop::CancelProvider | Stop::InputChange | Stop::RevokeRead
        ) {
            self.log.lock().map_err(|_| fail("log poisoned"))?.activated_stops.push(self.stop);
        }
        Ok(())
    }
}
impl ConversationView for Control {
    fn interaction(&self) -> Option<&dyn DeveloperInteraction> {
        Some(self)
    }
    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
    fn render(&self) -> String {
        "Read the disposable fixture files.".into()
    }
    fn effective_permissions(&self) -> HostPermissions {
        if self.read.load(Ordering::Acquire) {
            HostPermissions::all()
        } else {
            HostPermissions::all().without(PermissionCapability::Read)
        }
    }
}
impl DeveloperInteraction for Control {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        Ok(DeveloperInput {
            revision: self.revision(),
            conversation: self.render(),
            images: Vec::new(),
        })
    }
    fn prepare_request(
        &self,
        _revision: u64,
        _request: &ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        Err(fail("fixture never requests inference"))
    }
    fn admit_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
        input_revision: u64,
        effect: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        if role != DeveloperModelRole::Writer
            || effect != DeveloperToolEffect::ReadOnly
            || input_revision != self.revision()
        {
            return Err(fail("incorrect child admission binding"));
        }
        let mut log = self.log.lock().map_err(|_| fail("log poisoned"))?;
        log.admitted.push(sequence);
        log.bindings.insert(sequence, invocation.to_owned());
        if self.stop == Stop::AdmissionYield && sequence as usize == self.trigger + 1 {
            log.activated_stops.push(Stop::AdmissionYield);
            drop(log);
            Ok(DeveloperControlFlow::Yield)
        } else {
            drop(log);
            Ok(DeveloperControlFlow::Continue)
        }
    }
    fn complete_tool(
        &self,
        _role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        let mut log = self.log.lock().map_err(|_| fail("log poisoned"))?;
        if log.bindings.get(&sequence).map(String::as_str) != Some(invocation) {
            return Err(fail("completion changed the admitted invocation identity"));
        }
        log.completed.push(sequence);
        let control = if sequence as usize == self.trigger + 1 {
            match self.stop {
                Stop::CompleteYield => {
                    log.activated_stops.push(Stop::CompleteYield);
                    DeveloperControlFlow::Yield
                }
                Stop::CompleteStop => {
                    log.activated_stops.push(Stop::CompleteStop);
                    DeveloperControlFlow::Stop
                }
                _ => DeveloperControlFlow::Continue,
            }
        } else {
            DeveloperControlFlow::Continue
        };
        drop(log);
        Ok(control)
    }
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        let event = match activity {
            DeveloperActivity::ToolStarted { name, .. } => format!("start:{name}"),
            DeveloperActivity::ToolFinished { name, .. } => format!("finish:{name}"),
            _ => return Err(fail("unexpected plan activity")),
        };
        self.log.lock().map_err(|_| fail("log poisoned"))?.events.push(event);
        Ok(())
    }
}

struct Fixture {
    workspace: PathBuf,
    root: PathBuf,
    trace: PathBuf,
    binding: WorkingBinding,
    control: Arc<Control>,
    memory: Option<LocalContextHandle>,
    serial: usize,
    conservative: bool,
    // Drop the SQLite/artifact owner before attempting recursive temporary cleanup.
    _temporary: tempfile::TempDir,
}
impl Fixture {
    fn new(enabled: bool, stop: Stop, trigger: usize) -> Result<Self, String> {
        Self::with_decision(enabled, false, stop, trigger)
    }

    fn with_decision(
        enabled: bool,
        conservative: bool,
        stop: Stop,
        trigger: usize,
    ) -> Result<Self, String> {
        let temporary = tempfile::Builder::new()
            .prefix("peritus-praxis-plans-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let workspace = temporary.path().join("workspace");
        fs::create_dir(&workspace).map_err(|e| e.to_string())?;
        for index in 0..20 {
            fs::write(
                workspace.join(format!("fixture-{index}.txt")),
                format!("fixture {index}: μ🙂\n"),
            )
            .map_err(|e| e.to_string())?;
        }
        let root = temporary.path().join("memory");
        let trace = temporary.path().join("trace.bin");
        let binding = WorkingBinding::new(
            RunId::new([71; 16]).map_err(|e| format!("{e:?}"))?,
            WorkspaceId::new([72; 16]).map_err(|e| format!("{e:?}"))?,
            ContextNodeId::new([73; 16]).map_err(|e| format!("{e:?}"))?,
            HarnessRole::Writer,
            7,
        );
        let mut fixture = Self {
            _temporary: temporary,
            workspace,
            root,
            trace,
            binding,
            control: Arc::new(Control::new(stop, trigger)),
            memory: None,
            serial: 0,
            conservative,
        };
        let mut memory = fixture.load(enabled)?;
        let tools = super::definitions().map_err(|e| e.to_string())?;
        let request = DeveloperLoopRequest {
            request_prefix: "praxis-plan-invocation".into(),
            system: "Read-only fixture policy.".into(),
            prompt: "Inspect disposable fixture files.".into(),
            cyber_access_program: None,
            attachments: Vec::new(),
            tools: tools.clone(),
            limits: DeveloperLoopLimits::new(2, 64).map_err(|e| e.to_string())?,
            cancellation: fixture.control.provider.clone(),
        };
        memory
            .begin(
                &request,
                &[
                    text(Role::System, "Read-only fixture policy.")?,
                    text(Role::User, "Inspect disposable fixture files.")?,
                ],
            )
            .map_err(|e| e.to_string())?;
        let messages = memory.prepare_view(&profile()?, &tools).map_err(|e| e.to_string())?;
        memory.publish(&messages).map_err(|e| e.to_string())?;
        let conversation: Arc<dyn ConversationView> = fixture.control.clone();
        fixture.memory = Some(LocalContextHandle::praxis_from_memory(memory, conversation));
        Ok(fixture)
    }
    fn load(&self, enabled: bool) -> Result<LocalMemory, String> {
        let config = LocalContextConfig {
            decision_view: if self.conservative {
                DecisionViewPolicy::Conservative
            } else {
                DecisionViewPolicy::History
            },
            read_only_plans: if enabled {
                ReadOnlyPlanExecution::Sequential
            } else {
                ReadOnlyPlanExecution::Disabled
            },
            ..LocalContextConfig::default()
        };
        let mut memory = LocalMemory::load_scoped(
            &self.root,
            &self.workspace,
            &self.trace,
            self.binding,
            config,
            WorkspaceScope { direct: true, protected: Vec::new() },
        )
        .map_err(|e| e.to_string())?;
        let conversation: Arc<dyn ConversationView> = self.control.clone();
        memory.conversation = Some(conversation);
        memory.cancellation = Some((self.control.cancelled.clone(), self.control.provider.clone()));
        Ok(memory)
    }
    fn handle(&self) -> Result<&LocalContextHandle, String> {
        self.memory.as_ref().ok_or_else(|| "fixture memory closed".into())
    }
    fn reopen(&mut self, enabled: bool) -> Result<(), String> {
        drop(self.memory.take());
        let memory = self.load(enabled)?;
        let conversation: Arc<dyn ConversationView> = self.control.clone();
        self.memory = Some(LocalContextHandle::praxis_from_memory(memory, conversation));
        Ok(())
    }
    fn invoke(
        &mut self,
        name: &str,
        args: &Value,
        base: &mut RecordingExecutor,
        host_calls: &mut u32,
        propose: bool,
    ) -> Result<DeveloperToolObservation, String> {
        self.serial += 1;
        let parent = call(&format!("praxis_parent_{}", self.serial), name, args)?;
        if propose {
            let message = Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolCall(parent.clone())],
                ProtocolLimits::PRODUCTION,
            )
            .map_err(|e| e.to_string())?;
            self.handle()?
                .lock()
                .map_err(|e| e.to_string())?
                .observe_message(&message)
                .map_err(|e| e.to_string())?;
        }
        let result = super::execute(
            self.handle()?,
            base,
            Some(DeveloperModelRole::Writer),
            &parent,
            host_calls,
        )
        .map_err(|e| e.to_string())?;
        if propose {
            self.handle()?
                .lock()
                .map_err(|e| e.to_string())?
                .observe_tool(&parent, &result)
                .map_err(|e| e.to_string())?;
        }
        Ok(result)
    }
    fn request(
        &self,
        nodes: Option<&Value>,
        cursor: Option<&str>,
        page_bytes: u16,
    ) -> Result<Value, String> {
        let owner = self.handle()?.lock().map_err(|e| e.to_string())?;
        let scope = hex(owner.store.scope_digest().as_bytes());
        drop(owner);
        Ok(
            json!({"version":1,"scope":scope,"input_revision":self.control.revision(),"plan":nodes,"cursor":cursor,"max_result_bytes":page_bytes}),
        )
    }
}

fn output(index: usize, payload_units: u16) -> Value {
    let next = format!("fixture-{}.txt", (index + 1) % 20);
    json!({"path":format!("fixture-{}.txt",index%20),"next_path":next,"items":[{"path":next}],"a/b":{"~key":next},"typed":17,"content":"μ🙂 line\n".repeat(usize::from(payload_units))})
}
struct RecordingExecutor {
    control: Arc<Control>,
    outputs: Vec<Value>,
}
impl RecordingExecutor {
    fn new(control: Arc<Control>, count: usize, payload: u16) -> Self {
        Self { control, outputs: (0..count).map(|i| output(i, payload)).collect() }
    }
}
impl DeveloperToolExecutor for RecordingExecutor {
    fn effect(&self, _call: &CompletedToolCall) -> DeveloperToolEffect {
        DeveloperToolEffect::ReadOnly
    }
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        crate::developer_tools::validate_inspection_plan_arguments(
            call.name().as_str(),
            &serde_json::from_slice::<Value>(call.arguments().canonical_bytes())
                .map_err(|_| fail("invalid fixture arguments"))?,
            &[],
        )?;
        let mut log = self.control.log.lock().map_err(|_| fail("log poisoned"))?;
        let index = log.executed.len();
        let sequence = child_sequence(index)?;
        if log.admitted.last().copied() != Some(sequence) || log.completed.len() != index {
            return Err(fail("child executed without fresh admission/completion ordering"));
        }
        if !self.control.read.load(Ordering::Acquire)
            || self.control.cancelled.load(Ordering::Acquire)
            || self.control.provider.is_cancelled()
        {
            return Err(fail("ordinary executor observed revoked authority"));
        }
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|_| fail("invalid fixture arguments"))?;
        log.executed.push((call.name().as_str().into(), arguments));
        drop(log);
        self.control.after_execute(index)?;
        if index == self.control.trigger && self.control.stop == Stop::ExecutorError {
            self.control
                .log
                .lock()
                .map_err(|_| fail("log poisoned"))?
                .activated_stops
                .push(Stop::ExecutorError);
            return Err(fail("fixture dispatch failure"));
        }
        let value =
            self.outputs.get(index).ok_or_else(|| fail("unexpected extra child dispatch"))?;
        let failed = index == self.control.trigger && self.control.stop == Stop::ChildError;
        if failed {
            self.control
                .log
                .lock()
                .map_err(|_| fail("log poisoned"))?
                .activated_stops
                .push(Stop::ChildError);
        }
        observe(value, failed)
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len().saturating_mul(2)), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
fn child_sequence(index: usize) -> Result<u32, DeveloperLoopError> {
    u32::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or_else(|| fail("fixture child ordinal exceeds u32"))
}
fn bump(metrics: &mut Metrics, name: &str, n: u64) {
    *metrics.entry(name.into()).or_default() += n;
}

/// Tiny independent JSON Pointer interpretation; no production resolver or `Value::pointer` call.
fn pointer<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for token in path.strip_prefix('/')?.split('/') {
        let mut key = String::new();
        let mut characters = token.chars();
        while let Some(c) = characters.next() {
            if c == '~' {
                key.push(match characters.next()? {
                    '0' => '~',
                    '1' => '/',
                    _ => return None,
                });
            } else {
                key.push(c);
            }
        }
        current = match current {
            Value::Object(map) => map.get(&key)?,
            Value::Array(items) => {
                if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
                    return None;
                }
                items.get(key.parse::<usize>().ok()?)?
            }
            _ => return None,
        };
    }
    Some(current)
}

fn nodes(specifications: &[NodeSpec]) -> Value {
    Value::Array(specifications.iter().enumerate().map(|(index,node)|{
    let mut arguments=json!({});if matches!(node.tool,Tool::Search){arguments["query"]=json!("fixture");}
    let reference=if index==0{None}else{node.reference.pointer()};
    if let Some(pointer)=reference{let source=usize::from(node.source)%index;json!({"id":format!("n{index}"),"tool":node.tool.name(),"arguments":arguments,"depends_on":[format!("n{source}")],"result_refs":[{"argument":"path","node":format!("n{source}"),"pointer":pointer}]})}
    else{arguments["path"]=json!(format!("fixture-{}.txt",index%20));json!({"id":format!("n{index}"),"tool":node.tool.name(),"arguments":arguments,"depends_on":[],"result_refs":[]})}
}).collect())
}

#[derive(Debug)]
struct Expected {
    calls: Vec<(String, Value)>,
    outputs: Vec<(Value, bool)>,
    status: &'static str,
    admission_attempts: usize,
}
fn interpret(specifications: &[NodeSpec], payload: u16, stop: Stop, trigger: usize) -> Expected {
    let mut expected = Expected {
        calls: Vec::new(),
        outputs: Vec::new(),
        status: "completed",
        admission_attempts: 0,
    };
    for (index, node) in specifications.iter().enumerate() {
        let mut arguments = json!({});
        if matches!(node.tool, Tool::Search) {
            arguments["query"] = json!("fixture");
        }
        let path = if index == 0 || matches!(node.reference, Reference::Literal) {
            Some(json!(format!("fixture-{}.txt", index % 20)))
        } else {
            let source = usize::from(node.source) % index;
            expected
                .outputs
                .get(source)
                .filter(|(_, failed)| !failed)
                .and_then(|(value, _)| pointer(value, node.reference.pointer()?))
                .cloned()
        };
        let Some(Value::String(path)) = path else {
            expected.status = "needs_judgment";
            break;
        };
        arguments["path"] = json!(path);
        expected.admission_attempts += 1;
        if stop == Stop::AdmissionYield && index == trigger {
            expected.status = "yielded";
            break;
        }
        expected.calls.push((node.tool.name().into(), arguments));
        let failed = index == trigger && matches!(stop, Stop::ChildError | Stop::ExecutorError);
        let value = if index == trigger && stop == Stop::ExecutorError {
            json!({"error":"execute developer tool: fixture dispatch failure"})
        } else {
            output(index, payload)
        };
        expected.outputs.push((value, failed));
        if index == trigger && !matches!(stop, Stop::None | Stop::AdmissionYield) {
            expected.status = if failed { "needs_judgment" } else { "yielded" };
            break;
        }
    }
    expected
}

fn direct_equivalence(
    expected: &Expected,
    stop: Stop,
    trigger: usize,
    payload: u16,
) -> Result<(), String> {
    let control = Arc::new(Control::new(stop, trigger));
    let mut executor = RecordingExecutor::new(control.clone(), expected.calls.len(), payload);
    for (index, (name, arguments)) in expected.calls.iter().enumerate() {
        let sequence = child_sequence(index).map_err(|error| error.to_string())?;
        if control
            .admit_tool(
                DeveloperModelRole::Writer,
                "direct-sequential-fixture",
                sequence,
                7,
                DeveloperToolEffect::ReadOnly,
            )
            .map_err(|e| e.to_string())?
            != DeveloperControlFlow::Continue
        {
            return Err("independent sequential fixture rejected an expected ordinary call".into());
        }
        let call = call(&format!("direct_{index}"), name, arguments)?;
        let observation = match executor.execute(&call) {
            Ok(observation) => observation,
            Err(error) => {
                observe(&json!({"error":error.to_string()}), true).map_err(|e| e.to_string())?
            }
        };
        let (value, failed) = &expected.outputs[index];
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if observation.output.canonical_bytes() != bytes || observation.is_error != *failed {
            return Err(
                "ordinary sequential execution differed from independent fixture output".into()
            );
        }
        control
            .complete_tool(DeveloperModelRole::Writer, "direct-sequential-fixture", sequence)
            .map_err(|e| e.to_string())?;
    }
    if control.log.lock().map_err(|_| "log poisoned")?.executed != expected.calls {
        return Err(
            "ordinary sequential calls differed from independently resolved arguments".into()
        );
    }
    Ok(())
}

fn retrieve(
    fixture: &mut Fixture,
    base: &mut RecordingExecutor,
    handle: &str,
    expected: &Value,
    metrics: &mut Metrics,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(expected).map_err(|e| e.to_string())?;
    let mut reconstructed = Vec::new();
    let mut offset = 0_u64;
    let mut calls = 0;
    for page in 0..=bytes.len() / 256 + 1 {
        let maximum = [256, 257, 513, 65_536][page % 4];
        let result = fixture.invoke(
            "inspection_result_read",
            &json!({"step":handle,"offset":offset,"max_bytes":maximum}),
            base,
            &mut calls,
            true,
        )?;
        if result.is_error {
            return Err("exact archived result read was rejected".into());
        }
        let value: Value =
            serde_json::from_slice(result.output.canonical_bytes()).map_err(|e| e.to_string())?;
        let data = value["data"].as_str().ok_or("missing result data")?;
        if value["source_bytes"] != json!(bytes.len())
            || value["source_sha256"] != json!(hex(peritus_codec::sha256(&bytes).as_bytes()))
            || value["offset"] != json!(offset)
            || value["returned_bytes"] != json!(data.len())
            || data.len() > maximum
        {
            return Err("exact result metadata disagrees with independent bytes".into());
        }
        reconstructed.extend_from_slice(data.as_bytes());
        bump(metrics, "result_pages", 1);
        if value["next_offset"].is_null() {
            break;
        }
        let next = value["next_offset"].as_u64().ok_or("invalid result cursor")?;
        if next <= offset {
            return Err("result pagination made no progress".into());
        }
        offset = next;
    }
    if reconstructed != bytes {
        return Err("exact JSON result pages did not reconstruct original bytes".into());
    }
    if calls != 0 {
        return Err("result retrieval dispatched ordinary child tools".into());
    }
    bump(metrics, "exact_results_reconstructed", 1);
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    match case {
        Case::Plan {
            enabled,
            nodes: specifications,
            page_bytes,
            payload_units,
            reopen,
            stop,
            after,
        } => check_plan(
            *enabled,
            specifications,
            *page_bytes,
            *payload_units,
            *reopen,
            *stop,
            *after,
        ),
        Case::Invalid { defect } => check_invalid(*defect),
        Case::Cursor { defect, reopen } => check_cursor(*defect, *reopen),
        Case::Archive { fault, disable_execution } => check_archive(*fault, *disable_execution),
    }
}

fn validate_page(
    value: &Value,
    specifications: &[NodeSpec],
    expected: &Expected,
    handles: &mut Vec<String>,
) -> Result<(), String> {
    for row in value["steps"].as_array().ok_or("plan response omitted steps")? {
        let index = handles.len();
        let (output, failed) =
            expected.outputs.get(index).ok_or("plan committed an unexpected extra child")?;
        let bytes = serde_json::to_vec(output).map_err(|e| e.to_string())?;
        if row["id"] != json!(format!("n{index}"))
            || row["tool"] != json!(specifications[index].tool.name())
            || row["is_error"] != json!(failed)
            || row["source_bytes"] != json!(bytes.len())
            || row["source_sha256"] != json!(hex(peritus_codec::sha256(&bytes).as_bytes()))
        {
            return Err(format!("committed child metadata mismatch at {index}: {row}"));
        }
        if let Some(inline) = row.get("output")
            && inline != output
        {
            return Err("inline child result differed from exact fixture result".into());
        }
        handles.push(row["step"].as_str().ok_or("child result handle missing")?.to_owned());
    }
    if value["completed_nodes"] != json!(handles.len())
        || value["total_nodes"] != json!(specifications.len())
        || value["authority"] != "none"
    {
        return Err("plan page progress or authority metadata was incorrect".into());
    }
    Ok(())
}

fn validate_execution_log(
    fixture: &Fixture,
    expected: &Expected,
    host_calls: u32,
    stop: Stop,
    trigger: usize,
) -> Result<bool, String> {
    let stop_reached = match stop {
        Stop::None => false,
        Stop::AdmissionYield => expected.admission_attempts > trigger,
        _ => expected.calls.len() > trigger,
    };
    let expected_stop_events = if stop_reached { vec![stop] } else { Vec::new() };
    let completed = u32::try_from(expected.calls.len()).map_err(|error| error.to_string())?;
    let admitted = u32::try_from(expected.admission_attempts).map_err(|error| error.to_string())?;
    let expected_events = expected
        .calls
        .iter()
        .flat_map(|(name, _)| [format!("start:{name}"), format!("finish:{name}")])
        .collect::<Vec<_>>();
    let log = fixture.control.log.lock().map_err(|_| "log poisoned")?;
    if log.activated_stops != expected_stop_events {
        return Err(format!(
            "actual stop events {:?}, expected {expected_stop_events:?}",
            log.activated_stops
        ));
    }
    if log.executed != expected.calls
        || log.completed != (1..=completed).collect::<Vec<_>>()
        || log.admitted != (1..=admitted).collect::<Vec<_>>()
        || host_calls as usize != expected.calls.len()
    {
        return Err(format!(
            "actual admission/execution/completion counts {}/{}/{}/{host_calls}, expected {}/{}",
            log.admitted.len(),
            log.executed.len(),
            log.completed.len(),
            expected.admission_attempts,
            expected.calls.len()
        ));
    }
    if log.events != expected_events {
        return Err("missing per-child public start/finish events".into());
    }
    drop(log);
    Ok(stop_reached)
}

fn check_plan(
    enabled: bool,
    specifications: &[NodeSpec],
    page_bytes: u16,
    payload: u16,
    reopen: bool,
    stop: Stop,
    after: u8,
) -> Result<Metrics, String> {
    if specifications.is_empty()
        || specifications.len() > 16
        || !(2048..=65535).contains(&page_bytes)
        || payload > 2000
    {
        return Err("invalid replay bounds".into());
    }
    let trigger = usize::from(after) % specifications.len();
    let expected = interpret(specifications, payload, stop, trigger);
    let mut fixture = Fixture::new(enabled, stop, trigger)?;
    let mut base = RecordingExecutor::new(fixture.control.clone(), specifications.len(), payload);
    let mut host_calls = 0;
    let mut metrics = Metrics::new();
    let graph = nodes(specifications);
    let mut request = fixture.request(Some(&graph), None, page_bytes)?;
    let mut handles = Vec::new();
    let mut final_status = None;
    for page in 0..=specifications.len() {
        let result =
            fixture.invoke("inspection_plan", &request, &mut base, &mut host_calls, true)?;
        let value: Value =
            serde_json::from_slice(result.output.canonical_bytes()).map_err(|e| e.to_string())?;
        if !enabled {
            if !result.is_error || value["status"] != "not_executed" || host_calls != 0 {
                return Err("disabled plans did not reject before dispatch".into());
            }
            let log = fixture.control.log.lock().map_err(|_| "log poisoned")?;
            if !log.executed.is_empty() || !log.admitted.is_empty() {
                return Err("disabled plans reached ordinary child admission or execution".into());
            }
            drop(log);
            bump(&mut metrics, "disabled_rejections", 1);
            return Ok(metrics);
        }
        let status = value["status"].as_str().ok_or("missing plan status")?;
        if result.is_error != matches!(status, "needs_judgment" | "yielded") {
            return Err(format!(
                "parent plan error flag {} disagrees with status {status}",
                result.is_error
            ));
        }
        if result.output.canonical_bytes().len() > usize::from(page_bytes) {
            return Err("plan response exceeded requested page bytes".into());
        }
        validate_page(&value, specifications, &expected, &mut handles)?;
        bump(&mut metrics, "plan_pages", 1);
        if status != "page_complete" {
            final_status = Some(status.to_owned());
            break;
        }
        let cursor =
            value["continuation"].as_str().ok_or("paged plan lost its continuation")?.to_owned();
        if reopen {
            fixture.reopen(true)?;
            bump(&mut metrics, "reopened_pages", 1);
        }
        request = fixture.request(None, Some(&cursor), page_bytes)?;
        if page == specifications.len() {
            return Err("plan continuation exceeded node progress bound".into());
        }
    }
    if final_status.as_deref() != Some(expected.status) {
        return Err(format!("final status {final_status:?}, expected {}", expected.status));
    }
    let stop_reached = validate_execution_log(&fixture, &expected, host_calls, stop, trigger)?;
    direct_equivalence(&expected, stop, trigger, payload)?;
    if !fixture.control.read.load(Ordering::Acquire) {
        let rejected=fixture.invoke("inspection_result_read",&json!({"step":handles.first().ok_or("revocation needs one result")?,"offset":0,"max_bytes":256}),&mut base,&mut host_calls,true)?;
        if !rejected.is_error {
            return Err("result read ignored current read revocation".into());
        }
        fixture.control.read.store(true, Ordering::Release);
        bump(&mut metrics, "revoked_result_reads_rejected", 1);
    }
    for (index, handle) in handles.iter().enumerate() {
        retrieve(&mut fixture, &mut base, handle, &expected.outputs[index].0, &mut metrics)?;
    }
    bump(&mut metrics, "enabled_plans", 1);
    bump(&mut metrics, "fresh_admissions", expected.calls.len() as u64);
    bump(&mut metrics, "ordinary_equivalent_calls", expected.calls.len() as u64);
    bump(&mut metrics, &format!("requested_stop_{stop:?}"), 1);
    if stop_reached {
        bump(&mut metrics, &format!("stop_{stop:?}"), 1);
    } else if stop != Stop::None {
        bump(&mut metrics, "stop_trigger_not_reached", 1);
    } else {
        bump(&mut metrics, "no_stop_plans", 1);
    }
    Ok(metrics)
}

fn basic_nodes(count: usize) -> Vec<NodeSpec> {
    (0..count)
        .map(|_| NodeSpec { tool: Tool::Read, reference: Reference::Literal, source: 0 })
        .collect()
}

fn check_invalid(defect: Defect) -> Result<Metrics, String> {
    let mut fixture = Fixture::new(true, Stop::None, 0)?;
    let mut base = RecordingExecutor::new(fixture.control.clone(), 2, 1);
    let mut graph = nodes(&basic_nodes(2));
    match defect {
        Defect::DuplicateId => graph[1]["id"] = graph[0]["id"].clone(),
        Defect::ForwardDependency => graph[0]["depends_on"] = json!(["n1"]),
        Defect::MissingDependency => {
            graph[1]["arguments"] = json!({});
            graph[1]["result_refs"] =
                json!([{"argument":"path","node":"n0","pointer":"/next_path"}]);
        }
        Defect::LiteralConflict
        | Defect::NullConflict
        | Defect::EmptyConflict
        | Defect::InvalidPointer => {
            graph[1]["depends_on"] = json!(["n0"]);
            graph[1]["result_refs"] = json!([{"argument":"path","node":"n0","pointer":if matches!(defect,Defect::InvalidPointer){"/bad~2escape"}else{"/next_path"}}]);
            graph[1]["arguments"] = match defect {
                Defect::LiteralConflict => json!({"path":"fixture-1.txt"}),
                Defect::NullConflict => json!({"path":null}),
                Defect::EmptyConflict => json!({"path":""}),
                _ => json!({}),
            };
        }
        Defect::ForbiddenTool => graph[0]["tool"] = json!("workspace_write"),
        Defect::UnknownArgument => graph[0]["arguments"]["undeclared"] = json!(true),
        Defect::EmptyPlan => graph = json!([]),
        Defect::TooManyNodes => graph = nodes(&basic_nodes(65)),
        _ => {}
    }
    let mut request = fixture.request(Some(&graph), None, 2048)?;
    match defect {
        Defect::AmbiguousRequest => request["cursor"] = json!("obs:1"),
        Defect::WrongScope => request["scope"] = json!("foreign"),
        Defect::WrongRevision => request["input_revision"] = json!(999),
        _ => {}
    }
    let result = fixture.invoke(
        "inspection_plan",
        &request,
        &mut base,
        &mut 0,
        !matches!(defect, Defect::MissingProposal),
    )?;
    let value: Value =
        serde_json::from_slice(result.output.canonical_bytes()).map_err(|e| e.to_string())?;
    if !result.is_error
        || value["status"] != "not_executed"
        || !fixture.control.log.lock().map_err(|_| "log poisoned")?.executed.is_empty()
    {
        return Err(format!("invalid {defect:?} plan reached child execution"));
    }
    Ok(BTreeMap::from([(format!("invalid_{defect:?}"), 1), ("invalid_plans_rejected".into(), 1)]))
}

fn first_page(fixture: &mut Fixture, base: &mut RecordingExecutor) -> Result<Value, String> {
    let graph = nodes(&basic_nodes(4));
    let request = fixture.request(Some(&graph), None, 2048)?;
    let result = fixture.invoke("inspection_plan", &request, base, &mut 0, true)?;
    serde_json::from_slice(result.output.canonical_bytes()).map_err(|e| e.to_string())
}
fn check_cursor(defect: CursorDefect, reopen: bool) -> Result<Metrics, String> {
    let mut fixture = Fixture::new(true, Stop::None, 0)?;
    let mut base = RecordingExecutor::new(fixture.control.clone(), 4, 1000);
    let first = first_page(&mut fixture, &mut base)?;
    let mut cursor =
        first["continuation"].as_str().ok_or("forced first page needs continuation")?.to_owned();
    match defect {
        CursorDefect::Malformed => cursor = "not-a-cursor".into(),
        CursorDefect::ForeignScope => {
            let tail = cursor.rsplit(':').next().ok_or("invalid fixture handle")?;
            cursor = format!("obs:{}:{tail}", "0".repeat(64));
        }
        CursorDefect::ProposalHandle => {
            cursor = first["proposal"].as_str().ok_or("proposal handle missing")?.into();
        }
    }
    if reopen {
        fixture.reopen(true)?;
    }
    let before = fixture.control.log.lock().map_err(|_| "log poisoned")?.executed.len();
    let request = fixture.request(None, Some(&cursor), 2048)?;
    let result = fixture.invoke("inspection_plan", &request, &mut base, &mut 0, true)?;
    if !result.is_error
        || fixture.control.log.lock().map_err(|_| "log poisoned")?.executed.len() != before
    {
        return Err("invalid cursor dispatched another child".into());
    }
    Ok(BTreeMap::from([(format!("cursor_{defect:?}"), 1), ("invalid_cursors_rejected".into(), 1)]))
}

fn check_archive(fault: ArchiveFault, disable_execution: bool) -> Result<Metrics, String> {
    let mut fixture = Fixture::new(true, Stop::None, 0)?;
    let mut base = RecordingExecutor::new(fixture.control.clone(), 4, 1000);
    let first = first_page(&mut fixture, &mut base)?;
    let step =
        first["steps"][0]["step"].as_str().ok_or("forced plan needs first result")?.to_owned();
    let bad_handle = {
        let mut owner = fixture.handle()?.lock().map_err(|e| e.to_string())?;
        let id = crate::local_context::tools::sequence(&owner, &step).map_err(|e| e.to_string())?;
        let super::PlanRecord::Step {
            proposal,
            parent,
            input_revision,
            index,
            arguments,
            mut output,
            is_error,
            dependencies,
        } = super::records::load(&owner, id).map_err(|e| e.to_string())?
        else {
            return Err("executed child lacked durable receipt".into());
        };
        match fault {
            ArchiveFault::Missing => {
                output = StoredArtifact { digest: Sha256Digest::new([255; 32]), bytes: 1 }
            }
            ArchiveFault::Oversized => output.bytes = u64::MAX,
        }
        let bad = super::records::archive(
            &mut owner,
            super::PlanRecord::Step {
                proposal,
                parent,
                input_revision,
                index,
                arguments,
                output,
                is_error,
                dependencies,
            },
            &[],
        )
        .map_err(|e| e.to_string())?;
        let handle = crate::local_context::tools::source_handle(&owner, bad);
        drop(owner);
        handle
    };
    if disable_execution {
        fixture.reopen(false)?;
    }
    let result = fixture.invoke(
        "inspection_result_read",
        &json!({"step":bad_handle,"offset":0,"max_bytes":256}),
        &mut base,
        &mut 0,
        true,
    );
    if result.is_ok_and(|observation| !observation.is_error) {
        return Err("missing or oversized archived result was accepted".into());
    }
    // The genuine exact result remains readable when plan execution is disabled.
    let mut metrics = Metrics::new();
    retrieve(&mut fixture, &mut base, &step, &output(0, 1000), &mut metrics)?;
    bump(&mut metrics, &format!("archive_{fault:?}_rejected"), 1);
    if disable_execution {
        bump(&mut metrics, "disabled_execution_exact_reads", 1);
    }
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    let mut cases = vec![calibration_case()];
    // A reference failure before the selected control boundary must not count as an activated stop.
    for stop in [Stop::RevokeRead, Stop::AdmissionYield] {
        let mut graph = basic_nodes(3);
        graph[1].reference = Reference::Missing;
        cases.push(Case::Plan {
            enabled: true,
            nodes: graph,
            page_bytes: 4096,
            payload_units: 1,
            reopen: false,
            stop,
            after: 2,
        });
    }
    for enabled in [false, true] {
        for stop in [
            Stop::None,
            Stop::ChildError,
            Stop::ExecutorError,
            Stop::CancelRun,
            Stop::CancelProvider,
            Stop::InputChange,
            Stop::RevokeRead,
            Stop::AdmissionYield,
            Stop::CompleteYield,
            Stop::CompleteStop,
        ] {
            cases.push(Case::Plan {
                enabled,
                nodes: basic_nodes(4),
                page_bytes: 2048,
                payload_units: 500,
                reopen: true,
                stop,
                after: 1,
            });
        }
    }
    for reference in [
        Reference::Direct,
        Reference::Array,
        Reference::Escaped,
        Reference::Missing,
        Reference::WrongType,
    ] {
        let mut graph = basic_nodes(4);
        graph[1].tool = Tool::List;
        graph[2].tool = Tool::Search;
        graph[1].reference = reference;
        graph[2].reference = reference;
        cases.push(Case::Plan {
            enabled: true,
            nodes: graph,
            page_bytes: 2048,
            payload_units: 250,
            reopen: true,
            stop: Stop::None,
            after: 0,
        });
    }
    for defect in [
        Defect::DuplicateId,
        Defect::ForwardDependency,
        Defect::MissingDependency,
        Defect::LiteralConflict,
        Defect::NullConflict,
        Defect::EmptyConflict,
        Defect::InvalidPointer,
        Defect::ForbiddenTool,
        Defect::UnknownArgument,
        Defect::EmptyPlan,
        Defect::TooManyNodes,
        Defect::AmbiguousRequest,
        Defect::WrongScope,
        Defect::WrongRevision,
        Defect::MissingProposal,
    ] {
        cases.push(Case::Invalid { defect });
    }
    for defect in
        [CursorDefect::Malformed, CursorDefect::ForeignScope, CursorDefect::ProposalHandle]
    {
        cases.push(Case::Cursor { defect, reopen: true });
    }
    for fault in [ArchiveFault::Missing, ArchiveFault::Oversized] {
        for disable_execution in [false, true] {
            cases.push(Case::Archive { fault, disable_execution });
        }
    }
    cases
}
pub fn calibration_case() -> Case {
    Case::Plan {
        enabled: true,
        nodes: basic_nodes(3),
        page_bytes: 4096,
        payload_units: 1,
        reopen: false,
        stop: Stop::RevokeRead,
        after: 0,
    }
}

#[test]
fn praxis_stage2_plans() {
    crate::local_context::praxis_support::run("plans", strategy, check, examples, calibration_case);
}

#[cfg(test)]
#[path = "stage8_context_flow.rs"]
mod stage8_context_flow;
