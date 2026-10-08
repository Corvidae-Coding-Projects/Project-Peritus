//! Immutable product-run text and deliverable-index projections.

use peritus_run_settlement::{CandidateStage, RunSettlement};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

use super::{
    ProductProviderSelection, ProductRunLegalControls, ProductRunMessageError,
    ProductRunOperationKind, ProductRunOperationState, ProductRunPhase,
};

/// Maximum UTF-8 bytes carried by one physical product-artifact response.
pub const MAX_PRODUCT_ARTIFACT_CHUNK_BYTES: usize = 32 * 1024;
/// Maximum immutable deliverable-index entries carried by one physical page.
pub const MAX_PRODUCT_DELIVERABLE_INDEX_PAGE: usize = 256;

/// Exact immutable UTF-8 artifact identity and complete logical byte length.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductArtifactReference {
    digest: Sha256Digest,
    bytes: u64,
}

impl ProductArtifactReference {
    /// Measures nonempty UTF-8 text without imposing a cumulative content limit.
    ///
    /// # Errors
    /// Rejects empty required text or a length that cannot be represented on the wire.
    pub fn measure(text: &str) -> Result<Self, ProductRunMessageError> {
        if text.is_empty() {
            return Err(ProductRunMessageError::Empty);
        }
        let bytes = u64::try_from(text.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        Ok(Self { digest: peritus_codec::sha256(text.as_bytes()), bytes })
    }

    /// Restores a previously measured nonempty artifact reference.
    ///
    /// # Errors
    /// Rejects the reserved empty logical body.
    pub const fn new(
        digest: Sha256Digest,
        bytes: u64,
    ) -> Result<Self, ProductRunMessageError> {
        if bytes == 0 {
            Err(ProductRunMessageError::Empty)
        } else {
            Ok(Self { digest, bytes })
        }
    }

    /// Exact SHA-256 digest of the complete UTF-8 body.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }

    /// Complete logical UTF-8 byte length.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    /// Checks that `text` is the exact referenced body.
    #[must_use]
    pub fn matches(self, text: &str) -> bool {
        self.matches_bytes(text.as_bytes())
    }

    /// Checks that `bytes` are the exact referenced body before UTF-8 decoding.
    #[must_use]
    pub fn matches_bytes(self, bytes: &[u8]) -> bool {
        self.bytes == u64::try_from(bytes.len()).unwrap_or(u64::MAX)
            && self.digest == peritus_codec::sha256(bytes)
    }
}

/// One immutable deliverable collection, identified independently of page size.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductDeliverableIndexReference {
    root: Sha256Digest,
    count: u64,
}

impl ProductDeliverableIndexReference {
    /// Restores an exact collection root and its complete logical item count.
    #[must_use]
    pub const fn new(root: Sha256Digest, count: u64) -> Self {
        Self { root, count }
    }

    /// Digest of the complete ordered collection and every referenced item identity.
    #[must_use]
    pub const fn root(self) -> Sha256Digest {
        self.root
    }

    /// Complete logical item count, independent of physical pages.
    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// Deliverable collection selected for a paged read.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductDeliverableIndexKind {
    /// Authority-safe paths changed by the exact candidate.
    ChangedPaths,
    /// Successful acceptance commands retained for the exact candidate.
    SuccessfulCommands,
}

impl ProductDeliverableIndexKind {
    /// Stable canonical wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::ChangedPaths => 1,
            Self::SuccessfulCommands => 2,
        }
    }

    /// Restores one stable canonical wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::ChangedPaths),
            2 => Some(Self::SuccessfulCommands),
            _ => None,
        }
    }
}

/// Content-free public handoff metadata for an exact candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductDeliverableReference {
    workspace_path: ProductArtifactReference,
    changed_paths: ProductDeliverableIndexReference,
    successful_commands: ProductDeliverableIndexReference,
    run_instructions: ProductArtifactReference,
    qualification: CandidateStage,
    accepted: bool,
    commit_revision: Option<ProductArtifactReference>,
    export_path: Option<ProductArtifactReference>,
    discarded: bool,
}

/// Content-free authoritative operation metadata whose explanatory text is range-readable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunOperationReference {
    kind: ProductRunOperationKind,
    state: ProductRunOperationState,
    identity: ProductArtifactReference,
    known: ProductArtifactReference,
    uncertainty: Option<ProductArtifactReference>,
    legal_controls: ProductRunLegalControls,
}

