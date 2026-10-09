//! Platform-neutral Windows compiler, protocol, and recovery contracts.

mod support;

use std::{
    io::{Cursor, Read},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use peritus_process::CommandSpec;
use peritus_sandbox::{
    AdmissionProfile, FileOperation, FileOperationSet, FilesystemRule, PathScope, RuleEffect,
    SandboxPath, SandboxResourceKind, admit_backend,
};
use peritus_sandbox_windows::{
    AclAccess, AppContainerProfile, CleanupState, EnvironmentEntry, HelperExit, HelperManifest,
    InheritedHandlePolicy, JobPlan, NetworkIsolation, PathEvidence, PathPolicy, ProbeEvidence,
    RecoveryClassification, RecoveryProbe, ReservedHelperExit, ResourceControlPlan,
    RuntimeIdentity, TerminalMapping, TokenProfile, WindowsBackend, WindowsBackendConfig,
    WindowsBackendDescriptor, WindowsPath, WindowsPhase, WindowsProbe, WindowsRecoveryRecord,
    compile_acl_plan, managed_wfp_policy_digest, production_resource_levels,
};
use peritus_types::Sha256Digest;

#[test]
fn paths_reject_device_ads_reserved_and_escape_forms() {
    let root = WindowsPath::new(r"c:\workspace\").unwrap();
    let child = WindowsPath::new(r"C:\workspace\src\main.rs").unwrap();
    assert_eq!(root.as_str(), "C:/workspace");
    assert!(root.contains(&child));
    assert_eq!(WindowsPath::new(r"c:\workspace\src\main.rs").unwrap(), child);
    for invalid in [
        r"\\server\share\x",
        r"\\?\C:\workspace",
        r"C:\workspace\..\escape",
        r"C:\workspace\file:stream",
        r"C:\workspace\CON.txt",
        r"C:\workspace\trail. ",
    ] {
        assert!(WindowsPath::new(invalid).is_err(), "accepted {invalid}");
    }
    let evidence = PathEvidence::new(child.clone(), child, 44, false, true).unwrap();
    assert!(peritus_sandbox_windows::ResolvedWindowsPath::from_evidence(evidence).is_err());
}

#[test]
fn external_inference_inputs_require_explicit_admission_and_cannot_be_writable() {
    let policy = PathPolicy::new(WindowsPath::new("C:/workspace").unwrap(), vec![]).unwrap();
    let external = WindowsPath::new("D:/models/local").unwrap();
    let rule = |operation| {
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("D:/models/local/weights.bin").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([operation]),
        )
        .unwrap()
    };
    let read = support::checked_plan(vec![rule(FileOperation::Read)]);
    assert!(compile_acl_plan(&read, &policy, "S-1-15-2-123").is_err());
    let admitted = policy.clone().with_read_only_inputs(vec![external]).unwrap();
    let acl = compile_acl_plan(&read, &admitted, "S-1-15-2-123").unwrap();
    assert!(
        acl.entries().iter().any(|entry| entry.path().as_str() == "D:/models/local/weights.bin"
            && !entry.access().contains(FileOperation::Write))
    );
    for operation in [FileOperation::Write, FileOperation::Remove, FileOperation::Create] {
        assert!(
            compile_acl_plan(
                &support::checked_plan(vec![rule(operation)]),
                &admitted,
                "S-1-15-2-123"
            )
            .is_err()
        );
    }
    let nested = policy
        .clone()
        .with_read_only_inputs(vec![WindowsPath::new("C:/workspace/private").unwrap()])
        .unwrap();
    let readonly_acl = compile_acl_plan(
        &read,
        &nested.with_read_only_inputs(vec![WindowsPath::new("D:/models").unwrap()]).unwrap(),
        "S-1-15-2-123",
    )
    .unwrap();
    assert!(
        readonly_acl.entries().iter().any(|entry| entry.effect() == RuleEffect::Deny
            && entry.access().contains(FileOperation::Write))
    );
    assert!(policy.with_read_only_inputs(vec![WindowsPath::new("C:/").unwrap()]).is_ok());
}

#[test]
fn acl_projection_is_deterministic_operation_complete_and_deny_dominant() {
    let extra = FilesystemRule::new(
        RuleEffect::Deny,
        SandboxPath::new("/workspace/private").unwrap(),
        PathScope::Descendants,
        FileOperationSet::from_operations([FileOperation::Read, FileOperation::Write]),
    )
    .unwrap();
    let plan = support::checked_plan(vec![extra]);
    let policy = PathPolicy::new(
        WindowsPath::new(r"C:\workspace").unwrap(),
        vec![WindowsPath::new(r"C:\workspace\.git").unwrap()],
    )
    .unwrap();
    let left = compile_acl_plan(&plan, &policy, "S-1-15-2-123").unwrap();
    let right = compile_acl_plan(&plan, &policy, "S-1-15-2-123").unwrap();
    assert_eq!(left, right);
    let transaction = left.planned();
    assert_eq!(transaction.cleanup_state(), CleanupState::Complete);
    assert_eq!(transaction.pending_reversal_count(), 0);
    assert!(left.entries().iter().any(|entry| {
        entry.effect() == RuleEffect::Deny
            && entry.path().as_str() == "C:/workspace/.git"
            && entry.access() == AclAccess::all()
    }));
    assert!(left.entries().iter().any(|entry| {
        entry.path().as_str() == "C:/workspace/workspace/private"
            && entry.access().contains(FileOperation::Read)
            && entry.access().contains(FileOperation::Write)
    }));
}

#[test]
fn manifest_round_trip_binds_every_domain_and_preserves_target_input() {
    let plan = support::checked_plan(Vec::new());
    let (descriptor, admission) = descriptor_and_admission(&plan);
    let policy = PathPolicy::new(WindowsPath::new(r"C:\workspace").unwrap(), Vec::new()).unwrap();
    let token = token();
    let acl = compile_acl_plan(&plan, &policy, token.principal_sid()).unwrap();
    let resources = ResourceControlPlan::from_checked_plan(&plan, production_resource_levels());
    let manifest = HelperManifest::build(
        plan.binding().process_id(),
        &plan,
        &admission,
        descriptor.identity().helper_digest(),
        &acl,
        token,
        &CommandSpec::new("/bin/tool", ["a b", "$(not-a-shell)"]).unwrap(),
        WindowsPath::new(r"C:\workspace").unwrap(),
        Vec::new(),
        JobPlan::from_checked_plan(&plan),
        peritus_sandbox_windows::ProcessPolicy::from_checked_plan(&plan),
        TerminalMapping::from_checked_plan(&plan).unwrap(),
        resources,
        NetworkIsolation::DenyAll,
        Vec::new(),
        InheritedHandlePolicy::new(Vec::new()).unwrap(),
    )
    .unwrap();
    let decoded = HelperManifest::decode(manifest.canonical_bytes()).unwrap();
    assert_eq!(decoded, manifest);
    assert_eq!(decoded.arguments(), ["a b", "$(not-a-shell)"]);
    assert_eq!(
        decoded.resources().control(SandboxResourceKind::Memory).level(),
        peritus_sandbox_windows::EnforcementLevel::Hard
    );
    assert_eq!(decoded.terminal(), TerminalMapping::Pipes { input: false });
    assert!(decoded.job().kill_on_close());
    assert!(decoded.process().tree_required());

    let mut tampered = manifest.canonical_bytes().to_vec();
    tampered[20] ^= 1;
    assert!(HelperManifest::decode(&tampered).is_err());
    let mut framed = Vec::new();
    framed
        .extend_from_slice(&u32::try_from(manifest.canonical_bytes().len()).unwrap().to_le_bytes());
    framed.extend_from_slice(manifest.canonical_bytes());
    framed.extend_from_slice(b"target input remains unread");
    let mut input = Cursor::new(framed);
    assert_eq!(HelperManifest::read_framed(&mut input).unwrap(), manifest);
    let mut remaining = Vec::new();
    input.read_to_end(&mut remaining).unwrap();
    assert_eq!(remaining, b"target input remains unread");
}

#[test]
fn helper_manifest_uses_wire_representations_instead_of_application_collection_ceilings() {
    let plan = support::checked_plan(Vec::new());
    let (descriptor, admission) = descriptor_and_admission(&plan);
    let policy = PathPolicy::new(WindowsPath::new(r"C:\workspace").unwrap(), Vec::new()).unwrap();
    let token = token();
    let acl = compile_acl_plan(&plan, &policy, token.principal_sid()).unwrap();
    let arguments =
        (0..4_097).map(|index| format!("arg-{index}")).chain(["x".repeat(4 * 1_024 * 1_024 + 1)]);
    let command = CommandSpec::new("/bin/tool", arguments).unwrap();
    let environment = (0..4_097)
        .map(|index| {
            EnvironmentEntry::new(
                format!("VARIABLE_{index}"),
                if index == 0 { "v".repeat(1_048_577) } else { "v".to_owned() },
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let resources = ResourceControlPlan::from_checked_plan(&plan, production_resource_levels());
    let manifest = HelperManifest::build(
        plan.binding().process_id(),
        &plan,
        &admission,
        descriptor.identity().helper_digest(),
        &acl,
        token,
        &command,
        WindowsPath::new(r"C:\workspace").unwrap(),
        environment,
        JobPlan::from_checked_plan(&plan),
        peritus_sandbox_windows::ProcessPolicy::from_checked_plan(&plan),
        TerminalMapping::from_checked_plan(&plan).unwrap(),
        resources,
        NetworkIsolation::DenyAll,
        Vec::new(),
        InheritedHandlePolicy::new(Vec::new()).unwrap(),
    )
    .unwrap();

    assert!(manifest.canonical_bytes().len() > 4 * 1_024 * 1_024);
    let decoded = HelperManifest::decode(manifest.canonical_bytes()).unwrap();
    assert_eq!(decoded.arguments().len(), 4_098);
    assert_eq!(decoded.environment().len(), 4_097);
}

#[test]
fn descriptor_admission_and_inert_config_fail_closed() {
    let plan = support::checked_plan(Vec::new());
    let (descriptor, admission) = descriptor_and_admission(&plan);
    assert_eq!(admission.descriptor_digest(), descriptor.common().digest());
    let unsupported = WindowsProbe::from_evidence(ProbeEvidence::unsupported()).unwrap();
    assert!(!unsupported.core_supported());
    assert_eq!(unsupported.supported_features().bits(), 0);

    let root = tempfile::tempdir().unwrap();
    let invalid = WindowsBackendConfig::new(
        root.path().join("helper.exe"),
        WindowsPath::new(r"C:\workspace").unwrap(),
        Vec::new(),
        root.path().join("acl"),
        token(),
        Some(Sha256Digest::new([9; 32])),
        None,
        None,
    );
    assert!(invalid.is_err());
    assert_eq!(HelperExit::from_code(120), HelperExit::Reserved(ReservedHelperExit::Protocol));
    assert_eq!(HelperExit::from_code(7), HelperExit::Target(7));
    assert_eq!(
        HelperExit::from_activated_code(ReservedHelperExit::TargetCreate.code()),
        HelperExit::Target(ReservedHelperExit::TargetCreate.code())
    );
}

#[test]
fn supported_backend_admits_deny_all_without_starting_native_effects() {
    let plan = support::checked_plan(Vec::new());
    let helper_digest = Sha256Digest::new([0xC3; 32]);
    let root = tempfile::tempdir().unwrap();
    let config = WindowsBackendConfig::new(
        root.path().join("peritus-windows-helper.exe"),
        WindowsPath::new(r"C:\workspace").unwrap(),
        vec![WindowsPath::new(r"C:\workspace\.git").unwrap()],
        root.path().join("peritus-acl"),
        token(),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(!config.has_proxy_preparation());
    assert!(!config.has_secret_preparation());
    let probe = WindowsProbe::from_evidence(supported_evidence(helper_digest)).unwrap();
    let backend = WindowsBackend::from_probe(config, probe).unwrap();
    assert!(backend.admit(&plan).is_ok());
}

#[test]
fn recovery_record_is_canonical_monotonic_and_never_guesses_ownership() {
    let identity = RuntimeIdentity::new(
        support::binding().process_id(),
        Sha256Digest::new([1; 32]),
        Sha256Digest::new([2; 32]),
        Sha256Digest::new([3; 32]),
        Sha256Digest::new([4; 32]),
        Sha256Digest::new([5; 32]),
    );
    let mut record = WindowsRecoveryRecord::prepared(identity);
    record.advance(WindowsPhase::Activated, false, false, false).unwrap();
    assert!(record.advance(WindowsPhase::Prepared, false, false, false).is_err());
    record.advance(WindowsPhase::Terminated, false, false, true).unwrap();
    record.advance(WindowsPhase::Released, true, true, true).unwrap();
    assert!(record.cleanup_complete());
    assert_eq!(WindowsRecoveryRecord::decode(record.canonical_bytes()).unwrap(), record);
    assert_eq!(
        peritus_sandbox_windows::classify(Some(&record), RecoveryProbe::Absent),
        RecoveryClassification::AbsentClean
    );
    let mismatch = RuntimeIdentity::new(
        support::binding().process_id(),
        Sha256Digest::new([9; 32]),
        Sha256Digest::new([2; 32]),
        Sha256Digest::new([3; 32]),
        Sha256Digest::new([4; 32]),
        Sha256Digest::new([5; 32]),
    );
    assert_eq!(
        peritus_sandbox_windows::classify(Some(&record), RecoveryProbe::LiveOwned(mismatch),),
        RecoveryClassification::Mismatched
    );
}

#[test]
fn prepared_abort_cleanup_is_canonical_and_proves_clean_absence() {
    let identity = RuntimeIdentity::new(
        support::binding().process_id(),
        Sha256Digest::new([11; 32]),
        Sha256Digest::new([12; 32]),
        Sha256Digest::new([13; 32]),
        Sha256Digest::new([14; 32]),
        Sha256Digest::new([15; 32]),
    );
    let mut record = WindowsRecoveryRecord::prepared(identity);
    assert!(record.record_cleanup(true, false, true).is_err());
    record.record_cleanup(true, true, true).unwrap();
    assert_eq!(record.phase(), WindowsPhase::Prepared);
    assert!(record.cleanup_complete());
    let decoded = WindowsRecoveryRecord::decode(record.canonical_bytes()).unwrap();
    assert_eq!(decoded, record);
    assert_eq!(
        peritus_sandbox_windows::classify(Some(&record), RecoveryProbe::Absent),
        RecoveryClassification::AbsentClean
    );
}

#[test]
fn managed_wfp_policy_identity_binds_controller_target_plan_and_proxy() {
    let endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 43_443);
    let controller = Sha256Digest::new([21; 32]);
    let plan = Sha256Digest::new([22; 32]);
    let exact = managed_wfp_policy_digest(controller, "S-1-15-2-123", endpoint, plan);
    assert_eq!(exact, managed_wfp_policy_digest(controller, "S-1-15-2-123", endpoint, plan));
    assert_ne!(
        exact,
        managed_wfp_policy_digest(
            controller,
            "S-1-15-2-124",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 43_444),
            plan,
        )
    );
}

fn descriptor_and_admission(
    plan: &peritus_sandbox::CheckedSandboxPlan,
) -> (WindowsBackendDescriptor, peritus_sandbox::BackendAdmission) {
    let descriptor = WindowsBackendDescriptor::from_probe(
        WindowsProbe::from_evidence(supported_evidence(Sha256Digest::new([0xC3; 32]))).unwrap(),
        None,
    )
    .unwrap();
    let admission = admit_backend(plan, descriptor.common(), AdmissionProfile::Production).unwrap();
    (descriptor, admission)
}

const fn supported_evidence(helper_digest: Sha256Digest) -> ProbeEvidence {
    let mut evidence = ProbeEvidence::supported_fixture();
    evidence.helper_digest = Some(helper_digest);
    evidence.managed_network = false;
    evidence
}

fn token() -> TokenProfile {
    TokenProfile::AppContainer(AppContainerProfile::new("Peritus.Test", "S-1-15-2-123").unwrap())
}

#[cfg(not(target_os = "windows"))]
#[test]
fn app_container_sid_derivation_requires_a_native_windows_host() {
    let error = AppContainerProfile::derive_for_current_host("Peritus.Test")
        .expect_err("non-Windows host cannot derive an AppContainer SID");
    assert!(error.to_string().contains("native Windows host"));
}

#[test]
fn selected_features_and_optional_resources_do_not_require_unused_controls() {
    use peritus_sandbox::{ResourceLimits, SandboxFeature};
    use peritus_sandbox_windows::EnforcementLevel;
    use peritus_types::ResourceQuantity;
    let mut evidence = supported_evidence(Sha256Digest::new([0xC3; 32]));
    evidence.conpty = false;
    evidence.credential_manager = false;
    evidence.resources[0] = EnforcementLevel::Unsupported;
    evidence.resources[1] = EnforcementLevel::Unsupported;
    evidence.resources[4] = EnforcementLevel::Unsupported;
    let probe = WindowsProbe::from_evidence(evidence).unwrap();
    assert!(probe.supported_features().contains(SandboxFeature::FilesystemRead));
    assert!(!probe.supported_features().contains(SandboxFeature::Pty));
    assert!(!probe.supported_features().contains(SandboxFeature::CpuTime));
    let quantity = ResourceQuantity::new(100);
    let limits = ResourceLimits::with_optional_wall_output_cpu(
        None, None, None, quantity, quantity, quantity, quantity, quantity,
    )
    .unwrap();
    let plan = support::checked_plan_with_limits(Vec::new(), limits);
    let resources =
        ResourceControlPlan::select_checked_plan(&plan, probe.evidence().resources).unwrap();
    assert_eq!(resources.control(SandboxResourceKind::CpuTime).ceiling(), 0);
    let descriptor = WindowsBackendDescriptor::from_probe(probe, None).unwrap();
    assert!(admit_backend(&plan, descriptor.common(), AdmissionProfile::Production).is_ok());
    let finite = support::checked_plan(Vec::new());
    assert!(
        ResourceControlPlan::select_checked_plan(&finite, descriptor.probe().evidence().resources)
            .is_err()
    );
}

#[test]
fn every_optional_resource_combination_requires_only_selected_native_controls() {
    use peritus_sandbox::{ResourceLimits, SandboxFeature};
    use peritus_sandbox_windows::EnforcementLevel;
    use peritus_types::ResourceQuantity;
    let quantity = ResourceQuantity::new(100);
    for mask in 0_u8..8 {
        let wall = (mask & 1 != 0).then_some(quantity);
        let cpu = (mask & 2 != 0).then_some(quantity);
        let output = (mask & 4 != 0).then_some(quantity);
        let limits = ResourceLimits::with_optional_wall_output_cpu(
            wall, output, cpu, quantity, quantity, quantity, quantity, quantity,
        )
        .unwrap();
        let plan = support::checked_plan_with_limits(Vec::new(), limits);
        let mut evidence = supported_evidence(Sha256Digest::new([0xC3; 32]));
        for (index, feature, selected) in [
            (0, SandboxFeature::WallTime, wall),
            (1, SandboxFeature::CpuTime, cpu),
            (4, SandboxFeature::Output, output),
        ] {
            assert_eq!(plan.required_features().contains(feature), selected.is_some());
            if selected.is_none() {
                evidence.resources[index] = EnforcementLevel::Unsupported;
            }
        }
        let descriptor = WindowsBackendDescriptor::from_probe(
            WindowsProbe::from_evidence(evidence).unwrap(),
            None,
        )
        .unwrap();
        assert!(
            admit_backend(&plan, descriptor.common(), AdmissionProfile::Production).is_ok(),
            "mask {mask}"
        );
        assert!(
            ResourceControlPlan::select_checked_plan(
                &plan,
                descriptor.probe().evidence().resources
            )
            .is_ok()
        );
    }
}

#[test]
fn large_deduplicated_native_roots_and_protected_descendants_keep_deny_precedence() {
    let workspace = WindowsPath::new("C:/workspace").unwrap();
    let protected = WindowsPath::new("C:/workspace/workspace/private").unwrap();
    let mut roots = (0..513)
        .map(|index| WindowsPath::new(format!("D:/private/{index}")).unwrap())
        .collect::<Vec<_>>();
    roots.extend(vec![protected.clone(); 600]);
    let policy = PathPolicy::new(workspace.clone(), roots).unwrap();
    assert_eq!(policy.protected_roots().len(), 514);
    let plan = support::checked_plan(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace/private/file").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Write]),
        )
        .unwrap(),
    ]);
    let acl = compile_acl_plan(&plan, &policy, "S-1-15-2-123").unwrap();
    assert!(
        acl.entries()
            .iter()
            .any(|entry| entry.effect() == RuleEffect::Deny && entry.path() == &protected)
    );
    assert!(
        !acl.entries()
            .iter()
            .any(|entry| entry.effect() == RuleEffect::Allow && protected.contains(entry.path()))
    );
    let unicode = WindowsPath::new("C:/workspace/Étage").unwrap();
    assert!(unicode.contains(&WindowsPath::new("c:/workspace/étage/file").unwrap()));
    assert!(!unicode.contains(&WindowsPath::new("C:/workspace/Étage-other").unwrap()));
    assert!(
        PathPolicy::new(workspace, vec![unicode, WindowsPath::new("C:/workspace/étage").unwrap()])
            .is_err()
    );
}

#[test]
fn protected_ancestor_is_projected_only_inside_the_configured_authority() {
    let workspace = WindowsPath::new("C:/workspace").unwrap();
    let policy =
        PathPolicy::new(workspace.clone(), vec![WindowsPath::new("C:/").unwrap()]).unwrap();
    let plan =
        compile_acl_plan(&support::checked_plan(Vec::new()), &policy, "S-1-15-2-123").unwrap();
    assert!(plan.entries().iter().all(|entry| workspace.contains(entry.path())));
    assert!(
        plan.entries()
            .iter()
            .any(|entry| entry.effect() == RuleEffect::Deny && entry.path() == &workspace)
    );
    assert!(!plan.entries().iter().any(|entry| entry.effect() == RuleEffect::Allow));
}

#[test]
fn a_drive_root_workspace_resolves_logical_components_without_an_empty_component() {
    let policy = PathPolicy::new(WindowsPath::new("C:/").unwrap(), Vec::new()).unwrap();
    assert_eq!(
        policy.resolve_logical(&SandboxPath::new("/bin/tool").unwrap()).unwrap().as_str(),
        "C:/bin/tool"
    );
}
