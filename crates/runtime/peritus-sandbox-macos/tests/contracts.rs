//! Unix-hosted macOS compiler, protocol, and recovery contracts.

#![cfg(unix)]

mod support;

use std::{
    io::{Cursor, Read},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use peritus_process::CommandSpec;
use peritus_sandbox::{
    AdmissionProfile, FileOperation, FileOperationSet, FilesystemRule, NetworkContract, PathScope,
    RuleEffect, SandboxPath, SandboxResourceKind, admit_backend,
};
use peritus_sandbox_macos::{
    ActivationRecord, CleanupProgress, EnforcementLevel, HelperLaunch, HelperManifest,
    InheritedDescriptor, MacosDescriptor, MacosHostProbe, MacosRecoveryRecord, ManifestHandle,
    ProcessContainment, ProfileCompiler, ProfileDecision, ProxyRoute, RecoveryClassification,
    ResourceControlPlan, RuntimeIdentity, TerminalMapping,
};
use peritus_types::Sha256Digest;

#[test]
fn profile_is_deterministic_escapes_paths_and_protects_metadata() {
    let quote_rule = FilesystemRule::new(
        RuleEffect::Allow,
        SandboxPath::new("/workspace/a\"b").unwrap(),
        PathScope::Exact,
        FileOperationSet::from_operations([
            FileOperation::Discover,
            FileOperation::Metadata,
            FileOperation::Read,
        ]),
    )
    .unwrap();
    let plan = support::checked_plan(vec![quote_rule]);
    let left = ProfileCompiler::compile(&plan, "/workspace".as_ref(), &[], None).unwrap();
    let right = ProfileCompiler::compile(&plan, "/workspace".as_ref(), &[], None).unwrap();
    assert_eq!(left, right);
    assert!(left.text().starts_with("(version 1)\n(deny default)\n"));
    assert!(left.text().contains("(literal \"/workspace/a\\\"b\")"));
    assert!(left.text().contains("(allow process-fork)"));
    assert!(!left.text().contains("(deny process-fork)"));
    assert!(left.text().contains("(deny file-read* (subpath \"/workspace/.git\"))"));
    assert!(left.text().contains("(deny file-write* (subpath \"/workspace/.git\"))"));
    assert!(left.text().contains("(deny process-exec (subpath \"/workspace/.git\"))"));
    assert_eq!(
        left.decide(&SandboxPath::new("/workspace/.git/config").unwrap(), FileOperation::Read),
        ProfileDecision::DeniedExplicitly
    );
    assert_eq!(
        left.decide(&SandboxPath::new("/workspace/input").unwrap(), FileOperation::Read),
        ProfileDecision::Allowed
    );
}

#[test]
fn compiler_rejects_unrepresentable_metadata_split_and_requires_proxy_for_egress() {
    let split = FilesystemRule::new(
        RuleEffect::Allow,
        SandboxPath::new("/workspace/split").unwrap(),
        PathScope::Exact,
        FileOperationSet::from_operations([FileOperation::Discover]),
    )
    .unwrap();
    let plan = support::checked_plan(vec![split]);
    assert!(ProfileCompiler::compile(&plan, "/workspace".as_ref(), &[], None).is_err());

    let deny_plan = support::checked_plan(Vec::new());
    let extraneous_proxy =
        ProxyRoute::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 31_337), 9).unwrap();
    assert!(
        ProfileCompiler::compile(&deny_plan, "/workspace".as_ref(), &[], Some(extraneous_proxy),)
            .is_err()
    );

    let network = NetworkContract::new(vec![support::tcp_rule(RuleEffect::Allow)]).unwrap();
    let plan = support::checked_plan_with_network(Vec::new(), network);
    assert!(ProfileCompiler::compile(&plan, "/workspace".as_ref(), &[], None).is_err());
    let proxy =
        ProxyRoute::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 31_337), 9).unwrap();
    let profile = ProfileCompiler::compile(&plan, "/workspace".as_ref(), &[], Some(proxy)).unwrap();
    assert!(profile.text().contains("127.0.0.1:31337"));
    assert!(profile.text().contains("(deny network-inbound)"));
    assert!(!profile.text().contains("(deny network*)"));
}

