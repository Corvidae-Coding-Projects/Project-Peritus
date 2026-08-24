//! Platform-neutral Linux backend protocol, projection, recovery, and refinement contracts.

mod support;

#[cfg(target_os = "linux")]
use peritus_sandbox::SandboxResourceKind;
use peritus_sandbox_linux::{
    ActivationRecord, EnvironmentEntry, HelperManifest, LandlockAccess, LandlockRule, LinuxError,
    LinuxErrorKind, LinuxOperation, LinuxRecovery, NativePhase, NetworkIsolation,
    RecoveryClassification, RefinementFacts, RuntimeRecord, TargetCommand,
};
#[cfg(target_os = "linux")]
use peritus_sandbox_linux::{EnforcementLevel, MountAction, MountPlan, MountPolicy, ResourcePlan};
use peritus_types::Sha256Digest;

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}

#[test]
fn manifest_round_trip_is_deterministic_and_checksum_bound() {
    let host_root = tempfile::tempdir().expect("host path fixture");
    let manifest = HelperManifest::new(
        digest(1),
        digest(2),
        digest(3),
        digest(4),
        TargetCommand::new(
            "/usr/bin/printf".to_owned(),
            vec!["%s".to_owned(), "literal;not-shell".to_owned()],
        )
        .expect("target"),
        host_root.path().to_path_buf(),
        host_root.path().join("peritus-contract-test"),
        false,
        vec![EnvironmentEntry::new("MODE".to_owned(), "checked".to_owned()).expect("env")],
        vec![
            LandlockRule::new(host_root.path().to_path_buf(), LandlockAccess::host_read_only())
                .expect("Landlock rule"),
        ],
        support::resource_plan(),
        NetworkIsolation::DenyAll,
        Vec::new(),
    )
    .expect("manifest");
    let bytes = manifest.encode().expect("encode");
    assert_eq!(HelperManifest::decode(&bytes).expect("decode"), manifest);
    assert_eq!(manifest.encode().expect("repeat encode"), bytes);
    assert!(!String::from_utf8_lossy(&bytes).contains("TOKEN_CANARY"));
    let mut corrupted = bytes;
    corrupted[20] ^= 1;
    assert_eq!(
        HelperManifest::decode(&corrupted).expect_err("checksum must reject corruption").kind(),
        LinuxErrorKind::Helper
    );
}

