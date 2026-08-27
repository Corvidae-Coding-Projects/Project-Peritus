//! Same-user Windows named-pipe endpoint and peer-token admission.

use std::{io, path::Path};

use peritus_journal::ApplicationPrincipalKind;
use tokio::{net::windows::named_pipe::NamedPipeServer, sync::Mutex};

use super::{AuthenticatedConnection, LocalEndpointAddress, PeerIdentity};
use crate::{DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery};

// Sixty-four production connections plus one always-owned pending accept instance.
const MAX_PIPE_INSTANCES: usize = 65;

pub(super) const fn recover_stale(
    _state_root: &Path,
    _identity: &DaemonIdentity,
) -> Result<(), DaemonError> {
    // Named-pipe instances disappear with the owning process; there is no filesystem object.
    Ok(())
}

/// One serialized pending named-pipe instance plus its immutable owner SID.
pub(super) struct PlatformEndpoint {
    pipe_name: String,
    owner_sid: Vec<u8>,
    pending: Mutex<NamedPipeServer>,
}

impl PlatformEndpoint {
    pub(super) fn owner_peer(&self) -> PeerIdentity {
        PeerIdentity::from_os_identity(ApplicationPrincipalKind::WindowsPeer, &self.owner_sid)
    }

    pub(super) async fn bind(
        _state_root: &Path,
        identity: &DaemonIdentity,
    ) -> Result<(Self, LocalEndpointAddress), DaemonError> {
        // The pipe identity deliberately has no caller-controlled path component.
        let pipe_name = format!(r"\\.\pipe\{}", identity.endpoint_name());
        let owner_sid = native::current_user_sid().map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Unsupported,
                DaemonRecovery::Operator,
                "resolve Windows daemon principal",
                "current process user SID could not be resolved",
                error,
            )
        })?;
        let server =
            native::create_server(&pipe_name, &owner_sid, true).map_err(first_instance_error)?;
        let endpoint =
            Self { pipe_name: pipe_name.clone(), owner_sid, pending: Mutex::new(server) };
        Ok((endpoint, LocalEndpointAddress::Windows(pipe_name)))
    }

    pub(super) async fn accept(&self) -> Result<AuthenticatedConnection, DaemonError> {
        // Exactly one task owns the pending-instance connect/rearm transition at a time.
        let mut pending = self.pending.lock().await;
        pending.connect().await.map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Transport,
                DaemonRecovery::Retry,
                "accept Windows pipe connection",
                "pending named-pipe instance could not accept a client",
                error,
            )
        })?;
        let replacement = match native::create_server(&self.pipe_name, &self.owner_sid, false) {
            Ok(server) => server,
            Err(error) => {
                let _ = pending.disconnect();
                return Err(DaemonError::with_source(
                    DaemonErrorCode::ResourceLimit,
                    DaemonRecovery::Retry,
                    "rearm Windows pipe endpoint",
                    "bounded named-pipe instance could not be rearmed",
                    error,
                ));
            }
        };
        let connected = core::mem::replace(&mut *pending, replacement);
        drop(pending);

        let peer_sid = native::connected_peer_sid(&connected).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Unauthorized,
                DaemonRecovery::Operator,
                "authenticate Windows pipe peer",
                "accepted named-pipe client token SID could not be verified",
                error,
            )
        })?;
        if peer_sid != self.owner_sid {
            return Err(DaemonError::new(
                DaemonErrorCode::Unauthorized,
                DaemonRecovery::Operator,
                "authenticate Windows pipe peer",
                "accepted named-pipe client is not the daemon user",
            ));
        }
        let peer = self.owner_peer();
        Ok(AuthenticatedConnection::new(Box::new(connected), peer))
    }
}

fn first_instance_error(error: io::Error) -> DaemonError {
    if error.kind() == io::ErrorKind::PermissionDenied
        || error.kind() == io::ErrorKind::AlreadyExists
    {
        DaemonError::with_source(
            DaemonErrorCode::AlreadyRunning,
            DaemonRecovery::Reconcile,
            "bind Windows pipe endpoint",
            "exclusive named-pipe identity already exists",
            error,
        )
    } else {
        DaemonError::with_source(
            DaemonErrorCode::Transport,
            DaemonRecovery::Operator,
            "bind Windows pipe endpoint",
            "secure named-pipe instance could not be created",
            error,
        )
    }
}

#[allow(
    unsafe_code,
    reason = "this module is the inventoried Win32 descriptor, token, SID, and impersonation TCB"
)]
mod native {
    mod identity;

