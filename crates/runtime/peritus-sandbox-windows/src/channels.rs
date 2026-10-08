//! Authorized proxy/secret preparation and exact protected-handle projection.

use std::{ffi::OsStr, fs::File, io::Cursor, net::SocketAddr};

use peritus_network::{ManagedProxy, NetworkError, NetworkErrorKind};
use peritus_process::{ExecutionPlan, NativeProtectedHandle, ProcessError};
use peritus_sandbox::{CheckedSandboxPlan, SecretDelivery, SecretRequirement};
use peritus_secrets::{
    DeliveryArtifact, SecretDeliverySession, SecretError, SecretErrorKind,
};

use crate::{
    NetworkIsolation, PreparationCleanup, ProtectedSecretHandle, ProxyRoute,
    SecretHandleDestination, WindowsBackendConfig, WindowsError, WindowsErrorKind,
    WindowsErrorSource, WindowsOperation, WindowsRecovery, network_filter::NetworkFilterOwner,
    secret_reference_digest,
};

/// Effect-free representation of the exact selected protected channels.
pub(crate) struct PreparedChannelPlan {
    network: NetworkIsolation,
    secrets: Vec<ProtectedSecretHandle>,
}

impl PreparedChannelPlan {
    pub(crate) fn preflight(
        config: &WindowsBackendConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        authorized: bool,
        managed_network_supported: bool,
        credential_delivery_supported: bool,
    ) -> Result<Self, WindowsError> {
        let network = preflight_network(
            config,
            sandbox,
            authorized,
            managed_network_supported,
        )?;
        let secrets = preflight_secrets(
            config,
            execution,
            sandbox,
            authorized,
            credential_delivery_supported,
        )?;
        validate_secret_destinations(
            config,
            execution,
            sandbox.requirements().secrets(),
            matches!(network, NetworkIsolation::ManagedProxy(_)),
        )?;
        Ok(Self { network, secrets })
    }

    pub(crate) const fn network(&self) -> NetworkIsolation {
        self.network
    }

    pub(crate) fn secrets(&self) -> &[ProtectedSecretHandle] {
        &self.secrets
    }
}

pub(crate) struct PreparedChannels {
    pub(crate) network: NetworkIsolation,
    pub(crate) secrets: Vec<ProtectedSecretHandle>,
    pub(crate) handles: Vec<NativeProtectedHandle>,
    pub(crate) proxy_owner: Option<ManagedProxy>,
    pub(crate) filter_owner: NetworkFilterOwner,
    pub(crate) secret_owner: Option<SecretDeliverySession>,
}

