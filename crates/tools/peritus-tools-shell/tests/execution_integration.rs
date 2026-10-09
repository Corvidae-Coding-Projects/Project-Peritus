//! End-to-end C4 router -> dispatcher -> restricted C2/C3 process coverage.

#![cfg(feature = "integration-fixtures")]

mod integration_support;
mod process_authority;
mod router_authority;

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use integration_support::{NativeBackend, quality_plan, shell_plan};
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_policy::{
    ActorRole, AuthorityInstant, OperationDescriptor, OperationRegistry, RiskSet,
};
use peritus_process::{
    ExecutionAuthorizationRequest, ExecutionCallerBinding, ExecutionCallerTarget, ExecutionGateway,
    ProcessStore, StdinPolicy,
};
use peritus_tool_protocol::{
    BoundedJson, CallLimits, IdempotencyKey, JsonLimits, ResultStatus, SemanticVersion, ToolCall,
    ToolControl, ToolDescriptor, ToolResult,
};
use peritus_tool_router::{
    DispatchOutcome, InvocationHandle, RouterLimits, ToolAuthorizationRequest, ToolRegistry,
    ToolRouter, tool_action_intent,
};
use peritus_tools_quality::{
    CheckCatalog, CheckDefinition, CheckRequirement, CheckSource, EnvironmentProfile,
    ExpectedSuccess, OutputParser, QualityRunDispatcher, run_descriptor,
};
use peritus_tools_shell::{ShellDispatcher, exec_descriptor};
use peritus_types::{GateId, Generation};

const TOOL_DEADLINE_TICK: u64 = 10_020;

#[test]
fn shell_exec_runs_literal_argv_accepts_stdin_and_publishes_output_artifacts() {
    let descriptor = exec_descriptor().expect("shell descriptor");
    let executable = fixture_binary();
    let argument_values = serde_json::Value::Array(
        ["shell", "a;printf-not-executed"]
            .into_iter()
            .map(|value| serde_json::Value::String(value.to_owned()))
            .collect(),
    );
    let arguments_value = json_object([
        ("arguments", argument_values),
        ("executable", serde_json::Value::String(executable)),
    ]);
    let arguments = BoundedJson::parse(&arguments_value.to_string(), JsonLimits::PRODUCTION)
        .expect("shell arguments");
    let mut setup = Setup::new(111, descriptor, arguments, "shell-argv");
    let caller = setup.caller_binding();
    let (plan, sandbox, admission) = shell_plan(
        &setup.process_root,
        &setup.process_ids,
        &setup.prepared,
        caller,
        &fixture_binary(),
        vec!["shell".to_owned(), "a;printf-not-executed".to_owned()],
        StdinPolicy::bounded(5, 5).expect("stdin policy"),
    );
    let process_intent = process_authority::intent(&setup.process_ids, &plan);
    let mut journal = process_authority::open_journal(&setup.process_root);
    let receipts = process_authority::commit_authority(
        &mut journal,
        &setup.process_ids,
        &process_intent,
        plan.resource_policy().wall_millis(),
    );
    let execution_request =
        execution_request(&setup.process_ids, &plan, &process_intent, &receipts);
    let gateway = gateway(&setup.process_root);
    let artifacts = artifact_store(&setup.process_root);
    let backend = NativeBackend::admitted(&admission);
    let mut dispatcher = ShellDispatcher::new(
        &gateway,
        &execution_request,
        plan,
        sandbox,
        admission,
        backend,
        artifacts,
    )
    .expect("shell dispatcher");
    let handle = setup.dispatch(&mut dispatcher);
    let update = setup
        .router
        .control(
            handle,
            ToolControl::stdin(b"hello".to_vec(), 5).expect("stdin control"),
            instant(21),
        )
        .expect("stdin accepted");
    let result =
        update.terminal().cloned().unwrap_or_else(|| await_result(&mut setup.router, handle, 22));
    assert_eq!(result.status(), ResultStatus::Succeeded, "{result:?}");
    assert!(
        result.human_rendering().as_str().contains("argv:a;printf-not-executed:hello"),
        "unexpected retained output: {:?}",
        result.human_rendering().as_str(),
    );
    assert_eq!(result.artifacts().len(), 2);
    assert!(result.artifacts().iter().all(|artifact| artifact.size() > 0));
}

