//! Crash/reopen discovery against the public durable AgentDriver and real C4 dispatch.

use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use peritus_agent::{
    ActivePhase, AgentBinding, AgentBudgetPlan, AgentBudgetPort, AgentBudgetPortError,
    AgentBudgetReservation, AgentCommandKind, AgentDriver, AgentFailure, AgentFailureKind,
    AgentLimits, AgentPhase, CompletionProposal, CompletionRequest, ContextRecord, ModelCallId,
    ProfileRevision, ProviderAdvance, ProviderRetryClass, ProviderRetryRecord, SafeText,
    TerminalKind, ToolBatchCoordinator, ToolInvocationPlan, ToolOrdinal, ToolResultStatus,
    ToolSlotPhase, TranscriptDigests, TransitionIdentity, load_agent_replay,
};
use peritus_budget::{BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetReceipt};
use peritus_codec::CodecLimits;
use peritus_journal::SqliteJournal;
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, CanonicalJson, Capability, CapabilityMatrix,
    CapabilityProvenance, CompletedToolCall, ContentBlock, EventEnvelope, FinishReason,
    GenerationConfig, ItemId, ItemKind, JsonBounds, Message, ModelEvent, ModelLimits, ModelName,
    ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ProviderName, ProviderProfile, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResumeKind, Role, SchemaDialect, StateMode, StreamFragment,
    StructuredOutput, ToolCallId, ToolChoice, ToolDefinition, ToolName, WireDialect, negotiate,
};
use peritus_policy::{
    ActorRole, CapabilityScope, OperationClass, OperationDescriptor, OperationRegistry, Permission,
    PermissionSet, RiskClass, RiskSet, UseLimit, ValidityWindow,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
    validate_request_profile,
};
use peritus_role::{HarnessRole, RoleProfile};
use peritus_tool_protocol::{
    BoundedJson, BoundedText as ToolText, CallLimits, ControlSet, IdempotencyKey,
    IdempotencySemantics, ImplementationIdentity, JsonLimits, LeaseRequirement, PreparedToolCall,
    ProtocolCompatibility, Schema, SchemaDigest, SemanticVersion, SideEffectClass, ToolDescriptor,
    ToolLimits, ToolResult, ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, ExecutionUpdate, RecoveryObservation, RouterLimits,
    ToolDispatcher, ToolExecution, ToolRegistry, ToolRouter, ToolStart,
};
use peritus_types::{BudgetId, BudgetReservationId, RevisionNumber, RunId, Sha256Digest};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[path = "agent_driver/stage3_driver_authority.rs"]
mod authority;
#[path = "agent_driver/stage3_driver_contract.rs"]
mod contract;
#[path = "agent_driver/stage3_driver_identity.rs"]
mod identity;
#[path = "agent_driver/stage3_driver_journal.rs"]
mod journal;
#[path = "agent_driver/stage3_driver_kernel.rs"]
mod kernel;

use identity::CommandIds;

#[path = "stage6/fs_dispatch.rs"]
pub mod stage6_fs_dispatch;
#[path = "stage6/kernel.rs"]
pub mod stage6_kernel;
#[path = "stage6/router.rs"]
pub mod stage6_router;
pub type Metrics = BTreeMap<String, u64>;

/// Each cut is an independently specified external observation boundary, not a reducer command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cut {
    ProviderIntent,
    ProviderPrefix,
    Authorized,
    BeforeEffect,
    AfterEffect,
    Active,
    Completed,
    Recording,
    ResultsRecorded,
    CompletionProposed,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ending {
    Complete,
    Cancel,
    Fail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub tools: u8,
    pub retries: u8,
    pub completed_before_cut: u8,
    pub cut: Cut,
    pub ending: Ending,
    pub pause: bool,
    pub reopens: u8,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let width = u8::try_from(max_steps.clamp(1, 3)).unwrap_or(3);
    (
        1..=width,
        0_u8..=2,
        0_u8..3,
        prop_oneof![
            Just(Cut::ProviderIntent),
            Just(Cut::ProviderPrefix),
            Just(Cut::Authorized),
            Just(Cut::BeforeEffect),
            Just(Cut::AfterEffect),
            Just(Cut::Active),
            Just(Cut::Completed),
            Just(Cut::Recording),
            Just(Cut::ResultsRecorded),
            Just(Cut::CompletionProposed),
            Just(Cut::Terminal),
        ],
        prop_oneof![Just(Ending::Complete), Just(Ending::Cancel), Just(Ending::Fail)],
        any::<bool>(),
        1_u8..=3,
    )
        .prop_map(|(tools, retries, before, cut, ending, pause, reopens)| Case {
            tools,
            retries,
            completed_before_cut: before % tools,
            cut,
            ending,
            pause,
            reopens,
        })
        .boxed()
}

pub fn examples() -> Vec<Case> {
    let mut cases = Vec::new();
    for cut in [
        Cut::ProviderIntent,
        Cut::ProviderPrefix,
        Cut::Authorized,
        Cut::BeforeEffect,
        Cut::AfterEffect,
        Cut::Active,
        Cut::Completed,
        Cut::Recording,
        Cut::ResultsRecorded,
        Cut::CompletionProposed,
        Cut::Terminal,
    ] {
        cases.push(Case {
            tools: 3,
            retries: 1,
            completed_before_cut: 1,
            cut,
            ending: Ending::Complete,
            pause: true,
            reopens: 2,
        });
    }
    for ending in [Ending::Cancel, Ending::Fail] {
        cases.push(Case {
            tools: 1,
            retries: 2,
            completed_before_cut: 0,
            cut: Cut::Terminal,
            ending,
            pause: false,
            reopens: 3,
        });
    }
    cases.push(calibration_case());
    cases
}