impl PreparedChannels {
    pub(crate) fn prepare(
        config: &mut WindowsBackendConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        plan: &PreparedChannelPlan,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<Self, WindowsError> {
        let mut prepared = Self {
            network: NetworkIsolation::DenyAll,
            secrets: Vec::new(),
            handles: Vec::new(),
            proxy_owner: None,
            filter_owner: NetworkFilterOwner::inactive(),
            secret_owner: None,
        };
        if !plan.secrets.is_empty()
            && let Err(error) = prepared.prepare_secrets(
                config,
                execution,
                sandbox,
                should_continue,
            )
        {
            return Err(prepared.cleanup(error));
        }
        if matches!(plan.network, NetworkIsolation::ManagedProxy(_))
            && let Err(error) = prepared.prepare_network(config, sandbox, should_continue)
        {
            return Err(prepared.cleanup(error));
        }
        Ok(prepared)
    }

    fn prepare_secrets(
        &mut self,
        config: &mut WindowsBackendConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<(), WindowsError> {
        ensure_continues(should_continue)?;
        let requirements = sandbox.requirements().secrets();
        let preparation = config.secrets.take().ok_or_else(|| {
            channel_error(WindowsErrorKind::Secret, "checked secrets lack exact inert preparation")
        })?;
        let session = preparation
            .prepare_cancellable(
                execution.identity().process_id(),
                execution.identity().environment_id(),
                sandbox.digest(),
                execution.digest(),
                requirements,
                || should_continue(),
                |_, _| {},
            )
            .map_err(secret_prepare_error)?;
        self.secret_owner = Some(session);
        let owner = self.secret_owner.as_ref().ok_or_else(|| {
            channel_error(WindowsErrorKind::Secret, "secret delivery owner was not retained")
        })?;
        let (secrets, handles) = stage_secret_handles(owner, requirements, should_continue)?;
        self.secrets = secrets;
        self.handles.extend(handles);
        Ok(())
    }

    fn prepare_network(
        &mut self,
        config: &mut WindowsBackendConfig,
        sandbox: &CheckedSandboxPlan,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<(), WindowsError> {
        ensure_continues(should_continue)?;
        let preparation = config.proxy.take().ok_or_else(|| {
            channel_error(WindowsErrorKind::Network, "network egress lacks its exact proxy preparation")
        })?;
        let proxy = preparation.prepare(sandbox).map_err(proxy_prepare_error)?;
        self.proxy_owner = Some(proxy);
        ensure_continues(should_continue)?;
        let proxy = self.proxy_owner.as_ref().ok_or_else(|| {
            channel_error(WindowsErrorKind::Network, "managed proxy owner was not retained")
        })?;
        let protected = proxy
            .routing_token()
            .expose_bytes(|bytes| {
                NativeProtectedHandle::from_reader(
                    "windows-managed-proxy-token",
                    Cursor::new(bytes),
                    || should_continue(),
                    |_| {},
                )
            })
            .map_err(protected_handle_error)?;
        let controller = config.managed_filter_digest.ok_or_else(|| {
            channel_error(WindowsErrorKind::Network, "managed network filter identity is absent")
        })?;
        let endpoint = proxy.endpoint().socket_addr();
        let filter = crate::network::managed_wfp_policy_digest(
            controller,
            config.token.principal_sid(),
            endpoint,
            sandbox.digest(),
        );
        let route = ProxyRoute::new(endpoint, protected.raw_handle(), sandbox.digest(), filter)?;
        self.handles.push(protected);
        self.filter_owner = NetworkFilterOwner::install(&config.token, route)?;
        self.network = NetworkIsolation::ManagedProxy(route);
        Ok(())
    }

    pub(crate) fn cleanup(&mut self, original: WindowsError) -> WindowsError {
        self.handles.clear();
        self.secrets.clear();
        self.network = NetworkIsolation::DenyAll;

        let secret_release = self
            .secret_owner
            .as_mut()
            .is_some_and(|owner| owner.release().is_err());
        if !secret_release {
            self.secret_owner = None;
        }
        let filter_release = self.filter_owner.release().is_err();
        let proxy_shutdown = self
            .proxy_owner
            .as_mut()
            .is_some_and(|owner| owner.reconcile_shutdown().is_err());
        if !proxy_shutdown {
            self.proxy_owner = None;
        }
        original.with_cleanup(PreparationCleanup::new(
            false,
            filter_release,
            proxy_shutdown,
            secret_release,
        ))
    }
}

fn preflight_network(
    config: &WindowsBackendConfig,
    sandbox: &CheckedSandboxPlan,
    authorized: bool,
    managed_network_supported: bool,
) -> Result<NetworkIsolation, WindowsError> {
    if sandbox.requirements().network().is_empty() {
        if !config.token.is_app_container() {
            return Err(channel_error(
                WindowsErrorKind::Network,
                "deny-all networking requires AppContainer isolation",
            ));
        }
        return Ok(NetworkIsolation::DenyAll);
    }
    if !authorized || !managed_network_supported {
        return Err(crate::error::unsupported(
            WindowsOperation::Prepare,
            "managed network preparation is unavailable before authorization or native support",
        ));
    }
    let preparation = config.proxy.as_ref().ok_or_else(|| {
        channel_error(WindowsErrorKind::Network, "network egress lacks its exact proxy preparation")
    })?;
    preparation.preflight(sandbox).map_err(proxy_prepare_error)?;
    let controller = config.managed_filter_digest.ok_or_else(|| {
        channel_error(WindowsErrorKind::Network, "managed network filter identity is absent")
    })?;
    let endpoint = SocketAddr::from(([127, 0, 0, 1], 1));
    let filter = crate::network::managed_wfp_policy_digest(
        controller,
        config.token.principal_sid(),
        endpoint,
        sandbox.digest(),
    );
    ProxyRoute::new(endpoint, 1, sandbox.digest(), filter).map(NetworkIsolation::ManagedProxy)
}

fn preflight_secrets(
    config: &WindowsBackendConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    authorized: bool,
    credential_delivery_supported: bool,
) -> Result<Vec<ProtectedSecretHandle>, WindowsError> {
    let requirements = sandbox.requirements().secrets();
    if requirements.is_empty() {
        return Ok(Vec::new());
    }
    if !authorized || !credential_delivery_supported {
        return Err(crate::error::unsupported(
            WindowsOperation::Prepare,
            "secret delivery cannot begin without authorization and credential-store support",
        ));
    }
    let preparation = config.secrets.as_ref().ok_or_else(|| {
        channel_error(WindowsErrorKind::Secret, "checked secrets lack exact inert preparation")
    })?;
    preparation
        .preflight(
            execution.identity().process_id(),
            execution.identity().environment_id(),
            sandbox.digest(),
            execution.digest(),
            requirements,
        )
        .map_err(secret_prepare_error)?;
    let mut descriptors = Vec::with_capacity(requirements.len());
    for (index, requirement) in requirements.iter().enumerate() {
        let handle = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| {
                channel_error(WindowsErrorKind::Handle, "secret handle count is not representable")
            })?;
        descriptors.push(ProtectedSecretHandle::new(
            handle,
            secret_reference_digest(requirement.reference()),
            SecretHandleDestination::from(requirement.delivery()),
        )?);
    }
    crate::canonical_handles(descriptors)
}

fn validate_secret_destinations(
    config: &WindowsBackendConfig,
    execution: &ExecutionPlan,
    requirements: &[SecretRequirement],
    managed_network: bool,
) -> Result<(), WindowsError> {
    for (index, requirement) in requirements.iter().enumerate() {
        match requirement.delivery() {
            SecretDelivery::Environment(name) => {
                let name = OsStr::new(name.as_str());
                if execution.environment().variables().iter().any(|variable| {
                    crate::manifest::windows_name_cmp(variable.name(), name).is_eq()
                }) || requirements[..index].iter().any(|prior| {
                    matches!(prior.delivery(), SecretDelivery::Environment(prior_name)
                        if crate::manifest::windows_name_cmp(
                            OsStr::new(prior_name.as_str()), name
                        ).is_eq())
                }) || (managed_network
                    && [OsStr::new("HTTP_PROXY"), OsStr::new("HTTPS_PROXY")]
                        .into_iter()
                        .any(|reserved| {
                            crate::manifest::windows_name_cmp(reserved, name).is_eq()
                        }))
                {
                    return Err(channel_error(
                        WindowsErrorKind::Secret,
                        "secret environment destination collides with another exact value",
                    ));
                }
            }
            SecretDelivery::File(path) => {
                let native = crate::WindowsPath::from_sandbox(&config.workspace, path)?;
                #[cfg(target_os = "windows")]
                validate_native_secret_destination(&native)?;
                #[cfg(not(target_os = "windows"))]
                let _ = native;
            }
            SecretDelivery::BrokeredHandle(_) => {}
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_native_secret_destination(path: &crate::WindowsPath) -> Result<(), WindowsError> {
    let native = path.to_path_buf();
    let parent = native.parent().ok_or_else(|| {
        channel_error(WindowsErrorKind::Path, "secret file destination lacks a parent")
    })?;
    let parent = crate::WindowsPath::from_os_str(parent.as_os_str())?;
    crate::ResolvedWindowsPath::resolve(parent)?;
    match std::fs::symlink_metadata(native) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(channel_error(
            WindowsErrorKind::Secret,
            "secret file destination already exists",
        )),
        Err(source) => Err(io_channel_error(
            "secret file destination cannot be inspected",
            &source,
        )),
    }
}

fn stage_secret_handles(
    session: &SecretDeliverySession,
    requirements: &[SecretRequirement],
    should_continue: &dyn Fn() -> bool,
) -> Result<(Vec<ProtectedSecretHandle>, Vec<NativeProtectedHandle>), WindowsError> {
    if session.artifacts().len() != requirements.len()
        || session.leases().len() != requirements.len()
    {
        return Err(channel_error(
            WindowsErrorKind::Secret,
            "prepared secret artifacts, leases, and requirements differ",
        ));
    }
    let mut descriptors = Vec::with_capacity(requirements.len());
    let mut handles = Vec::with_capacity(requirements.len());
    for (index, ((requirement, artifact), lease)) in requirements
        .iter()
        .zip(session.artifacts())
        .zip(session.leases())
        .enumerate()
    {
        if lease.reference() != requirement.reference()
            || lease.delivery() != requirement.delivery()
        {
            return Err(channel_error(
                WindowsErrorKind::Secret,
                "prepared secret lease differs from checked delivery",
            ));
        }
        validate_artifact(requirement.delivery(), artifact)?;
        let (protected, payload_len) = stage_artifact(
            format!("windows-secret-{index}"),
            artifact,
            should_continue,
        )?;
        descriptors.push(ProtectedSecretHandle::new_bound(
            protected.raw_handle(),
            secret_reference_digest(requirement.reference()),
            SecretHandleDestination::from(requirement.delivery()),
            payload_len,
        )?);
        handles.push(protected);
    }
    Ok((crate::canonical_handles(descriptors)?, handles))
}

fn stage_artifact(
    label: String,
    artifact: &DeliveryArtifact,
    should_continue: &dyn Fn() -> bool,
) -> Result<(NativeProtectedHandle, u64), WindowsError> {
    if let Some(handle) = artifact.expose_environment(|name, bytes| {
        let text = core::str::from_utf8(bytes).map_err(|_| {
            channel_error(
                WindowsErrorKind::Secret,
                "environment secret is not valid UTF-8",
            )
        })?;
        if text.contains('\0') {
            return Err(channel_error(
                WindowsErrorKind::Secret,
                "environment secret contains NUL",
            ));
        }
        let native_units = name
            .as_str()
            .encode_utf16()
            .count()
            .checked_add(text.encode_utf16().count())
            .and_then(|value| value.checked_add(3))
            .ok_or_else(|| {
                channel_error(
                    WindowsErrorKind::Secret,
                    "environment secret native size overflowed",
                )
            })?;
        if native_units > 32_767 {
            return Err(channel_error(
                WindowsErrorKind::Secret,
                "environment secret exceeds native Windows capacity",
            ));
        }
        stage_reader(
            label.clone(),
            Cursor::new(bytes),
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            should_continue,
        )
    }) {
        return handle;
    }
    if let Some(handle) = artifact.expose_brokered(|_, bytes| {
        stage_reader(
            label.clone(),
            Cursor::new(bytes),
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            should_continue,
        )
    }) {
        return handle;
    }
    let (path, _) = artifact.file_paths().ok_or_else(|| {
        channel_error(WindowsErrorKind::Secret, "secret artifact has no representable destination")
    })?;
    let (mut file, opened) = open_staged_secret(path)?;
    let total = opened.len();
    let handle = stage_reader(label, &mut file, total, should_continue)?;
    let after = file.metadata().map_err(|source| {
        io_channel_error("staged secret identity cannot be rechecked", &source)
    })?;
    if !same_file_identity(&opened, &after) || opened.len() != after.len() {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "staged secret identity changed during protected transfer",
        ));
    }
    Ok(handle)
}

fn open_staged_secret(path: &std::path::Path) -> Result<(File, std::fs::Metadata), WindowsError> {
    let selected = std::fs::symlink_metadata(path)
        .map_err(|source| io_channel_error("staged secret path cannot be inspected", &source))?;
    if !selected.is_file() || selected.file_type().is_symlink() {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "staged secret path changed kind before protected transfer",
        ));
    }
    let file = File::open(path)
        .map_err(|source| io_channel_error("staged secret file cannot be opened", &source))?;
    let opened = file.metadata().map_err(|source| {
        io_channel_error("opened staged secret cannot be inspected", &source)
    })?;
    if !same_file_identity(&selected, &opened) {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "staged secret identity changed before protected transfer",
        ));
    }
    Ok((file, opened))
}

