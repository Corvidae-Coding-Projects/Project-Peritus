//! Restricted primary-token, low-integrity, and `AppContainer` identity setup.

use core::{ffi::c_void, mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{
        Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW},
        CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, EqualSid,
        Isolation::DeriveAppContainerSidFromAppContainerName,
        PSID, SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES, SetTokenInformation, TOKEN_ADJUST_DEFAULT,
        TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
        TokenIntegrityLevel,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

use crate::{
    AppContainerProfile, TokenProfile, WindowsError, WindowsErrorKind, WindowsOperation,
    WindowsRecovery,
};

const SE_GROUP_INTEGRITY: u32 = 0x20;
const MAX_SID_TEXT_UNITS: usize = 256;

pub(crate) fn derive_profile(name: String) -> Result<AppContainerProfile, WindowsError> {
    let wide_name = wide(&name);
    let mut derived = ptr::null_mut();
    // SAFETY: the name is NUL terminated and the output points to writable SID storage.
    if unsafe { DeriveAppContainerSidFromAppContainerName(wide_name.as_ptr(), &raw mut derived) }
        < 0
    {
        return Err(app_error("AppContainer SID cannot be derived for the profile name"));
    }
    let derived = OwnedAppContainerSid(derived);
    let mut text = ptr::null_mut();
    // SAFETY: the derived SID is valid and the output points to writable local-string storage.
    if unsafe { ConvertSidToStringSidW(derived.as_ptr(), &raw mut text) } == 0 {
        return Err(app_error("derived AppContainer SID cannot be rendered"));
    }
    let text = OwnedLocalString(text);
    let length = text.bounded_length()?;
    // SAFETY: `bounded_length` found a NUL within the allocation and returns its prefix length.
    let units = unsafe { core::slice::from_raw_parts(text.as_ptr(), length) };
    let sid = String::from_utf16(units)
        .map_err(|_| app_error("derived AppContainer SID text is malformed"))?;
    AppContainerProfile::new(name, sid)
}

pub(super) struct RestrictedToken(HANDLE);

impl RestrictedToken {
    pub(super) fn create(profile: &TokenProfile) -> Result<Self, WindowsError> {
        let restriction = match profile {
            TokenProfile::RestrictedLowIntegrity { .. } => {
                Some(OwnedLocalSid::parse(profile.principal_sid())?)
            }
            TokenProfile::AppContainer(_) => None,
        };
        let mut current = ptr::null_mut();
        // SAFETY: output points to valid storage; the pseudo current-process handle is valid.
        if unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ASSIGN_PRIMARY | TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ADJUST_DEFAULT,
                &raw mut current,
            )
        } == 0
        {
            return Err(token_error("current process token cannot be opened"));
        }
        let current = OwnedHandle(current);
        // CreateRestrictedToken requires every restricting-SID attribute field to be zero.
        // AppContainer package identity belongs in SECURITY_CAPABILITIES during process creation,
        // not in this independent restricting-SID list.
        let restricting =
            restriction.as_ref().map(|sid| SID_AND_ATTRIBUTES { Sid: sid.as_ptr(), Attributes: 0 });
        let restricting_count = u32::from(restricting.is_some());
        let restricting_pointer = restricting.as_ref().map_or(ptr::null(), ptr::from_ref);
        let mut restricted = ptr::null_mut();
        // SAFETY: current and SID remain live, all optional arrays use zero/null pairs.
        if unsafe {
            CreateRestrictedToken(
                current.raw(),
                DISABLE_MAX_PRIVILEGE,
                0,
                ptr::null(),
                0,
                ptr::null(),
                restricting_count,
                restricting_pointer,
                &raw mut restricted,
            )
        } == 0
        {
            return Err(token_error("restricted primary token cannot be created"));
        }
        let token = Self(restricted);
        token.install_low_integrity()?;
        Ok(token)
    }

    pub(super) const fn raw(&self) -> HANDLE {
        self.0
    }

    fn install_low_integrity(&self) -> Result<(), WindowsError> {
        let low = OwnedLocalSid::parse("S-1-16-4096")?;
        let label = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES { Sid: low.as_ptr(), Attributes: SE_GROUP_INTEGRITY },
        };
        let length = u32::try_from(size_of::<TOKEN_MANDATORY_LABEL>() + low.length())
            .map_err(|_| token_error("low-integrity token record exceeds Windows bounds"))?;
        // SAFETY: token and label SID remain valid for the duration and length is exact.
        if unsafe {
            SetTokenInformation(
                self.0,
                TokenIntegrityLevel,
                (&raw const label).cast::<c_void>(),
                length,
            )
        } == 0
        {
            return Err(token_error("low mandatory-integrity label cannot be installed"));
        }
        Ok(())
    }
}