pub fn calibration_case() -> Case {
    Case {
        tools: 1,
        retries: 0,
        completed_before_cut: 0,
        cut: Cut::AfterEffect,
        ending: Ending::Complete,
        pause: false,
        reopens: 1,
    }
}

fn validate(case: &Case) -> Result<(), String> {
    if !(1..=3).contains(&case.tools)
        || case.retries > 2
        || case.completed_before_cut >= case.tools
        || !(1..=3).contains(&case.reopens)
    {
        return Err("agent-driver case exceeds bounded fixture capacities".into());
    }
    Ok(())
}

fn root_ids() -> Result<(peritus_spec::AcceptanceContract, CommandIds), String> {
    let run = RunId::new([93; 16]).map_err(debug)?;
    let acceptance = contract::command_contract(run, 0)?;
    let ids = CommandIds::new(run, 0, &acceptance)?;
    Ok((acceptance, ids))
}

fn binding(ids: &CommandIds) -> Result<AgentBinding, String> {
    AgentBinding::new(
        ids.turn,
        ids.attempt,
        ids.actor,
        ActorRole::Writer,
        RoleProfile::for_harness_role(HarnessRole::Writer),
        ids.session,
        ids.environment,
        ids.revision,
        ids.revision.provider_profile_id(),
        ProfileRevision::new(1).map_err(debug)?,
        RevisionNumber::first(),
    )
    .map_err(debug)
}

fn limits() -> Result<AgentLimits, String> {
    AgentLimits::new(12, 128, 8, 1_048_576, 1_048_576, 3, 256).map_err(debug)
}

fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
fn digest(label: &str) -> Sha256Digest {
    peritus_codec::sha256(label.as_bytes())
}
fn text(value: &str) -> Result<SafeText, String> {
    SafeText::new(value.into()).map_err(debug)
}

fn transition(ids: &CommandIds, label: &str) -> Result<TransitionIdentity, String> {
    Ok(TransitionIdentity::new(ids.command(label)?, ids.event(label)?))
}

fn drive(
    driver: &mut AgentDriver,
    store: &mut SqliteJournal,
    ids: &CommandIds,
    command: AgentCommandKind,
) -> Result<(), String> {
    let label = format!("agent-step-{}", driver.state().sequence().get() + 1);
    driver.drive_once(store, transition(ids, &label)?, command).map_err(debug)?;
    Ok(())
}

fn append_record(path: &Path, value: &str) -> Result<(), String> {
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(debug)?;
    writeln!(file, "{value}").map_err(debug)?;
    file.sync_all().map_err(debug)
}

fn records(path: &Path) -> Result<Vec<String>, String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents.lines().map(str::to_owned).collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(debug(error)),
    }
}

fn wait_at_cut(directory: &Path, case: &Case, cut: Cut) -> Result<(), String> {
    if case.cut != cut {
        return Ok(());
    }
    let mut ready = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("ready.pending"))
        .map_err(debug)?;
    ready.write_all(serde_json::to_string(&cut).map_err(debug)?.as_bytes()).map_err(debug)?;
    ready.sync_all().map_err(debug)?;
    fs::rename(directory.join("ready.pending"), directory.join("ready")).map_err(debug)?;
    // The parent owns this child and terminates it. Timeout is a fixture failure, never success.
    for _ in 0..600 {
        thread::sleep(Duration::from_millis(100));
    }
    Err("parent did not terminate the owned crash-boundary child within 60 seconds".into())
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Runs a real owned process; all effect logs survive outside its memory and are fsynced.
pub fn check(case: &Case) -> Result<Metrics, String> {
    let executable = std::env::current_exe().map_err(debug)?;
    check_with_child(case, &executable)
}

/// Runs the crash/reopen check with an executable that dispatches
/// `stage3-driver-child` to [`child_main`].
///
/// The ordinary praxis binary can use [`check`]. Embedders such as Cargo
/// integration tests must supply a real child executable rather than spawning
/// their test harness, which otherwise accepts the arguments and exits without
/// reaching a crash boundary.
pub fn check_with_child(case: &Case, child_executable: &Path) -> Result<Metrics, String> {
    validate(case)?;
    let directory =
        tempfile::Builder::new().prefix("peritus-stage3-driver-").tempdir().map_err(debug)?;
    let input = directory.path().join("case.json");
    fs::write(&input, serde_json::to_vec(case).map_err(debug)?).map_err(debug)?;
    let stderr_path = directory.path().join("child.stderr");
    let stderr = File::create(&stderr_path).map_err(debug)?;
    let mut child = OwnedChild(
        Command::new(child_executable)
            .arg("stage3-driver-child")
            .arg(&input)
            .arg(directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .map_err(debug)?,
    );
    let started = Instant::now();
    while !directory.path().join("ready").exists() {
        if let Some(status) = child.0.try_wait().map_err(debug)? {
            return Err(format!(
                "driver child exited before crash cut ({status}): {}",
                fs::read_to_string(&stderr_path).unwrap_or_default()
            ));
        }
        if started.elapsed() > Duration::from_secs(45) {
            return Err(format!("driver child failed to reach {:?} within 45 seconds", case.cut));
        }
        thread::sleep(Duration::from_millis(5));
    }
    let marker = fs::read_to_string(directory.path().join("ready")).map_err(debug)?;
    if serde_json::from_str::<Cut>(&marker).map_err(debug)? != case.cut {
        return Err("owned child reached the wrong external crash cut".into());
    }
    child.0.kill().map_err(debug)?;
    let status = child.0.wait().map_err(debug)?;
    if status.success() {
        return Err("crash boundary exited successfully instead of being killed".into());
    }
    inspect_recovery(case, directory.path())
}

/// Main-binary-only child hook. It accepts an exact bounded Case file and a disposable directory.
pub fn child_main(arguments: &[String]) -> Result<(), String> {
    if arguments.len() != 2 {
        return Err("stage3-driver-child requires CASE.json DIRECTORY".into());
    }
    let bytes = fs::read(&arguments[0]).map_err(debug)?;
    if bytes.len() > 16_384 {
        return Err("driver child case exceeds 16 KiB".into());
    }
    let case: Case = serde_json::from_slice(&bytes).map_err(debug)?;
    validate(&case)?;
    let runtime = tokio::runtime::Builder::new_current_thread().build().map_err(debug)?;
    runtime.block_on(run_child(&case, Path::new(&arguments[1])))
}

struct LedgerPort(BudgetLedger);
impl AgentBudgetPort for LedgerPort {
    fn commit(&mut self, command: BudgetCommand) -> Result<BudgetReceipt, AgentBudgetPortError> {
        let transition = self.0.transition(command)?;
        let receipt = transition.receipt();
        self.0 = transition.into_ledger();
        Ok(receipt)
    }
}

struct RecordingProvider {
    profile: ProviderProfile,
    events: Vec<EventEnvelope>,
    directory: PathBuf,
    fail_start: bool,
}
impl ModelProvider for RecordingProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            validate_request_profile(&self.profile, &request)?;
            append_record(&self.directory.join("provider-effects"), "start").map_err(|_| {
                ProviderCoreError::transport(
                    "fixture_start",
                    "failed to fsync recording provider effect",
                )
            })?;
            if self.fail_start {
                return Err(ProviderCoreError::transport(
                    "fixture_start",
                    "scripted provider startup failure",
                ));
            }
            Ok(OwnedModelStream::new(RecordingStream(self.events.clone().into()), cancellation))
        })
    }
}
struct RecordingStream(VecDeque<EventEnvelope>);
impl ModelStream for RecordingStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.0.pop_front()) })
    }
}

