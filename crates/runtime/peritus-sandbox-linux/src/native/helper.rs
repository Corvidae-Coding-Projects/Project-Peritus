//! Version-one helper handshake, enforcement installation, and literal target exec.

mod proxy;

use crate::{HelperManifest, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use peritus_types::Sha256Digest;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use zeroize::Zeroizing;

const MANIFEST_LIMIT: usize = 1024 * 1024;

pub(super) fn helper_main() -> Result<(), LinuxError> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [mode] if mode == "--probe-landlock" => {
            let abi = super::landlock_policy::probe_abi().unwrap_or(0);
            std::io::stdout()
                .write_all(abi.to_string().as_bytes())
                .and_then(|()| std::io::stdout().write_all(b"\n"))
                .and_then(|()| std::io::stdout().flush())
                .map_err(|error| {
                    LinuxError::io(LinuxOperation::Probe, "write Landlock probe", &error)
                })?;
            Ok(())
        }
        [mode] if mode == "--probe-seccomp" => super::seccomp_policy::install(),
        [mode, endpoint] if mode == "--probe-proxy" => probe_proxy(endpoint),
        [mode, digest_flag, manifest, preparation_flag, preparation]
            if mode == "--run"
                && digest_flag == "--manifest-digest"
                && preparation_flag == "--preparation-digest" =>
        {
            run_target(parse_digest(manifest)?, parse_digest(preparation)?)
        }
        _ => Err(helper_error("helper invocation is invalid")),
    }
}

fn run_target(
    expected_manifest: Sha256Digest,
    expected_preparation: Sha256Digest,
) -> Result<(), LinuxError> {
    let pty = peritus_process::NativePtyAttachment::from_environment()
        .map_err(|_| helper_error("process-owned PTY slave could not be opened"))?;
    let ready = peritus_process::native_ready_record();
    std::io::stdout()
        .write_all(ready.as_bytes())
        .and_then(|()| std::io::stdout().flush())
        .map_err(|error| {
            LinuxError::io(LinuxOperation::Activate, "write helper-ready record", &error)
        })?;
    let bytes = read_manifest_frame(&mut std::io::stdin().lock())?;
    if peritus_codec::sha256(&bytes) != expected_manifest {
        return Err(helper_error("delivered manifest digest differs from launch binding"));
    }
    let manifest = HelperManifest::decode(&bytes)?;
    if manifest.preparation_digest() != expected_preparation {
        return Err(helper_error("manifest preparation digest differs from launch binding"));
    }
    if manifest.expects_pty() != pty.is_some() {
        return Err(helper_error(
            "process-owned PTY presence differs from the checked terminal mode",
        ));
    }
    proxy::validate_handles(&manifest)?;
    sanitize_descriptors(&manifest, pty.as_ref().map(AsRawFd::as_raw_fd))?;
    let mut exec_status = manifest
        .inherited_handles()
        .iter()
        .find(|handle| handle.label() == crate::EXEC_STATUS_LABEL)
        .map(|handle| crate::exec_status::open_helper_attachment(handle.descriptor()))
        .transpose()?;
    let prepared_proxy = proxy::prepare(&manifest)?;
    let protected_payloads = prepare_protected_payloads(&manifest)?;
    attach_prepared_cgroup(&manifest)?;
    super::rlimit::install(manifest.resources())?;
    super::landlock_policy::install(&manifest)?;
    super::seccomp_policy::install()?;
    let mut command = Command::new(manifest.target().program());
    command
        .args(manifest.target().arguments())
        .current_dir(manifest.working_directory())
        .env_clear();
    for entry in manifest.environment() {
        command.env(entry.name(), entry.value());
    }
    for (index, payload) in protected_payloads.iter().enumerate() {
        match payload {
            PreparedPayload::Environment { name, value } => {
                command.env(name, OsStr::from_bytes(value));
            }
            PreparedPayload::Brokered { label, descriptor } => {
                command.env(format!("PERITUS_BROKERED_HANDLE_LABEL_V1_{index}"), label);
                command
                    .env(format!("PERITUS_BROKERED_HANDLE_FD_V1_{index}"), descriptor.to_string());
            }
        }
    }
    if let Some(proxy) = prepared_proxy {
        proxy.configure(&mut command);
    }
    if let Some(attachment) = pty {
        attachment
            .configure(&mut command)
            .map_err(|_| helper_error("process-owned PTY streams could not be configured"))?;
    }
    // No fallible target setup remains after activation. From this point the status descriptor is
    // closed only by a successful `exec`, or receives the exact failure record below.
    let activation =
        peritus_process::native_activation_record(expected_manifest, expected_preparation);
    std::io::stdout()
        .write_all(activation.as_bytes())
        .and_then(|()| std::io::stdout().flush())
        .map_err(|error| {
            LinuxError::io(LinuxOperation::Activate, "write helper-activation record", &error)
        })?;
    let error = command.exec();
    if let Some(status) = exec_status.as_mut() {
        crate::exec_status::report_helper_failure(status, expected_manifest, expected_preparation)?;
    }
    Err(LinuxError::io(LinuxOperation::Activate, "exec literal target", &error))
}

