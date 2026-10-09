//! Bounded native Windows capability probes.

use core::mem::size_of;

use windows_sys::Win32::System::{
    LibraryLoader::{GetModuleHandleW, GetProcAddress},
    SystemInformation::OSVERSIONINFOW,
};

use crate::{
    EnforcementLevel, ProbeEvidence, ProbeRequest, TokenProfile, WindowsError,
    production_resource_levels,
};

#[allow(
    clippy::unnecessary_wraps,
    reason = "native probe preserves the stable typed-failure boundary for future probe failures"
)]
pub(crate) fn run(request: &ProbeRequest) -> Result<ProbeEvidence, WindowsError> {
    let helper_bytes = std::fs::read(request.helper_path()).ok();
    let helper_digest = helper_bytes.as_deref().map(peritus_codec::sha256);
    let helper = helper_digest.is_some();
    let architecture = supported_architecture();
    let restricted_token = super::token::RestrictedToken::create(request.token_profile()).is_ok();
    let (app_container, app_container_sid_exact) = match request.token_profile() {
        TokenProfile::RestrictedLowIntegrity { .. } => (false, false),
        TokenProfile::AppContainer(profile) => {
            let exact = super::token::AppContainerSid::derive(profile).is_ok();
            (true, exact)
        }
    };
    let job_object = super::job::OwnedJob::probe();
    let acl = system_tool_exists("icacls.exe");
    let reparse = crate::WindowsPath::from_canonicalized(request.helper_path())
        .and_then(crate::ResolvedWindowsPath::resolve)
        .is_ok();
    let conpty = function_exists("kernel32.dll", b"CreatePseudoConsole\0");
    let app_isolation = app_container && app_container_sid_exact;
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
        kill_on_close: job_object,
        acl,
        reparse,
        inherited_handle_list: function_exists("kernel32.dll", b"UpdateProcThreadAttribute\0"),
        conpty,
        credential_manager: true,
        deny_network: app_isolation,
        managed_network: request.managed_filter_digest().is_some_and(|identity| {
            app_isolation && super::wfp::WfpSession::probe(request.token_profile(), identity)
        }),
        resources: if job_object {
            production_resource_levels()
        } else {
            [EnforcementLevel::Unsupported; 8]
        },
    })
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

fn function_exists(module: &str, symbol: &[u8]) -> bool {
    let module = wide(module);
    // SAFETY: both module and symbol are NUL terminated and remain live through the lookup.
    let loaded = unsafe { GetModuleHandleW(module.as_ptr()) };
    !loaded.is_null() && unsafe { GetProcAddress(loaded, symbol.as_ptr()) }.is_some()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

fn system_tool_exists(name: &str) -> bool {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return false;
    };
    std::path::PathBuf::from(root).join("System32").join(name).is_file()
}
