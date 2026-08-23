//! Canonical complete credential-registry snapshot encoding and digest.

use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use super::{
    CanonicalEncoder, MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES, authority_tier_tag,
    canonical::be_u64, enum_byte, list_tag_bytes, list_vec_bytes, role_tag, validity_bytes,
};

const fn credential_status_tag(value: crate::CredentialStatus) -> u8 {
    match value {
        crate::CredentialStatus::Enabled => 1,
        crate::CredentialStatus::Disabled => 2,
    }
}

/// Encodes every field of one checked snapshot into its canonical domain-separated payload.
pub fn credential_registry_bytes(
    registry: &crate::CredentialRegistrySnapshot,
) -> Result<Vec<u8>, crate::ApprovalError> {
    let mut entries = Vec::with_capacity(registry.entries().len());
    for credential in registry.entries() {
        let mut entry = CanonicalEncoder::record(
            b"credential-registry-entry",
            MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES,
        )?;
        entry.field(1, credential.key_id().sha256().as_bytes())?;
        entry.field(2, credential.public_key().as_bytes())?;
        entry.field(3, credential.actor().as_bytes())?;
        entry.field(4, &enum_byte(role_tag(credential.principal_role())))?;
        entry.field(5, credential.environment().as_bytes())?;
        entry.field(6, credential.workspace().as_bytes())?;
        entry.field(7, &enum_byte(authority_tier_tag(credential.maximum_tier())))?;
        let roles: Vec<[u8; 1]> = credential
            .allowed_approval_roles()
            .iter()
            .map(|role| enum_byte(role_tag(*role)))
            .collect();
        entry.field(8, &list_tag_bytes(roles.as_slice())?)?;
        entry.field(9, &validity_bytes(credential.validity()))?;
        entry.field(10, &be_u64(credential.generation().get()))?;
        entry.field(11, &enum_byte(credential_status_tag(credential.status())))?;
        entries.push(entry.finish());
    }

    let mut registry_bytes = CanonicalEncoder::record(
        b"credential-registry-snapshot",
        MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES,
    )?;
    registry_bytes.field(1, &be_u64(registry.revision().get()))?;
    registry_bytes.field(2, &list_vec_bytes(entries.as_slice())?)?;
    Ok(registry_bytes.finish())
}

/// Hashes the canonical complete snapshot payload.
pub fn credential_registry_digest(
    registry: &crate::CredentialRegistrySnapshot,
) -> Result<Sha256Digest, crate::ApprovalError> {
    let mut hasher = Sha256::new();
    hasher.update(credential_registry_bytes(registry)?);
    Ok(Sha256Digest::new(hasher.finalize().into()))
}