fn profile(ids: &CommandIds) -> Result<ProviderProfile, String> {
    ProviderProfile::new(
        ids.revision.provider_profile_id(),
        1,
        ProviderName::new("praxis".into()).map_err(debug)?,
        ModelName::new("agent-driver-recording".into()).map_err(debug)?,
        WireDialect::OpenAiResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::ParallelToolCalls], &[])
            .map_err(debug)?,
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 1024, 8, 3, 1).map_err(debug)?,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(debug)
}

fn model_call(ordinal: u8) -> Result<CompletedToolCall, String> {
    CompletedToolCall::new(
        ToolCallId::new(format!("call-{ordinal}")).map_err(debug)?,
        ToolName::new("shell.exec".into()).map_err(debug)?,
        CanonicalJson::parse("{}", JsonBounds::value(ProtocolLimits::PRODUCTION)).map_err(debug)?,
    )
    .map_err(debug)
}

fn provider_events(count: u8) -> Result<Vec<EventEnvelope>, String> {
    let mut events = vec![ModelEvent::ResponseStarted { response_id: None, model: None }];
    for ordinal in 0..count {
        let item_id = ItemId::new(format!("item-{ordinal}")).map_err(debug)?;
        let call = model_call(ordinal)?;
        events.extend([
            ModelEvent::ItemStarted {
                item_id: item_id.clone(),
                index: u32::from(ordinal),
                kind: ItemKind::ToolCall,
            },
            ModelEvent::ToolCallStarted {
                item_id: item_id.clone(),
                call_id: call.id().clone(),
                name: call.name().clone(),
            },
            ModelEvent::ToolArgumentDelta {
                call_id: call.id().clone(),
                fragment: StreamFragment::new(b"{}".to_vec(), ProtocolLimits::PRODUCTION)
                    .map_err(debug)?,
            },
            ModelEvent::ItemCompleted(item_id),
        ]);
    }
    events.extend([
        ModelEvent::Finish(if count == 0 { FinishReason::Stop } else { FinishReason::ToolCalls }),
        ModelEvent::ResponseCompleted,
    ]);
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            EventEnvelope::new(
                u64::try_from(index).map_err(debug)? + 1,
                None,
                None,
                digest(&format!("raw-event-{index}")),
                event,
            )
            .map_err(debug)
        })
        .collect()
}

fn model_request(profile: &ProviderProfile) -> Result<ModelRequest, String> {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls],
            &[],
            profile.limits(),
        )
        .map_err(debug)?,
    )
    .map_err(debug)?;
    let message = Message::new(
        Role::User,
        vec![ContentBlock::Text(
            BoundedText::new("record bounded tool work".into(), limits).map_err(debug)?,
        )],
        limits,
    )
    .map_err(debug)?;
    let parameters = peritus_model_protocol::JsonSchema::parse(
        r#"{"type":"object","properties":{},"additionalProperties":false}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(limits),
    )
    .map_err(debug)?;
    let definition = ToolDefinition::new(
        ToolName::new("shell.exec".into()).map_err(debug)?,
        None,
        parameters,
        false,
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("driver-request".into()).map_err(debug)?,
        vec![message],
        vec![definition],
        ToolChoice::Auto,
        ParallelToolPolicy::Allowed(3),
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(128, Vec::new(), None, None, None).map_err(debug)?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .map_err(debug)
}