fn stage_reader(
    label: String,
    reader: impl std::io::Read,
    total: u64,
    should_continue: &dyn Fn() -> bool,
) -> Result<(NativeProtectedHandle, u64), WindowsError> {
    if total == 0 {
        return Err(channel_error(
            WindowsErrorKind::Secret,
            "secret payload is empty",
        ));
    }
    let handle = NativeProtectedHandle::from_reader(
        label,
        reader,
        || should_continue(),
        |_| {},
    )
    .map_err(protected_handle_error)?;
    if handle
        .payload_len()
        .and_then(|length| u64::try_from(length).ok())
        != Some(total)
    {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "secret artifact changed while it was streamed",
        ));
    }
    Ok((handle, total))
}

#[cfg(unix)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    left.dev() == right.dev() && left.ino() == right.ino() && left.nlink() == 1 && right.nlink() == 1
}

#[cfg(windows)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    left.volume_serial_number().is_some()
        && left.volume_serial_number() == right.volume_serial_number()
        && left.file_index().is_some()
        && left.file_index() == right.file_index()
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    left.len() == right.len()
}

fn validate_artifact(
    expected: &SecretDelivery,
    artifact: &DeliveryArtifact,
) -> Result<(), WindowsError> {
    let exact = match expected {
        SecretDelivery::Environment(name) => {
            artifact.expose_environment(|actual, _| actual == name).unwrap_or(false)
        }
        SecretDelivery::File(path) => {
            artifact.file_paths().is_some_and(|(_, actual)| actual == path)
        }
        SecretDelivery::BrokeredHandle(label) => {
            artifact.expose_brokered(|actual, _| actual == label).unwrap_or(false)
        }
    };
    if exact {
        Ok(())
    } else {
        Err(channel_error(
            WindowsErrorKind::Secret,
            "prepared secret artifact differs from checked delivery",
        ))
    }
}

