//! Transactional application-artifact catalog persistence.

use peritus_types::ArtifactId;
use rusqlite::params;

use super::{
    store::{conflict, corrupt, invalid, load_artifact, to_i64},
    types::{ApplicationArtifact, NewApplicationArtifact},
};
use crate::{JournalError, SqliteJournal};

impl SqliteJournal {
    /// Inserts exact pending application artifact metadata.
    ///
    /// Repeating exact metadata is idempotent.
    ///
    /// # Errors
    ///
    /// Returns conflict for identity/digest drift, or a typed storage error.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the journal consumes a new-record command even when SQLite binds its fields"
    )]
    pub fn begin_application_artifact(
        &mut self,
        artifact: NewApplicationArtifact,
    ) -> Result<ApplicationArtifact, JournalError> {
        if let Some(existing) = load_artifact(&self.connection, artifact.artifact_id)? {
            if existing.digest() == artifact.digest
                && existing.byte_size() == artifact.byte_size
                && existing.media_type() == artifact.media_type
            {
                return Ok(existing);
            }
            return Err(conflict(
                "application artifact identity is already bound to different metadata",
            ));
        }
        self.connection.execute(
            "INSERT INTO app_artifacts(artifact_id, digest, byte_size, media_type, state) VALUES (?1, ?2, ?3, ?4, 1)",
            params![artifact.artifact_id.as_bytes().as_slice(), artifact.digest.as_bytes().as_slice(), to_i64(artifact.byte_size, "application artifact size")?, artifact.media_type],
        ).map_err(|error| JournalError::sqlite("insert application artifact", error))?;
        load_artifact(&self.connection, artifact.artifact_id)?
            .ok_or_else(|| corrupt("inserted application artifact is not observable"))
    }

    /// Marks finalized artifact metadata available at its exact producing event position.
    ///
    /// # Errors
    ///
    /// Returns not found, conflict, or a typed storage error.
    pub fn complete_application_artifact(
        &mut self,
        artifact_id: ArtifactId,
        producing_position: u64,
    ) -> Result<ApplicationArtifact, JournalError> {
        if producing_position == 0 {
            return Err(invalid("artifact producing position must be positive"));
        }
        let affected = self.connection.execute(
            "UPDATE app_artifacts SET state = 2, producing_position = ?1 WHERE artifact_id = ?2 AND (state = 1 OR (state = 2 AND producing_position = ?1))",
            params![to_i64(producing_position, "artifact producing position")?, artifact_id.as_bytes().as_slice()],
        ).map_err(|error| JournalError::sqlite("complete application artifact", error))?;
        if affected == 0 {
            return Err(conflict(
                "application artifact cannot be completed from its current state",
            ));
        }
        load_artifact(&self.connection, artifact_id)?
            .ok_or_else(|| corrupt("completed application artifact disappeared"))
    }

    /// Reads application artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Option<ApplicationArtifact>, JournalError> {
        load_artifact(&self.connection, artifact_id)
    }
}