async fn start_provider(
    driver: &mut AgentDriver,
    store: &mut SqliteJournal,
    ids: &CommandIds,
    directory: &Path,
    count: u8,
    attempt: u64,
    fail: bool,
) -> Result<(AgentBudgetReservation, LedgerPort), String> {
    let budget_id =
        BudgetId::new(contract::id(ids.run, attempt, "provider-budget")).map_err(debug)?;
    let reservation_id =
        BudgetReservationId::new(contract::id(ids.run, attempt, "provider-reservation"))
            .map_err(debug)?;
    let mut port = LedgerPort(BudgetLedger::new_root(
        budget_id,
        ids.revision,
        BudgetLimits::new(BudgetAmounts::from_units(100, 1000, 1000, 10, 10)),
    ));
    let plan = AgentBudgetPlan::new(
        reservation_id,
        budget_id,
        ids.revision,
        ids.action,
        digest("provider-action"),
        BudgetAmounts::from_units(0, 0, 1, 0, 0),
        // Every fixture call owns a distinct account. This is its first B1 attempt;
        // D0 retries are checked separately and do not fabricate prior B1 lineage.
        false,
    )
    .map_err(debug)?;
    let mut budget = AgentBudgetReservation::begin(&mut port, plan).map_err(debug)?;
    let provider = RecordingProvider {
        profile: profile(ids)?,
        events: provider_events(count)?,
        directory: directory.to_owned(),
        fail_start: fail,
    };
    let label = format!("agent-model-start-{}", driver.state().sequence().get() + 1);
    let result = driver
        .start_model_once(
            store,
            transition(ids, &label)?,
            ModelCallId::new(digest(&format!("call-attempt-{attempt}"))).map_err(debug)?,
            &mut budget,
            &mut port,
            digest("provider-active"),
            &provider,
            model_request(provider.profile())?,
            ProtocolLimits::PRODUCTION,
            CancellationToken::new(),
        )
        .await;
    if fail {
        if result.is_ok() {
            return Err("scripted provider startup failure was swallowed".into());
        }
        if driver.model().is_some() || !driver.state().model().in_flight() {
            return Err("failed provider start did not retain only durable attempt intent".into());
        }
    } else {
        result.map_err(debug)?;
    }
    Ok((budget, port))
}

async fn drain_provider(
    driver: &mut AgentDriver,
    store: &mut SqliteJournal,
    ids: &CommandIds,
) -> Result<(), String> {
    loop {
        let label = format!("agent-model-event-{}", driver.state().sequence().get() + 1);
        if matches!(
            driver.drive_model_once(store, transition(ids, &label)?).await.map_err(debug)?,
            ProviderAdvance::Closed
        ) {
            break;
        }
    }
    Ok(())
}

fn descriptor() -> Result<Arc<ToolDescriptor>, String> {
    let name = peritus_types::CapabilityName::new("shell.exec".into()).map_err(debug)?;
    let operation = OperationDescriptor::new(
        name.clone(),
        OperationClass::Execution,
        RiskSet::new(vec![RiskClass::Execution]).map_err(debug)?,
    )
    .map_err(debug)?;
    ToolDescriptor::new(
        name,
        SemanticVersion::new(1, 0, 0).map_err(debug)?,
        Schema::object(Vec::new(), false).map_err(debug)?,
        operation,
        SideEffectClass::Process,
        LeaseRequirement::None,
        IdempotencySemantics::ReportPriorOutcome,
        ImplementationIdentity::new("praxis.recording-effect.v1".into()).map_err(debug)?,
        ToolLimits::new(100, 4096, 1024, 1024, 8, 1, 64).map_err(debug)?,
        ControlSet::new(false, false, false, true, true),
        ProtocolCompatibility::V1,
        ToolText::new("Fsynced disposable recording tool".into()).map_err(debug)?,
    )
    .map(Arc::new)
    .map_err(debug)
}

fn tool_setup(ids: &CommandIds, count: u8) -> Result<(ToolRouter, ToolBatchCoordinator), String> {
    let descriptor = descriptor()?;
    let operation = OperationDescriptor::new(
        descriptor.name().clone(),
        OperationClass::Execution,
        RiskSet::new(vec![RiskClass::Execution]).map_err(debug)?,
    )
    .map_err(debug)?;
    let operations = OperationRegistry::new(vec![operation]).map_err(debug)?;
    let registry = ToolRegistry::new(vec![descriptor], &operations).map_err(debug)?;
    let router = ToolRouter::new(registry, RouterLimits::new(3, 8).map_err(debug)?);
    let scope = CapabilityScope::new(
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        PermissionSet::new(vec![Permission::new(ids.resource, ids.capability.clone())])
            .map_err(debug)?,
        ids.revision,
        ValidityWindow::new(authority::instant(10), authority::instant(1000)).map_err(debug)?,
        UseLimit::limited(3).map_err(debug)?,
    );
    let exposed = router.exposed(ActorRole::ProviderToolWorker, &scope).map_err(debug)?;
    let mut plans = Vec::new();
    for ordinal in 0..count {
        plans.push(
            ToolInvocationPlan::from_model(
                model_call(ordinal)?,
                tool_ids(ids, ordinal)?.action,
                SemanticVersion::new(1, 0, 0).map_err(debug)?,
                CallLimits::new(100, 4096, 1024, 1024, 8, 1).map_err(debug)?,
                ids.revision,
                authority::instant(120),
                IdempotencyKey::new(format!("effect-{ordinal}")).map_err(debug)?,
                JsonLimits::PRODUCTION,
            )
            .map_err(debug)?,
        );
    }
    let batch = ToolBatchCoordinator::prepare(&router, &exposed, plans, 3, 3).map_err(debug)?;
    Ok((router, batch))
}

fn tool_ids(root: &CommandIds, ordinal: u8) -> Result<CommandIds, String> {
    let acceptance = contract::command_contract(RunId::new([93; 16]).map_err(debug)?, 0)?;
    let mut ids =
        CommandIds::new(RunId::new([93; 16]).map_err(debug)?, u64::from(ordinal) + 1, &acceptance)?;
    ids.revision = root.revision;
    ids.actor = root.actor;
    ids.session = root.session;
    ids.turn = root.turn;
    ids.workspace = root.workspace;
    ids.resource = root.resource;
    ids.environment = root.environment;
    Ok(ids)
}

