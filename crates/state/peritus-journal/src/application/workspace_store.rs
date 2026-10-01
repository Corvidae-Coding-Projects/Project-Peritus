//! Durable application workspace catalog and bounded recovery enumeration.

use peritus_types::WorkspaceId;
use rusqlite::{Connection, OptionalExtension, params};

use super::{
    rows::WorkspaceRow,
    types::{
        ApplicationWorkspace, ApplicationWorkspacePage, ApplicationWorkspaceState,
        MAX_APPLICATION_WORKSPACE_PAGE, NewApplicationWorkspace,
    },
};
use crate::{JournalError, JournalErrorKind, SqliteJournal};

const WORKSPACE_COLUMNS: &str = "workspace_id, registration_bytes, registration_digest, state";

impl SqliteJournal {
    /// Replaces a registration after its domain owner validates an authorized baseline refresh.
    ///
    /// # Errors
    /// Returns conflict if the exact previous registration changed or is missing.
    pub fn refresh_application_workspace(
        &mut self,
        expected: &ApplicationWorkspace,
        replacement: &NewApplicationWorkspace,
    ) -> Result<ApplicationWorkspace, JournalError> {
        if expected.workspace_id() != replacement.workspace_id {
            return Err(conflict("workspace refresh cannot change identity"));
        }
        let changed = self
            .connection
            .execute(
                "UPDATE app_workspaces SET registration_bytes = ?1, registration_digest = ?2
             WHERE workspace_id = ?3 AND registration_bytes = ?4 AND registration_digest = ?5",
                params![
                    replacement.registration_bytes,
                    replacement.registration_digest.as_bytes().as_slice(),
                    expected.workspace_id().as_bytes().as_slice(),
                    expected.registration_bytes(),
                    expected.registration_digest().as_bytes().as_slice()
                ],
            )
            .map_err(|error| JournalError::sqlite("refresh application workspace", error))?;
        if changed != 1 {
            return Err(conflict("workspace registration changed during refresh"));
        }
        load_workspace(&self.connection, replacement.workspace_id)?
            .ok_or_else(|| corrupt("refreshed workspace disappeared"))
    }