#[test]
fn quality_run_emits_candidate_evidence_and_invalid_json_never_passes() {
    let passed = run_quality(131, "quality-valid", "quality.valid");
    assert_eq!(passed.status(), ResultStatus::Succeeded, "{passed:?}");
    assert_eq!(candidate_outcome(&passed), "passed");
    assert!(!passed.artifacts().is_empty());

    let invalid = run_quality(151, "quality-invalid", "quality.invalid");
    assert_eq!(invalid.status(), ResultStatus::Failed);
    assert_eq!(candidate_outcome(&invalid), "invalid-result");
    assert_eq!(
        invalid.failure_value().expect("typed parser failure").code().as_str(),
        "quality-parser-invalid"
    );
}

fn run_quality(seed: u8, mode: &str, gate_name: &str) -> ToolResult {
    let descriptor = run_descriptor().expect("quality descriptor");
    let arguments_value = json_object([("gate", serde_json::Value::String(gate_name.to_owned()))]);
    let arguments = BoundedJson::parse(&arguments_value.to_string(), JsonLimits::PRODUCTION)
        .expect("quality arguments");
    let mut setup = Setup::new(seed, descriptor, arguments, gate_name);
    std::fs::write(setup.process_root.workspace().join("quality.marker"), "project-under-test")
        .expect("temporary quality project marker");
    let definition = CheckDefinition::new(
        gate_name,
        GateId::new([seed; 16]).expect("gate id"),
        CheckSource::Explicit("integration-test".to_owned()),
        CheckRequirement::Required,
        fixture_binary(),
        vec![mode.to_owned(), "quality.marker".to_owned()],
        None,
        EnvironmentProfile::new("integration").expect("profile"),
        5_000,
        4_096,
        OutputParser::Json { maximum_bytes: 128 },
        ExpectedSuccess::ExitCode(0),
    )
    .expect("check definition");
    let catalog = CheckCatalog::from_explicit(vec![definition.clone()]).expect("catalog");
    let caller = setup.caller_binding();
    let (plan, sandbox, admission) =
        quality_plan(&setup.process_root, &setup.process_ids, &setup.prepared, caller, &definition);
    let process_intent = process_authority::intent(&setup.process_ids, &plan);
    let mut journal = process_authority::open_journal(&setup.process_root);
    let receipts = process_authority::commit_authority(
        &mut journal,
        &setup.process_ids,
        &process_intent,
        plan.resource_policy().wall_millis(),
    );
    let execution_request =
        execution_request(&setup.process_ids, &plan, &process_intent, &receipts);
    let gateway = gateway(&setup.process_root);
    let artifacts = artifact_store(&setup.process_root);
    let backend = NativeBackend::admitted(&admission);
    let mut dispatcher = QualityRunDispatcher::new(
        &gateway,
        &execution_request,
        plan,
        sandbox,
        admission,
        backend,
        artifacts,
        catalog,
    )
    .expect("quality dispatcher");
    let handle = setup.dispatch(&mut dispatcher);
    await_result(&mut setup.router, handle, 21)
}

struct Setup {
    process_root: process_authority::TestRoot,
    process_ids: process_authority::Ids,
    _router_root: router_authority::TestRoot,
    router_ids: router_authority::Ids,
    router: ToolRouter,
    prepared: peritus_tool_protocol::PreparedToolCall,
    intent: peritus_protocol::ActionIntentDto,
    receipts: Option<router_authority::AuthorityReceipts>,
}

impl Setup {
    fn new(seed: u8, descriptor: ToolDescriptor, arguments: BoundedJson, key: &str) -> Self {
        let process_root = process_authority::TestRoot::new();
        let process_ids = process_authority::Ids::new(seed);
        let router_root = router_authority::TestRoot::new();
        let mut router_ids = router_authority::Ids::new(seed);
        router_ids.capability = descriptor.name().clone();
        let router = router(descriptor);
        let prepared =
            router.prepare(tool_call(&router_ids, arguments, key)).expect("prepared call");
        let intent = tool_action_intent(
            &prepared,
            router_ids.actor,
            ActorRole::ProviderToolWorker,
            router_ids.environment,
            router_ids.resource,
        );
        let mut journal = router_authority::open_journal(&router_root);
        let receipts =
            router_authority::commit_authority(&mut journal, &router_ids, &intent, 5_000, true);
        Self {
            process_root,
            process_ids,
            _router_root: router_root,
            router_ids,
            router,
            prepared,
            intent,
            receipts: Some(receipts),
        }
    }

