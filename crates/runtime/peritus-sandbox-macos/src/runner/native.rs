//! Inventoried macOS Seatbelt, rlimit, and descriptor-hygiene boundary.

#![allow(
    unsafe_code,
    reason = "single inventoried macOS FFI boundary for Seatbelt, rlimits, and descriptor hygiene"
)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::{
    io::{Read as _, Seek as _, Write as _},
    os::unix::fs::OpenOptionsExt as _,
};

use peritus_sandbox::SandboxResourceKind;

use crate::{
    EnforcementLevel, HelperManifest, MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    ResourceControlPlan,
};

type SandboxInit = unsafe extern "C" fn(*const c_char, u64, *mut *mut c_char) -> c_int;
type SandboxFreeError = unsafe extern "C" fn(*mut c_char);

struct SandboxLibrary {
    handle: *mut c_void,
    init: SandboxInit,
    free_error: SandboxFreeError,
}

impl SandboxLibrary {
    fn open() -> Result<Self, MacosError> {
        // SAFETY: all names are static NUL-terminated ASCII. The two resolved symbols have the
        // signatures declared by `<sandbox.h>` on the supported macOS 15 platform. The library
        // handle remains live for every call through the stored function pointers.
        unsafe {
            let handle = libc::dlopen(
                b"/usr/lib/libsandbox.1.dylib\0".as_ptr().cast(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            if handle.is_null() {
                return Err(seatbelt_library_error());
            }
            let init = libc::dlsym(handle, b"sandbox_init\0".as_ptr().cast());
            let free_error = libc::dlsym(handle, b"sandbox_free_error\0".as_ptr().cast());
            if init.is_null() || free_error.is_null() {
                let _ = libc::dlclose(handle);
                return Err(seatbelt_library_error());
            }
            Ok(Self {
                handle,
                init: core::mem::transmute::<*mut c_void, SandboxInit>(init),
                free_error: core::mem::transmute::<*mut c_void, SandboxFreeError>(free_error),
            })
        }
    }
}

impl Drop for SandboxLibrary {
    fn drop(&mut self) {
        // SAFETY: `open` created this live handle, and this owner closes it exactly once after all
        // calls through its function pointers are complete.
        let _ = unsafe { libc::dlclose(self.handle) };
    }
}

pub(super) fn verify_protected_channels(manifest: &HelperManifest) -> Result<(), MacosError> {
    for descriptor in core::iter::once(manifest.exec_status_descriptor())
        .chain(manifest.proxy().map(|route| route.routing_handle()).into_iter())
        .chain(manifest.secrets().iter().map(crate::SecretHandleDescriptor::descriptor))
    {
        // SAFETY: F_GETFD takes no third argument and the manifest bounded the descriptor.
        let flags = unsafe { libc::fcntl(descriptor.cast_signed(), libc::F_GETFD) };
        if flags < 0 || flags & libc::FD_CLOEXEC != 0 {
            return Err(MacosError::new(
                MacosErrorKind::HelperFailure,
                MacosOperation::Activate,
                RecoveryAction::Reauthorize,
                "a protected inherited descriptor is unavailable or closes on exec",
            ));
        }
    }
    Ok(())
}

pub(super) fn close_unrelated_descriptors(
    manifest: &HelperManifest,
    retained_pty: Option<u32>,
) -> Result<(), MacosError> {
    let mut retained = manifest
        .secrets()
        .iter()
        .filter(|secret| {
            matches!(secret.destination(), crate::SecretHandleDestination::Brokered(_))
        })
        .map(crate::SecretHandleDescriptor::descriptor)
        .collect::<Vec<_>>();
    retained.push(manifest.exec_status_descriptor());
    retained.extend(retained_pty);
    let entries = std::fs::read_dir("/dev/fd").map_err(|_| descriptor_error())?;
    let mut descriptors = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| descriptor_error())?;
        let Some(descriptor) =
            entry.file_name().to_str().and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        if descriptor > 2 && !retained.contains(&descriptor) {
            descriptors.push(descriptor);
        }
    }
    for descriptor in descriptors {
        // SAFETY: `/dev/fd` supplied this nonstandard, non-whitelisted descriptor. The helper is
        // single-threaded before exec; a concurrently stale entry only yields harmless EBADF.
        let _ = unsafe { libc::close(descriptor.cast_signed()) };
    }
    Ok(())
}

pub(super) fn mark_exec_status_close_on_exec(descriptor: u32) -> Result<(), MacosError> {
    let descriptor = descriptor.cast_signed();
    // SAFETY: the checksummed manifest bounds this live inherited descriptor, and F_GETFD/F_SETFD
    // affect only its close-on-exec flag in the single-threaded helper before target replacement.
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
    {
        return Err(protected_error(
            "helper exec status descriptor could not be made close-on-exec",
        ));
    }
    Ok(())
}