impl ProductRunOperationReference {
    /// Binds exact immutable operation text to its authoritative state and controls.
    ///
    /// # Errors
    /// Rejects an outcome-unknown projection that omits its material uncertainty.
    pub fn new(
        kind: ProductRunOperationKind,
        state: ProductRunOperationState,
        identity: ProductArtifactReference,
        known: ProductArtifactReference,
        uncertainty: Option<ProductArtifactReference>,
        legal_controls: ProductRunLegalControls,
    ) -> Result<Self, ProductRunMessageError> {
        if state == ProductRunOperationState::OutcomeUnknown && uncertainty.is_none() {
            return Err(ProductRunMessageError::Empty);
        }
        Ok(Self { kind, state, identity, known, uncertainty, legal_controls })
    }

    #[must_use]
    pub const fn kind(&self) -> ProductRunOperationKind { self.kind }
    #[must_use]
    pub const fn state(&self) -> ProductRunOperationState { self.state }
    #[must_use]
    pub const fn identity(&self) -> ProductArtifactReference { self.identity }
    #[must_use]
    pub const fn known(&self) -> ProductArtifactReference { self.known }
    #[must_use]
    pub const fn uncertainty(&self) -> Option<ProductArtifactReference> { self.uncertainty }
    #[must_use]
    pub const fn legal_controls(&self) -> ProductRunLegalControls { self.legal_controls }
}

impl ProductDeliverableReference {
    /// Binds exact immutable handoff artifacts and candidate state.
    ///
    /// # Errors
    /// Rejects a deliverable without any changed path.
    #[allow(clippy::too_many_arguments, reason = "every exact handoff field is independently bound")]
    pub fn new(
        workspace_path: ProductArtifactReference,
        changed_paths: ProductDeliverableIndexReference,
        successful_commands: ProductDeliverableIndexReference,
        run_instructions: ProductArtifactReference,
        qualification: CandidateStage,
        accepted: bool,
        commit_revision: Option<ProductArtifactReference>,
        export_path: Option<ProductArtifactReference>,
        discarded: bool,
    ) -> Result<Self, ProductRunMessageError> {
        if changed_paths.count() == 0
            || qualification == CandidateStage::Qualified && successful_commands.count() == 0
        {
            return Err(ProductRunMessageError::TooManyDeliverableItems);
        }
        Ok(Self {
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            qualification,
            accepted,
            commit_revision,
            export_path,
            discarded,
        })
    }

    #[must_use]
    pub const fn workspace_path(&self) -> ProductArtifactReference { self.workspace_path }
    #[must_use]
    pub const fn changed_paths(&self) -> ProductDeliverableIndexReference { self.changed_paths }
    #[must_use]
    pub const fn successful_commands(&self) -> ProductDeliverableIndexReference {
        self.successful_commands
    }
    #[must_use]
    pub const fn run_instructions(&self) -> ProductArtifactReference { self.run_instructions }
    #[must_use]
    pub const fn qualification(&self) -> CandidateStage { self.qualification }
    #[must_use]
    pub const fn accepted(&self) -> bool { self.accepted }
    #[must_use]
    pub const fn commit_revision(&self) -> Option<ProductArtifactReference> {
        self.commit_revision
    }
    #[must_use]
    pub const fn export_path(&self) -> Option<ProductArtifactReference> { self.export_path }
    #[must_use]
    pub const fn discarded(&self) -> bool { self.discarded }
}

/// Content-free product snapshot whose logical text is independently range-readable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunReferenceSnapshot {
    run_id: RunId,
    workspace_id: WorkspaceId,
    providers: ProductProviderSelection,
    phase: ProductRunPhase,
    cycle: u32,
    task: ProductArtifactReference,
    status: ProductArtifactReference,
    diff: Option<ProductArtifactReference>,
    gates: Option<ProductArtifactReference>,
    review: Option<ProductArtifactReference>,
    summary: Option<ProductArtifactReference>,
    operation: ProductRunOperationReference,
    deliverable: Option<ProductDeliverableReference>,
    settlement: Option<RunSettlement>,
}

