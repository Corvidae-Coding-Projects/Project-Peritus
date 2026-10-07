//! Inventoried macOS Seatbelt, rlimit, and descriptor-hygiene boundary.

#![allow(
    unsafe_code,
    reason = "single inventoried macOS FFI boundary for Seatbelt, rlimits, and descriptor hygiene"
)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::{
    io::{Read as _, Seek as _, Write},
    os::unix::fs::OpenOptionsExt as _,
};

use peritus_sandbox::SandboxResourceKind;
use zeroize::Zeroizing;

use crate::{
    EnforcementLevel, HelperManifest, MacosError, MacosErrorKind, MacosOperation, RecoveryAction,
    NativeResourceCeiling, NativeResourceControlReport, ResourceControlPlan,
};

use super::materialized::MaterializedSecretFiles;

pub(crate) struct NonblockingDescriptor {
    descriptor: c_int,
    original_flags: c_int,
}

impl NonblockingDescriptor {
    fn restore(mut self) -> Result<(), MacosError> {
        // SAFETY: this guard still owns restoration for the live helper descriptor.
        if unsafe { libc::fcntl(self.descriptor, libc::F_SETFL, self.original_flags) } < 0 {
            return Err(protected_error(
                "helper status descriptor flags could not be restored",
            ));
        }
        self.descriptor = -1;
        Ok(())
    }
}

impl Drop for NonblockingDescriptor {
    fn drop(&mut self) {
        // SAFETY: this guard is scoped inside the single-threaded helper while the descriptor is
        // live. Restoring the exact captured status flags also restores target stdin semantics.
        if self.descriptor >= 0 {
            let _ = unsafe { libc::fcntl(self.descriptor, libc::F_SETFL, self.original_flags) };
        }
    }
}

pub(crate) fn make_nonblocking(descriptor: c_int) -> Result<NonblockingDescriptor, MacosError> {
    make_nonblocking_for(
        descriptor,
        "helper protocol input could not be made cancellable",
    )
}

fn make_nonblocking_for(
    descriptor: c_int,
    detail: &'static str,
) -> Result<NonblockingDescriptor, MacosError> {
    // SAFETY: F_GETFL and F_SETFL operate on the live helper-owned protocol descriptor.
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(protected_error(detail));
    }
    Ok(NonblockingDescriptor { descriptor, original_flags: flags })
}

pub(crate) fn parent_process() -> libc::pid_t {
    // SAFETY: getppid has no pointer arguments and only observes this process identity.
    unsafe { libc::getppid() }
}

pub(crate) fn parent_process_is(expected: libc::pid_t) -> bool {
    expected > 1 && parent_process() == expected
}

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
                c"/usr/lib/libsandbox.1.dylib".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            if handle.is_null() {
                return Err(seatbelt_library_error());
            }
            let init = libc::dlsym(handle, c"sandbox_init".as_ptr());
            let free_error = libc::dlsym(handle, c"sandbox_free_error".as_ptr());
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
        .chain(manifest.proxy().map(crate::ProxyRoute::routing_handle))
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

pub(crate) fn write_status_while(
    descriptor: u32,
    mut bytes: &[u8],
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), MacosError> {
    let descriptor = descriptor.cast_signed();
    let nonblocking = make_nonblocking_for(
        descriptor,
        "helper status descriptor could not be made cancellable",
    )?;
    while !bytes.is_empty() {
        if !should_continue() {
            return Err(MacosError::new(
                MacosErrorKind::SupervisorFailure,
                MacosOperation::Cancel,
                RecoveryAction::CancelAndReap,
                "helper status delivery was cancelled by its execution owner",
            ));
        }
        // SAFETY: the helper owns this live protocol/status descriptor, and the slice remains live
        // for the exact length supplied to the async-signal-safe write call.
        let written = unsafe { libc::write(descriptor, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if error.kind() == std::io::ErrorKind::WouldBlock {
                std::thread::yield_now();
                continue;
            }
            return Err(protected_error("helper status could not be reported"));
        }
        let written = usize::try_from(written)
            .map_err(|_| protected_error("helper status length is invalid"))?;
        if written == 0 {
            return Err(protected_error("helper status channel closed"));
        }
        bytes = &bytes[written..];
    }
    nonblocking.restore()
}

pub(super) fn read_protected_payload_while(
    descriptor: u32,
    expected_len: u32,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<Vec<u8>, MacosError> {
    let mut file = std::fs::File::open(format!("/dev/fd/{descriptor}"))
        .map_err(|_| protected_error("protected payload descriptor cannot be opened"))?;
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|_| protected_error("protected payload descriptor cannot be rewound"))?;
    let expected_len = usize::try_from(expected_len).unwrap_or(usize::MAX);
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(expected_len)
        .map_err(|_| protected_error("protected payload cannot be represented in memory"))?;
    let mut buffer = Zeroizing::new([0_u8; 64 * 1_024]);
    while payload.len() < expected_len {
        ensure_staging_continues(should_continue)?;
        let remaining = expected_len.saturating_sub(payload.len());
        let capacity = remaining.min(buffer.len());
        let count = match file.read(&mut buffer[..capacity]) {
            Ok(count) => count,
            Err(source) if source.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                return Err(protected_error(
                    "protected payload descriptor cannot be read",
                ));
            }
        };
        if count == 0 {
            return Err(protected_error("protected payload length differs from manifest"));
        }
        payload.extend_from_slice(&buffer[..count]);
    }
    ensure_staging_continues(should_continue)?;
    let mut trailing = [0_u8; 1];
    match file.read(&mut trailing) {
        Ok(0) => {}
        Ok(_) => return Err(protected_error("protected payload length differs from manifest")),
        Err(source) if source.kind() == std::io::ErrorKind::Interrupted => {
            loop {
                ensure_staging_continues(should_continue)?;
                match file.read(&mut trailing) {
                    Ok(0) => break,
                    Ok(_) => {
                        return Err(protected_error(
                            "protected payload length differs from manifest",
                        ));
                    }
                    Err(source) if source.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => {
                        return Err(protected_error(
                            "protected payload descriptor cannot be read",
                        ));
                    }
                }
            }
        }
        Err(_) => {
            return Err(protected_error(
                "protected payload descriptor cannot be read",
            ));
        }
    }
    Ok(payload)
}