fn ensure_continues(should_continue: &dyn Fn() -> bool) -> Result<(), WindowsError> {
    if should_continue() {
        Ok(())
    } else {
        Err(crate::probe::probe_cancelled())
    }
}

fn proxy_prepare_error(source: NetworkError) -> WindowsError {
    let error = channel_error(WindowsErrorKind::Network, "managed proxy cannot be prepared")
        .with_source(crate::error::network_source(&source));
    if source.kind() == NetworkErrorKind::IncompleteTeardown {
        error.with_cleanup(PreparationCleanup::new(false, false, true, false))
    } else {
        error
    }
}

fn secret_prepare_error(source: SecretError) -> WindowsError {
    let error = channel_error(WindowsErrorKind::Secret, "exact secret leases cannot be prepared")
        .with_source(crate::error::secret_source(&source));
    if source.kind() == SecretErrorKind::Cleanup {
        error.with_cleanup(PreparationCleanup::new(false, false, false, true))
    } else {
        error
    }
}

fn protected_handle_error(source: ProcessError) -> WindowsError {
    channel_error(WindowsErrorKind::Handle, "protected channel handle cannot be staged")
        .with_source(crate::error::process_source(&source))
}

fn io_channel_error(detail: &'static str, source: &std::io::Error) -> WindowsError {
    channel_error(WindowsErrorKind::Io, detail).with_source(WindowsErrorSource::Io(source.kind()))
}

fn channel_error(kind: WindowsErrorKind, detail: &'static str) -> WindowsError {
    WindowsError::new(kind, WindowsOperation::Prepare, WindowsRecovery::CancelAndReap, detail)
}