impl ProductRunReferenceSnapshot {
    /// Constructs one exact metadata projection.
    ///
    /// # Errors
    /// Rejects settlement evidence that disagrees with the run, workspace, phase, or candidate.
    #[allow(clippy::too_many_arguments, reason = "snapshot artifact fields are independently readable")]
    pub fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        providers: ProductProviderSelection,
        phase: ProductRunPhase,
        cycle: u32,
        task: ProductArtifactReference,
        status: ProductArtifactReference,
        diff: Option<ProductArtifactReference>,
        gates: Option<ProductArtifactReference>,
        review: Option<ProductArtifactReference>,
        summary: Option<ProductArtifactReference>,
        operation: ProductRunOperationReference,
        deliverable: Option<ProductDeliverableReference>,
        settlement: Option<RunSettlement>,
    ) -> Result<Self, ProductRunMessageError> {
        if let Some(settlement) = settlement {
            let checkpoint = settlement.checkpoint();
            if checkpoint.is_some() != deliverable.is_some()
                || checkpoint.is_some_and(|checkpoint| {
                    checkpoint.identity().run_id() != run_id
                        || checkpoint.identity().workspace_id() != workspace_id
                        || deliverable
                            .as_ref()
                            .is_none_or(|value| value.qualification() != checkpoint.stage())
                })
                || settlement.disposition() == peritus_run_settlement::RunDisposition::Accepted
                    && (phase != ProductRunPhase::Complete
                        || deliverable.as_ref().is_none_or(|value| {
                            value.qualification() != CandidateStage::Qualified
                        }))
            {
                return Err(ProductRunMessageError::InvalidSettlement);
            }
        } else if deliverable
            .as_ref()
            .is_some_and(|value| value.qualification() != CandidateStage::Qualified)
        {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        Ok(Self {
            run_id,
            workspace_id,
            providers,
            phase,
            cycle,
            task,
            status,
            diff,
            gates,
            review,
            summary,
            operation,
            deliverable,
            settlement,
        })
    }

    #[must_use]
    pub const fn run_id(&self) -> RunId { self.run_id }
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId { self.workspace_id }
    #[must_use]
    pub const fn providers(&self) -> ProductProviderSelection { self.providers }
    #[must_use]
    pub const fn phase(&self) -> ProductRunPhase { self.phase }
    #[must_use]
    pub const fn cycle(&self) -> u32 { self.cycle }
    #[must_use]
    pub const fn task(&self) -> ProductArtifactReference { self.task }
    #[must_use]
    pub const fn status(&self) -> ProductArtifactReference { self.status }
    #[must_use]
    pub const fn diff(&self) -> Option<ProductArtifactReference> { self.diff }
    #[must_use]
    pub const fn gates(&self) -> Option<ProductArtifactReference> { self.gates }
    #[must_use]
    pub const fn review(&self) -> Option<ProductArtifactReference> { self.review }
    #[must_use]
    pub const fn summary(&self) -> Option<ProductArtifactReference> { self.summary }
    #[must_use]
    pub const fn operation(&self) -> &ProductRunOperationReference { &self.operation }
    #[must_use]
    pub const fn deliverable(&self) -> Option<&ProductDeliverableReference> {
        self.deliverable.as_ref()
    }
    #[must_use]
    pub const fn settlement(&self) -> Option<RunSettlement> { self.settlement }
}

/// First-page, continuation, or exact-run request for referenced run metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunReferenceQuery {
    run_id: Option<RunId>,
    cursor: Option<super::ProductRunPageCursor>,
}

impl ProductRunReferenceQuery {
    #[must_use]
    pub const fn first() -> Self { Self { run_id: None, cursor: None } }
    #[must_use]
    pub const fn after(cursor: super::ProductRunPageCursor) -> Self {
        Self { run_id: None, cursor: Some(cursor) }
    }
    #[must_use]
    pub const fn exact(run_id: RunId) -> Self { Self { run_id: Some(run_id), cursor: None } }
    #[must_use]
    pub const fn run_id(self) -> Option<RunId> { self.run_id }
    #[must_use]
    pub const fn cursor(self) -> Option<super::ProductRunPageCursor> { self.cursor }
}

/// One durably ordered referenced run snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunReferencePageEntry {
    sequence: u64,
    snapshot: ProductRunReferenceSnapshot,
}

impl ProductRunReferencePageEntry {
    /// Binds a referenced snapshot to its immutable admission sequence.
    ///
    /// # Errors
    /// Rejects the reserved zero sequence.
    pub fn new(
        sequence: u64,
        snapshot: ProductRunReferenceSnapshot,
    ) -> Result<Self, ProductRunMessageError> {
        if sequence == 0 {
            Err(ProductRunMessageError::InvalidPage)
        } else {
            Ok(Self { sequence, snapshot })
        }
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 { self.sequence }
    #[must_use]
    pub const fn snapshot(&self) -> &ProductRunReferenceSnapshot { &self.snapshot }
}

/// One lossless stable run-catalog page carrying only bounded metadata and artifact references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunReferencePage {
    store: super::ProductRunStoreId,
    entries: Vec<ProductRunReferencePageEntry>,
    next: Option<super::ProductRunPageCursor>,
}

