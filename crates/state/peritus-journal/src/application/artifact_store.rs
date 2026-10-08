//! Transactional application-artifact catalog persistence.

use peritus_types::{ArtifactId, EventId};
use rusqlite::{OptionalExtension, params};

use super::{
    rows::ArtifactRow,
    store::{ARTIFACT_COLUMNS, conflict, corrupt, invalid, load_artifact, to_i64},
    types::{ApplicationArtifact, NewApplicationArtifact},
};
use crate::{JournalError, SqliteJournal};

impl SqliteJournal {
    /// Reads one bounded identity-ordered page of application artifacts awaiting publication.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an out-of-range page size, or a typed storage/integrity error.
    pub fn uploading_application_artifacts_after(
        &self,
        cursor: Option<ArtifactId>,
        maximum: usize,
    ) -> Result<Vec<ApplicationArtifact>, JournalError> {
        if maximum == 0 || maximum > 4_096 {
            return Err(invalid("application artifact recovery page bound is invalid"));
        }
        let sql = format!(
            "SELECT {ARTIFACT_COLUMNS} FROM app_artifacts
              WHERE state = 1 AND (?1 IS NULL OR artifact_id > ?1)
              ORDER BY artifact_id LIMIT ?2"
        );
        let cursor = cursor.map(|artifact| artifact.as_bytes().to_vec());
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|error| JournalError::sqlite("prepare uploading application artifacts", error))?;
        let rows = statement
            .query_map(
                params![cursor, to_i64(maximum as u64, "artifact recovery page size")?],
                ArtifactRow::read,
            )
            .map_err(|error| JournalError::sqlite("query uploading application artifacts", error))?;
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(
                row.map_err(|error| {
                    JournalError::sqlite("read uploading application artifact", error)
                })?
                .parse()?,
            );
        }
        Ok(artifacts)
    }

    /// Completes an uploading application artifact from its exact accepted event identity.
    ///
    /// Returns `None` when that event was never committed. Exact already-completed state is
    /// idempotent through [`Self::complete_application_artifact`].
    ///
    /// # Errors
    ///
    /// Returns conflict, integrity, or typed storage errors.
    pub fn complete_application_artifact_from_event(
        &mut self,
        artifact_id: ArtifactId,
        event_id: EventId,
    ) -> Result<Option<ApplicationArtifact>, JournalError> {
        let position = self
            .connection
            .query_row(
                "SELECT global_position FROM events WHERE event_id = ?1",
                [event_id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| JournalError::sqlite("read application artifact event", error))?;
        let Some(position) = position else {
            return Ok(None);
        };
        let position = u64::try_from(position)
            .ok()
            .filter(|position| *position != 0)
            .ok_or_else(|| corrupt("application artifact event has an invalid position"))?;
        self.complete_application_artifact(artifact_id, position).map(Some)
    }

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