pub(super) fn materialize_secret_file(
    descriptor: u32,
    expected_len: u32,
    destination: &str,
    materialized: &mut MaterializedSecretFiles,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), MacosError> {
    let mut source = std::fs::File::open(format!("/dev/fd/{descriptor}"))
        .map_err(|_| protected_error("protected file payload descriptor cannot be opened"))?;
    source
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|_| protected_error("protected file payload descriptor cannot be rewound"))?;
    ensure_staging_continues(should_continue)?;
    let registration = materialized.register(destination);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW);
    let mut file = match options.open(destination) {
        Ok(file) => {
            materialized.created(registration);
            file
        }
        Err(_) => {
            materialized.cancel(registration);
            return Err(protected_error(
                "secret file destination cannot be created privately",
            ));
        }
    };
    let result = (|| {
        let mut remaining = u64::from(expected_len);
        let mut buffer = Zeroizing::new([0_u8; 64 * 1_024]);
        while remaining != 0 {
            ensure_staging_continues(should_continue)?;
            let capacity = usize::try_from(
                remaining.min(u64::try_from(buffer.len()).unwrap_or(u64::MAX)),
            )
                .map_err(|_| protected_error("protected file payload length is invalid"))?;
            let count = match source.read(&mut buffer[..capacity]) {
                Ok(count) => count,
                Err(source) if source.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    return Err(protected_error(
                        "protected file payload descriptor cannot be read",
                    ));
                }
            };
            if count == 0 {
                return Err(protected_error("protected file payload is truncated"));
            }
            write_all_while(&mut file, &buffer[..count], should_continue)?;
            remaining = remaining.saturating_sub(u64::try_from(count).unwrap_or(u64::MAX));
        }
        ensure_staging_continues(should_continue)?;
        let mut trailing = [0_u8; 1];
        loop {
            match source.read(&mut trailing) {
                Ok(0) => break,
                Ok(_) => {
                    return Err(protected_error(
                        "protected file payload exceeds its manifest length",
                    ));
                }
                Err(source) if source.kind() == std::io::ErrorKind::Interrupted => {
                    ensure_staging_continues(should_continue)?;
                }
                Err(_) => {
                    return Err(protected_error(
                        "protected file payload descriptor cannot be read",
                    ));
                }
            }
        }
        ensure_staging_continues(should_continue)?;
        file.sync_all()
            .map_err(|_| protected_error("secret file destination cannot be synchronized"))?;
        ensure_staging_continues(should_continue)
    })();
    if let Err(error) = result {
        drop(file);
        return Err(error);
    }
    Ok(())
}