fn tool_result(prepared: &PreparedToolCall) -> Result<ToolResult, String> {
    ToolResult::success(
        prepared,
        BoundedJson::boolean(true),
        ToolText::new("recorded".into()).map_err(debug)?,
        ToolText::new("recorded".into()).map_err(debug)?,
        Vec::new(),
        ToolTiming::new(authority::instant(20), authority::instant(21)).map_err(debug)?,
        TruncationMetadata {
            output: Truncation::Complete,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        0,
    )
    .map_err(debug)
}

struct RecordingDispatcher {
    descriptor: Arc<ToolDescriptor>,
    directory: PathBuf,
    case: Case,
    ordinal: u8,
}
impl ToolDispatcher for RecordingDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        self.descriptor.implementation_identity()
    }
    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor.descriptor_digest()
    }
    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        // A fixture setup failure is a panic caught by the outer campaign, not a forged tool failure.
        Ok(self.record(invocation).unwrap_or_else(|error| panic!("recording dispatcher: {error}")))
    }
}
impl RecordingDispatcher {
    fn record(&self, invocation: AuthorizedInvocation) -> Result<ToolStart, String> {
        if self.ordinal == self.case.completed_before_cut {
            wait_at_cut(&self.directory, &self.case, Cut::BeforeEffect)?;
        }
        append_record(&self.directory.join("tool-effects"), &self.ordinal.to_string())?;
        if self.ordinal == self.case.completed_before_cut {
            wait_at_cut(&self.directory, &self.case, Cut::AfterEffect)?;
        }
        if self.case.cut == Cut::Active && self.ordinal == self.case.completed_before_cut {
            Ok(ToolStart::Active(Box::new(RecordingExecution(invocation.into_prepared()))))
        } else {
            Ok(ToolStart::Completed(tool_result(invocation.prepared())?))
        }
    }
}

struct RecordingExecution(PreparedToolCall);
impl ToolExecution for RecordingExecution {
    fn poll(
        &mut self,
        _at: peritus_policy::AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        Ok(ExecutionUpdate::new(&self.0, Vec::new(), None)
            .unwrap_or_else(|error| panic!("empty recording update: {error}")))
    }
    fn control(
        &mut self,
        _control: peritus_tool_protocol::ToolControl,
        at: peritus_policy::AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll(at)
    }
    fn cancel(
        &mut self,
        _reason: peritus_tool_protocol::CancellationReason,
        at: peritus_policy::AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll(at)
    }
    fn recover(
        &mut self,
        at: peritus_policy::AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        self.poll(at).map(RecoveryObservation::Active)
    }
}

fn next_identity(
    driver: &AgentDriver,
    ids: &CommandIds,
    label: &str,
) -> Result<TransitionIdentity, String> {
    transition(ids, &format!("{label}-{}", driver.state().sequence().get() + 1))
}

