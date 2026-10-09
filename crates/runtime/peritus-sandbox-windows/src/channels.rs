//! Authorized proxy/secret preparation and exact protected-handle projection.

use std::{ffi::OsStr, net::SocketAddr};

use peritus_network::{ManagedProxy, NetworkError, NetworkErrorKind};
use peritus_process::{ExecutionPlan, NativeProtectedHandle, ProcessError};
use peritus_sandbox::{CheckedSandboxPlan, SecretDelivery, SecretRequirement};
use peritus_secrets::{SecretDeliverySession, SecretError, SecretErrorKind};

use crate::{
    NetworkIsolation, PreparationCleanup, ProtectedSecretHandle, ProxyRoute,
    SecretHandleDestination, WindowsBackendConfig, WindowsError, WindowsErrorKind,
    WindowsErrorSource, WindowsOperation, WindowsRecovery, network_filter::NetworkFilterOwner,
    secret_reference_digest,
};

mod staging;
use staging::stage_secret_handles;

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
        let network = preflight_network(config, sandbox, authorized, managed_network_supported)?;
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

#[derive(Debug)]
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
            && let Err(error) =
                prepared.prepare_secrets(config, execution, sandbox, should_continue)
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
        self.secret_owner = Some(SecretDeliverySession::new());
        let session = self
            .secret_owner
            .as_mut()
            .ok_or_else(|| channel_error(WindowsErrorKind::Secret, "secret owner is absent"))?;
        preparation
            .prepare_into(
                execution.identity().process_id(),
                execution.identity().environment_id(),
                sandbox.digest(),
                execution.digest(),
                requirements,
                session,
            )
            .map_err(|source| secret_prepare_error(&source))?;
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
            channel_error(
                WindowsErrorKind::Network,
                "network egress lacks its exact proxy preparation",
            )
        })?;
        let proxy = preparation.prepare(sandbox).map_err(|source| proxy_prepare_error(&source))?;
        self.proxy_owner = Some(proxy);
        ensure_continues(should_continue)?;
        let proxy = self.proxy_owner.as_ref().ok_or_else(|| {
            channel_error(WindowsErrorKind::Network, "managed proxy owner was not retained")
        })?;
        let protected = proxy
            .routing_token()
            .expose_bytes(|bytes| {
                NativeProtectedHandle::from_bytes("windows-managed-proxy-token", bytes.to_vec())
            })
            .map_err(|source| protected_handle_error(&source))?;
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

        let secret_release =
            self.secret_owner.as_mut().is_some_and(|owner| owner.release().is_err());
        if !secret_release {
            self.secret_owner = None;
        }
        let filter_release = self.filter_owner.release().is_err();
        let proxy_shutdown =
            self.proxy_owner.as_mut().is_some_and(|owner| owner.reconcile_shutdown().is_err());
        if !proxy_shutdown {
            self.proxy_owner = None;
        }
        let original = original.with_cleanup(PreparationCleanup::new([
            false,
            filter_release,
            proxy_shutdown,
            secret_release,
        ]));
        if filter_release || proxy_shutdown || secret_release {
            let owner = core::mem::replace(
                self,
                Self {
                    network: NetworkIsolation::DenyAll,
                    secrets: Vec::new(),
                    handles: Vec::new(),
                    proxy_owner: None,
                    filter_owner: NetworkFilterOwner::inactive(),
                    secret_owner: None,
                },
            );
            original.retain_cleanup(crate::error::CleanupOwner::Channels(Box::new(owner)))
        } else {
            original
        }
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
    preparation.preflight(sandbox).map_err(|source| proxy_prepare_error(&source))?;
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
        .map_err(|source| secret_prepare_error(&source))?;
    let mut descriptors = Vec::with_capacity(requirements.len());
    for (index, requirement) in requirements.iter().enumerate() {
        let handle =
            u64::try_from(index).ok().and_then(|value| value.checked_add(2)).ok_or_else(|| {
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
                    crate::manifest::windows_name_cmp(OsStr::new(variable.name()), name).is_eq()
                }) || requirements[..index].iter().any(|prior| {
                    matches!(prior.delivery(), SecretDelivery::Environment(prior_name)
                        if crate::manifest::windows_name_cmp(
                            OsStr::new(prior_name.as_str()), name
                        ).is_eq())
                }) || (managed_network
                    && [OsStr::new("HTTP_PROXY"), OsStr::new("HTTPS_PROXY")]
                        .into_iter()
                        .any(|reserved| crate::manifest::windows_name_cmp(reserved, name).is_eq()))
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
        Ok(_) => {
            Err(channel_error(WindowsErrorKind::Secret, "secret file destination already exists"))
        }
        Err(source) => {
            Err(io_channel_error("secret file destination cannot be inspected", &source))
        }
    }
}

fn ensure_continues(should_continue: &dyn Fn() -> bool) -> Result<(), WindowsError> {
    if should_continue() { Ok(()) } else { Err(crate::probe::probe_cancelled()) }
}

fn proxy_prepare_error(source: &NetworkError) -> WindowsError {
    let error = channel_error(WindowsErrorKind::Network, "managed proxy cannot be prepared")
        .with_source(crate::error::network_source(source));
    if source.kind() == NetworkErrorKind::IncompleteTeardown {
        error.with_cleanup(PreparationCleanup::new([false, false, true, false]))
    } else {
        error
    }
}

fn secret_prepare_error(source: &SecretError) -> WindowsError {
    let error = channel_error(WindowsErrorKind::Secret, "exact secret leases cannot be prepared")
        .with_source(crate::error::secret_source(source));
    if source.kind() == SecretErrorKind::Cleanup {
        error.with_cleanup(PreparationCleanup::new([false, false, false, true]))
    } else {
        error
    }
}

fn protected_handle_error(source: &ProcessError) -> WindowsError {
    channel_error(WindowsErrorKind::Handle, "protected channel handle cannot be staged")
        .with_source(crate::error::process_source(source))
}

fn io_channel_error(detail: &'static str, source: &std::io::Error) -> WindowsError {
    channel_error(WindowsErrorKind::Io, detail).with_source(WindowsErrorSource::Io(source.kind()))
}

fn channel_error(kind: WindowsErrorKind, detail: &'static str) -> WindowsError {
    WindowsError::new(kind, WindowsOperation::Prepare, WindowsRecovery::CancelAndReap, detail)
}

#[cfg(test)]
#[path = "channels_tests.rs"]
mod tests;
