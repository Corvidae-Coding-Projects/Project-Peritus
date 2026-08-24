pub mod authority;
mod authority_kernel;
mod authority_lease;
mod journal;
mod sandbox;

pub use authority::{commit_authority, commit_authority_with_lease, intent};
pub use journal::open as open_journal;

use std::{fs, path::Path};

use peritus_codec::CodecLimits;
use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, EnvironmentVariable, ExecutionIdentity,
    ExecutionPlan, GracefulAction, IoMode, OutputOverflowAction, OutputPolicy,
    ProcessResourcePolicy, StdinPolicy, WorkingDirectory, WorkspaceAccess,
};
use peritus_protocol::AcceptanceContractDto;
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, BudgetReservationId, CapabilityName, EnvironmentId,
    HarnessId, PolicyId, ProcessId, ProjectId, ProviderProfileId, ResourceId, RevisionNumber,
    RevisionTuple, RunId, SessionId, TurnId, WorkspaceId,
};
use tempfile::TempDir;

pub struct TestRoot {
    temporary: TempDir,
}

impl TestRoot {
    pub fn new() -> Self {
        let temporary = TempDir::new().expect("temporary process root");
        fs::create_dir(temporary.path().join("workspace")).expect("workspace directory");
        fs::create_dir(temporary.path().join("registry")).expect("registry directory");
        Self { temporary }
    }

    pub fn path(&self) -> &Path {
        self.temporary.path()
    }

    pub fn workspace(&self) -> std::path::PathBuf {
        self.path().join("workspace")
    }

    pub fn registry(&self) -> std::path::PathBuf {
        self.path().join("registry")
    }
}

pub struct Ids {
    pub workspace: WorkspaceId,
    pub resource: ResourceId,
    pub environment: EnvironmentId,
    pub actor: ActorId,
    pub session: SessionId,
    pub action: ActionId,
    pub process: ProcessId,
    pub capability: CapabilityName,
    pub revision: RevisionTuple,
    pub project: ProjectId,
    pub run: RunId,
    pub attempt: AttemptId,
    pub turn: TurnId,
    pub kernel_root_budget: BudgetId,
    pub kernel_child_budget: BudgetId,
    pub execution_budget: BudgetId,
    pub reservation: BudgetReservationId,
}

impl Ids {
    pub fn new(seed: u8) -> Self {
        let contract =
            contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("acceptance contract");
        let value = |offset: u8| seed.wrapping_add(offset).max(1);
        let workspace = WorkspaceId::new([value(1); 16]).expect("workspace");
        let revision = RevisionTuple::new(
            contract.id(),
            HarnessId::new([value(2); 16]).expect("harness"),
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([value(3); 16]).expect("policy"),
            ProviderProfileId::new([value(4); 16]).expect("provider"),
        );
        Self {
            workspace,
            resource: ResourceId::new([value(5); 16]).expect("resource"),
            environment: EnvironmentId::new([value(6); 16]).expect("environment"),
            actor: ActorId::new([value(7); 16]).expect("actor"),
            session: SessionId::new([value(8); 16]).expect("session"),
            action: ActionId::new([value(9); 16]).expect("action"),
            process: ProcessId::new([value(10); 16]).expect("process"),
            capability: CapabilityName::new("process.execute".to_owned()).expect("capability"),
            revision,
            project: ProjectId::new([value(11); 16]).expect("project"),
            run: RunId::new([value(12); 16]).expect("run"),
            attempt: AttemptId::new([value(13); 16]).expect("attempt"),
            turn: TurnId::new([value(14); 16]).expect("turn"),
            kernel_root_budget: BudgetId::new([value(15); 16]).expect("root budget"),
            kernel_child_budget: BudgetId::new([value(16); 16]).expect("child budget"),
            execution_budget: BudgetId::new([value(17); 16]).expect("execution budget"),
            reservation: BudgetReservationId::new([value(18); 16]).expect("reservation"),
        }
    }

    pub const fn identity(&self) -> ExecutionIdentity {
        ExecutionIdentity::new(
            self.project,
            self.session,
            self.run,
            self.attempt,
            self.turn,
            self.action,
            self.process,
            self.workspace,
            self.resource,
            self.environment,
            self.actor,
            self.revision,
        )
    }
}

pub struct PlanOptions<'a> {
    pub arguments: Vec<String>,
    pub environment: Vec<(&'a str, &'a str)>,
    pub io: IoMode,
    pub stdin: StdinPolicy,
    pub output_limit: u64,
    pub wall_timeout: Option<u64>,
    pub graceful: GracefulAction,
    pub grace_millis: u64,
    pub process_count: u64,
    pub descendants: u32,
    pub workspace_access: WorkspaceAccess,
    pub resize_allowed: bool,
    pub environment_authority: Option<(Vec<&'a str>, Vec<&'a str>)>,
    pub resource_fidelity: peritus_sandbox::ResourceFidelity,
}