async fn run_child(case: &Case, directory: &Path) -> Result<(), String> {
    let (acceptance, ids) = root_ids()?;
    let mut store = journal::open(&directory.join("agent.sqlite"), &ids, "agent-store")?;
    let mut driver = AgentDriver::start(
        &mut store,
        binding(&ids)?,
        limits()?,
        transition(&ids, "agent-genesis")?,
        CodecLimits::PRODUCTION,
    )
    .map_err(debug)?;
    let context = ContextRecord::new(
        digest("context"),
        digest("plan"),
        digest("memory"),
        digest("estimator"),
        Some(digest("compaction")),
    );
    drive(&mut driver, &mut store, &ids, AgentCommandKind::ContextPrepared(context))?;
    if case.pause {
        drive(&mut driver, &mut store, &ids, AgentCommandKind::Paused)?;
        let before = driver.state().canonical_bytes();
        if driver
            .drive_once(
                &mut store,
                next_identity(&driver, &ids, "rejected-resume")?,
                AgentCommandKind::Resumed { recovery_checked: false },
            )
            .is_ok()
            || before != driver.state().canonical_bytes()
        {
            return Err("resume without recovery checks changed durable state".into());
        }
        drive(&mut driver, &mut store, &ids, AgentCommandKind::Resumed { recovery_checked: true })?;
    }
    for attempt in 0..case.retries {
        let _ = start_provider(
            &mut driver,
            &mut store,
            &ids,
            directory,
            case.tools,
            u64::from(attempt),
            true,
        )
        .await?;
        let request = model_request(&profile(&ids)?)?.fingerprint().map_err(debug)?.digest();
        drive(
            &mut driver,
            &mut store,
            &ids,
            AgentCommandKind::ProviderRetryScheduled(ProviderRetryRecord::new(
                digest("start-failure"),
                request,
                ProviderRetryClass::SafeNewRequest,
            )),
        )?;
    }
    let (mut budget, mut budget_port) = start_provider(
        &mut driver,
        &mut store,
        &ids,
        directory,
        case.tools,
        u64::from(case.retries),
        false,
    )
    .await?;
    wait_at_cut(directory, case, Cut::ProviderIntent)?;
    let event_id = next_identity(&driver, &ids, "first-model-event")?;
    driver.drive_model_once(&mut store, event_id).await.map_err(debug)?;
    wait_at_cut(directory, case, Cut::ProviderPrefix)?;
    drain_provider(&mut driver, &mut store, &ids).await?;
    budget.finalize_ambiguous(&mut budget_port, digest("settled-provider")).map_err(debug)?;
    let (mut router, coordinator) = tool_setup(&ids, case.tools)?;
    let identity = next_identity(&driver, &ids, "tool-proposal")?;
    driver.propose_tools_once(&mut store, identity, coordinator, &budget).map_err(debug)?;
    let identity = next_identity(&driver, &ids, "tool-authorization")?;
    driver.request_tool_authorization_once(&mut store, identity).map_err(debug)?;
    for ordinal in 0..case.tools {
        let identity = next_identity(&driver, &ids, "tool-authorized")?;
        driver
            .authorize_tool_once(
                &mut store,
                identity,
                ToolOrdinal::new(u16::from(ordinal)),
                digest(&format!("authorized-{ordinal}")),
            )
            .map_err(debug)?;
    }
    let identity = next_identity(&driver, &ids, "tools-executing")?;
    driver.begin_tool_execution_once(&mut store, identity).map_err(debug)?;
    wait_at_cut(directory, case, Cut::Authorized)?;
    for ordinal in 0..case.tools {
        let prepared = driver
            .tools()
            .and_then(|batch| batch.slots().get(usize::from(ordinal)))
            .ok_or("prepared runtime tool is missing")?
            .prepared()
            .clone();
        let action_ids = tool_ids(&ids, ordinal)?;
        let authority = authority::commit_tool(
            &directory.join(format!("authority-{ordinal}.sqlite")),
            &action_ids,
            &acceptance,
            &prepared,
            100,
        )?;
        let request = authority.request(&action_ids, &prepared);
        let mut dispatcher = RecordingDispatcher {
            descriptor: descriptor()?,
            directory: directory.to_owned(),
            case: case.clone(),
            ordinal,
        };
        let identity = next_identity(&driver, &ids, "tool-dispatch")?;
        driver
            .dispatch_tool_once(
                &mut store,
                identity,
                ToolOrdinal::new(u16::from(ordinal)),
                &mut router,
                &request,
                &mut dispatcher,
            )
            .map_err(debug)?;
        let identity = next_identity(&driver, &ids, "tool-observation")?;
        driver
            .record_tool_observation_once(
                &mut store,
                identity,
                ToolOrdinal::new(u16::from(ordinal)),
                Vec::new(),
            )
            .map_err(debug)?;
        if ordinal == case.completed_before_cut {
            wait_at_cut(directory, case, Cut::Active)?;
            wait_at_cut(directory, case, Cut::Completed)?;
        }
    }
    drive(&mut driver, &mut store, &ids, AgentCommandKind::ResultRecordingStarted)?;
    wait_at_cut(directory, case, Cut::Recording)?;
    drive(
        &mut driver,
        &mut store,
        &ids,
        AgentCommandKind::ResultsRecorded { transcript_digest: digest("tools") },
    )?;
    wait_at_cut(directory, case, Cut::ResultsRecorded)?;
    driver.clear_runtime_resources();
    drive(&mut driver, &mut store, &ids, AgentCommandKind::ContextPrepared(context))?;
    let (mut final_budget, mut final_port) =
        start_provider(&mut driver, &mut store, &ids, directory, 0, 10, false).await?;
    drain_provider(&mut driver, &mut store, &ids).await?;
    final_budget
        .finalize_ambiguous(&mut final_port, digest("completion-provider"))
        .map_err(debug)?;
    let terminal = driver.model_terminal_record(&final_budget).map_err(debug)?;
    let proposal = CompletionProposal::new(
        text("bounded fixture work completed")?,
        Vec::new(),
        Vec::new(),
        ids.revision,
        TranscriptDigests::new(
            context.render_digest(),
            terminal.response_digest(),
            digest("tools"),
        ),
        CompletionRequest::RunGates,
    )
    .map_err(debug)?;
    drive(
        &mut driver,
        &mut store,
        &ids,
        AgentCommandKind::CompletionProposed { terminal, proposal },
    )?;
    wait_at_cut(directory, case, Cut::CompletionProposed)?;
    match case.ending {
        Ending::Complete => {
            drive(&mut driver, &mut store, &ids, AgentCommandKind::CompletionCommitted)?
        }
        Ending::Cancel => {
            drive(&mut driver, &mut store, &ids, AgentCommandKind::CancellationRequested)?;
            drive(&mut driver, &mut store, &ids, AgentCommandKind::CancellationFinished)?;
        }
        Ending::Fail => drive(
            &mut driver,
            &mut store,
            &ids,
            AgentCommandKind::Failed(AgentFailure::new(
                AgentFailureKind::Tool,
                text("scripted terminal failure")?,
            )),
        )?,
    }
    wait_at_cut(directory, case, Cut::Terminal)?;
    Err("requested crash cut was not reached".into())
}

/// Tiny independent expected observation model. It never invokes AgentReducer/replay/verified.
struct Expected {
    phase: AgentPhase,
    effects: Vec<String>,
    completed: usize,
    lost: Option<u16>,
    model_lost: bool,
    provider_starts: usize,
    context_cycles: u16,
    tool_calls: u16,
    cursor: u64,
    has_batch: bool,
    retries: u32,
}
fn expected(case: &Case) -> Expected {
    let early_provider = matches!(case.cut, Cut::ProviderIntent | Cut::ProviderPrefix);
    let after_results =
        matches!(case.cut, Cut::ResultsRecorded | Cut::CompletionProposed | Cut::Terminal);
    let late = matches!(case.cut, Cut::CompletionProposed | Cut::Terminal);
    let completed = match case.cut {
        Cut::ProviderIntent | Cut::ProviderPrefix | Cut::Authorized => 0,
        Cut::BeforeEffect | Cut::AfterEffect | Cut::Active => {
            usize::from(case.completed_before_cut)
        }
        Cut::Completed => usize::from(case.completed_before_cut) + 1,
        _ => usize::from(case.tools),
    };
    let effect_count = completed + usize::from(matches!(case.cut, Cut::AfterEffect | Cut::Active));
    let phase = match case.cut {
        Cut::ProviderIntent | Cut::ProviderPrefix => {
            AgentPhase::Active(ActivePhase::StreamingResponse)
        }
        Cut::Authorized | Cut::BeforeEffect | Cut::AfterEffect | Cut::Active | Cut::Completed => {
            AgentPhase::Active(ActivePhase::ExecutingTools)
        }
        Cut::Recording => AgentPhase::Active(ActivePhase::RecordingResults),
        Cut::ResultsRecorded => AgentPhase::Active(ActivePhase::PreparingContext),
        Cut::CompletionProposed => AgentPhase::Active(ActivePhase::ProposedCompletion),
        Cut::Terminal => AgentPhase::Terminal(match case.ending {
            Ending::Complete => TerminalKind::Completed,
            Ending::Cancel => TerminalKind::Cancelled,
            Ending::Fail => TerminalKind::Failed,
        }),
    };
    let cursor = match case.cut {
        Cut::ProviderIntent | Cut::ResultsRecorded => 0,
        Cut::ProviderPrefix => 1,
        Cut::CompletionProposed | Cut::Terminal => 3,
        _ => u64::from(case.tools) * 4 + 3,
    };
    Expected {
        phase,
        effects: (0..effect_count).map(|index| index.to_string()).collect(),
        completed,
        lost: matches!(case.cut, Cut::BeforeEffect | Cut::AfterEffect | Cut::Active)
            .then_some(u16::from(case.completed_before_cut)),
        model_lost: early_provider,
        provider_starts: usize::from(case.retries) + 1 + usize::from(late),
        context_cycles: 1 + u16::from(late),
        tool_calls: if early_provider { 0 } else { u16::from(case.tools) },
        cursor,
        has_batch: !early_provider && !after_results,
        retries: if after_results { 0 } else { u32::from(case.retries) },
    }
}

