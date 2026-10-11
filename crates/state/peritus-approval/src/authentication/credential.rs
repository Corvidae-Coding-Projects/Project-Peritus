//! Canonical human credential and supplied registry snapshot values.

use core::cmp::Ordering;
use peritus_policy::{ActorRole, AuthorityTier, ValidityWindow};
use peritus_types::{
    ActorId, EnvironmentId, Generation, RevisionNumber, Sha256Digest, WorkspaceId,
};

mod projection;
mod specification;

#[doc = " Maximum credentials in one checked supplied registry snapshot."]
pub const MAX_CREDENTIAL_REGISTRY_ENTRIES: usize = 4_096;
#[doc = " Maximum canonical approval-role labels on one human credential."]
pub const MAX_CREDENTIAL_APPROVAL_ROLES: usize = 11;
const fn compare_key_bytes_from(left: &[u8; 32], right: &[u8; 32], index: usize) -> Ordering {
    if index == 32 {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_key_bytes_from(left, right, index + 1)
    }
}
const fn compare_key_ids(left: crate::ApprovalKeyId, right: crate::ApprovalKeyId) -> Ordering {
    compare_key_bytes_from(left.sha256().as_bytes(), right.sha256().as_bytes(), 0)
}
#[doc = " Checked registry status of one credential."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CredentialStatus {
    #[doc = " Credential may authenticate decisions within all other bounds."]
    Enabled,
    #[doc = " Credential is retained only to reject stale or revoked use."]
    Disabled,
}
#[doc = " One checked human credential in a supplied immutable registry snapshot."]
#[derive(Debug, Eq, PartialEq)]
pub struct ApproverCredential {
    key_id: crate::ApprovalKeyId,
    public_key: crate::ApprovalPublicKey,
    actor: ActorId,
    principal_role: ActorRole,
    environment: EnvironmentId,
    workspace: WorkspaceId,
    maximum_tier: AuthorityTier,
    allowed_approval_roles: Vec<ActorRole>,
    validity: ValidityWindow,
    generation: Generation,
    status: CredentialStatus,
}
#[doc = " Checked immutable credential-registry snapshot."]
#[doc = ""]
#[doc = " This value is deliberately not evidence that its revision is durably current."]
#[derive(Debug, Eq, PartialEq)]
pub struct CredentialRegistrySnapshot {
    revision: RevisionNumber,
    entries: Vec<ApproverCredential>,
}
impl ApproverCredential {
    #[allow(
        clippy::too_many_arguments,
        reason = "all credential authority dimensions are explicit"
    )]
    fn new_with_validated_key(
        key_id: crate::ApprovalKeyId,
        public_key: crate::ApprovalPublicKey,
        actor: ActorId,
        principal_role: ActorRole,
        environment: EnvironmentId,
        workspace: WorkspaceId,
        maximum_tier: AuthorityTier,
        allowed_approval_roles: Vec<ActorRole>,
        validity: ValidityWindow,
        generation: Generation,
        status: CredentialStatus,
    ) -> Result<Self, crate::ApprovalError> {
        if principal_role != ActorRole::HumanAuthority {
            return Err(crate::ApprovalError::CredentialMismatch(
                crate::CredentialDimension::PrincipalRole,
            ));
        }
        if allowed_approval_roles.is_empty() {
            return Err(crate::ApprovalError::EmptyCollection(
                crate::CanonicalCollection::CredentialApprovalRoles,
            ));
        }
        if allowed_approval_roles.len() > MAX_CREDENTIAL_APPROVAL_ROLES {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::CredentialApprovalRoles,
            ));
        }
        let mut index = 0;
        while index < allowed_approval_roles.len() {
            if index > 0 {
                let previous = crate::digest::role_tag(allowed_approval_roles[index - 1]);
                let current = crate::digest::role_tag(allowed_approval_roles[index]);
                if previous == current {
                    return Err(crate::ApprovalError::DuplicateCanonicalValue(
                        crate::CanonicalCollection::CredentialApprovalRoles,
                    ));
                }
                if previous > current {
                    return Err(crate::ApprovalError::NonCanonicalOrder(
                        crate::CanonicalCollection::CredentialApprovalRoles,
                    ));
                }
            }
            index += 1;
        }
        Ok(Self {
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
        })
    }
}
impl CredentialRegistrySnapshot {
    #[doc = " Validates a bounded strict key-ID ordering. Empty snapshots are valid."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects over-limit, duplicate, or noncanonically ordered key IDs."]
    pub fn new(
        revision: RevisionNumber,
        entries: Vec<ApproverCredential>,
    ) -> Result<Self, crate::ApprovalError> {
        if entries.len() > MAX_CREDENTIAL_REGISTRY_ENTRIES {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::CredentialRegistry,
            ));
        }
        let mut index = 0;
        while index < entries.len() {
            if index > 0 {
                match compare_key_ids(entries[index - 1].key_id(), entries[index].key_id()) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        return Err(crate::ApprovalError::DuplicateCanonicalValue(
                            crate::CanonicalCollection::CredentialRegistry,
                        ));
                    }
                    Ordering::Greater => {
                        return Err(crate::ApprovalError::NonCanonicalOrder(
                            crate::CanonicalCollection::CredentialRegistry,
                        ));
                    }
                }
            }
            index += 1;
        }
        Ok(Self { revision, entries })
    }
}

impl ApproverCredential {
    /// Validates one exact human credential.
    ///
    /// # Errors
    ///
    /// Rejects non-human principals, key-ID mismatches, and noncanonical role constraints.
    #[allow(
        clippy::too_many_arguments,
        reason = "all credential authority dimensions are explicit"
    )]
    pub fn new(
        key_id: crate::ApprovalKeyId,
        public_key: crate::ApprovalPublicKey,
        actor: ActorId,
        principal_role: ActorRole,
        environment: EnvironmentId,
        workspace: WorkspaceId,
        maximum_tier: AuthorityTier,
        allowed_approval_roles: Vec<ActorRole>,
        validity: ValidityWindow,
        generation: Generation,
        status: CredentialStatus,
    ) -> Result<Self, crate::ApprovalError> {
        if crate::ApprovalKeyId::compute(public_key)? != key_id {
            return Err(crate::ApprovalError::CredentialMismatch(
                crate::CredentialDimension::KeyId,
            ));
        }
        Self::new_with_validated_key(
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
    }
}

impl CredentialRegistrySnapshot {
    /// Returns the canonical, domain-separated bytes binding every snapshot field.
    ///
    /// These are the payload bytes persisted by C0 for exact registry-currentness checks. They do
    /// not claim that this supplied snapshot is durably current.
    ///
    /// # Errors
    ///
    /// Returns `PreimageTooLarge` rather than producing a truncated representation.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, crate::ApprovalError> {
        crate::digest::credential_registry_bytes(self)
    }

    /// Computes the digest of the complete canonical snapshot representation.
    ///
    /// # Errors
    ///
    /// Returns `PreimageTooLarge` rather than hashing a truncated representation.
    pub fn digest(&self) -> Result<Sha256Digest, crate::ApprovalError> {
        crate::digest::credential_registry_digest(self)
    }
}
