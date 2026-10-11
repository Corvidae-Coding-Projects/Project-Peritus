//! Bounded native Windows capability probes.

use core::mem::size_of;
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use windows_sys::Win32::{
    Foundation::{FreeLibrary, GetLastError},
    System::{
        LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW},
        SystemInformation::OSVERSIONINFOW,
    },
};

use crate::{
    EnforcementLevel, JobPlan, ProbeEvidence, ProbeRequest, TokenProfile, WindowsError,
    probe::{HelperImageEvidence, HelperImageFailure, inspect_helper_image},
};
use peritus_sandbox::CheckedSandboxPlan;

static ACL_PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[allow(
    clippy::unnecessary_wraps,
    reason = "native probe preserves the stable typed-failure boundary for future probe failures"
)]
pub(crate) fn run(
    request: &ProbeRequest,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<ProbeEvidence, WindowsError> {
    ensure_continues(should_continue)?;
    let helper_image = helper_image(request.helper_path(), should_continue)?;
    let helper = helper_image.is_some_and(|image| image.bytes() != 0);
    let helper_digest = helper_image.map(HelperImageEvidence::digest);
    ensure_continues(should_continue)?;
    let architecture = supported_architecture();
    let restricted_token = super::token::RestrictedToken::create(request.token_profile()).is_ok();
    ensure_continues(should_continue)?;
    let (app_container, app_container_sid_exact) = match request.token_profile() {
        TokenProfile::RestrictedLowIntegrity { .. } => (false, false),
        TokenProfile::AppContainer(profile) => {
            let exact = super::token::AppContainerSid::derive(profile).is_ok();
            (true, exact)
        }
    };
    ensure_continues(should_continue)?;
    let job_object = probe_job(None, None, None);
    let kill_on_close = job_object;
    let job_cpu = job_object && probe_job(None, None, Some(1_000));
    let job_memory = job_object && probe_job(None, Some(64 * 1_024 * 1_024), None);
    let job_processes = job_object && probe_job(Some(1), None, None);
    let acl = match request.acl_probe_root() {
        Some(root) => {
            acl_round_trip(root, request.token_profile().principal_sid(), should_continue)?
        }
        None => false,
    };
    let reparse = crate::WindowsPath::from_canonicalized(request.helper_path())
        .and_then(crate::ResolvedWindowsPath::resolve)
        .is_ok();
    let conpty = super::handle::probe_conpty();
    let app_isolation = app_container && app_container_sid_exact;
    ensure_continues(should_continue)?;
    Ok(ProbeEvidence {
        os_build: os_build(),
        platform: true,
        architecture,
        helper,
        helper_digest,
        restricted_token,
        low_integrity: restricted_token,
        app_container,
        app_container_sid_exact,
        job_object,
        kill_on_close,
        acl,
        reparse,
        inherited_handle_list: super::handle::probe_handle_list(),
        conpty,
        credential_manager: credential_manager_usable(),
        deny_network: app_isolation,
        managed_network: request.managed_filter_digest().is_some_and(|identity| {
            app_isolation && super::wfp::WfpSession::probe(request.token_profile(), identity)
        }),
        resources: resource_levels(job_cpu, job_memory, job_processes),
    })
}

pub(crate) fn validate_selected_capacity(
    plan: &CheckedSandboxPlan,
    profile: &TokenProfile,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<(), WindowsError> {
    ensure_continues(should_continue)?;
    let token = super::token::RestrictedToken::create(profile)?;
    drop(token);
    if let TokenProfile::AppContainer(app_container) = profile {
        let identity = super::token::AppContainerSid::derive(app_container)?;
        drop(identity);
    }
    ensure_continues(should_continue)?;
    let job = super::job::OwnedJob::create(JobPlan::from_checked_plan(plan))?;
    drop(job);
    ensure_continues(should_continue)
}

fn helper_image(
    path: &Path,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<HelperImageEvidence>, WindowsError> {
    match inspect_helper_image(path, should_continue) {
        Ok(image) => Ok(Some(image)),
        Err(HelperImageFailure::Cancelled) => Err(crate::probe::probe_cancelled()),
        Err(HelperImageFailure::Unavailable | HelperImageFailure::Changed) => Ok(None),
    }
}

fn probe_job(
    active_process_limit: Option<u32>,
    job_memory_bytes: Option<u64>,
    cpu_time_millis: Option<u64>,
) -> bool {
    super::job::OwnedJob::create(JobPlan::from_probe_limits(
        active_process_limit,
        job_memory_bytes,
        cpu_time_millis,
    ))
    .is_ok()
}

const fn resource_levels(cpu_time: bool, memory: bool, processes: bool) -> [EnforcementLevel; 8] {
    [
        EnforcementLevel::Supervisor,
        if cpu_time { EnforcementLevel::Hard } else { EnforcementLevel::Unsupported },
        if memory { EnforcementLevel::Hard } else { EnforcementLevel::Unsupported },
        EnforcementLevel::Supervisor,
        EnforcementLevel::Supervisor,
        EnforcementLevel::Supervisor,
        if processes { EnforcementLevel::Hard } else { EnforcementLevel::Unsupported },
        EnforcementLevel::Supervisor,
    ]
}

fn ensure_continues(should_continue: &mut impl FnMut() -> bool) -> Result<(), WindowsError> {
    if should_continue() { Ok(()) } else { Err(crate::probe::probe_cancelled()) }
}

const fn supported_architecture() -> bool {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        true
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        false
    }
}

fn os_build() -> Option<u32> {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOW>()).ok()?,
        ..OSVERSIONINFOW::default()
    };
    let module = wide("ntdll.dll");
    // SAFETY: the module name is NUL terminated and ntdll is loaded in every Windows process.
    let loaded = unsafe { GetModuleHandleW(module.as_ptr()) };
    if loaded.is_null() {
        return None;
    }
    // SAFETY: the symbol is NUL terminated and queried from the exact ntdll module.
    let address = unsafe { GetProcAddress(loaded, c"RtlGetVersion".as_ptr().cast()) }?;
    // SAFETY: ntdll exports RtlGetVersion with this documented system ABI and signature.
    let rtl_get_version: unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32 =
        unsafe { core::mem::transmute(address) };
    // SAFETY: the initialized Windows version record has its exact structure size.
    (unsafe { rtl_get_version(&raw mut version) } >= 0).then_some(version.dwBuildNumber)
}

