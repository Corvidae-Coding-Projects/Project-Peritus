//! Shared helper protocol fixtures for serialized native enforcement tests.

use peritus_process::{native_activation_record, native_ready_record};
use peritus_sandbox_linux::{
    HelperManifest, LandlockRule, LinuxLaunchDescription, LinuxProbe, NetworkIsolation,
    ProbeRequest, TargetCommand,
};
use peritus_types::Sha256Digest;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};

static NATIVE_TEST_LOCK: Mutex<()> = Mutex::new(());

pub fn native_test_guard() -> MutexGuard<'static, ()> {
    NATIVE_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}

pub fn helper_path() -> PathBuf {
    std::env::current_exe()
        .expect("current test executable")
        .parent()
        .and_then(Path::parent)
        .expect("Cargo target directory")
        .join("peritus-linux-sandbox-helper")
}

pub fn native_sandbox_available() -> bool {
    let request = ProbeRequest::new(
        PathBuf::from("/usr/bin/bwrap"),
        helper_path(),
        PathBuf::from("/sys/fs/cgroup"),
        None,
    )
    .expect("native test probe request");
    LinuxProbe::run(&request).is_ok_and(|probe| probe.baseline_supported())
}

pub fn manifest(
    target: TargetCommand,
    workspace: &Path,
    rules: Vec<LandlockRule>,
) -> HelperManifest {
    manifest_for_terminal(target, workspace, rules, false)
}

pub fn manifest_for_terminal(
    target: TargetCommand,
    workspace: &Path,
    rules: Vec<LandlockRule>,
    pty: bool,
) -> HelperManifest {
    let cgroup_leaf = workspace.join("peritus-test-cgroup");
    std::fs::create_dir_all(&cgroup_leaf).expect("cgroup stand-in");
    std::fs::write(cgroup_leaf.join("cgroup.procs"), b"").expect("membership stand-in");
    HelperManifest::new(
        digest(11),
        digest(12),
        digest(13),
        digest(14),
        target,
        workspace.to_path_buf(),
        cgroup_leaf,
        pty,
        Vec::new(),
        rules,
        super::support::resource_plan(),
        NetworkIsolation::DenyAll,
        Vec::new(),
    )
    .expect("helper manifest")
}

pub fn run_bubblewrapped(
    mounts: &peritus_sandbox_linux::MountPlan,
    manifest: &HelperManifest,
) -> HelperOutput {
    let launch = LinuxLaunchDescription::build(
        Path::new("/usr/bin/bwrap"),
        &helper_path(),
        digest_file(&helper_path()),
        mounts,
        manifest,
    )
    .expect("launch");
    let mut command = Command::new(launch.command().executable());
    command.args(launch.command().arguments());
    run_spawned(command, manifest)
}

pub struct HelperOutput {
    pub status: std::process::ExitStatus,
    pub target_stdout: Vec<u8>,
    pub stderr: String,
}

pub fn run_direct_helper(manifest: &HelperManifest) -> HelperOutput {
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
    run_spawned_with_bytes(command, manifest, &bytes, manifest_digest)
}

pub fn run_spawned(command: Command, manifest: &HelperManifest) -> HelperOutput {
    let bytes = manifest.encode().expect("manifest encode");
    let manifest_digest = peritus_codec::sha256(&bytes);
    run_spawned_with_bytes(command, manifest, &bytes, manifest_digest)
}

pub fn run_spawned_with_bytes(
    mut command: Command,
    manifest: &HelperManifest,
    bytes: &[u8],
    manifest_digest: Sha256Digest,
) -> HelperOutput {
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn helper");
    if let Err(handshake) =
        verify_and_feed(&mut child, bytes, manifest_digest, manifest.preparation_digest())
    {
        child.stdin.take();
        let output = child.wait_with_output().expect("wait failed handshake");
        panic!(
            "{handshake}; status={:?}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = child.wait_with_output().expect("wait helper");
    HelperOutput {
        status: output.status,
        target_stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

pub fn run_until_pre_activation_rejection(
    mut command: Command,
    manifest: &[u8],
) -> std::process::Output {
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn helper");
    let mut ready = [0_u8; 32];
    child.stdout.as_mut().expect("stdout").read_exact(&mut ready).expect("ready record");
    assert_eq!(ready, *native_ready_record().as_bytes());
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin
            .write_all(&u32::try_from(manifest.len()).expect("length").to_le_bytes())
            .and_then(|()| stdin.write_all(manifest))
            .and_then(|()| stdin.flush())
            .expect("manifest frame");
    }
    child.stdin.take();
    child.wait_with_output().expect("wait rejected helper")
}

fn verify_and_feed(
    child: &mut Child,
    manifest: &[u8],
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
) -> Result<(), String> {
    let stdout = child.stdout.as_mut().expect("stdout");
    let mut ready = [0_u8; 32];
    stdout.read_exact(&mut ready).map_err(|error| format!("ready record: {error}"))?;
    if ready != *native_ready_record().as_bytes() {
        return Err("ready record mismatch".to_owned());
    }
    let stdin = child.stdin.as_mut().expect("stdin");
    stdin
        .write_all(&u32::try_from(manifest.len()).expect("manifest length").to_le_bytes())
        .and_then(|()| stdin.write_all(manifest))
        .and_then(|()| stdin.flush())
        .map_err(|error| format!("send manifest: {error}"))?;
    let mut activation = [0_u8; 32];
    stdout.read_exact(&mut activation).map_err(|error| format!("activation record: {error}"))?;
    if activation != *native_activation_record(manifest_digest, preparation_digest).as_bytes() {
        return Err("activation record mismatch".to_owned());
    }
    child.stdin.take();
    Ok(())
}

pub fn digest_file(path: &Path) -> Sha256Digest {
    peritus_codec::sha256(&std::fs::read(path).expect("read helper"))
}

pub fn hex(digest: Sha256Digest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

pub fn make_inheritable(file: &std::fs::File) -> nix::fcntl::FdFlag {
    let bits =
        nix::fcntl::fcntl(file, nix::fcntl::FcntlArg::F_GETFD).expect("get descriptor flags");
    let flags = nix::fcntl::FdFlag::from_bits_retain(bits);
    nix::fcntl::fcntl(file, nix::fcntl::FcntlArg::F_SETFD(flags & !nix::fcntl::FdFlag::FD_CLOEXEC))
        .expect("enable exact test inheritance");
    flags
}

pub fn restore_descriptor_flags(file: &std::fs::File, flags: nix::fcntl::FdFlag) {
    nix::fcntl::fcntl(file, nix::fcntl::FcntlArg::F_SETFD(flags))
        .expect("restore descriptor flags");
}