    fn caller_binding(&self) -> ExecutionCallerBinding {
        ExecutionCallerBinding::new(
            self.prepared.call().action_id(),
            self.prepared.descriptor().name().clone(),
            self.prepared.descriptor_digest().get(),
            self.prepared.prepared_digest(),
            ExecutionCallerTarget::new(
                self.router_ids.actor,
                ActorRole::ProviderToolWorker,
                self.router_ids.environment,
                self.router_ids.resource,
            ),
        )
    }

    fn dispatch(
        &mut self,
        dispatcher: &mut dyn peritus_tool_router::ToolDispatcher,
    ) -> InvocationHandle {
        let receipts = self.receipts.as_ref().expect("router receipts");
        let request = ToolAuthorizationRequest::new(
            &self.intent,
            &receipts.kernel,
            &receipts.capability,
            &receipts.budget,
            None,
            &receipts.epoch,
            self.router_ids.revision,
            self.router_ids.session,
            receipts.observed_at,
            self.router_ids.revision.workspace_generation(),
            self.router_ids.revision.workspace_revision(),
            self.prepared.prepared_digest(),
        );
        match self
            .router
            .dispatch(self.prepared.clone(), &request, dispatcher)
            .expect("authorized dispatch")
        {
            DispatchOutcome::Active(handle) => handle,
            other => panic!("expected active dispatch, observed {other:?}"),
        }
    }
}

fn router(descriptor: ToolDescriptor) -> ToolRouter {
    let operation = OperationDescriptor::new(
        descriptor.operation().name().clone(),
        descriptor.operation().operation_class(),
        RiskSet::new(descriptor.operation().risks().as_slice().to_vec()).expect("risks"),
    )
    .expect("operation");
    ToolRouter::new(
        ToolRegistry::new(
            vec![Arc::new(descriptor)],
            &OperationRegistry::new(vec![operation]).expect("operations"),
        )
        .expect("registry"),
        RouterLimits::new(2, 4).expect("router limits"),
    )
}

fn tool_call(ids: &router_authority::Ids, arguments: BoundedJson, key: &str) -> ToolCall {
    ToolCall::new(
        ids.action,
        ids.capability.clone(),
        SemanticVersion::new(1, 0, 0).expect("version"),
        arguments,
        CallLimits::new(5_000, 4_096, 512, 512, 512, 2).expect("call limits"),
        ids.revision,
        instant(TOOL_DEADLINE_TICK),
        IdempotencyKey::new(key.to_owned()).expect("idempotency key"),
    )
}

const fn execution_request<'a>(
    ids: &process_authority::Ids,
    plan: &peritus_process::ExecutionPlan,
    intent: &'a peritus_protocol::ActionIntentDto,
    receipts: &'a process_authority::AuthorityReceipts,
) -> ExecutionAuthorizationRequest<'a> {
    ExecutionAuthorizationRequest::new(
        intent,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        plan.digest(),
    )
}

fn gateway(root: &process_authority::TestRoot) -> ExecutionGateway {
    ExecutionGateway::new(ProcessStore::open(root.registry(), root.workspace()).expect("store"))
}

fn artifact_store(root: &process_authority::TestRoot) -> ArtifactStore {
    ArtifactStore::open(
        StoreConfig::new(root.path().join("artifacts"), 4_096, 32_768)
            .expect("artifact configuration"),
    )
    .expect("artifact store")
}

fn await_result(router: &mut ToolRouter, handle: InvocationHandle, first_tick: u64) -> ToolResult {
    let began = Instant::now();
    let deadline = Duration::from_secs(10);
    while began.elapsed() < deadline {
        let elapsed_millis = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let observed_at = instant(first_tick.saturating_add(elapsed_millis));
        let update = router.poll(handle, observed_at).expect("poll execution");
        if let Some(result) = update.terminal() {
            return result.clone();
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("execution did not terminate within the ten-second test deadline")
}

fn candidate_outcome(result: &ToolResult) -> String {
    result
        .structured()
        .and_then(|value| value.property("candidate"))
        .and_then(|value| value.property("outcome"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("candidate outcome missing from {result:?}"))
}

fn fixture_binary() -> String {
    std::env::var("CARGO_BIN_EXE_peritus-c4-process-fixture").expect("Cargo process fixture path")
}

fn json_object<const N: usize>(fields: [(&str, serde_json::Value); N]) -> serde_json::Value {
    let mut value = serde_json::Map::new();
    for (name, field) in fields {
        value.insert(name.to_owned(), field);
    }
    serde_json::Value::Object(value)
}

const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}
