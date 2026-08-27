//! Canonical credential-registry snapshot decoding.

use peritus_types::{ActorId, EnvironmentId, WorkspaceId};

use super::reader::{CanonicalReader, ListReader, invalid};
use super::value::{
    decode_authority_tier, decode_generation, decode_revision_number, decode_role, decode_roles,
    decode_sha256, decode_validity, exact,
};
use crate::{
    ApprovalError, ApprovalKeyId, ApprovalPublicKey, ApproverCredential,
    CredentialRegistrySnapshot, CredentialStatus, MAX_CREDENTIAL_APPROVAL_ROLES,
    MAX_CREDENTIAL_REGISTRY_ENTRIES, MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES,
};

const REGISTRY_DOMAIN: &[u8] = b"credential-registry-snapshot";
const CREDENTIAL_DOMAIN: &[u8] = b"credential-registry-entry";

fn decode_status(bytes: &[u8]) -> Result<CredentialStatus, ApprovalError> {
    match bytes {
        [1] => Ok(CredentialStatus::Enabled),
        [2] => Ok(CredentialStatus::Disabled),
        _ => Err(invalid()),
    }
}

fn decode_credential(bytes: &[u8]) -> Result<ApproverCredential, ApprovalError> {
    let mut reader =
        CanonicalReader::record(bytes, CREDENTIAL_DOMAIN, MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES)?;
    let key_id = ApprovalKeyId::from_sha256(decode_sha256(reader.field(1)?)?);
    let public_key = ApprovalPublicKey::from_slice(reader.field(2)?).map_err(|_| invalid())?;
    let actor = ActorId::new(exact(reader.field(3)?)?).map_err(|_| invalid())?;
    let principal_role = decode_role(reader.field(4)?)?;
    let environment = EnvironmentId::new(exact(reader.field(5)?)?).map_err(|_| invalid())?;
    let workspace = WorkspaceId::new(exact(reader.field(6)?)?).map_err(|_| invalid())?;
    let maximum_tier = decode_authority_tier(reader.field(7)?)?;
    let allowed_approval_roles = decode_roles(reader.field(8)?, MAX_CREDENTIAL_APPROVAL_ROLES)?;
    let validity = decode_validity(reader.field(9)?)?;
    let generation = decode_generation(reader.field(10)?)?;
    let status = decode_status(reader.field(11)?)?;
    reader.finish()?;

    ApproverCredential::new(
        key_id,
        public_key,
        actor,
        principal_role,
        environment,
        workspace,
        maximum_tier,
        allowed_approval_roles,
        validity,
        generation,
        status,
    )
    .map_err(|_| invalid())
}

/// Decodes one exact canonical credential-registry snapshot.
///
/// Every credential key ID is recomputed by its checked constructor. Decoding does not claim that
/// the supplied snapshot revision is the current durable registry revision.
///
/// # Errors
///
/// Returns `InvalidCanonicalEncoding` for malformed, noncanonical, trailing, over-limit, or
/// internally inconsistent input.
pub fn decode_credential_registry(
    bytes: &[u8],
) -> Result<CredentialRegistrySnapshot, ApprovalError> {
    let mut reader =
        CanonicalReader::record(bytes, REGISTRY_DOMAIN, MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES)?;
    let revision = decode_revision_number(reader.field(1)?)?;
    let entries_bytes = reader.field(2)?;
    reader.finish()?;

    let mut list = ListReader::new(entries_bytes, MAX_CREDENTIAL_REGISTRY_ENTRIES)?;
    let mut entries = Vec::with_capacity(list.len());
    while list.len() != 0 {
        entries.push(decode_credential(list.item()?)?);
    }
    list.finish()?;
    let registry = CredentialRegistrySnapshot::new(revision, entries).map_err(|_| invalid())?;
    let reencoded = registry.canonical_bytes().map_err(|_| invalid())?;
    if reencoded != bytes {
        return Err(invalid());
    }
    Ok(registry)
}