#[test]
fn activation_record_is_fixed_and_corruption_detected() {
    let record = ActivationRecord::new(digest(5), digest(6), digest(7), true, true, true);
    let bytes = record.encode();
    assert_eq!(bytes.len(), 139);
    assert_eq!(ActivationRecord::decode(&bytes).expect("activation decode"), record);
    let mut corrupted = bytes;
    corrupted[40] ^= 1;
    assert!(ActivationRecord::decode(&corrupted).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn filesystem_projection_is_deterministic_and_protected_metadata_dominates() {
    let workspace = tempfile::tempdir().expect("workspace");
    std::fs::create_dir(workspace.path().join(".git")).expect("metadata root");
    std::fs::create_dir(workspace.path().join(".peritus")).expect("Peritus metadata root");
    std::fs::create_dir(workspace.path().join(".crosslink")).expect("Crosslink metadata root");
    std::fs::write(workspace.path().join("input.txt"), b"input").expect("input");
    let plan = support::checked_plan(workspace.path());
    let resources = ResourcePlan::from_sandbox(&plan);
    assert_eq!(
        resources.processes(),
        1,
        "a denied-descendant process contract must narrow the generic resource ceiling"
    );
    assert_eq!(
        resources.enforcement(SandboxResourceKind::CpuTime),
        EnforcementLevel::Supervisor,
        "rounded RLIMIT_CPU is only a safeguard; C2 owns the exact millisecond ceiling"
    );
    assert_eq!(resources.enforcement(SandboxResourceKind::Memory), EnforcementLevel::Hard);
    let policy = MountPolicy::new(workspace.path(), Vec::new()).expect("mount policy");
    let first = MountPlan::project(&plan, &policy).expect("projection");
    let second = MountPlan::project(&plan, &policy).expect("repeat projection");
    assert_eq!(first, second);
    let writable = first
        .actions()
        .iter()
        .position(|action| matches!(action, MountAction::WritableBind { target, .. } if target == policy.workspace_root()))
        .expect("workspace writable bind");
    let masked = first
        .actions()
        .iter()
        .position(
            |action| matches!(action, MountAction::Mask { target } if target.ends_with(".git")),
        )
        .expect("protected mask");
    assert!(writable < masked);
}

#[cfg(target_os = "linux")]
#[test]
fn absent_protected_root_overlapping_creation_fails_closed() {
    let workspace = tempfile::tempdir().expect("workspace");
    std::fs::write(workspace.path().join("input.txt"), b"input").expect("input");
    let plan = support::checked_plan(workspace.path());
    let policy = MountPolicy::new(workspace.path(), Vec::new()).expect("mount policy");
    assert!(MountPlan::project(&plan, &policy).is_err());
    assert!(!workspace.path().join(".peritus").exists());
    assert!(!workspace.path().join(".crosslink").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn protected_root_alias_cannot_escape_the_workspace() {
    let workspace = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("outside");
    let alias = workspace.path().join("metadata-link");
    std::os::unix::fs::symlink(outside.path(), &alias).expect("symlink");
    assert!(MountPolicy::new(workspace.path(), vec![alias]).is_err());
}

#[test]
fn recovery_record_round_trip_and_absent_state_are_fail_closed() {
    let record = RuntimeRecord::new(
        digest(8),
        digest(9),
        "peritus-0123456789abcdef01234567".to_owned(),
        Some(std::process::id()),
        Some(1),
        NativePhase::Activated,
        false,
    )
    .expect("record");
    let bytes = record.encode().expect("encode");
    assert_eq!(RuntimeRecord::decode(&bytes).expect("decode"), record);
    let absent_root = tempfile::tempdir().expect("cgroup stand-in");
    assert_eq!(record.classify(absent_root.path()), RecoveryClassification::Indeterminate);
    assert!(record.cleanup_exact(absent_root.path()).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn recovery_classification_distinguishes_owned_mismatch_and_clean_absence() {
    let root = tempfile::tempdir().expect("cgroup stand-in");
    let leaf_id = "peritus-abcdef0123456789abcdef01";
    let leaf = root.path().join(leaf_id);
    std::fs::create_dir(&leaf).expect("leaf");
    std::fs::write(leaf.join("cgroup.procs"), std::process::id().to_string()).expect("membership");
    let start = process_start_token(std::process::id());
    let owned = RuntimeRecord::new(
        digest(10),
        digest(11),
        leaf_id.to_owned(),
        Some(std::process::id()),
        Some(start),
        NativePhase::Activated,
        false,
    )
    .expect("owned record");
    assert_eq!(owned.classify(root.path()), RecoveryClassification::LiveOwned);

    let mut descendant =
        std::process::Command::new("/bin/sleep").arg("30").spawn().expect("spawn descendant");
    std::fs::write(leaf.join("cgroup.procs"), descendant.id().to_string())
        .expect("descendant membership");
    let descendant_classification = owned.classify(root.path());
    descendant.kill().expect("kill descendant");
    descendant.wait().expect("reap descendant");
    assert_eq!(descendant_classification, RecoveryClassification::LiveOwned);

    let mismatch = RuntimeRecord::new(
        digest(10),
        digest(11),
        leaf_id.to_owned(),
        Some(std::process::id()),
        Some(start.wrapping_add(1)),
        NativePhase::Activated,
        false,
    )
    .expect("mismatch record");
    assert_eq!(mismatch.classify(root.path()), RecoveryClassification::Mismatched);

    std::fs::remove_file(leaf.join("cgroup.procs")).expect("remove membership");
    std::fs::remove_dir(&leaf).expect("remove leaf");
    let clean = RuntimeRecord::new(
        digest(10),
        digest(11),
        leaf_id.to_owned(),
        None,
        None,
        NativePhase::Released,
        true,
    )
    .expect("clean record");
    assert_eq!(clean.classify(root.path()), RecoveryClassification::AbsentClean);
}

#[cfg(target_os = "linux")]
fn process_start_token(pid: u32) -> u64 {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("process stat");
    let close = text.rfind(')').expect("command close");
    text[close + 2..]
        .split_ascii_whitespace()
        .nth(19)
        .expect("start token")
        .parse()
        .expect("numeric start token")
}

#[test]
fn refinement_predicates_deny_mismatch_and_require_empty_cleanup() {
    let mismatch = RefinementFacts {
        required_features: 1,
        supported_features: 1,
        plan_digest: digest(1),
        manifest_plan_digest: digest(2),
        descriptor_digest: digest(3),
        manifest_descriptor_digest: digest(3),
        probe_digest: digest(4),
        manifest_probe_digest: digest(4),
        preparation_digest: digest(5),
        manifest_preparation_digest: digest(5),
        process_activated: false,
        network_activated: false,
        secrets_activated: false,
        cleanup_complete: true,
        owned_backend_resources: 0,
        owned_proxy_resources: 0,
        owned_secret_resources: 0,
    };
    assert!(!mismatch.admission_is_exact());
    assert!(mismatch.mismatch_has_no_activation());
    assert!(mismatch.complete_teardown_is_empty());

    let activated_mismatch = RefinementFacts { process_activated: true, ..mismatch };
    assert!(!activated_mismatch.mismatch_has_no_activation());
    let leaked_proxy = RefinementFacts { owned_proxy_resources: 1, ..mismatch };
    assert!(!leaked_proxy.complete_teardown_is_empty());
    let exact = RefinementFacts { manifest_plan_digest: digest(1), ..mismatch };
    assert!(exact.admission_is_exact());
}

#[test]
fn error_detail_is_bounded_on_utf8_boundary() {
    let detail = "é".repeat(1_000);
    let error = LinuxError::new(
        LinuxErrorKind::Io,
        LinuxOperation::Probe,
        LinuxRecovery::ConfigureHost,
        detail,
    );
    assert!(error.detail().len() <= 512);
    assert!(error.to_string().starts_with("PERITUS-LINUX-IO-001"));
}
