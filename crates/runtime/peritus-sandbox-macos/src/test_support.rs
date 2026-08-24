//! Crate-local fixtures for private lifecycle tests.

use peritus_process::CommandSpec;
use peritus_sandbox::{
    AdmissionProfile, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, InputPermission, IsolationRequirement, NetworkContract, PathScope,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SecretDelivery, SecretGrant, SecretReference, SignalPolicy, TerminalContract,
    TerminalLimits, TerminalMode, TerminalModes, TerminalRequirements, TerminalSignalPermission,
    TreeContainment, admit_backend, compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use std::path::Path;

use crate::{
    HelperManifest, MacosDescriptor, MacosHostProbe, ProcessContainment, ProfileCompiler,
    ResourceControlPlan, ResourceProbe, SecretHandleDescriptor, SecretHandleDestination,
    TerminalMapping,
};

pub(crate) fn manifest() -> HelperManifest {
    manifest_with_options(None, 10, Path::new("/workspace"))
}

pub(crate) fn manifest_with_file_secret(path: SandboxPath) -> HelperManifest {
    manifest_with_options(Some(path), 10, Path::new("/workspace"))
}

pub(crate) fn manifest_with_exec_status(
    exec_status_descriptor: u32,
    working_directory: &Path,
) -> HelperManifest {
    manifest_with_options(None, exec_status_descriptor, working_directory)
}

fn manifest_with_options(
    file_secret: Option<SandboxPath>,
    exec_status_descriptor: u32,
    working_directory: &Path,
) -> HelperManifest {
    let resources = limits();
    let filesystem = FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/bin/tool").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .unwrap(),
    ])
    .unwrap();
    let process = ProcessContract::new(
        vec![SandboxPath::new("/bin/tool").unwrap()],
        DescendantPolicy::Bounded(1),
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        2,
    )
    .unwrap();
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Denied,
        TerminalLimits::new(None, ResourceQuantity::new(16), ResourceQuantity::new(100)).unwrap(),
    )
    .unwrap();
    let (secret_contract, secret_requirements, secret_descriptors) = secret_fixture(file_secret);
    let contract = SandboxContract::new(
        filesystem,
        process,
        EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).unwrap(),
        NetworkContract::deny_all(),
        secret_contract,
        resources,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(SandboxPath::new("/bin/tool").unwrap(), FileOperation::Execute)],
        ProcessRequirements::new(SandboxPath::new("/bin/tool").unwrap(), 1, true),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).unwrap(),
        Vec::new(),
        secret_requirements,
        resources,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Denied,
            None,
            ResourceQuantity::new(8),
            ResourceQuantity::new(50),
        )
        .unwrap(),
    )
    .unwrap();
    let plan = compile_sandbox(
        binding(),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .unwrap();
    let descriptor = MacosDescriptor::from_probe(
        MacosHostProbe::from_evidence(crate::ProbeEvidence::supported_fixture()).unwrap(),
    )
    .unwrap();
    let admission =
        admit_backend(&plan, descriptor.descriptor(), AdmissionProfile::Production).unwrap();
    let profile = ProfileCompiler::compile(&plan, working_directory, &[], None).unwrap();
    HelperManifest::build(
        plan.binding().process_id(),
        &plan,
        admission.descriptor_digest(),
        admission.support_digest(),
        admission.preparation_digest(),
        &profile,
        "/usr/bin/sandbox-exec".into(),
        &CommandSpec::new("/bin/tool", std::iter::empty::<String>()).unwrap(),
        working_directory.to_path_buf(),
        Vec::new(),
        exec_status_descriptor,
        None,
        ResourceControlPlan::from_checked_plan(&plan, ResourceProbe::macos_production().levels()),
        ProcessContainment::from_checked_plan(&plan),
        TerminalMapping::from_checked_plan(&plan).unwrap(),
        secret_descriptors,
    )
    .unwrap()
}

fn secret_fixture(
    file_secret: Option<SandboxPath>,
) -> (SecretContract, Vec<SecretGrant>, Vec<SecretHandleDescriptor>) {
    file_secret.map_or_else(
        || (SecretContract::deny_all(), Vec::new(), Vec::new()),
        |path| {
            let reference = SecretReference::new(
                ResourceId::new([19; 16]).unwrap(),
                Sha256Digest::new([20; 32]),
            );
            let grant = SecretGrant::new(reference, SecretDelivery::File(path.clone()));
            let descriptor = SecretHandleDescriptor::new(
                9,
                "peritus-macos-test-secret".to_owned(),
                8,
                crate::secret_reference_digest(reference),
                SecretHandleDestination::File(path),
            )
            .unwrap();
            (SecretContract::new(vec![grant.clone()]).unwrap(), vec![grant], vec![descriptor])
        },
    )
}

fn limits() -> ResourceLimits {
    ResourceLimits::new(
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
        ResourceQuantity::new(100),
    )
    .unwrap()
}

fn binding() -> SandboxBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([11; 16]).unwrap(),
        HarnessId::new([12; 16]).unwrap(),
        WorkspaceId::new([13; 16]).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([14; 16]).unwrap(),
        ProviderProfileId::new([15; 16]).unwrap(),
    );
    SandboxBinding::new(
        ProcessId::new([16; 16]).unwrap(),
        ResourceId::new([17; 16]).unwrap(),
        EnvironmentId::new([18; 16]).unwrap(),
        revision,
    )
}