fn credential_manager_usable() -> bool {
    let module_name = wide("advapi32.dll");
    // SAFETY: the system module name is NUL terminated; this reference is released below.
    let module = unsafe { LoadLibraryW(module_name.as_ptr()) };
    if module.is_null() {
        return false;
    }
    // SAFETY: both symbol names are NUL terminated and the module remains loaded.
    let read = unsafe { GetProcAddress(module, c"CredReadW".as_ptr().cast()) };
    // SAFETY: both symbol names are NUL terminated and the module remains loaded.
    let free = unsafe { GetProcAddress(module, c"CredFree".as_ptr().cast()) };
    let Some(read) = read else {
        // SAFETY: releases the exact LoadLibraryW reference above.
        unsafe { FreeLibrary(module) };
        return false;
    };
    let Some(free) = free else {
        // SAFETY: releases the exact LoadLibraryW reference above.
        unsafe { FreeLibrary(module) };
        return false;
    };
    // SAFETY: advapi32 exports these exact system-ABI functions under the queried names.
    let read: unsafe extern "system" fn(*const u16, u32, u32, *mut *mut core::ffi::c_void) -> i32 =
        unsafe { core::mem::transmute(read) };
    // SAFETY: advapi32 exports CredFree with this documented system ABI.
    let free: unsafe extern "system" fn(*mut core::ffi::c_void) =
        unsafe { core::mem::transmute(free) };
    let target = wide("Peritus.Native.Capability.Probe.Nonexistent");
    let mut credential = core::ptr::null_mut();
    // SAFETY: the target is NUL terminated and output points to writable pointer storage.
    let succeeded = unsafe { read(target.as_ptr(), 1, 0, &raw mut credential) } != 0;
    // SAFETY: GetLastError reads only the calling thread's error slot after CredReadW.
    // ERROR_NOT_FOUND proves the callable API without requiring any credential to exist.
    let usable = succeeded || unsafe { GetLastError() } == 1_168;
    if !credential.is_null() {
        // SAFETY: a non-null successful CredReadW result is released by CredFree.
        unsafe { free(credential) };
    }
    // SAFETY: releases the exact LoadLibraryW reference above after all calls complete.
    unsafe { FreeLibrary(module) };
    usable
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

pub(crate) fn system_acl_tool() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    let mut units = vec![0_u16; 32_768];
    // SAFETY: writable buffer length is exact and no returned NUL is interpreted as path data.
    let count = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            units.as_mut_ptr(),
            32_768,
        )
    };
    let count = usize::try_from(count).ok()?;
    if count == 0 || count >= units.len() {
        return None;
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&units[..count])).join("icacls.exe");
    path.is_file().then_some(path)
}

fn acl_round_trip(
    root: &Path,
    principal: &str,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, WindowsError> {
    let Some(tool) = system_acl_tool() else {
        return Ok(false);
    };
    let root_existed = root.is_dir();
    if std::fs::create_dir_all(root).is_err() {
        return Ok(false);
    }
    let probe = loop {
        if let Err(error) = ensure_continues(should_continue) {
            if !root_existed {
                let _ = std::fs::remove_dir(root);
            }
            return Err(error);
        }
        let sequence = ACL_PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = root.join(format!(".peritus-acl-probe-{}-{sequence}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => {
                if !root_existed {
                    let _ = std::fs::remove_dir(root);
                }
                return Ok(false);
            }
        }
    };
    let target = probe.join("target");
    let backup = probe.join("saved-acl.txt");
    let result: Result<bool, WindowsError> = (|| {
        if OpenOptions::new().write(true).create_new(true).open(&target).is_err() {
            return Ok(false);
        }
        if !run_icacls(
            &tool,
            [
                target.as_os_str(),
                std::ffi::OsStr::new("/save"),
                backup.as_os_str(),
                std::ffi::OsStr::new("/q"),
            ],
            should_continue,
        )? {
            return Ok(false);
        }
        let grant = format!("*{principal}:(R)");
        if !run_icacls(
            &tool,
            [
                target.as_os_str(),
                std::ffi::OsStr::new("/grant:r"),
                std::ffi::OsStr::new(&grant),
                std::ffi::OsStr::new("/q"),
            ],
            should_continue,
        )? {
            return Ok(false);
        }
        run_icacls(
            &tool,
            [
                probe.as_os_str(),
                std::ffi::OsStr::new("/restore"),
                backup.as_os_str(),
                std::ffi::OsStr::new("/q"),
            ],
            should_continue,
        )
    })();
    let _ = std::fs::remove_dir_all(&probe);
    if !root_existed {
        let _ = std::fs::remove_dir(root);
    }
    result
}

fn run_icacls<'a>(
    tool: &Path,
    arguments: impl IntoIterator<Item = &'a std::ffi::OsStr>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<bool, WindowsError> {
    let Ok(mut child) = Command::new(tool)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Ok(false);
    };
    loop {
        if !should_continue() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::probe::probe_cancelled());
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => std::thread::park_timeout(std::time::Duration::from_millis(5)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(false);
            }
        }
    }
}
