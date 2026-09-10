//! Revision-fenced bounded inspection queries, rows, and pages.

use super::{
    AppProtocolError, MAX_WORKBENCH_GUIDANCE_PAGE, MAX_WORKBENCH_GUIDANCE_RECORDS,
    WorkbenchGuidanceIdentity, WorkbenchGuidanceLifecycle, WorkbenchGuidanceRecord,
    WorkbenchGuidanceScope, WorkbenchGuidanceTombstone, WorkbenchQuery, capacity, invalid,
    scope_matches,
};

/// Revision-fenced bounded guidance inspection query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchMemoryQuery {
    query: WorkbenchQuery,
    dependency_revision: u64,
    offset: u32,
    include_forgotten: bool,
}

impl WorkbenchMemoryQuery {
    /// Validates first-page/current and later-page/exact-revision semantics.
    ///
    /// # Errors
    /// Rejects an unfenced later page or an offset beyond the project bound.
    pub const fn new(
        query: WorkbenchQuery,
        dependency_revision: u64,
        offset: u32,
        include_forgotten: bool,
    ) -> Result<Self, AppProtocolError> {
        if (offset != 0 && dependency_revision == 0)
            || offset as usize > MAX_WORKBENCH_GUIDANCE_RECORDS
        {
            return Err(invalid());
        }
        Ok(Self { query, dependency_revision, offset, include_forgotten })
    }

    /// Returns exact selected conversation and workspace.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }

    /// Returns zero for a current first page or the exact paging fence.
    #[must_use]
    pub const fn dependency_revision(self) -> u64 {
        self.dependency_revision
    }

    /// Returns the first selected project row.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// Reports whether content-free tombstones were explicitly requested.
    #[must_use]
    pub const fn include_forgotten(self) -> bool {
        self.include_forgotten
    }
}

/// One active or forgotten project-guidance inspection row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkbenchMemoryRow {
    /// Active exact reusable content.
    Active(WorkbenchGuidanceRecord),
    /// Content-free durable forget marker.
    Forgotten(WorkbenchGuidanceTombstone),
}

impl WorkbenchMemoryRow {
    /// Returns stable guidance identity.
    #[must_use]
    pub const fn identity(&self) -> WorkbenchGuidanceIdentity {
        match self {
            Self::Active(record) => record.identity(),
            Self::Forgotten(tombstone) => tombstone.identity(),
        }
    }

    /// Returns the row lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> WorkbenchGuidanceLifecycle {
        match self {
            Self::Active(record) => record.lifecycle(),
            Self::Forgotten(tombstone) => tombstone.lifecycle(),
        }
    }

    const fn scope(&self) -> WorkbenchGuidanceScope {
        match self {
            Self::Active(record) => record.content().scope(),
            Self::Forgotten(tombstone) => tombstone.prior().scope(),
        }
    }

    const fn dependency_revision(&self) -> u64 {
        match self {
            Self::Active(record) => record.version().dependency(),
            Self::Forgotten(tombstone) => tombstone.dependency_revision(),
        }
    }
}

/// Complete bounded project-guidance inspection page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchMemory {
    query: WorkbenchMemoryQuery,
    dependency_revision: u64,
    total: u32,
    rows: Vec<WorkbenchMemoryRow>,
}

impl WorkbenchMemory {
    /// Validates scope, paging fence, ordering, lifecycle visibility, and project bounds.
    ///
    /// # Errors
    /// Rejects inconsistent, excessive, duplicate, out-of-order, or stale rows.
    pub fn new(
        query: WorkbenchMemoryQuery,
        dependency_revision: u64,
        total: u32,
        rows: Vec<WorkbenchMemoryRow>,
    ) -> Result<Self, AppProtocolError> {
        let end = usize::try_from(query.offset())
            .ok()
            .and_then(|offset| offset.checked_add(rows.len()))
            .ok_or_else(capacity)?;
        if rows.len() > MAX_WORKBENCH_GUIDANCE_PAGE
            || total as usize > MAX_WORKBENCH_GUIDANCE_RECORDS
            || end > total as usize
            || (query.dependency_revision() != 0
                && query.dependency_revision() != dependency_revision)
            || rows.windows(2).any(|pair| pair[0].identity() >= pair[1].identity())
            || rows.iter().any(|row| {
                row.identity().workspace() != query.query().workspace()
                    || row.dependency_revision() > dependency_revision
                    || (!query.include_forgotten()
                        && row.lifecycle() == WorkbenchGuidanceLifecycle::Forgotten)
                    || !scope_matches(row.scope(), query.query().conversation())
            })
            || (dependency_revision == 0 && (total != 0 || !rows.is_empty()))
        {
            return Err(invalid());
        }
        Ok(Self { query, dependency_revision, total, rows })
    }

    /// Returns the exact inspection query and paging fence.
    #[must_use]
    pub const fn query(&self) -> WorkbenchMemoryQuery {
        self.query
    }

    /// Returns the current workspace dependency revision.
    #[must_use]
    pub const fn dependency_revision(&self) -> u64 {
        self.dependency_revision
    }

    /// Returns all matching active and requested tombstone rows before paging.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }

    /// Borrows the canonical stable-identity page.
    #[must_use]
    pub fn rows(&self) -> &[WorkbenchMemoryRow] {
        &self.rows
    }
}