#[test]
fn manifest_round_trip_is_canonical_and_tamper_evident() {
    let plan = support::checked_plan(Vec::new());
    let descriptor = MacosDescriptor::from_probe(
        MacosHostProbe::from_evidence(peritus_sandbox_macos::ProbeEvidence::supported_fixture())
            .unwrap(),
    )
    .unwrap();
    let admission =
        admit_backend(&plan, descriptor.descriptor(), AdmissionProfile::Production).unwrap();
    let profile = ProfileCompiler::compile(&plan, "/workspace".as_ref(), &[], None).unwrap();
    let resources = ResourceControlPlan::from_checked_plan(
        &plan,
        peritus_sandbox_macos::ResourceProbe::macos_production().levels(),
    );
    let manifest = HelperManifest::build(
        plan.binding().process_id(),
        &plan,
        admission.descriptor_digest(),
        admission.support_digest(),
        admission.preparation_digest(),
        &profile,
        "/usr/bin/sandbox-exec".into(),
        &CommandSpec::new("/bin/tool", ["a b", "$(not-a-shell)"]).unwrap(),
        "/workspace".into(),
        vec![
            peritus_sandbox_macos::EnvironmentEntry::new(
                "PERITUS_LITERAL".to_owned(),
                "literal value".to_owned(),
            )
            .unwrap(),
        ],
        10,
        None,
        resources,
        ProcessContainment::from_checked_plan(&plan),
        TerminalMapping::from_checked_plan(&plan).unwrap(),
        Vec::new(),
    )
    .unwrap();
    let decoded = HelperManifest::decode(manifest.canonical_bytes()).unwrap();
    assert_eq!(decoded, manifest);
    assert_eq!(decoded.target_arguments(), ["a b", "$(not-a-shell)"]);
    assert_eq!(decoded.environment()[0].name(), "PERITUS_LITERAL");
    assert_eq!(decoded.environment()[0].value(), "literal value");
    assert_eq!(
        decoded.resources().control(SandboxResourceKind::Processes).level(),
        EnforcementLevel::Supervisor
    );
    assert_eq!(
        decoded.resources().control(SandboxResourceKind::CpuTime).level(),
        EnforcementLevel::Supervisor
    );
    assert!(decoded.containment().new_process_group());
    assert!(decoded.containment().tree_required());
    assert_eq!(decoded.containment().descendant_limit(), 2);
    assert_eq!(decoded.terminal(), TerminalMapping::Pipes { input: false });

    let helper_launch = HelperLaunch::new(
        "/opt/peritus/bin/peritus-macos-sandbox-helper".into(),
        ManifestHandle::protected_stdin(),
        None,
        &[],
        ProcessContainment::from_checked_plan(&plan),
        TerminalMapping::from_checked_plan(&plan).unwrap(),
    )
    .unwrap();
    assert!(helper_launch.arguments().is_empty());
    assert_eq!(helper_launch.inherited_descriptors(), [InheritedDescriptor::Manifest(0)]);

    let mut tampered = manifest.canonical_bytes().to_vec();
    tampered[20] ^= 1;
    assert!(HelperManifest::decode(&tampered).is_err());
    let activation = ActivationRecord::new(manifest.digest(), manifest.preparation_digest());
    assert!(
        ActivationRecord::decode(activation.encode())
            .matches(manifest.digest(), manifest.preparation_digest())
    );

    let mut framed = Vec::new();
    framed
        .extend_from_slice(&u32::try_from(manifest.canonical_bytes().len()).unwrap().to_le_bytes());
    framed.extend_from_slice(manifest.canonical_bytes());
    framed.extend_from_slice(b"target input remains unread");
    let mut channel = Cursor::new(framed);
    assert_eq!(HelperManifest::read_framed(&mut channel).unwrap(), manifest);
    let mut target_input = Vec::new();
    channel.read_to_end(&mut target_input).unwrap();
    assert_eq!(target_input, b"target input remains unread");
    assert_ne!(peritus_process::native_ready_record(), Sha256Digest::new([0; 32]));
}

#[test]
fn recovery_classification_never_claims_mismatched_ownership() {
    let identity = RuntimeIdentity::new(
        support::binding().process_id(),
        Sha256Digest::new([1; 32]),
        Sha256Digest::new([2; 32]),
        Sha256Digest::new([3; 32]),
        None,
        None,
        Some(44),
        Some(44),
    );
    let record =
        MacosRecoveryRecord::new(identity, true, CleanupProgress::prepared(false, false)).unwrap();
    assert_eq!(record.classify(Some(identity), true), RecoveryClassification::LiveOwned);
    let mismatch = RuntimeIdentity::new(
        support::binding().process_id(),
        Sha256Digest::new([9; 32]),
        Sha256Digest::new([2; 32]),
        Sha256Digest::new([3; 32]),
        None,
        None,
        Some(44),
        Some(44),
    );
    assert_eq!(record.classify(Some(mismatch), true), RecoveryClassification::Mismatched);
    assert_eq!(record.classify(Some(identity), false), RecoveryClassification::Indeterminate);
    assert_eq!(MacosRecoveryRecord::decode(record.canonical_bytes()).unwrap(), record);
}

#[test]
fn non_macos_system_probe_is_strictly_unsupported() {
    #[cfg(not(target_os = "macos"))]
    {
        let probe = peritus_sandbox_macos::SystemProbe::run(
            &peritus_sandbox_macos::ProbeRequest::new(
                "/helper".into(),
                "/usr/bin/sandbox-exec".into(),
                None,
                std::time::Duration::from_millis(10),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!probe.core_supported());
        assert_eq!(probe.supported_features().bits(), 0);
    }
}
