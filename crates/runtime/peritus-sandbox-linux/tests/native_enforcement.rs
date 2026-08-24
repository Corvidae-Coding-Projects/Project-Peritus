#![doc = "Real Linux probe, helper handshake, policy installation, and namespace enforcement tests."]
#![cfg(target_os = "linux")]

#[path = "native_enforcement/native_support.rs"]
mod native_support;
#[path = "native_enforcement/network.rs"]
mod network;
mod support;

use native_support::*;
use peritus_process::NativeProtectedHandle;
use peritus_sandbox::{EnvironmentName, SecretDelivery, SecretGrant, SecretReference};
use peritus_sandbox_linux::{
    InheritedHandle, LandlockAccess, LandlockRule, LinuxBackendDescriptor, LinuxProbe,
    LinuxProtectedPayload, MINIMUM_KERNEL, MINIMUM_LANDLOCK_ABI, MountPlan, MountPolicy,
    ProbeRequest, ProtectedPayloadBinding, TargetCommand,
};
use peritus_types::ResourceId;
use std::io::{Read, Seek, Write};
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn real_probe_reports_host_facilities_without_false_cgroup_claims() {
    let _guard = native_test_guard();
    let host_listener = TcpListener::bind("127.0.0.1:0").expect("host listener");
    let proxy = peritus_sandbox_linux::ProxyRoute::new(
        host_listener.local_addr().expect("listener address"),
    )
    .expect("proxy route");
    let request = ProbeRequest::new(
        PathBuf::from("/usr/bin/bwrap"),
        helper_path(),
        PathBuf::from("/sys/fs/cgroup"),
        Some(proxy),
    )
    .expect("probe request");
    let probe = LinuxProbe::run(&request).expect("probe");
    let expected_baseline = probe.kernel().is_some_and(|version| version >= MINIMUM_KERNEL)
        && probe.architecture().supported()
        && probe.namespaces().complete()
        && probe.bubblewrap().functional()
        && probe.helper_digest().is_some()
        && probe.landlock_abi().is_some_and(|abi| abi >= MINIMUM_LANDLOCK_ABI)
        && probe.seccomp()
        && probe.pty();
    assert_eq!(probe.baseline_supported(), expected_baseline);
    assert_eq!(probe.namespaces().functional, probe.bubblewrap().functional());
    assert!(
        !probe.proxy_reachable(),
        "a host-loopback listener must not be reachable from the fresh network namespace"
    );
    if std::fs::OpenOptions::new().write(true).open("/sys/fs/cgroup/cgroup.procs").is_err() {
        assert!(!probe.cgroup().delegated());
    }
}

#[test]
fn missing_and_nonfunctional_installations_never_advertise_support() {
    let _guard = native_test_guard();
    let missing = tempfile::tempdir().expect("missing installation root");
    let request = ProbeRequest::new(
        missing.path().join("missing-bwrap"),
        missing.path().join("missing-helper"),
        missing.path().join("missing-cgroup"),
        None,
    )
    .expect("missing probe request");
    let probe = LinuxProbe::run(&request).expect("missing probe");
    assert!(!probe.baseline_supported());
    assert!(probe.helper_digest().is_none());
    assert!(!probe.namespaces().functional);
    assert!(LinuxBackendDescriptor::from_probe(&probe).is_err());

    let request = ProbeRequest::new(
        PathBuf::from("/usr/bin/false"),
        helper_path(),
        missing.path().join("cgroup"),
        None,
    )
    .expect("nonfunctional probe request");
    let probe = LinuxProbe::run(&request).expect("nonfunctional probe");
    let descriptor = LinuxBackendDescriptor::from_probe(&probe).expect("identified binaries");
    assert!(!probe.baseline_supported());
    assert_eq!(descriptor.common().supported_features().bits(), 0);
}

