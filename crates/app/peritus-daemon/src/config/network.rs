//! Trusted service-host managed-network grants for exact product gates.

use std::net::IpAddr;

use peritus_product_runner::{ManagedGateNetworkDestination, ManagedGateNetworkGrant};
use peritus_types::{Sha256Digest, WorkspaceId};
use serde::Deserialize;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// One exact configured TCP destination.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ManagedGateNetworkDestinationDeclaration {
    /// Exact canonical DNS name and TCP port.
    Dns { name: String, port: u16 },
    /// Exact IP address and TCP port.
    Ip { address: IpAddr, port: u16 },
}

impl ManagedGateNetworkDestinationDeclaration {
    fn destination(&self) -> Result<ManagedGateNetworkDestination, DaemonError> {
        match self {
            Self::Dns { name, port } => ManagedGateNetworkDestination::dns(name.clone(), *port),
            Self::Ip { address, port } => ManagedGateNetworkDestination::ip(*address, *port),
        }
        .map_err(network_error)
    }
}

/// One trusted exact-command managed-network grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManagedGateNetworkGrantDeclaration {
    workspace_id: String,
    base_command_digest: String,
    destinations: Vec<ManagedGateNetworkDestinationDeclaration>,
    #[serde(default)]
    windows_controller_digest: Option<String>,
}

impl ManagedGateNetworkGrantDeclaration {
    /// Constructs the complete nonsensitive semantic grant used by the product runner.
    ///
    /// # Errors
    /// Rejects invalid identities, destinations, or controller policy.
    pub fn grant(&self) -> Result<ManagedGateNetworkGrant, DaemonError> {
        let workspace = WorkspaceId::new(super::decode_identifier(
            &self.workspace_id,
            "managed gate network workspace identity",
        )?)
        .map_err(|_| invalid_network())?;
        let base_command = decode_digest(
            &self.base_command_digest,
            "managed gate base command digest is invalid",
        )?;
        let controller = self
            .windows_controller_digest
            .as_deref()
            .map(|value| decode_digest(value, "managed gate Windows controller digest is invalid"))
            .transpose()?;
        let destinations = self
            .destinations
            .iter()
            .map(ManagedGateNetworkDestinationDeclaration::destination)
            .collect::<Result<Vec<_>, _>>()?;
        ManagedGateNetworkGrant::configured(workspace, base_command, destinations, controller)
            .map_err(network_error)
    }
}

pub(super) fn validate(
    declarations: &[ManagedGateNetworkGrantDeclaration],
) -> Result<(), DaemonError> {
    for declaration in declarations {
        let _ = declaration.grant()?;
    }
    Ok(())
}

fn decode_digest(value: &str, detail: &'static str) -> Result<Sha256Digest, DaemonError> {
    if value.len() != Sha256Digest::LENGTH * 2
        || !value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(DaemonError::new(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            "validate managed gate network",
            detail,
        ));
    }
    let mut bytes = [0_u8; Sha256Digest::LENGTH];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or_else(invalid_network)?;
        let low = hex_nibble(pair[1]).ok_or_else(invalid_network)?;
        bytes[index] = (high << 4) | low;
    }
    Ok(Sha256Digest::new(bytes))
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn invalid_network() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate managed gate network",
        "managed gate network grant is invalid",
    )
}

fn network_error(error: peritus_product_runner::ProductRunnerError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate managed gate network",
        error.to_string(),
        error,
    )
}