fn write_all_while(
    writer: &mut impl Write,
    mut bytes: &[u8],
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), MacosError> {
    while !bytes.is_empty() {
        ensure_staging_continues(should_continue)?;
        match writer.write(bytes) {
            Ok(0) => {
                return Err(protected_error(
                    "secret file destination cannot be synchronized",
                ));
            }
            Ok(written) => bytes = &bytes[written..],
            Err(source) if source.kind() == std::io::ErrorKind::Interrupted => {}
            Err(source) if source.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::yield_now();
            }
            Err(_) => {
                return Err(protected_error(
                    "secret file destination cannot be synchronized",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn ensure_staging_continues(
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), MacosError> {
    if should_continue() {
        Ok(())
    } else {
        Err(MacosError::new(
            MacosErrorKind::SupervisorFailure,
            MacosOperation::Cancel,
            RecoveryAction::CancelAndReap,
            "protected helper staging was cancelled by its execution owner",
        ))
    }
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

pub(crate) fn negotiate_resource_controls(
    controls: &ResourceControlPlan,
) -> Result<ResourceControlPlan, MacosError> {
    let mut effective = controls.clone();
    for control in controls.controls() {
        if !control.is_selected() || control.level() != EnforcementLevel::Hard {
            continue;
        }
        let Some((resource, requested)) = native_rlimit(*control) else {
            continue;
        };
        let mut inherited = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: `inherited` is writable and resource is selected from the closed constants in
        // `native_rlimit`. Negotiation only observes the preparing process and has no side effect.
        if unsafe { libc::getrlimit(resource, &raw mut inherited) } != 0 {
            return Err(resource_negotiation_error(
                "macOS getrlimit could not negotiate a selected hard ceiling",
            ));
        }
        let negotiated = requested.min(inherited.rlim_max);
        if negotiated == 0 {
            return Err(resource_negotiation_error(
                "inherited macOS hard authority cannot represent a selected ceiling",
            ));
        }
        effective.set_effective_ceiling(
            control.kind(),
            native_ceiling_in_plan_units(control.kind(), negotiated),
        );
    }
    Ok(effective)
}

pub(super) fn install_resource_controls(
    controls: &ResourceControlPlan,
) -> Result<NativeResourceControlReport, MacosError> {
    let mut negotiated = Vec::new();
    for control in controls.controls() {
        if !control.is_selected() || control.level() != EnforcementLevel::Hard {
            continue;
        }
        let Some((resource, ceiling)) = native_rlimit(*control) else {
            continue;
        };
        let mut inherited = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: `inherited` is writable and resource is selected from the closed constants
        // above. All required authority is inspected before the first rlimit is changed.
        if unsafe { libc::getrlimit(resource, &raw mut inherited) } != 0 {
            return Err(resource_error(
                "macOS getrlimit could not inspect a required hard ceiling",
            ));
        }
        if ceiling > inherited.rlim_max {
            return Err(resource_error(
                "a selected resource ceiling exceeds inherited macOS hard authority",
            ));
        }
        negotiated.push(NegotiatedRlimit {
            kind: control.kind(),
            resource,
            ceiling,
            effective_ceiling: control.ceiling(),
            inherited_soft: inherited.rlim_cur,
            inherited_hard: inherited.rlim_max,
        });
    }

    let mut installed = Vec::with_capacity(negotiated.len());
    for control in negotiated {
        let desired = libc::rlimit {
            rlim_cur: control.ceiling,
            rlim_max: control.ceiling,
        };
        // SAFETY: `desired` remains live, preparation negotiated this exact ceiling against the
        // inherited hard authority, and the helper is single-threaded before target exec. Setting
        // both values prevents the target from raising its own soft ceiling after replacement.
        if unsafe { libc::setrlimit(control.resource, &raw const desired) } != 0 {
            return Err(resource_error("macOS setrlimit rejected a required hard ceiling"));
        }
        let mut observed = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: `observed` is writable and names the resource installed immediately above.
        if unsafe { libc::getrlimit(control.resource, &raw mut observed) } != 0
            || observed.rlim_cur != control.ceiling
            || observed.rlim_max != control.ceiling
        {
            return Err(resource_error(
                "macOS did not retain the exact installed resource ceiling",
            ));
        }
        installed.push(NativeResourceCeiling::new(
            control.kind,
            control.effective_ceiling,
            native_ceiling_in_plan_units(control.kind, control.inherited_soft),
            native_ceiling_in_plan_units(control.kind, control.inherited_hard),
        ));
    }
    Ok(NativeResourceControlReport::new(installed))
}

fn native_rlimit(
    control: crate::ResourceControl,
) -> Option<(c_int, libc::rlim_t)> {
    match control.kind() {
        SandboxResourceKind::CpuTime => Some((
            libc::RLIMIT_CPU,
            control.ceiling().saturating_add(999) / 1_000,
        )),
        SandboxResourceKind::Memory => Some((libc::RLIMIT_AS, control.ceiling())),
        SandboxResourceKind::OpenHandles => Some((libc::RLIMIT_NOFILE, control.ceiling())),
        SandboxResourceKind::Processes => Some((libc::RLIMIT_NPROC, control.ceiling())),
        SandboxResourceKind::WallTime
        | SandboxResourceKind::Disk
        | SandboxResourceKind::Output
        | SandboxResourceKind::Concurrency => None,
    }
}

struct NegotiatedRlimit {
    kind: SandboxResourceKind,
    resource: c_int,
    ceiling: libc::rlim_t,
    effective_ceiling: u64,
    inherited_soft: libc::rlim_t,
    inherited_hard: libc::rlim_t,
}

fn native_ceiling_in_plan_units(
    kind: SandboxResourceKind,
    ceiling: libc::rlim_t,
) -> u64 {
    match kind {
        SandboxResourceKind::CpuTime => ceiling.saturating_mul(1_000),
        SandboxResourceKind::WallTime
        | SandboxResourceKind::Memory
        | SandboxResourceKind::Disk
        | SandboxResourceKind::Output
        | SandboxResourceKind::OpenHandles
        | SandboxResourceKind::Processes
        | SandboxResourceKind::Concurrency => ceiling,
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

fn resource_negotiation_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::ResourceLimit,
        MacosOperation::Prepare,
        RecoveryAction::SelectSupportedBackend,
        detail,
    )
}
