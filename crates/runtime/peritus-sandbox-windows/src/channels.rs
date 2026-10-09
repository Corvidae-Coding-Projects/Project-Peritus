//! Authorized proxy/secret preparation and exact protected-handle projection.

use peritus_network::ManagedProxy;
use peritus_process::{ExecutionPlan, NativeProtectedHandle};
use peritus_sandbox::{CheckedSandboxPlan, SecretDelivery};
use peritus_secrets::{DeliveryArtifact, SecretDeliverySession};

use crate::{
    NetworkIsolation, ProtectedSecretHandle, ProxyRoute, SecretHandleDestination,
    WindowsBackendConfig, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery,
    network_filter::NetworkFilterOwner, secret_reference_digest,
};

pub(crate) struct PreparedChannels {
    pub(crate) network: NetworkIsolation,
    pub(crate) secrets: Vec<ProtectedSecretHandle>,
    pub(crate) handles: Vec<NativeProtectedHandle>,
    pub(crate) proxy_owner: Option<ManagedProxy>,
    pub(crate) filter_owner: NetworkFilterOwner,
    pub(crate) secret_owner: Option<SecretDeliverySession>,
}

struct PreparedNetwork {
    isolation: NetworkIsolation,
    handles: Vec<NativeProtectedHandle>,
    proxy: Option<ManagedProxy>,
    filter: NetworkFilterOwner,
}

type SecretChannels =
    (Vec<ProtectedSecretHandle>, Vec<NativeProtectedHandle>, Option<SecretDeliverySession>);

impl PreparedChannels {
    pub(crate) fn prepare(
        config: &mut WindowsBackendConfig,
        execution: &ExecutionPlan,
        sandbox: &CheckedSandboxPlan,
        authorized: bool,
        managed_network_supported: bool,
    ) -> Result<Self, WindowsError> {
        let network = prepare_network(config, sandbox, authorized, managed_network_supported)?;
        let mut handles = network.handles;
        let (secrets, secret_handles, secret_owner) =
            prepare_secrets(config, execution, sandbox, authorized)?;
        handles.extend(secret_handles);
        Ok(Self {
            network: network.isolation,
            secrets,
            handles,
            proxy_owner: network.proxy,
            filter_owner: network.filter,
            secret_owner,
        })
    }
}

fn prepare_network(
    config: &mut WindowsBackendConfig,
    sandbox: &CheckedSandboxPlan,
    authorized: bool,
    managed_network_supported: bool,
) -> Result<PreparedNetwork, WindowsError> {
    if sandbox.requirements().network().is_empty() {
        if config.proxy.is_some() || !config.token.is_app_container() {
            return Err(channel_error(
                WindowsErrorKind::Network,
                "deny-all networking requires AppContainer and no managed proxy",
            ));
        }
        return Ok(PreparedNetwork {
            isolation: NetworkIsolation::DenyAll,
            handles: Vec::new(),
            proxy: None,
            filter: NetworkFilterOwner::inactive(),
        });
    }
    if !authorized || !managed_network_supported {
        return Err(crate::error::unsupported(
            WindowsOperation::Prepare,
            "managed network preparation is unavailable before authorization or native support",
        ));
    }
    let preparation = config.proxy.take().ok_or_else(|| {
        channel_error(WindowsErrorKind::Network, "network egress lacks its exact proxy preparation")
    })?;
    let proxy = preparation.prepare(sandbox).map_err(|_| {
        channel_error(WindowsErrorKind::Network, "managed proxy cannot be prepared")
    })?;
    let protected = proxy
        .routing_token()
        .expose_bytes(|bytes| {
            NativeProtectedHandle::from_bytes("windows-managed-proxy-token", bytes.to_vec())
        })
        .map_err(|_| {
            channel_error(WindowsErrorKind::Handle, "proxy token handle cannot be staged")
        })?;
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
    let owner = NetworkFilterOwner::install(&config.token, route)?;
    Ok(PreparedNetwork {
        isolation: NetworkIsolation::ManagedProxy(route),
        handles: vec![protected],
        proxy: Some(proxy),
        filter: owner,
    })
}

fn prepare_secrets(
    config: &mut WindowsBackendConfig,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
    authorized: bool,
) -> Result<SecretChannels, WindowsError> {
    let requirements = sandbox.requirements().secrets();
    if requirements.is_empty() {
        if config.secrets.is_some() {
            return Err(channel_error(
                WindowsErrorKind::Secret,
                "secret preparation is surplus to the checked plan",
            ));
        }
        return Ok((Vec::new(), Vec::new(), None));
    }
    if !authorized {
        return Err(crate::error::unsupported(
            WindowsOperation::Prepare,
            "secret delivery cannot begin outside authorized preparation",
        ));
    }
    let preparation = config.secrets.take().ok_or_else(|| {
        channel_error(WindowsErrorKind::Secret, "checked secrets lack exact inert preparation")
    })?;
    let session = preparation
        .prepare(
            execution.identity().process_id(),
            execution.identity().environment_id(),
            sandbox.digest(),
            execution.digest(),
            requirements,
        )
        .map_err(|_| {
            channel_error(WindowsErrorKind::Secret, "exact secret leases cannot be prepared")
        })?;
    let mut descriptors = Vec::with_capacity(requirements.len());
    let mut handles = Vec::with_capacity(requirements.len());
    for (index, (requirement, artifact)) in requirements.iter().zip(session.artifacts()).enumerate()
    {
        validate_artifact(requirement.delivery(), artifact)?;
        let bytes = artifact_bytes(artifact)?;
        let protected = NativeProtectedHandle::from_bytes(format!("windows-secret-{index}"), bytes)
            .map_err(|_| {
                channel_error(WindowsErrorKind::Handle, "secret handle cannot be staged")
            })?;
        descriptors.push(ProtectedSecretHandle::new(
            protected.raw_handle(),
            secret_reference_digest(requirement.reference()),
            SecretHandleDestination::from(requirement.delivery()),
        )?);
        handles.push(protected);
    }
    Ok((descriptors, handles, Some(session)))
}

fn artifact_bytes(artifact: &DeliveryArtifact) -> Result<Vec<u8>, WindowsError> {
    if let Some(bytes) = artifact.expose_environment(|_, bytes| bytes.to_vec()) {
        return Ok(bytes);
    }
    if let Some(bytes) = artifact.expose_brokered(|_, bytes| bytes.to_vec()) {
        return Ok(bytes);
    }
    let (path, _) = artifact.file_paths().ok_or_else(|| {
        channel_error(WindowsErrorKind::Secret, "secret artifact has no representable destination")
    })?;
    std::fs::read(path)
        .map_err(|_| channel_error(WindowsErrorKind::Secret, "staged secret file cannot be read"))
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

fn channel_error(kind: WindowsErrorKind, detail: &'static str) -> WindowsError {
    WindowsError::new(kind, WindowsOperation::Prepare, WindowsRecovery::CancelAndReap, detail)
}