    /// Registers exact workspace configuration bytes.
    ///
    /// Repeating the exact registration is idempotent.
    ///
    /// # Errors
    ///
    /// Returns conflict for registration drift, or a typed storage error.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "catalog insertion consumes owned canonical registration bytes"
    )]
    pub fn register_application_workspace(
        &mut self,
        workspace: NewApplicationWorkspace,
    ) -> Result<ApplicationWorkspace, JournalError> {
        if let Some(existing) = load_workspace(&self.connection, workspace.workspace_id)? {
            if existing.registration_digest() == workspace.registration_digest
                && existing.registration_bytes() == workspace.registration_bytes
            {
                return Ok(existing);
            }
            return Err(conflict(
                "application workspace is already registered with different bytes",
            ));
        }
        self.connection.execute(
            "INSERT INTO app_workspaces(workspace_id, registration_bytes, registration_digest, state) VALUES (?1, ?2, ?3, 1)",
            params![
                workspace.workspace_id.as_bytes().as_slice(),
                workspace.registration_bytes,
                workspace.registration_digest.as_bytes().as_slice(),
            ],
        ).map_err(|error| JournalError::sqlite("register application workspace", error))?;
        load_workspace(&self.connection, workspace.workspace_id)?
            .ok_or_else(|| corrupt("registered application workspace is not observable"))
    }

    /// Reads exact workspace registration bytes.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or integrity error.
    pub fn application_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Option<ApplicationWorkspace>, JournalError> {
        load_workspace(&self.connection, workspace_id)
    }

    /// Reads one bounded deterministic page of durable workspace registrations for recovery.
    ///
    /// Pass `None` for the first page. When [`ApplicationWorkspacePage::next_after`] is present,
    /// pass that exact exclusive identity cursor to continue without offset races or duplicates.
    /// Removed registrations remain visible so startup can retain complete catalog truth.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless `max_records` is 1 through
    /// [`MAX_APPLICATION_WORKSPACE_PAGE`], or a typed storage/integrity error.
    pub fn application_workspace_page(
        &self,
        after: Option<WorkspaceId>,
        max_records: usize,
    ) -> Result<ApplicationWorkspacePage, JournalError> {
        if max_records == 0 || max_records > MAX_APPLICATION_WORKSPACE_PAGE {
            return Err(invalid("application workspace recovery bound is invalid"));
        }
        let query_limit = max_records
            .checked_add(1)
            .ok_or_else(|| invalid("application workspace recovery bound overflowed"))?;
        let sql = format!(
            "SELECT {WORKSPACE_COLUMNS} FROM app_workspaces \
             WHERE (?1 IS NULL OR workspace_id > ?1) ORDER BY workspace_id LIMIT ?2",
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|error| JournalError::sqlite("prepare application workspace page", error))?;
        let after_bytes = after.map(|workspace| workspace.as_bytes().to_vec());
        let rows = statement
            .query_map(
                params![
                    after_bytes,
                    i64::try_from(query_limit).map_err(|_| {
                        invalid("application workspace recovery bound cannot be represented")
                    })?,
                ],
                WorkspaceRow::read,
            )
            .map_err(|error| JournalError::sqlite("query application workspace page", error))?;
        let mut workspaces = rows
            .map(|row| {
                row.map_err(|error| JournalError::sqlite("read application workspace page", error))
                    .and_then(WorkspaceRow::parse)
            })
            .collect::<Result<Vec<_>, JournalError>>()?;
        let has_more = workspaces.len() > max_records;
        if has_more {
            workspaces.truncate(max_records);
        }
        let next_after =
            has_more.then(|| workspaces.last().map(ApplicationWorkspace::workspace_id)).flatten();
        Ok(ApplicationWorkspacePage { workspaces, next_after })
    }

    /// Changes workspace availability while retaining registration history.
    ///
    /// # Errors
    ///
    /// Returns not found or a typed storage error.
    pub fn set_application_workspace_state(
        &mut self,
        workspace_id: WorkspaceId,
        state: ApplicationWorkspaceState,
    ) -> Result<ApplicationWorkspace, JournalError> {
        let affected = self
            .connection
            .execute(
                "UPDATE app_workspaces SET state = ?1 WHERE workspace_id = ?2",
                params![state.tag(), workspace_id.as_bytes().as_slice()],
            )
            .map_err(|error| JournalError::sqlite("set application workspace state", error))?;
        if affected == 0 {
            return Err(not_found("application workspace does not exist"));
        }
        load_workspace(&self.connection, workspace_id)?
            .ok_or_else(|| corrupt("updated application workspace disappeared"))
    }
}

fn load_workspace(
    connection: &Connection,
    workspace: WorkspaceId,
) -> Result<Option<ApplicationWorkspace>, JournalError> {
    let sql = format!("SELECT {WORKSPACE_COLUMNS} FROM app_workspaces WHERE workspace_id = ?1");
    connection
        .query_row(&sql, params![workspace.as_bytes().as_slice()], WorkspaceRow::read)
        .optional()
        .map_err(|error| JournalError::sqlite("read application workspace", error))?
        .map(WorkspaceRow::parse)
        .transpose()
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(
        JournalErrorKind::InvalidInput,
        "operate application workspace catalog",
        detail,
    )
}

const fn conflict(detail: &'static str) -> JournalError {
    JournalError::new(
        JournalErrorKind::IdempotencyConflict,
        "operate application workspace catalog",
        detail,
    )
}

const fn not_found(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::NotFound, "operate application workspace catalog", detail)
}

const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(
        JournalErrorKind::CorruptJournal,
        "operate application workspace catalog",
        detail,
    )
}
