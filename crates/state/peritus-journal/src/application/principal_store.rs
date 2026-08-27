//! Transactional application-principal persistence.

use peritus_types::Sha256Digest;
use rusqlite::{TransactionBehavior, params};

use super::{
    store::{conflict, corrupt, load_principal_by_actor, load_principal_by_digest, not_found},
    types::{ApplicationPrincipal, NewApplicationPrincipal},
};
use crate::{JournalError, SqliteJournal};

impl SqliteJournal {
    /// Installs an immutable one-to-one OS-principal to actor binding.
    ///
    /// Repeating the exact binding is idempotent. Rebinding either identity is rejected.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict or storage error.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "binding consumes the one-shot principal input as part of its public contract"
    )]
    pub fn bind_application_principal(
        &mut self,
        principal: NewApplicationPrincipal,
    ) -> Result<ApplicationPrincipal, JournalError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin principal binding", error))?;
        if let Some(existing) = load_principal_by_digest(&transaction, principal.principal_digest)?
        {
            if existing.kind() != principal.kind
                || existing.actor_id() != principal.actor_id
                || existing.binding_digest() != principal.binding_digest
            {
                return Err(conflict("principal digest is already bound to different facts"));
            }
            transaction
                .commit()
                .map_err(|error| JournalError::sqlite("complete principal binding", error))?;
            return Ok(existing);
        }
        if load_principal_by_actor(&transaction, principal.actor_id)?.is_some() {
            return Err(conflict("application actor is already bound to another OS principal"));
        }
        transaction.execute(
            "INSERT INTO app_principals(principal_digest, principal_kind, actor_id, binding_digest, state) VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                principal.principal_digest.as_bytes().as_slice(),
                principal.kind.tag(),
                principal.actor_id.as_bytes().as_slice(),
                principal.binding_digest.as_bytes().as_slice(),
            ],
        ).map_err(|error| JournalError::sqlite("insert principal binding", error))?;
        let installed = load_principal_by_digest(&transaction, principal.principal_digest)?
            .ok_or_else(|| corrupt("inserted principal binding is not observable"))?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit principal binding", error))?;
        Ok(installed)
    }

    /// Reads a principal binding by canonical digest.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_principal(
        &self,
        digest: Sha256Digest,
    ) -> Result<Option<ApplicationPrincipal>, JournalError> {
        load_principal_by_digest(&self.connection, digest)
    }

    /// Revokes an existing principal binding without deleting its identity history.
    ///
    /// # Errors
    ///
    /// Returns not found or a typed storage error.
    pub fn revoke_application_principal(
        &mut self,
        digest: Sha256Digest,
    ) -> Result<ApplicationPrincipal, JournalError> {
        let affected = self
            .connection
            .execute(
                "UPDATE app_principals SET state = 2 WHERE principal_digest = ?1",
                params![digest.as_bytes().as_slice()],
            )
            .map_err(|error| JournalError::sqlite("revoke principal binding", error))?;
        if affected == 0 {
            return Err(not_found("application principal does not exist"));
        }
        load_principal_by_digest(&self.connection, digest)?
            .ok_or_else(|| corrupt("revoked principal binding disappeared"))
    }
}
