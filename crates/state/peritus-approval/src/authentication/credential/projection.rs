//! Closed credential and registry projections for public reducer contracts.

use peritus_policy::{ActorRole, AuthorityTier, ValidityWindow};
use peritus_types::{ActorId, EnvironmentId, Generation, RevisionNumber, WorkspaceId};
use vstd::prelude::*;

verus! {

impl super::ApproverCredential {
    /// Returns the exact derived key identity used by specifications.
    pub closed spec fn spec_key_id(&self) -> crate::ApprovalKeyId { self.key_id }
    /// Returns the exact credential actor used by specifications.
    pub closed spec fn spec_actor(&self) -> ActorId { self.actor }
    /// Returns the exact human principal role used by specifications.
    pub closed spec fn spec_principal_role(&self) -> ActorRole { self.principal_role }
    /// Returns the exact environment used by specifications.
    pub closed spec fn spec_environment(&self) -> EnvironmentId { self.environment }
    /// Returns the exact workspace used by specifications.
    pub closed spec fn spec_workspace(&self) -> WorkspaceId { self.workspace }
    /// Returns the maximum credential tier used by specifications.
    pub closed spec fn spec_maximum_tier(&self) -> AuthorityTier { self.maximum_tier }
    /// Returns the canonical allowed approval-role sequence used by specifications.
    pub closed spec fn spec_allowed_approval_roles(&self) -> Seq<ActorRole> {
        self.allowed_approval_roles@
    }
    /// Returns the exact credential validity used by specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.validity }
    /// Returns the exact credential generation used by specifications.
    pub closed spec fn spec_generation(&self) -> Generation { self.generation }
    /// Returns the exact credential status used by specifications.
    pub closed spec fn spec_status(&self) -> super::CredentialStatus { self.status }
}

impl super::ApproverCredential {
    /// Returns the derived key ID.
    #[must_use]
    pub const fn key_id(&self) -> (key_id: crate::ApprovalKeyId)
        ensures key_id == self.spec_key_id(),
    { self.key_id }

    /// Returns exact public key bytes.
    #[must_use]
    pub const fn public_key(&self) -> crate::ApprovalPublicKey { self.public_key }

    /// Returns the human actor bound to this credential.
    #[must_use]
    pub const fn actor(&self) -> (actor: ActorId)
        ensures actor == self.spec_actor(),
    { self.actor }

    /// Returns the required human principal role.
    #[must_use]
    pub const fn principal_role(&self) -> (role: ActorRole)
        ensures role == self.spec_principal_role(),
    { self.principal_role }

    /// Returns the exact environment scope.
    #[must_use]
    pub const fn environment(&self) -> (environment: EnvironmentId)
        ensures environment == self.spec_environment(),
    { self.environment }

    /// Returns the exact workspace scope.
    #[must_use]
    pub const fn workspace(&self) -> (workspace: WorkspaceId)
        ensures workspace == self.spec_workspace(),
    { self.workspace }

    /// Returns the maximum authority tier this credential may satisfy.
    #[must_use]
    pub const fn maximum_tier(&self) -> (tier: AuthorityTier)
        ensures tier == self.spec_maximum_tier(),
    { self.maximum_tier }

    /// Borrows canonical approval-role labels.
    #[must_use]
    pub const fn allowed_approval_roles(&self) -> (roles: &[ActorRole])
        ensures roles@ == self.spec_allowed_approval_roles(),
    { self.allowed_approval_roles.as_slice() }

    /// Returns the credential validity interval.
    #[must_use]
    pub const fn validity(&self) -> (validity: ValidityWindow)
        ensures validity == self.spec_validity(),
    { self.validity }

    /// Returns the exact revocation/reissue generation.
    #[must_use]
    pub const fn generation(&self) -> (generation: Generation)
        ensures generation == self.spec_generation(),
    { self.generation }

    /// Returns the supplied registry status.
    #[must_use]
    pub const fn status(&self) -> (status: super::CredentialStatus)
        ensures status == self.spec_status(),
    { self.status }
}

impl super::CredentialRegistrySnapshot {
    /// Returns the exact supplied registry revision used by specifications.
    pub closed spec fn spec_revision(&self) -> RevisionNumber { self.revision }
    /// Returns the exact canonical credential sequence used by specifications.
    pub closed spec fn spec_entries(&self) -> Seq<super::ApproverCredential> { self.entries@ }
    /// Looks up one exact key identity in the specification registry sequence.
    pub closed spec fn spec_credential(
        &self,
        key_id: crate::ApprovalKeyId,
    ) -> Option<super::ApproverCredential> {
        super::specification::credential_from(self.spec_entries(), key_id, 0)
    }
}

impl super::CredentialRegistrySnapshot {
    /// Returns the exact supplied snapshot revision.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionNumber)
        ensures revision == self.spec_revision(),
    { self.revision }

    /// Borrows canonical snapshot entries.
    #[must_use]
    pub const fn entries(&self) -> &[super::ApproverCredential] { self.entries.as_slice() }

    /// Looks up one exact key ID.
    #[must_use]
    pub fn credential(
        &self,
        key_id: crate::ApprovalKeyId,
    ) -> (result: Option<&super::ApproverCredential>)
        ensures
            match result {
                Some(credential) => self.spec_credential(key_id) == Some(*credential),
                None => self.spec_credential(key_id).is_none(),
            },
    {
        proof {
            reveal(super::CredentialRegistrySnapshot::spec_credential);
            reveal(super::CredentialRegistrySnapshot::spec_entries);
        }
        let mut index = 0;
        while index < self.entries.len()
            invariant
                0 <= index <= self.entries.len(),
                super::specification::credential_from(self.entries@, key_id, 0)
                    == super::specification::credential_from(
                        self.entries@,
                        key_id,
                        index as nat,
                    ),
            decreases self.entries.len() - index,
        {
            let candidate = self.entries[index].key_id().sha256();
            let target = key_id.sha256();
            if crate::state::exact::digest_bytes_equal(*candidate.as_bytes(), *target.as_bytes()) {
                proof {
                    reveal(super::ApproverCredential::spec_key_id);
                    reveal_with_fuel(super::specification::credential_from, 1);
                    assert(candidate.spec_bytes()
                        == self.entries@[index as int].spec_key_id().spec_bytes());
                    assert(target.spec_bytes() == key_id.spec_bytes());
                }
                assert(super::specification::credential_from(
                    self.entries@,
                    key_id,
                    index as nat,
                ) == Some(self.entries@[index as int]));
                return Some(&self.entries[index]);
            }
            proof {
                reveal(super::ApproverCredential::spec_key_id);
                reveal_with_fuel(super::specification::credential_from, 1);
                assert(candidate.spec_bytes()
                    == self.entries@[index as int].spec_key_id().spec_bytes());
                assert(target.spec_bytes() == key_id.spec_bytes());
                assert(super::specification::credential_from(
                    self.entries@,
                    key_id,
                    index as nat,
                ) == super::specification::credential_from(
                    self.entries@,
                    key_id,
                    index as nat + 1,
                ));
            }
            index += 1;
        }
        proof { reveal_with_fuel(super::specification::credential_from, 1); }
        assert(super::specification::credential_from(
            self.entries@,
            key_id,
            self.entries.len() as nat,
        ).is_none());
        None
    }
}

} // verus!
