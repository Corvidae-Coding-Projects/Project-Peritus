//! Auxiliary inference cannot inherit workspace, secret, network, or raw-effect authority.

use super::*;
use crate::LocalCompactorSandbox;
use peritus_sandbox::{
    EnvironmentContract, FileDecision, FileOperation, IsolationRequirement, NetworkContract,
    SandboxOperationClass, SandboxPath, SecretContract,
};
use peritus_types::RunId;

#[test]
fn compiled_compactor_contract_admits_exact_inputs_only() {
    let root = tempfile::tempdir().unwrap();
    let work = create_work_directory(root.path()).unwrap();
    let executable = root.path().join("installed");
    let weights = root.path().join("weights");
    std::fs::write(&executable, "projection fixture only").unwrap();
    std::fs::write(&weights, "preinstalled fixture weights").unwrap();
    let contract = contract::command_contract(RunId::new([1; 16]).unwrap(), 1).unwrap();
    let ids = identity::CommandIds::new(RunId::new([1; 16]).unwrap(), 1, &contract).unwrap();
    let resources = ProcessResourcePolicy::new(
        1000,
        4000,
        16 * 1024 * 1024,
        16 * 1024 * 1024,
        8192,
        32,
        256,
        1,
    )
    .unwrap();
    let checked = sandbox::compile(&ids, &work, &executable, &weights, resources).unwrap();
    #[cfg(target_os = "linux")]
    {
        let policy = peritus_sandbox_linux::MountPolicy::new(&work, vec![])
            .unwrap()
            .with_private_filesystem(executable.clone())
            .unwrap();
        let mounts = peritus_sandbox_linux::MountPlan::project(&checked, &policy).unwrap();
        assert!(
            matches!(mounts.actions().first(), Some(peritus_sandbox_linux::MountAction::Tmpfs { target }) if target == Path::new("/"))
        );
    }
    assert_eq!(checked.isolation(), IsolationRequirement::Restricted);
    assert_eq!(checked.operation_class(), SandboxOperationClass::Execution);
    assert_eq!(checked.contract().network(), &NetworkContract::deny_all());
    assert_eq!(checked.contract().secrets(), &SecretContract::deny_all());
    assert_eq!(
        checked.contract().environment(),
        &EnvironmentContract::new(peritus_sandbox::EnvironmentMode::Cleared, vec![]).unwrap()
    );
    for (path, operation, expected) in [
        (weights.clone(), FileOperation::Read, FileDecision::Allowed),
        (weights, FileOperation::Write, FileDecision::DeniedByDefault),
        (executable, FileOperation::Execute, FileDecision::Allowed),
        (root.path().join("credential"), FileOperation::Read, FileDecision::DeniedByDefault),
    ] {
        let path = SandboxPath::new(sandbox::normalized_path(&path).unwrap()).unwrap();
        assert_eq!(checked.contract().filesystem().decide(&path, operation), expected);
    }
}

#[test]
fn missing_or_directory_weights_never_allocate_process_authority() {
    let workspace = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let runtime = CommandRuntime::open_for_test(workspace.path(), RunId::new([1; 16]).unwrap());
    let executable = inputs.path().join("compactor");
    std::fs::write(&executable, "not executed").unwrap();
    let mut config = LocalProcessConfig {
        executable,
        model_path: inputs.path().join("missing"),
        timeout_millis: 1,
        max_input_bytes: 1024,
        max_output_bytes: 1024,
        memory_bytes: 16 * 1024 * 1024,
        sandbox: LocalCompactorSandbox::Linux {
            bubblewrap: inputs.path().join("bwrap"),
            helper: inputs.path().join("helper"),
            cgroup_root: inputs.path().join("delegated"),
        },
    };
    let ordinal = runtime.inner.state.lock().unwrap().next_ordinal;
    assert!(runtime.compact_local(&config, b"{}").unwrap_err().contains("weights unavailable"));
    config.model_path = inputs.path().to_path_buf();
    assert!(runtime.compact_local(&config, b"{}").unwrap_err().contains("weights unavailable"));
    assert_eq!(runtime.inner.state.lock().unwrap().next_ordinal, ordinal);
}
