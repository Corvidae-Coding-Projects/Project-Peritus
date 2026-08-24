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
    AclAccess, AppContainerProfile, CleanupState, HelperExit, HelperManifest,
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
fn descriptor_admission_and_inert_config_fail_closed() {
    let plan = support::checked_plan(Vec::new());
    let (descriptor, admission) = descriptor_and_admission(&plan);
    assert_eq!(admission.descriptor_digest(), descriptor.common().digest());
    let unsupported = WindowsProbe::from_evidence(ProbeEvidence::unsupported()).unwrap();
    assert!(!unsupported.core_supported());
    assert_eq!(unsupported.supported_features().bits(), 0);

    let invalid = WindowsBackendConfig::new(
        "/tmp/helper.exe".into(),
        WindowsPath::new(r"C:\workspace").unwrap(),
        Vec::new(),
        "/tmp/acl".into(),
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
    let config = WindowsBackendConfig::new(
        "/tmp/peritus-windows-helper.exe".into(),
        WindowsPath::new(r"C:\workspace").unwrap(),
        vec![WindowsPath::new(r"C:\workspace\.git").unwrap()],
        "/tmp/peritus-acl".into(),
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