fn inspect_recovery(case: &Case, directory: &Path) -> Result<Metrics, String> {
    let expected = expected(case);
    let (_, ids) = root_ids()?;
    let mut expected_starts = vec!["start".to_owned(); expected.provider_starts];
    if records(&directory.join("tool-effects"))? != expected.effects
        || records(&directory.join("provider-effects"))? != expected_starts
    {
        return Err("external fsynced effect history differs from independent cut model".into());
    }
    let mut stable_bytes = None;
    let mut transitions = 0;
    for _ in 0..case.reopens {
        let store = journal::open(&directory.join("agent.sqlite"), &ids, "agent-store")?;
        let driver =
            AgentDriver::restore(&store, binding(&ids)?, limits()?, CodecLimits::PRODUCTION)
                .map_err(debug)?;
        let observed = driver.state();
        let recovery = driver.recovery_report();
        let lost: Vec<_> = recovery.tool_ordinals().iter().map(|ordinal| ordinal.get()).collect();
        if observed.phase() != expected.phase
            || observed.model().cursor() != expected.cursor
            || observed.counters().context_cycles() != expected.context_cycles
            || observed.counters().tool_calls() != expected.tool_calls
            || observed.model().retry_count() != expected.retries
            || recovery.model_in_flight() != expected.model_lost
            || lost != expected.lost.into_iter().collect::<Vec<_>>()
            || driver.model().is_some()
            || driver.tools().is_some()
        {
            return Err(format!(
                "restored observations disagree with {:?}: phase={:?}, cursor={}, \
                cycles={}, calls={}, retries={}, model_lost={}, lost={lost:?}",
                case.cut,
                observed.phase(),
                observed.model().cursor(),
                observed.counters().context_cycles(),
                observed.counters().tool_calls(),
                observed.model().retry_count(),
                recovery.model_in_flight()
            ));
        }
        if observed.tools().is_some() != expected.has_batch {
            return Err("restored tool-batch lifetime differs from cut model".into());
        }
        if let Some(batch) = observed.tools() {
            if batch.slots().len() != usize::from(case.tools) {
                return Err("durable tool-batch cardinality changed after process loss".into());
            }
            for (index, slot) in batch.slots().iter().enumerate() {
                let wanted = if index < expected.completed {
                    ToolSlotPhase::Terminal
                } else if expected.lost == u16::try_from(index).ok() {
                    if case.cut == Cut::Active {
                        ToolSlotPhase::Active
                    } else {
                        ToolSlotPhase::Dispatched
                    }
                } else {
                    ToolSlotPhase::Authorized
                };
                if slot.phase() != wanted
                    || (index < expected.completed
                        && slot.result().map(|result| result.status())
                            != Some(ToolResultStatus::Succeeded))
                {
                    return Err(format!(
                        "slot {index} lost completed/uncertain/unstarted distinction"
                    ));
                }
            }
        }
        let durable = load_agent_replay(&store, ids.turn).map_err(debug)?;
        if u64::try_from(durable.events().len()).map_err(debug)? != observed.sequence().get()
            || u64::from(observed.counters().transitions()) != observed.sequence().get()
        {
            return Err("reopened event chain and durable transition count disagree".into());
        }
        let bytes = observed.canonical_bytes();
        if stable_bytes.as_ref().is_some_and(|previous| previous != &bytes) {
            return Err("effect-free reopen changed canonical AgentDriver state".into());
        }
        stable_bytes = Some(bytes);
        transitions = observed.sequence().get();
    }
    if records(&directory.join("tool-effects"))? != expected.effects
        || records(&directory.join("provider-effects"))? != expected_starts
    {
        return Err("reopening real journal automatically repeated an external effect".into());
    }
    let mut store = journal::open(&directory.join("agent.sqlite"), &ids, "agent-store")?;
    let mut driver =
        AgentDriver::restore(&store, binding(&ids)?, limits()?, CodecLimits::PRODUCTION)
            .map_err(debug)?;
    let mut rejected = 0;
    if expected.model_lost {
        let request_digest = model_request(&profile(&ids)?)?.fingerprint().map_err(debug)?.digest();
        drive(
            &mut driver,
            &mut store,
            &ids,
            AgentCommandKind::ProviderRetryScheduled(ProviderRetryRecord::new(
                digest("lost-provider"),
                request_digest,
                ProviderRetryClass::SafeNewRequest,
            )),
        )?;
        if records(&directory.join("provider-effects"))? != expected_starts {
            return Err(
                "scheduling recovery retry contacted the provider before explicit start".into()
            );
        }
        let runtime = tokio::runtime::Builder::new_current_thread().build().map_err(debug)?;
        runtime.block_on(async {
            let (mut budget, mut port) =
                start_provider(&mut driver, &mut store, &ids, directory, 0, 100, false).await?;
            drain_provider(&mut driver, &mut store, &ids).await?;
            budget
                .finalize_ambiguous(&mut port, digest("recovery-retry-settled"))
                .map_err(debug)?;
            let terminal = driver.model_terminal_record(&budget).map_err(debug)?;
            let proposal = CompletionProposal::new(
                text("provider retry finished")?,
                Vec::new(),
                Vec::new(),
                ids.revision,
                TranscriptDigests::new(
                    digest("context"),
                    terminal.response_digest(),
                    digest("no-tools"),
                ),
                CompletionRequest::RunGates,
            )
            .map_err(debug)?;
            drive(
                &mut driver,
                &mut store,
                &ids,
                AgentCommandKind::CompletionProposed { terminal, proposal },
            )
        })?;
        expected_starts.push("start".into());
        if driver.state().model().retry_count() != u32::from(case.retries) + 1
            || !driver.recovery_report().is_clean()
        {
            return Err(
                "explicit post-restart provider retry did not settle its fresh attempt".into()
            );
        }
    }
    if expected.has_batch {
        for ordinal in 0..expected.completed {
            let before = driver.state().canonical_bytes();
            let identity = next_identity(&driver, &ids, "reject-repeat-completed-effect")?;
            if driver
                .drive_once(
                    &mut store,
                    identity,
                    AgentCommandKind::ToolDispatched {
                        ordinal: ToolOrdinal::new(u16::try_from(ordinal).map_err(debug)?),
                    },
                )
                .is_ok()
                || before != driver.state().canonical_bytes()
            {
                return Err("completed durable tool could be dispatched a second time".into());
            }
            rejected += 1;
        }
    }
    if let Some(ordinal) = expected.lost {
        let identity = next_identity(&driver, &ids, "classify-lost-tool")?;
        driver
            .classify_lost_tool_once(&mut store, identity, ToolOrdinal::new(ordinal))
            .map_err(debug)?;
        let result = driver
            .state()
            .tools()
            .and_then(|batch| batch.slots().get(usize::from(ordinal)))
            .and_then(|slot| slot.result())
            .ok_or("classified lost tool result missing")?;
        if result.status() != ToolResultStatus::Indeterminate
            || !driver.recovery_report().is_clean()
        {
            return Err("lost ordinary tool was not truthfully classified indeterminate".into());
        }
        let before = driver.state().canonical_bytes();
        let identity = next_identity(&driver, &ids, "reject-repeat-classification")?;
        if driver.classify_lost_tool_once(&mut store, identity, ToolOrdinal::new(ordinal)).is_ok()
            || before != driver.state().canonical_bytes()
        {
            return Err("lost tool classification was accepted twice".into());
        }
        rejected += 1;
        // Record a settled single-call batch to exercise persistent unresolved-outcome truth.
        if case.tools == 1 {
            drive(&mut driver, &mut store, &ids, AgentCommandKind::ResultRecordingStarted)?;
            drive(
                &mut driver,
                &mut store,
                &ids,
                AgentCommandKind::ResultsRecorded { transcript_digest: digest("uncertain-tools") },
            )?;
            if !driver.state().has_unresolved_indeterminate() {
                return Err("result recording erased unresolved indeterminate effect truth".into());
            }
        }
    }
    if !driver.state().phase().is_terminal() {
        drive(&mut driver, &mut store, &ids, AgentCommandKind::CancellationRequested)?;
        drive(&mut driver, &mut store, &ids, AgentCommandKind::CancellationFinished)?;
    } else {
        let before = driver.state().canonical_bytes();
        let identity = next_identity(&driver, &ids, "reject-terminal-command")?;
        if driver
            .drive_once(
                &mut store,
                identity,
                AgentCommandKind::ContextPrepared(ContextRecord::new(
                    digest("x"),
                    digest("x"),
                    digest("x"),
                    digest("x"),
                    Some(digest("x")),
                )),
            )
            .is_ok()
            || before != driver.state().canonical_bytes()
        {
            return Err("terminal AgentDriver accepted additional work".into());
        }
        rejected += 1;
    }
    let final_bytes = driver.state().canonical_bytes();
    drop(driver);
    drop(store);
    let store = journal::open(&directory.join("agent.sqlite"), &ids, "agent-store")?;
    let restored = AgentDriver::restore(&store, binding(&ids)?, limits()?, CodecLimits::PRODUCTION)
        .map_err(debug)?;
    if restored.state().canonical_bytes() != final_bytes
        || records(&directory.join("tool-effects"))? != expected.effects
        || records(&directory.join("provider-effects"))? != expected_starts
    {
        return Err("finalization/reopen changed state or repeated an effect".into());
    }
    Ok(BTreeMap::from([
        ("owned_process_crashes".into(), 1),
        ("sqlite_reopens".into(), u64::from(case.reopens) + 2),
        ("provider_effects".into(), u64::try_from(expected_starts.len()).map_err(debug)?),
        ("tool_effects".into(), u64::try_from(expected.effects.len()).map_err(debug)?),
        ("completed_effects_retained".into(), u64::try_from(expected.completed).map_err(debug)?),
        ("lost_tools_classified".into(), u64::from(expected.lost.is_some())),
        ("provider_start_retries".into(), u64::from(case.retries)),
        ("provider_restart_retries".into(), u64::from(expected.model_lost)),
        ("rejected_replays".into(), rejected),
        ("durable_transitions".into(), transitions),
        (format!("cut_{:?}", case.cut).to_lowercase(), 1),
    ]))
}
