//! Actual C2 gateway and Linux backend qualification with disposable files and children.
//!
//! Arguments: bubblewrap helper delegated-cgroup-root shell. All are caller-selected paths.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use peritus_process::{
        CancellationReason, ExecutionGateway, OsExitObservation, OutputStream, ProcessStore,
    };
    use std::{
        fs,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    let mut arguments = std::env::args_os().skip(1);
    let selected = (0..4)
        .map(|_| arguments.next().map(PathBuf::from))
        .collect::<Option<Vec<_>>>()
        .ok_or("expected bubblewrap helper delegated-cgroup-root shell")?;
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    let scratch = tempfile::tempdir()?;
    let work = scratch.path().join("work");
    let sibling = scratch.path().join("sibling");
    let state = scratch.path().join("authority");
    let registry = scratch.path().join("registry");
    fs::create_dir_all(&work)?;
    fs::create_dir_all(&sibling)?;
    fs::create_dir_all(&state)?;
    for protected in [".git", ".peritus", ".crosslink"] {
        fs::create_dir_all(work.join(protected))?;
    }
    let canary = sibling.join("canary");
    fs::write(&canary, b"original")?;
    let shell = fs::canonicalize(&selected[3])?;
    let gateway = ExecutionGateway::new(ProcessStore::open(&registry, &work)?);
    let paths = Paths {
        work,
        state,
        bubblewrap: selected[0].clone(),
        helper: selected[1].clone(),
        cgroup: selected[2].clone(),
        shell,
    };

    let first = launch(
        &gateway,
        &paths,
        1,
        "printf OUT; printf ERR >&2; printf accepted > allowed.txt",
        vec![],
    )?;
    let first_control = first.control();
    let first_terminal = first.wait()?;
    assert!(
        matches!(first_terminal.os_exit(), OsExitObservation::Code(0)),
        "allowed child failed: {first_terminal:?}"
    );
    assert!(first_terminal.tree_cleanup_complete() && first_terminal.support_tasks_joined());
    assert_eq!(first_control.full_spooled_stream_output(OutputStream::Stdout)?, b"OUT");
    assert_eq!(first_control.full_spooled_stream_output(OutputStream::Stderr)?, b"ERR");
    assert_eq!(fs::read(paths.work.join("allowed.txt"))?, b"accepted");
    assert_eq!(fs::read(&canary)?, b"original");

    let second = launch(
        &gateway,
        &paths,
        2,
        "printf forbidden > \"$1\"",
        vec!["fixture".into(), canary.to_string_lossy().into_owned()],
    )?;
    let second_terminal = second.wait()?;
    assert!(
        !matches!(second_terminal.os_exit(), OsExitObservation::Code(0)),
        "outside write unexpectedly succeeded: {second_terminal:?}"
    );
    assert!(second_terminal.tree_cleanup_complete() && second_terminal.support_tasks_joined());
    assert_eq!(fs::read(&canary)?, b"original");

    let third = launch(&gateway, &paths, 3, "/usr/bin/sleep 30 & printf READY; wait", vec![])?;
    let third_control = third.control();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !third_control.retained_stream_output(OutputStream::Stdout).ends_with(b"READY") {
        if Instant::now() >= deadline {
            let _ = third_control.cancel(CancellationReason::User);
            let _ = third.wait();
            return Err("owned descendant did not become ready".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    third_control.cancel(CancellationReason::User)?;
    let third_terminal = third.wait()?;
    assert!(
        third_terminal.tree_cleanup_complete() && third_terminal.support_tasks_joined(),
        "owned descendant cleanup incomplete: {third_terminal:?}"
    );
    assert_eq!(fs::read(&canary)?, b"original");
    run_output_matrix(&gateway, &paths, &canary)?;
    println!(
        "stage6-native-process pass: allowed/denied filesystem, owned descendant, exact output matrix, sibling canary"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
struct Paths {
    work: std::path::PathBuf,
    state: std::path::PathBuf,
    bubblewrap: std::path::PathBuf,
    helper: std::path::PathBuf,
    cgroup: std::path::PathBuf,
    shell: std::path::PathBuf,
}

#[cfg(target_os = "linux")]
fn launch(
    gateway: &peritus_process::ExecutionGateway,
    paths: &Paths,
    ordinal: u64,
    script: &str,
    additional: Vec<String>,
) -> Result<peritus_process::OwnedProcess, Box<dyn std::error::Error>> {
    launch_with_output_policy(gateway, paths, ordinal, script, additional, None)
}

#[cfg(target_os = "linux")]
fn launch_with_output_policy(
    gateway: &peritus_process::ExecutionGateway,
    paths: &Paths,
    ordinal: u64,
    script: &str,
    additional: Vec<String>,
    selected_output_policy: Option<peritus_process::OutputPolicy>,
) -> Result<peritus_process::OwnedProcess, Box<dyn std::error::Error>> {
    use peritus_process::{
        CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionPlan, GracefulAction, IoMode,
        NativeSandboxBackend, OutputOverflowAction, OutputPolicy, ProcessResourcePolicy,
        StdinPolicy, WorkingDirectory, WorkspaceAccess,
    };
    use peritus_product_runner::RestrictedProcessContext;
    use peritus_sandbox::{
        AdmissionProfile, DescendantPolicy, EnvironmentContract, EnvironmentMode,
        EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement,
        FilesystemContract, FilesystemRule, InputPermission, IsolationRequirement, NetworkContract,
        PathScope, ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits,
        RuleEffect, SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath,
        SandboxRequirements, SecretContract, SignalPolicy, TerminalContract, TerminalLimits,
        TerminalMode, TerminalModes, TerminalRequirements, TerminalSignalPermission,
        TreeContainment, admit_backend, compile_sandbox,
    };
    use peritus_sandbox_linux::{LinuxBackend, LinuxBackendConfig};
    use peritus_types::{ResourceQuantity, RunId};

    let source_run = RunId::new([21; 16]).expect("fixed nonzero fixture identity");
    let context = RestrictedProcessContext::new(&paths.state, source_run, ordinal)?;
    let identity = context.identity();
    let executable = sandbox_path(&paths.shell)?;
    let work_path = sandbox_path(&paths.work)?;
    let null_device = SandboxPath::new("/dev/null")?;
    let memory = 512 * 1024 * 1024;
    let disk = 64 * 1024 * 1024;
    let output = 1024 * 1024;
    let policy = ProcessResourcePolicy::new(
        Some(10_000),
        Some(10_000),
        memory,
        disk,
        Some(output),
        3,
        128,
        1,
    )?;
    let limits = ResourceLimits::new(
        ResourceQuantity::new(10_000),
        ResourceQuantity::new(10_000),
        ResourceQuantity::new(memory),
        ResourceQuantity::new(disk),
        ResourceQuantity::new(output),
        ResourceQuantity::new(128),
        ResourceQuantity::new(3),
        ResourceQuantity::new(1),
    )?;
    let executable_ops = FileOperationSet::from_operations([
        FileOperation::Discover,
        FileOperation::Metadata,
        FileOperation::Read,
        FileOperation::Execute,
    ]);
    let mut rules = vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            work_path.clone(),
            PathScope::Descendants,
            FileOperationSet::from_operations([
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Read,
                FileOperation::Execute,
                FileOperation::Create,
                FileOperation::Write,
                FileOperation::Remove,
            ]),
        )?,
        FilesystemRule::new(
            RuleEffect::Allow,
            executable.clone(),
            PathScope::Exact,
            executable_ops,
        )?,
        FilesystemRule::new(
            RuleEffect::Allow,
            null_device.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Read,
                FileOperation::Write,
                FileOperation::Metadata,
            ]),
        )?,
    ];
    for directory in
        ["/usr/bin", "/usr/lib", "/usr/lib64", "/usr/libexec", "/bin", "/lib", "/lib64"]
    {
        if let Ok(canonical) = std::fs::canonicalize(directory) {
            rules.push(FilesystemRule::new(
                RuleEffect::Allow,
                sandbox_path(&canonical)?,
                PathScope::Descendants,
                executable_ops,
            )?);
        }
    }
    let event_bound = ResourceQuantity::new(256);
    let output_bound = ResourceQuantity::new(output);
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(None, event_bound, output_bound)?,
    )?;
    let checked = compile_sandbox(
        SandboxBinding::new(
            identity.process_id(),
            identity.resource_id(),
            identity.environment_id(),
            identity.revision(),
        ),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        SandboxContract::new(
            FilesystemContract::new(rules)?,
            ProcessContract::new(
                vec![executable.clone()],
                DescendantPolicy::Bounded(2),
                SignalPolicy::GracefulAndForced,
                TreeContainment::Required,
                3,
            )?,
            EnvironmentContract::new(EnvironmentMode::Cleared, vec![])?,
            NetworkContract::deny_all(),
            SecretContract::deny_all(),
            limits,
            terminal,
        ),
        SandboxRequirements::new(
            vec![
                FileRequirement::new(executable.clone(), FileOperation::Execute),
                FileRequirement::new(null_device.clone(), FileOperation::Read),
                FileRequirement::new(null_device, FileOperation::Write),
            ],
            ProcessRequirements::new(executable, 2, true),
            EnvironmentRequirements::new(vec![], vec![])?,
            vec![],
            vec![],
            limits,
            TerminalRequirements::new(
                TerminalMode::Pipes,
                InputPermission::Denied,
                ResizePermission::Denied,
                TerminalSignalPermission::Allowed,
                None,
                event_bound,
                output_bound,
            )?,
        )?,
    )?;
    let backend = LinuxBackend::new(LinuxBackendConfig::new(
        paths.work.clone(),
        Vec::new(),
        paths.bubblewrap.clone(),
        paths.helper.clone(),
        paths.cgroup.clone(),
        None,
    )?)?;
    let admission = admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)?;
    let working = WorkingDirectory::open(
        &paths.work,
        identity.workspace_id(),
        identity.resource_id(),
        identity.environment_id(),
        identity.revision().workspace_generation(),
        identity.revision().workspace_revision(),
        WorkspaceAccess::Writable,
    )?;
    let mut args = vec!["-c".to_owned(), script.to_owned()];
    args.extend(additional);
    let plan = ExecutionPlan::new(
        identity,
        CommandSpec::new(paths.shell.to_string_lossy().into_owned(), args)?,
        working,
        EnvironmentPlan::cleared(vec![])?,
        IoMode::Pipes,
        StdinPolicy::Closed,
        selected_output_policy.unwrap_or(OutputPolicy::new(
            1024,
            4096,
            output,
            256,
            output,
            output,
            output,
            OutputOverflowAction::Terminate,
        )?),
        DeadlinePolicy::new(Some(10_000), GracefulAction::Terminate, 250, 4_000)?,
        policy,
        &checked,
        &admission,
    )?;
    let authority = context.authorize(&plan)?;
    let request = authority.request(&plan)?;
    let launched = gateway.launch_with_backend(&request, plan, &checked, &admission, backend)?;
    Ok(launched)
}