impl ProductRunReferencePage {
    /// Checks catalog order and continuation binding using the existing durable run cursor.
    ///
    /// # Errors
    /// Rejects oversized, unordered, duplicate, or cursor-inconsistent pages.
    pub fn new(
        store: super::ProductRunStoreId,
        entries: Vec<ProductRunReferencePageEntry>,
        next: Option<super::ProductRunPageCursor>,
    ) -> Result<Self, ProductRunMessageError> {
        let ordered = entries.windows(2).all(|pair| pair[0].sequence() > pair[1].sequence());
        let unique = entries.iter().enumerate().all(|(index, entry)| {
            entries[..index]
                .iter()
                .all(|prior| prior.snapshot().run_id() != entry.snapshot().run_id())
        });
        let continuation = next.is_none_or(|cursor| {
            entries.last().is_some_and(|last| {
                cursor.store() == store
                    && entries.len() == super::MAX_PRODUCT_RUN_PAGE
                    && (cursor.after_sequence(), cursor.after_run())
                        == (last.sequence(), last.snapshot().run_id())
                    && entries.first().is_some_and(|first| {
                        first.sequence() < cursor.highwater_sequence()
                            || first.sequence() == cursor.highwater_sequence()
                                && first.snapshot().run_id() == cursor.highwater_run()
                    })
            })
        });
        if entries.len() > super::MAX_PRODUCT_RUN_PAGE || !ordered || !unique || !continuation {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { store, entries, next })
    }
    #[must_use]
    pub const fn store(&self) -> super::ProductRunStoreId { self.store }
    #[must_use]
    pub const fn entries(&self) -> &[ProductRunReferencePageEntry] { self.entries.as_slice() }
    #[must_use]
    pub const fn next(&self) -> Option<super::ProductRunPageCursor> { self.next }
}

/// Exact immutable text range request, scoped to the run that exposes the reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductArtifactQuery {
    run_id: RunId,
    source: ProductArtifactReference,
    offset: u64,
}

impl ProductArtifactQuery {
    /// Constructs a range request within the exact source length.
    ///
    /// # Errors
    /// Rejects an offset beyond the complete source.
    pub const fn new(
        run_id: RunId,
        source: ProductArtifactReference,
        offset: u64,
    ) -> Result<Self, ProductRunMessageError> {
        if offset > source.bytes() {
            Err(ProductRunMessageError::InvalidPage)
        } else {
            Ok(Self { run_id, source, offset })
        }
    }
    #[must_use]
    pub const fn run_id(self) -> RunId { self.run_id }
    #[must_use]
    pub const fn source(self) -> ProductArtifactReference { self.source }
    #[must_use]
    pub const fn offset(self) -> u64 { self.offset }
}

/// One progress-making UTF-8 slice of an immutable product artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductArtifactPage {
    query: ProductArtifactQuery,
    bytes: Vec<u8>,
    next: Option<u64>,
}

impl ProductArtifactPage {
    /// Checks the physical chunk and its exact logical continuation.
    ///
    /// # Errors
    /// Rejects oversized, empty-progress, out-of-range, or inconsistent chunks.
    pub fn new(
        query: ProductArtifactQuery,
        bytes: Vec<u8>,
        next: Option<u64>,
    ) -> Result<Self, ProductRunMessageError> {
        let end = query
            .offset()
            .checked_add(u64::try_from(bytes.len()).map_err(|_| ProductRunMessageError::TooLong)?)
            .ok_or(ProductRunMessageError::InvalidPage)?;
        if bytes.len() > MAX_PRODUCT_ARTIFACT_CHUNK_BYTES
            || end > query.source().bytes()
            || end < query.source().bytes() && (bytes.is_empty() || next != Some(end))
            || end == query.source().bytes() && next.is_some()
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { query, bytes, next })
    }
    #[must_use]
    pub const fn query(&self) -> ProductArtifactQuery { self.query }
    #[must_use]
    pub fn bytes(&self) -> &[u8] { &self.bytes }
    #[must_use]
    pub const fn next(&self) -> Option<u64> { self.next }
}

