//! Closed credential and registry projections for public reducer contracts.

use peritus_policy::{ActorRole, AuthorityTier, ValidityWindow};
use peritus_types::{ActorId, EnvironmentId, Generation, RevisionNumber, WorkspaceId};

impl super::ApproverCredential {}
impl super::ApproverCredential {
    #[doc = " Returns the derived key ID."]
    #[must_use]
    pub const fn key_id(&self) -> crate::ApprovalKeyId {
        self.key_id
    }
    #[doc = " Returns exact public key bytes."]
    #[must_use]
    pub const fn public_key(&self) -> crate::ApprovalPublicKey {
        self.public_key
    }
    #[doc = " Returns the human actor bound to this credential."]
    #[must_use]
    pub const fn actor(&self) -> ActorId {
        self.actor
    }
    #[doc = " Returns the required human principal role."]
    #[must_use]
    pub const fn principal_role(&self) -> ActorRole {
        self.principal_role
    }
    #[doc = " Returns the exact environment scope."]
    #[must_use]
    pub const fn environment(&self) -> EnvironmentId {
        self.environment
    }
    #[doc = " Returns the exact workspace scope."]
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId {
        self.workspace
    }
    #[doc = " Returns the maximum authority tier this credential may satisfy."]
    #[must_use]
    pub const fn maximum_tier(&self) -> AuthorityTier {
        self.maximum_tier
    }
    #[doc = " Borrows canonical approval-role labels."]
    #[must_use]
    pub const fn allowed_approval_roles(&self) -> &[ActorRole] {
        self.allowed_approval_roles.as_slice()
    }
    #[doc = " Returns the credential validity interval."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    #[doc = " Returns the exact revocation/reissue generation."]
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    #[doc = " Returns the supplied registry status."]
    #[must_use]
    pub const fn status(&self) -> super::CredentialStatus {
        self.status
    }
}
impl super::CredentialRegistrySnapshot {}
impl super::CredentialRegistrySnapshot {
    #[doc = " Returns the exact supplied snapshot revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    #[doc = " Borrows canonical snapshot entries."]
    #[must_use]
    pub const fn entries(&self) -> &[super::ApproverCredential] {
        self.entries.as_slice()
    }
    #[doc = " Looks up one exact key ID."]
    #[must_use]
    pub fn credential(&self, key_id: crate::ApprovalKeyId) -> Option<&super::ApproverCredential> {
        let mut index = 0;
        while index < self.entries.len() {
            let candidate = self.entries[index].key_id().sha256();
            let target = key_id.sha256();
            if crate::state::exact::digest_bytes_equal(*candidate.as_bytes(), *target.as_bytes()) {
                return Some(&self.entries[index]);
            }
            index += 1;
        }
        None
    }
}