#[cfg(target_os = "linux")]
fn sandbox_path(
    path: &std::path::Path,
) -> Result<peritus_sandbox::SandboxPath, Box<dyn std::error::Error>> {
    Ok(peritus_sandbox::SandboxPath::new(path.to_str().ok_or("non-UTF8 fixture path")?.to_owned())?)
}

#[cfg(target_os = "linux")]
fn run_output_matrix(
    gateway: &peritus_process::ExecutionGateway,
    paths: &Paths,
    canary: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use peritus_praxis::stage6::process_output::{expected, native_cases};
    use peritus_process::{
        CancellationReason, OsExitObservation, OutputCompleteness, OutputOverflowAction,
        OutputPolicy, OutputStream, TerminalDisposition,
    };
    use std::fs;

    for (index, case) in native_cases().iter().enumerate() {
        let expectation = expected(case);
        let encoded = case.bytes.iter().map(|byte| format!("\\{byte:03o}")).collect::<String>();
        let mut script = format!("printf '{encoded}'{}", if case.stderr { " >&2" } else { "" });
        if case.terminate && expectation.dropped > 0 {
            script.push_str("; /usr/bin/sleep 5");
        }
        let action = if case.terminate {
            OutputOverflowAction::Terminate
        } else {
            OutputOverflowAction::ContinueIncomplete
        };
        let policy = OutputPolicy::new(
            u64::from(case.chunk),
            u64::from(case.window),
            u64::from(case.spool),
            u64::from(case.events),
            u64::from(case.stream_ceiling),
            u64::from(case.stream_ceiling),
            u64::from(case.stream_ceiling),
            action,
        )?;
        let child = launch_with_output_policy(
            gateway,
            paths,
            20 + index as u64,
            &script,
            Vec::new(),
            Some(policy),
        )?;
        let control = child.control();
        let terminal = child.wait()?;
        let record = terminal
            .output()
            .streams()
            .iter()
            .find(|record| record.stream() == expectation.stream)
            .ok_or("missing output accounting stream")?;
        assert_eq!(record.observed(), expectation.observed, "case {index}: observed bytes");
        assert_eq!(
            record.retained(),
            expectation.spooled.len() as u64,
            "case {index}: spool count"
        );
        assert_eq!(record.dropped(), expectation.dropped, "case {index}: dropped bytes");
        assert_eq!(
            record.completeness(),
            if expectation.dropped > 0 {
                OutputCompleteness::Truncated
            } else {
                OutputCompleteness::Complete
            },
            "case {index}: completeness"
        );
        assert_eq!(
            control.full_spooled_stream_output(expectation.stream)?,
            expectation.spooled,
            "case {index}: exact spool bytes"
        );
        assert_eq!(
            control.retained_stream_output(expectation.stream),
            expectation.tail,
            "case {index}: retained tail"
        );
        let other = if case.stderr { OutputStream::Stdout } else { OutputStream::Stderr };
        assert!(control.full_spooled_stream_output(other)?.is_empty());
        assert!(control.retained_stream_output(other).is_empty());
        if case.terminate && expectation.dropped > 0 {
            assert_eq!(terminal.disposition(), TerminalDisposition::OutputLimit);
            assert_eq!(
                terminal.first_trigger().map(|trigger| trigger.reason()),
                Some(CancellationReason::OutputLimit)
            );
        } else {
            assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
            assert!(matches!(terminal.os_exit(), OsExitObservation::Code(0)));
            assert!(terminal.first_trigger().is_none());
        }
        assert!(terminal.tree_cleanup_complete() && terminal.support_tasks_joined());
        assert_eq!(fs::read(canary)?, b"original");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("stage6-native-process unsupported on this OS");
}