/// Snapshot-bound query for one deliverable index page.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductDeliverableIndexQuery {
    run_id: RunId,
    kind: ProductDeliverableIndexKind,
    index: ProductDeliverableIndexReference,
    after: Option<u64>,
}

impl ProductDeliverableIndexQuery {
    /// Starts or continues the exact immutable ordered collection.
    ///
    /// # Errors
    /// Rejects a continuation outside the collection.
    pub const fn new(
        run_id: RunId,
        kind: ProductDeliverableIndexKind,
        index: ProductDeliverableIndexReference,
        after: Option<u64>,
    ) -> Result<Self, ProductRunMessageError> {
        if after.is_some_and(|after| after >= index.count()) {
            Err(ProductRunMessageError::InvalidPage)
        } else {
            Ok(Self { run_id, kind, index, after })
        }
    }
    #[must_use]
    pub const fn run_id(self) -> RunId { self.run_id }
    #[must_use]
    pub const fn kind(self) -> ProductDeliverableIndexKind { self.kind }
    #[must_use]
    pub const fn index(self) -> ProductDeliverableIndexReference { self.index }
    #[must_use]
    pub const fn after(self) -> Option<u64> { self.after }
}

/// One ordered deliverable item represented by its independently readable artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductDeliverableIndexEntry {
    ordinal: u64,
    value: ProductArtifactReference,
}

impl ProductDeliverableIndexEntry {
    #[must_use]
    pub const fn new(ordinal: u64, value: ProductArtifactReference) -> Self {
        Self { ordinal, value }
    }
    #[must_use]
    pub const fn ordinal(self) -> u64 { self.ordinal }
    #[must_use]
    pub const fn value(self) -> ProductArtifactReference { self.value }
}

/// One bounded page of an exact immutable changed-path or command index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductDeliverableIndexPage {
    query: ProductDeliverableIndexQuery,
    entries: Vec<ProductDeliverableIndexEntry>,
    next: Option<u64>,
}

impl ProductDeliverableIndexPage {
    /// Checks contiguous ordinals and the exact next position.
    ///
    /// # Errors
    /// Rejects oversized, gapped, out-of-range, or inconsistent pages.
    pub fn new(
        query: ProductDeliverableIndexQuery,
        entries: Vec<ProductDeliverableIndexEntry>,
        next: Option<u64>,
    ) -> Result<Self, ProductRunMessageError> {
        let first = query.after().map_or(0, |after| after.saturating_add(1));
        let contiguous = entries.iter().enumerate().all(|(index, entry)| {
            u64::try_from(index)
                .ok()
                .and_then(|index| first.checked_add(index))
                == Some(entry.ordinal())
        });
        let end = entries.last().map_or(first, |entry| entry.ordinal().saturating_add(1));
        if entries.len() > MAX_PRODUCT_DELIVERABLE_INDEX_PAGE
            || !contiguous
            || end > query.index().count()
            || end < query.index().count()
                && (entries.is_empty() || next != entries.last().map(|entry| entry.ordinal()))
            || end == query.index().count() && next.is_some()
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { query, entries, next })
    }
    #[must_use]
    pub const fn query(&self) -> ProductDeliverableIndexQuery { self.query }
    #[must_use]
    pub const fn entries(&self) -> &[ProductDeliverableIndexEntry] { self.entries.as_slice() }
    #[must_use]
    pub const fn next(&self) -> Option<u64> { self.next }
}

/// Computes the exact ordered collection root used by deliverable page cursors.
#[must_use]
pub fn product_deliverable_index_reference(
    kind: ProductDeliverableIndexKind,
    values: &[String],
) -> Result<ProductDeliverableIndexReference, ProductRunMessageError> {
    let count = u64::try_from(values.len()).map_err(|_| ProductRunMessageError::TooLong)?;
    let mut bytes = b"peritus.product-deliverable-index.v1\0".to_vec();
    bytes.extend_from_slice(&kind.tag().to_be_bytes());
    bytes.extend_from_slice(&count.to_be_bytes());
    for value in values {
        let reference = ProductArtifactReference::measure(value)?;
        bytes.extend_from_slice(reference.digest().as_bytes());
        bytes.extend_from_slice(&reference.bytes().to_be_bytes());
    }
    Ok(ProductDeliverableIndexReference::new(
        peritus_codec::sha256(&bytes),
        count,
    ))
}