#[test]
fn helper_performs_bound_handshake_installs_controls_and_execs_literal_argv() {
    let _guard = native_test_guard();
    let workspace = tempfile::tempdir().expect("workspace");
    let manifest = manifest(
        TargetCommand::new(
            "/usr/bin/printf".to_owned(),
            vec!["%s".to_owned(), "literal;not-shell".to_owned()],
        )
        .expect("target"),
        workspace.path(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
    );
    let output = run_direct_helper(&manifest);
    assert!(output.status.success(), "helper stderr: {}", output.stderr);
    assert_eq!(output.target_stdout, b"literal;not-shell");
}

#[test]
fn helper_keeps_protocol_pipes_separate_from_target_pty() {
    let _guard = native_test_guard();
    let workspace = tempfile::tempdir().expect("workspace");
    let manifest = manifest_for_terminal(
        TargetCommand::new(
            "/usr/bin/printf".to_owned(),
            vec!["%s".to_owned(), "pty-output".to_owned()],
        )
        .expect("target"),
        workspace.path(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
        true,
    );
    let pair = nix::pty::openpty(None::<&nix::pty::Winsize>, None::<&nix::sys::termios::Termios>)
        .expect("PTY pair");
    nix::fcntl::fcntl(&pair.master, nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC))
        .expect("close-on-exec PTY master");
    let slave_path = nix::unistd::ttyname(&pair.slave).expect("PTY slave path");
    drop(pair.slave);
    let mut master = std::fs::File::from(pair.master);
    let bytes = manifest.encode().expect("manifest encode");
    let manifest_digest = peritus_codec::sha256(&bytes);
    let mut command = Command::new(helper_path());
    command
        .args([
            "--run",
            "--manifest-digest",
            &hex(manifest_digest),
            "--preparation-digest",
            &hex(manifest.preparation_digest()),
        ])
        .env(peritus_process::NATIVE_PTY_SLAVE_ENV, slave_path);
    let output = run_spawned_with_bytes(command, &manifest, &bytes, manifest_digest);
    assert!(output.status.success(), "helper stderr: {}", output.stderr);
    assert!(output.target_stdout.is_empty());
    let mut target_output = [0_u8; 64];
    let read = master.read(&mut target_output).expect("target PTY output");
    assert_eq!(&target_output[..read], b"pty-output");
}

#[test]
fn helper_self_attaches_to_manifest_cgroup_before_activation() {
    let _guard = native_test_guard();
    let workspace = tempfile::tempdir().expect("workspace");
    let leaf = workspace.path().join("peritus-test-cgroup");
    let manifest = manifest(
        TargetCommand::new("/usr/bin/true".to_owned(), Vec::new()).expect("target"),
        workspace.path(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
    );
    let output = run_direct_helper(&manifest);
    assert!(output.status.success(), "helper stderr: {}", output.stderr);
    let attached = std::fs::read_to_string(leaf.join("cgroup.procs")).expect("membership");
    assert!(attached.parse::<u32>().is_ok(), "helper did not write its PID before activation");
}

#[test]
fn helper_inherits_only_bound_secret_handle_and_installs_exact_environment() {
    let _guard = native_test_guard();
    let workspace = tempfile::tempdir().expect("workspace");
    let secret = b"PROTECTED-ENV-CANARY";
    let requirement = SecretGrant::new(
        SecretReference::new(ResourceId::new([31; 16]).expect("resource identity"), digest(32)),
        SecretDelivery::Environment(EnvironmentName::new("TOKEN").expect("environment name")),
    );
    let process_owned = NativeProtectedHandle::from_bytes("secret-token-v1", secret.to_vec())
        .expect("process-owned handle");
    let linux_payload = LinuxProtectedPayload::new(requirement.clone(), process_owned)
        .expect("Linux payload binding");
    assert!(!format!("{linux_payload:?}").contains("PROTECTED-ENV-CANARY"));

    let mut inherited = tempfile::tempfile().expect("anonymous inherited payload");
    inherited.write_all(secret).expect("write payload");
    inherited.seek(std::io::SeekFrom::Start(0)).expect("rewind payload");
    let inherited_flags = make_inheritable(&inherited);
    let unrelated = tempfile::tempfile().expect("unrelated inherited descriptor");
    let unrelated_flags = make_inheritable(&unrelated);
    let binding = ProtectedPayloadBinding::new(
        requirement,
        InheritedHandle::new(
            u64::try_from(inherited.as_raw_fd()).expect("descriptor"),
            "secret-token-v1".to_owned(),
        )
        .expect("inherited handle"),
        secret.len(),
    )
    .expect("protected binding");
    let manifest = manifest(
        TargetCommand::new("/usr/bin/printenv".to_owned(), vec!["TOKEN".to_owned()])
            .expect("target"),
        workspace.path(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
    )
    .with_protected_payloads(vec![binding])
    .expect("protected manifest");
    let encoded = manifest.encode().expect("encode manifest");
    assert!(!encoded.windows(secret.len()).any(|window| window == secret));
    let output = run_direct_helper(&manifest);
    restore_descriptor_flags(&inherited, inherited_flags);
    restore_descriptor_flags(&unrelated, unrelated_flags);
    assert!(output.status.success(), "helper stderr: {}", output.stderr);
    let mut expected = secret.to_vec();
    expected.push(b'\n');
    assert_eq!(output.target_stdout, expected);
}

#[test]
fn helper_rejects_corrupted_manifest_before_activation() {
    let _guard = native_test_guard();
    let workspace = tempfile::tempdir().expect("workspace");
    let manifest = manifest(
        TargetCommand::new("/usr/bin/true".to_owned(), Vec::new()).expect("target"),
        workspace.path(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
    );
    let mut bytes = manifest.encode().expect("manifest encode");
    let manifest_digest = peritus_codec::sha256(&bytes);
    bytes[20] ^= 1;
    let mut command = Command::new(helper_path());
    command.args([
        "--run",
        "--manifest-digest",
        &hex(manifest_digest),
        "--preparation-digest",
        &hex(manifest.preparation_digest()),
    ]);
    let output = run_until_pre_activation_rejection(command, &bytes);
    assert_eq!(output.status.code(), Some(121));
    assert!(output.stdout.is_empty(), "activation bytes leaked after manifest rejection");
}

#[test]
fn helper_rejects_missing_checked_pty_before_activation() {
    let _guard = native_test_guard();
    let workspace = tempfile::tempdir().expect("workspace");
    let manifest = manifest_for_terminal(
        TargetCommand::new("/usr/bin/true".to_owned(), Vec::new()).expect("target"),
        workspace.path(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
        true,
    );
    let bytes = manifest.encode().expect("manifest encode");
    let manifest_digest = peritus_codec::sha256(&bytes);
    let mut command = Command::new(helper_path());
    command.args([
        "--run",
        "--manifest-digest",
        &hex(manifest_digest),
        "--preparation-digest",
        &hex(manifest.preparation_digest()),
    ]);
    let output = run_until_pre_activation_rejection(command, &bytes);
    assert_eq!(output.status.code(), Some(121));
    assert!(output.stdout.is_empty(), "activation bytes leaked after PTY mismatch");
}

#[test]
fn real_bubblewrap_enforces_writable_mount_and_read_only_metadata_mask() {
    let _guard = native_test_guard();
    if !native_sandbox_available() {
        return;
    }
    let workspace = tempfile::tempdir().expect("workspace");
    std::fs::create_dir(workspace.path().join(".git")).expect("metadata root");
    std::fs::create_dir(workspace.path().join(".peritus")).expect("Peritus metadata root");
    std::fs::create_dir(workspace.path().join(".crosslink")).expect("Crosslink metadata root");
    std::fs::write(workspace.path().join(".git/config"), b"protected").expect("metadata");
    std::fs::write(workspace.path().join("input.txt"), b"input").expect("input");
    let plan = support::checked_plan(workspace.path());
    let policy = MountPolicy::new(workspace.path(), Vec::new()).expect("policy");
    let mounts = MountPlan::project(&plan, &policy).expect("mount projection");

    let literal = workspace.path().join("literal;not-shell");
    let allowed_manifest = manifest(
        TargetCommand::new(
            "/usr/bin/touch".to_owned(),
            vec![literal.to_string_lossy().into_owned()],
        )
        .expect("allowed target"),
        workspace.path(),
        mounts.landlock_rules().to_vec(),
    );
    let allowed = run_bubblewrapped(&mounts, &allowed_manifest);
    assert!(allowed.status.success(), "bubblewrap/helper stderr: {}", allowed.stderr);
    assert!(literal.exists());

    let read_only_root = tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
        .expect("read-only host fixture");
    let denied_host = read_only_root.path().join("forbidden");
    let read_only_manifest = manifest(
        TargetCommand::new(
            "/usr/bin/touch".to_owned(),
            vec![denied_host.to_string_lossy().into_owned()],
        )
        .expect("read-only target"),
        workspace.path(),
        mounts.landlock_rules().to_vec(),
    );
    let read_only = run_bubblewrapped(&mounts, &read_only_manifest);
    assert!(!read_only.status.success());
    assert!(!denied_host.exists());

    let denied = workspace.path().join(".git/forbidden");
    let denied_manifest = manifest(
        TargetCommand::new(
            "/usr/bin/touch".to_owned(),
            vec![denied.to_string_lossy().into_owned()],
        )
        .expect("denied target"),
        workspace.path(),
        mounts.landlock_rules().to_vec(),
    );
    let denied_output = run_bubblewrapped(&mounts, &denied_manifest);
    assert!(!denied_output.status.success());
    assert!(!denied.exists());
    assert_eq!(
        std::fs::read(workspace.path().join(".git/config")).expect("metadata intact"),
        b"protected"
    );
}