fn attach_prepared_cgroup(manifest: &HelperManifest) -> Result<(), LinuxError> {
    let mut membership = std::fs::OpenOptions::new()
        .write(true)
        .open(manifest.cgroup_leaf().join("cgroup.procs"))
        .map_err(|error| {
            LinuxError::io(LinuxOperation::Attach, "open prepared cgroup membership", &error)
        })?;
    membership
        .write_all(std::process::id().to_string().as_bytes())
        .and_then(|()| membership.flush())
        .map_err(|error| {
            LinuxError::io(LinuxOperation::Attach, "attach helper to prepared cgroup", &error)
        })
}

enum PreparedPayload {
    Environment { name: String, value: Zeroizing<Vec<u8>> },
    Brokered { label: String, descriptor: u64 },
}

fn sanitize_descriptors(
    manifest: &HelperManifest,
    pty_descriptor: Option<i32>,
) -> Result<(), LinuxError> {
    let mut retained = BTreeSet::new();
    if let Some(descriptor) = pty_descriptor {
        retained.insert(descriptor);
    }
    for binding in manifest.protected_payloads() {
        let descriptor = i32::try_from(binding.handle().descriptor())
            .map_err(|_| helper_error("protected payload descriptor exceeds Linux bounds"))?;
        if descriptor < 3 || !retained.insert(descriptor) {
            return Err(helper_error("protected payload descriptor is invalid or collides"));
        }
        if std::fs::metadata(format!("/proc/self/fd/{descriptor}")).is_err() {
            return Err(helper_error("protected payload descriptor was not inherited"));
        }
    }
    for handle in manifest.inherited_handles() {
        let descriptor = i32::try_from(handle.descriptor())
            .map_err(|_| helper_error("inherited descriptor exceeds Linux bounds"))?;
        if descriptor < 3 || !retained.insert(descriptor) {
            return Err(helper_error("inherited descriptor is invalid or collides"));
        }
        if std::fs::metadata(format!("/proc/self/fd/{descriptor}")).is_err() {
            return Err(helper_error("manifest-bound descriptor was not inherited"));
        }
    }
    let entries = std::fs::read_dir("/proc/self/fd").map_err(|error| {
        LinuxError::io(LinuxOperation::Activate, "inspect helper descriptors", &error)
    })?;
    let candidates: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u32>().ok())
        .filter(|descriptor| *descriptor >= 3)
        .collect();
    for descriptor in candidates {
        let descriptor = i32::try_from(descriptor)
            .map_err(|_| helper_error("inherited descriptor exceeds Linux bounds"))?;
        if !retained.contains(&descriptor)
            && nix::unistd::close(descriptor).is_err()
            && std::fs::metadata(format!("/proc/self/fd/{descriptor}")).is_ok()
        {
            return Err(helper_error("unrelated inherited descriptor could not be closed"));
        }
    }
    Ok(())
}