    use std::{io, mem::size_of, os::windows::io::AsRawHandle, ptr};

    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE},
        Security::{
            ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, AddAccessAllowedAce, CreateWellKnownSid,
            InitializeAcl, InitializeSecurityDescriptor, RevertToSelf, SECURITY_ATTRIBUTES,
            SECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE, SetSecurityDescriptorDacl, TOKEN_QUERY,
            WinLocalSystemSid,
        },
        System::{
            Pipes::ImpersonateNamedPipeClient,
            Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
        },
    };

    use super::MAX_PIPE_INSTANCES;
    use identity::{aligned_sid, invalid_native_data, token_user_sid};

    const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
    const MAX_TOKEN_INFORMATION_BYTES: usize = 64 * 1024;
    const MAX_SID_BYTES: usize = 68;
    const MINIMUM_SID_BYTES: usize = 8;

    struct OwnedToken(HANDLE);

    impl Drop for OwnedToken {
        fn drop(&mut self) {
            // SAFETY: OpenProcessToken/OpenThreadToken returned this non-null owned handle, and
            // this Drop is its sole close site.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    struct ImpersonationGuard {
        active: bool,
    }

    impl ImpersonationGuard {
        fn revert(mut self) -> io::Result<()> {
            // SAFETY: this guard is constructed only after successful thread impersonation and no
            // await or thread migration occurs before this call.
            if unsafe { RevertToSelf() } == 0 {
                return Err(io::Error::last_os_error());
            }
            self.active = false;
            Ok(())
        }
    }

    impl Drop for ImpersonationGuard {
        fn drop(&mut self) {
            if self.active {
                // SAFETY: the active flag denotes successful impersonation on this exact thread.
                // Windows requires process termination if reverting the security context fails.
                if unsafe { RevertToSelf() } == 0 {
                    std::process::abort();
                }
            }
        }
    }

    pub(super) fn current_user_sid() -> io::Result<Vec<u8>> {
        let mut token = ptr::null_mut();
        // SAFETY: the pseudo-process handle is always valid; token points to writable HANDLE
        // storage and ownership transfers to OwnedToken only on success.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if token.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OpenProcessToken returned a null handle",
            ));
        }
        token_user_sid(&OwnedToken(token))
    }

    pub(super) fn connected_peer_sid(server: &NamedPipeServer) -> io::Result<Vec<u8>> {
        let pipe = server.as_raw_handle();
        // SAFETY: NamedPipeServer owns a connected pipe handle; the guard prevents any return or
        // unwind from retaining the client impersonation context.
        if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let guard = ImpersonationGuard { active: true };
        let mut token = ptr::null_mut();
        // SAFETY: execution has not crossed an await since impersonation, the current-thread
        // pseudo-handle is valid, and token points to writable HANDLE storage.
        let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) };
        let open_error = if opened == 0 { Some(io::Error::last_os_error()) } else { None };
        guard.revert()?;
        if let Some(error) = open_error {
            return Err(error);
        }
        if token.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OpenThreadToken returned a null handle",
            ));
        }
        token_user_sid(&OwnedToken(token))
    }

    pub(super) fn create_server(
        pipe_name: &str,
        owner_sid: &[u8],
        first_instance: bool,
    ) -> io::Result<NamedPipeServer> {
        let mut owner = aligned_sid(owner_sid)?;
        let mut system = vec![0_u32; MAX_SID_BYTES.div_ceil(4)];
        let mut system_length = SECURITY_MAX_SID_SIZE;
        // SAFETY: system is DWORD-aligned writable storage of system_length bytes and the null
        // domain SID is required for WinLocalSystemSid.
        if unsafe {
            CreateWellKnownSid(
                WinLocalSystemSid,
                ptr::null_mut(),
                system.as_mut_ptr().cast(),
                &mut system_length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let system_length = usize::try_from(system_length).map_err(|_| invalid_native_data())?;
        let owner_length = owner_sid.len();
        let ace_overhead = size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>();
        let acl_length = size_of::<ACL>()
            .checked_add(ace_overhead)
            .and_then(|value| value.checked_add(owner_length))
            .and_then(|value| value.checked_add(ace_overhead))
            .and_then(|value| value.checked_add(system_length))
            .ok_or_else(invalid_native_data)?;
        let acl_length_u32 = u32::try_from(acl_length).map_err(|_| invalid_native_data())?;
        let mut acl_storage = vec![0_u32; acl_length.div_ceil(size_of::<u32>())];
        let acl = acl_storage.as_mut_ptr().cast::<ACL>();

        // SAFETY: acl_storage is DWORD-aligned and provides at least acl_length writable bytes.
        if unsafe { InitializeAcl(acl, acl_length_u32, ACL_REVISION) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: owner is a validated DWORD-aligned SID and both buffers outlive the ACL use.
        if unsafe {
            AddAccessAllowedAce(
                acl,
                ACL_REVISION,
                GENERIC_READ | GENERIC_WRITE,
                owner.as_mut_ptr().cast(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: system is an initialized DWORD-aligned LocalSystem SID and remains live.
        if unsafe {
            AddAccessAllowedAce(
                acl,
                ACL_REVISION,
                GENERIC_READ | GENERIC_WRITE,
                system.as_mut_ptr().cast(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }

        let mut descriptor = SECURITY_DESCRIPTOR::default();
        // SAFETY: descriptor points to writable storage for one absolute security descriptor.
        if unsafe {
            InitializeSecurityDescriptor(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                SECURITY_DESCRIPTOR_REVISION,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: descriptor was initialized, acl is initialized, and acl_storage remains alive
        // until CreateNamedPipeW has copied the complete descriptor.
        if unsafe {
            SetSecurityDescriptorDacl(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                1,
                acl,
                0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .map_err(|_| invalid_native_data())?,
            lpSecurityDescriptor: (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
            bInheritHandle: 0,
        };
        let mut options = ServerOptions::new();
        options
            .first_pipe_instance(first_instance)
            .reject_remote_clients(true)
            .max_instances(MAX_PIPE_INSTANCES);
        // SAFETY: attributes, descriptor, ACL, and SID buffers are initialized and live for the
        // complete synchronous CreateNamedPipeW call made by Tokio.
        unsafe {
            options.create_with_security_attributes_raw(
                pipe_name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )
        }
    }
}
