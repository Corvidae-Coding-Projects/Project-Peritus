//! Production C4 plan compilers fed complete C2 fixture inputs.

use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionCallerBinding, ExecutionPlan,
    GracefulAction, IoMode, OutputOverflowAction, OutputPolicy, ProcessResourcePolicy, StdinPolicy,
    WorkingDirectory, WorkspaceAccess,
};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_tool_protocol::PreparedToolCall;
use peritus_tools_quality::{CheckDefinition, EnvironmentProfile, QualityPlanInputs};
use peritus_tools_shell::ExecutionPlanInputs;

use crate::process_authority::{Ids, TestRoot};

pub fn shell_plan(
    root: &TestRoot,
    ids: &Ids,
    prepared: &PreparedToolCall,
    caller: ExecutionCallerBinding,
    executable: &str,
    arguments: Vec<String>,
    stdin: StdinPolicy,
) -> (ExecutionPlan, CheckedSandboxPlan, BackendAdmission) {
    let (inputs, sandbox, admission) = inputs(root, ids, executable, stdin);
    let plan = inputs
        .compile(
            prepared,
            caller,
            CommandSpec::new(executable, arguments).expect("shell command"),
            &sandbox,
            &admission,
        )
        .expect("shell plan");
    (plan, sandbox, admission)
}

pub fn quality_plan(
    root: &TestRoot,
    ids: &Ids,
    prepared: &PreparedToolCall,
    caller: ExecutionCallerBinding,
    definition: &CheckDefinition,
) -> (ExecutionPlan, CheckedSandboxPlan, BackendAdmission) {
    let (inputs, sandbox, admission) =
        inputs(root, ids, definition.executable(), StdinPolicy::Closed);
    let quality = QualityPlanInputs {
        identity: inputs.identity,
        working_directory: inputs.working_directory,
        environment: inputs.environment,
        environment_profile: EnvironmentProfile::new("integration").expect("profile"),
        io_mode: inputs.io_mode,
        stdin: inputs.stdin,
        output: inputs.output,
        deadlines: inputs.deadlines,
        resources: inputs.resources,
    };
    let plan =
        quality.compile(prepared, caller, definition, &sandbox, &admission).expect("quality plan");
    (plan, sandbox, admission)
}

fn inputs(
    root: &TestRoot,
    ids: &Ids,
    executable: &str,
    stdin: StdinPolicy,
) -> (ExecutionPlanInputs, CheckedSandboxPlan, BackendAdmission) {
    let environment = EnvironmentPlan::cleared(Vec::new()).expect("cleared environment");
    let output =
        OutputPolicy::new(4, 512, 4_096, 512, 4_096, 4_096, 4_096, OutputOverflowAction::Terminate)
            .expect("output policy");
    let deadlines = DeadlinePolicy::new(Some(5_000), GracefulAction::Terminate, 100, 1_000)
        .expect("deadline policy");
    let resources = ProcessResourcePolicy::new(
        5_000,
        5_000,
        64 * 1_024 * 1_024,
        1_024 * 1_024,
        4_096,
        1,
        32,
        1,
    )
    .expect("resource policy");
    let working_directory = WorkingDirectory::open(
        root.workspace(),
        ids.workspace,
        ids.resource,
        ids.environment,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        WorkspaceAccess::ReadOnly,
    )
    .expect("working directory");
    let (sandbox, admission) =
        super::sandbox(ids, &root.workspace(), executable, &environment, resources, stdin);
    (
        ExecutionPlanInputs {
            identity: ids.identity(),
            working_directory,
            environment,
            io_mode: IoMode::Pipes,
            stdin,
            output,
            deadlines,
            resources,
        },
        sandbox,
        admission,
    )
}