pub(crate) fn write_exec_status(descriptor: u32, mut bytes: &[u8]) -> Result<(), MacosError> {
    let descriptor = descriptor.cast_signed();
    while !bytes.is_empty() {
        // SAFETY: the helper verified this manifest-bound descriptor before activation, and the
        // slice remains live for the exact length supplied to the async-signal-safe write call.
        let written = unsafe { libc::write(descriptor, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(protected_error("helper exec failure status could not be reported"));
        }
        let written = usize::try_from(written)
            .map_err(|_| protected_error("helper exec failure status length is invalid"))?;
        if written == 0 {
            return Err(protected_error("helper exec failure status channel closed"));
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

pub(super) fn read_protected_payload(
    descriptor: u32,
    expected_len: u32,
) -> Result<Vec<u8>, MacosError> {
    let mut file = std::fs::File::open(format!("/dev/fd/{descriptor}"))
        .map_err(|_| protected_error("protected payload descriptor cannot be opened"))?;
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|_| protected_error("protected payload descriptor cannot be rewound"))?;
    let mut payload = Vec::with_capacity(usize::try_from(expected_len).unwrap_or(0));
    file.take(u64::from(expected_len).saturating_add(1))
        .read_to_end(&mut payload)
        .map_err(|_| protected_error("protected payload descriptor cannot be read"))?;
    if payload.len() != usize::try_from(expected_len).unwrap_or(usize::MAX) {
        return Err(protected_error("protected payload length differs from manifest"));
    }
    Ok(payload)
}

pub(super) fn materialize_secret_file(destination: &str, payload: &[u8]) -> Result<(), MacosError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW);
    let mut file = options
        .open(destination)
        .map_err(|_| protected_error("secret file destination cannot be created privately"))?;
    if file.write_all(payload).and_then(|()| file.sync_all()).is_err() {
        drop(file);
        let _ = std::fs::remove_file(destination);
        return Err(protected_error("secret file destination cannot be synchronized"));
    }
    Ok(())
}

pub(super) fn install_seatbelt(profile: &str) -> Result<(), MacosError> {
    let profile = CString::new(profile).map_err(|_| {
        MacosError::new(
            MacosErrorKind::ProfileCompilation,
            MacosOperation::Activate,
            RecoveryAction::CorrectRequest,
            "compiled Seatbelt profile contains NUL",
        )
    })?;
    let library = SandboxLibrary::open()?;
    let mut error_buffer = core::ptr::null_mut();
    // SAFETY: the profile and out-pointer are live, and this single-threaded helper owns setup.
    let status = unsafe { (library.init)(profile.as_ptr(), 0, &raw mut error_buffer) };
    if status == 0 {
        return Ok(());
    }
    if !error_buffer.is_null() {
        // SAFETY: Seatbelt returned this diagnostic on failure; it is inspected without disclosure
        // and freed exactly once through the paired function.
        let _has_detail = unsafe { !CStr::from_ptr(error_buffer).to_bytes().is_empty() };
        unsafe { (library.free_error)(error_buffer) };
    }
    Err(MacosError::new(
        MacosErrorKind::SandboxDenied,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        "Seatbelt rejected the compiled profile",
    ))
}

fn seatbelt_library_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::UnsupportedHost,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        "macOS Seatbelt runtime symbols are unavailable",
    )
}

pub(super) fn install_resource_controls(controls: &ResourceControlPlan) -> Result<(), MacosError> {
    for control in controls.controls() {
        if control.level() != EnforcementLevel::Hard {
            continue;
        }
        let (resource, ceiling) = match control.kind() {
            SandboxResourceKind::CpuTime => {
                (libc::RLIMIT_CPU, control.ceiling().saturating_add(999) / 1_000)
            }
            SandboxResourceKind::Memory => (libc::RLIMIT_AS, control.ceiling()),
            SandboxResourceKind::OpenHandles => (libc::RLIMIT_NOFILE, control.ceiling()),
            SandboxResourceKind::Processes => (libc::RLIMIT_NPROC, control.ceiling()),
            SandboxResourceKind::WallTime
            | SandboxResourceKind::Disk
            | SandboxResourceKind::Output
            | SandboxResourceKind::Concurrency => continue,
        };
        install(resource, ceiling.max(1))?;
    }
    Ok(())
}

fn install(resource: c_int, ceiling: u64) -> Result<(), MacosError> {
    let mut current = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `current` is writable and resource is selected from the closed constants above.
    if unsafe { libc::getrlimit(resource, &raw mut current) } != 0 {
        return Err(resource_error("macOS getrlimit could not inspect a required hard ceiling"));
    }
    let effective = ceiling.min(current.rlim_cur).min(current.rlim_max);
    let desired = libc::rlimit { rlim_cur: effective, rlim_max: effective };
    // SAFETY: `desired` remains live and the helper is single-threaded before target exec.
    if unsafe { libc::setrlimit(resource, &raw const desired) } == 0 {
        Ok(())
    } else {
        Err(resource_error("macOS setrlimit rejected a required hard ceiling"))
    }
}

fn descriptor_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Activate,
        RecoveryAction::RepairHelper,
        "helper descriptor enumeration was incomplete",
    )
}

fn protected_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Activate,
        RecoveryAction::CancelAndReap,
        detail,
    )
}

fn resource_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::ResourceLimit,
        MacosOperation::Activate,
        RecoveryAction::SelectSupportedBackend,
        detail,
    )
}