pub fn plan(
    root: &TestRoot,
    ids: &Ids,
    options: PlanOptions<'_>,
) -> Result<ExecutionPlan, peritus_process::ProcessError> {
    build_plan(root, ids, options, false).map(|(execution, _, _)| execution)
}

#[allow(dead_code, reason = "shared support is also compiled by the raw conformance target")]
pub fn native_plan(
    root: &TestRoot,
    ids: &Ids,
    options: PlanOptions<'_>,
) -> Result<
    (ExecutionPlan, peritus_sandbox::CheckedSandboxPlan, peritus_sandbox::BackendAdmission),
    peritus_process::ProcessError,
> {
    build_plan(root, ids, options, true)
}

fn build_plan(
    root: &TestRoot,
    ids: &Ids,
    options: PlanOptions<'_>,
    native: bool,
) -> Result<
    (ExecutionPlan, peritus_sandbox::CheckedSandboxPlan, peritus_sandbox::BackendAdmission),
    peritus_process::ProcessError,
> {
    let executable = fixture_binary();
    let command =
        CommandSpec::new(executable.clone(), options.arguments).expect("structured command");
    let working_directory = WorkingDirectory::open(
        root.workspace(),
        ids.workspace,
        ids.resource,
        ids.environment,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        options.workspace_access,
    )
    .expect("working directory");
    let bindings = options
        .environment
        .iter()
        .map(|(name, value)| EnvironmentVariable::new(*name, *value).expect("environment"))
        .collect();
    let environment = match &options.environment_authority {
        Some((inherited, _)) => EnvironmentPlan::allowlisted(
            inherited.iter().map(|name| (*name).to_owned()).collect(),
            bindings,
        ),
        None => EnvironmentPlan::cleared(bindings),
    }
    .expect("environment plan");
    let output = OutputPolicy::new(
        options.output_limit.min(4),
        options.output_limit.max(64),
        options.output_limit.max(64),
        256,
        options.output_limit,
        options.output_limit,
        options.output_limit,
        OutputOverflowAction::Terminate,
    )
    .expect("output policy");
    let wall = options.wall_timeout.unwrap_or(5_000).max(100);
    let deadlines =
        DeadlinePolicy::new(options.wall_timeout, options.graceful, options.grace_millis, 1_000)
            .expect("deadline policy");
    let resources = ProcessResourcePolicy::new(
        wall,
        wall,
        64 * 1_024 * 1_024,
        1_024 * 1_024,
        options.output_limit.max(64),
        options.process_count,
        32,
        1,
    )
    .expect("resources");
    let mut sandbox_projection = sandbox::Projection::literal(
        &environment,
        options.io,
        options.stdin,
        resources,
        options.descendants,
        options.resource_fidelity,
    )
    .with_resize(options.resize_allowed);
    if let Some((inherited, literals)) = options.environment_authority {
        sandbox_projection = sandbox_projection.with_environment(inherited, literals);
    }
    let (sandbox, admission) = if native {
        sandbox::compile_native(ids, &executable, sandbox_projection)
    } else {
        sandbox::compile(ids, &executable, sandbox_projection)
    };
    let execution = ExecutionPlan::new(
        ids.identity(),
        command,
        working_directory,
        environment,
        options.io,
        options.stdin,
        output,
        deadlines,
        resources,
        &sandbox,
        &admission,
    )?;
    Ok((execution, sandbox, admission))
}

pub fn fixture_binary() -> String {
    let test_binary = std::env::current_exe().expect("integration test executable");
    let profile = test_binary
        .parent()
        .and_then(Path::parent)
        .expect("Cargo integration test profile directory");
    let fixture = profile.join(format!("peritus-process-fixture{}", std::env::consts::EXE_SUFFIX));
    assert!(fixture.is_file(), "fixture binary was not built: {}", fixture.display());
    fixture.into_os_string().into_string().expect("UTF-8 fixture binary path")
}

#[allow(dead_code, reason = "shared support is also compiled by the raw conformance target")]
pub fn native_helper_binary() -> String {
    sibling_binary("peritus-native-helper-fixture")
}

#[allow(dead_code, reason = "shared support is also compiled by the raw conformance target")]
fn sibling_binary(name: &str) -> String {
    let test_binary = std::env::current_exe().expect("integration test executable");
    let profile = test_binary
        .parent()
        .and_then(Path::parent)
        .expect("Cargo integration test profile directory");
    let binary = profile.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(binary.is_file(), "fixture binary was not built: {}", binary.display());
    binary.into_os_string().into_string().expect("UTF-8 fixture binary path")
}

pub fn contract_dto() -> AcceptanceContractDto {
    let current = std::env::current_dir().expect("test working directory");
    let path = current
        .ancestors()
        .map(|root| root.join("protocol/fixtures/v1/acceptance-contract.bin"))
        .find(|path| path.is_file())
        .expect("checked-in acceptance contract path");
    let bytes = fs::read(path).expect("checked-in acceptance contract");
    peritus_codec::decode_message(&bytes, CodecLimits::PRODUCTION).expect("contract DTO")
}