fn prepare_protected_payloads(
    manifest: &HelperManifest,
) -> Result<Vec<PreparedPayload>, LinuxError> {
    let mut prepared = Vec::with_capacity(manifest.protected_payloads().len());
    for binding in manifest.protected_payloads() {
        let descriptor = binding.handle().descriptor();
        match binding.requirement().delivery() {
            peritus_sandbox::SecretDelivery::Environment(name) => {
                let value = read_protected_payload(descriptor, binding.payload_len())?;
                if value.contains(&0) {
                    return Err(helper_error("protected environment payload contains NUL"));
                }
                close_consumed_descriptor(descriptor)?;
                prepared.push(PreparedPayload::Environment {
                    name: name.as_str().to_owned(),
                    value: Zeroizing::new(value),
                });
            }
            peritus_sandbox::SecretDelivery::File(path) => {
                let metadata = std::fs::symlink_metadata(path.as_str())
                    .map_err(|_| helper_error("protected file destination was not materialized"))?;
                #[cfg(target_os = "linux")]
                {
                    use std::os::unix::fs::PermissionsExt;

                    if !metadata.file_type().is_file()
                        || metadata.len() != u64::from(binding.payload_len())
                        || metadata.permissions().mode() & 0o222 != 0
                    {
                        return Err(helper_error(
                            "protected file destination is not an exact read-only regular file",
                        ));
                    }
                }
                close_consumed_descriptor(descriptor)?;
            }
            peritus_sandbox::SecretDelivery::BrokeredHandle(label) => {
                prepared.push(PreparedPayload::Brokered {
                    label: label.as_str().to_owned(),
                    descriptor,
                });
            }
        }
    }
    Ok(prepared)
}

fn read_protected_payload(descriptor: u64, expected_len: u32) -> Result<Vec<u8>, LinuxError> {
    let file = File::open(format!("/proc/self/fd/{descriptor}"))
        .map_err(|_| helper_error("protected payload descriptor could not be opened"))?;
    let mut bytes = Vec::with_capacity(expected_len as usize);
    file.take(u64::from(expected_len) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| helper_error("protected payload descriptor could not be read"))?;
    if bytes.len() != expected_len as usize {
        return Err(helper_error("protected payload length differs from its manifest binding"));
    }
    Ok(bytes)
}

fn close_consumed_descriptor(descriptor: u64) -> Result<(), LinuxError> {
    let descriptor = i32::try_from(descriptor)
        .map_err(|_| helper_error("consumed protected descriptor exceeds Linux bounds"))?;
    nix::unistd::close(descriptor)
        .map_err(|_| helper_error("consumed protected descriptor could not be closed"))
}

fn read_manifest_frame(reader: &mut impl Read) -> Result<Vec<u8>, LinuxError> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length).map_err(|error| {
        LinuxError::io(LinuxOperation::Manifest, "read manifest length", &error)
    })?;
    let length = usize::try_from(u32::from_le_bytes(length))
        .map_err(|_| helper_error("manifest frame length is invalid"))?;
    if length == 0 || length > MANIFEST_LIMIT {
        return Err(helper_error("manifest frame length exceeds its bound"));
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| LinuxError::io(LinuxOperation::Manifest, "read manifest bytes", &error))?;
    Ok(bytes)
}

fn parse_digest(value: &str) -> Result<Sha256Digest, LinuxError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(helper_error("helper digest argument is invalid"));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Ok(Sha256Digest::new(bytes))
}

fn hex_value(value: u8) -> Result<u8, LinuxError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(helper_error("helper digest contains a non-hexadecimal byte")),
    }
}

fn probe_proxy(value: &str) -> Result<(), LinuxError> {
    let endpoint: SocketAddr = value.parse().map_err(|_| {
        LinuxError::new(
            LinuxErrorKind::Network,
            LinuxOperation::Probe,
            LinuxRecovery::CorrectRequest,
            "proxy probe endpoint is invalid",
        )
    })?;
    TcpStream::connect_timeout(&endpoint, Duration::from_secs(1)).map(|_| ()).map_err(|_| {
        LinuxError::new(
            LinuxErrorKind::Network,
            LinuxOperation::Probe,
            LinuxRecovery::ConfigureHost,
            "proxy route is unreachable from the network namespace",
        )
    })
}

fn helper_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Helper,
        LinuxOperation::Manifest,
        LinuxRecovery::CancelAndReap,
        detail,
    )
}