impl Drop for RestrictedToken {
    fn drop(&mut self) {
        // SAFETY: this type uniquely owns a non-null token handle.
        unsafe { CloseHandle(self.0) };
    }
}

pub(super) struct AppContainerSid {
    derived: PSID,
}

impl AppContainerSid {
    pub(super) fn derive(profile: &AppContainerProfile) -> Result<Self, WindowsError> {
        let name = wide(profile.name());
        let mut derived = ptr::null_mut();
        // SAFETY: NUL-terminated name and output storage are valid.
        if unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &raw mut derived) } < 0
        {
            return Err(app_error("configured AppContainer SID cannot be derived"));
        }
        let expected = OwnedLocalSid::parse(profile.sid())?;
        // SAFETY: both SID buffers are live and were produced by Windows SID APIs.
        if unsafe { EqualSid(derived, expected.as_ptr()) } == 0 {
            // SAFETY: `derived` was allocated by the AppContainer SID API.
            unsafe { windows_sys::Win32::Security::FreeSid(derived) };
            return Err(app_error("derived AppContainer SID differs from configured identity"));
        }
        Ok(Self { derived })
    }

    pub(super) const fn capabilities(&self) -> SECURITY_CAPABILITIES {
        SECURITY_CAPABILITIES {
            AppContainerSid: self.derived,
            Capabilities: ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        }
    }
}

impl Drop for AppContainerSid {
    fn drop(&mut self) {
        // SAFETY: `derived` is uniquely owned and allocated by the AppContainer SID API.
        unsafe { windows_sys::Win32::Security::FreeSid(self.derived) };
    }
}

struct OwnedLocalSid(PSID);

impl OwnedLocalSid {
    fn parse(text: &str) -> Result<Self, WindowsError> {
        let wide = wide(text);
        let mut sid = ptr::null_mut();
        // SAFETY: NUL-terminated SID text and output storage are valid.
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &raw mut sid) } == 0 {
            return Err(token_error("configured SID text is not accepted by Windows"));
        }
        Ok(Self(sid))
    }

    const fn as_ptr(&self) -> PSID {
        self.0
    }

    fn length(&self) -> usize {
        // SAFETY: this value is a valid SID allocated by ConvertStringSidToSidW.
        usize::try_from(unsafe { windows_sys::Win32::Security::GetLengthSid(self.0) })
            .unwrap_or(usize::MAX)
    }
}

impl Drop for OwnedLocalSid {
    fn drop(&mut self) {
        // SAFETY: ConvertStringSidToSidW allocates with LocalAlloc and ownership is unique.
        unsafe { LocalFree(self.0) };
    }
}

struct OwnedAppContainerSid(PSID);

impl OwnedAppContainerSid {
    const fn as_ptr(&self) -> PSID {
        self.0
    }
}

impl Drop for OwnedAppContainerSid {
    fn drop(&mut self) {
        // SAFETY: this SID is uniquely owned and allocated by the AppContainer SID API.
        unsafe { windows_sys::Win32::Security::FreeSid(self.0) };
    }
}

struct OwnedLocalString(*mut u16);

impl OwnedLocalString {
    const fn as_ptr(&self) -> *const u16 {
        self.0
    }

    fn bounded_length(&self) -> Result<usize, WindowsError> {
        for index in 0..MAX_SID_TEXT_UNITS {
            // SAFETY: ConvertSidToStringSidW returns a NUL-terminated SID string, and the
            // documented SID string bound is far below this defensive ceiling.
            if unsafe { *self.0.add(index) } == 0 {
                return Ok(index);
            }
        }
        Err(app_error("derived AppContainer SID text exceeds its bound"))
    }
}

impl Drop for OwnedLocalString {
    fn drop(&mut self) {
        // SAFETY: ConvertSidToStringSidW allocates this unique string with LocalAlloc.
        unsafe { LocalFree(self.0.cast()) };
    }
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    const fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: this value uniquely owns a non-null kernel handle.
        unsafe { CloseHandle(self.0) };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

fn token_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Token,
        WindowsOperation::Activate,
        WindowsRecovery::CancelAndReap,
        detail,
    )
}

fn app_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::AppContainer,
        WindowsOperation::Activate,
        WindowsRecovery::ConfigureHost,
        detail,
    )
}
