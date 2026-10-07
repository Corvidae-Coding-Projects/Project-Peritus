//! Exact operation replay, scoped C0 ownership, and restart-safe state publication.

use super::ControlStoreError as Error;
use peritus_codec::sha256;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendPlan, AppendRequest, CommandResolution,
    EventDraft, HeadExpectation, JournalCancellation, SqliteJournal, StateInstall, StoreId,
};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ControlReceipt, ConversationId,
    ConversationRecord, OperationId,
};
use peritus_types::{CommandId, EventId, EventSequence, RunId, WorkspaceId};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

const FRAME_FAMILY: u16 = 3402;
const ROOT_NAMESPACE: u16 = 3402;
const RECEIPT_NAMESPACE: u16 = 3403;
const REQUEST_NAMESPACE: u16 = 3404;
const MANIFEST_NAMESPACE: u16 = 3405;
const REPLY_NAMESPACE: u16 = 3406;
const REPLY_DESCRIPTOR_NAMESPACE: u16 = 3487;
const IMAGE_DESCRIPTOR_NAMESPACE: u16 = 3488;
const FILE_DESCRIPTOR_NAMESPACE: u16 = 3490;
const HOST_GOAL_OPERATION_NAMESPACE: u16 = 3420;

mod checkpoints;
pub use checkpoints::snapshot::CheckpointSnapshots;
pub(crate) use checkpoints::snapshot::{
    PreparedCheckpointSnapshots, PreparedRestoreEvidence,
};
pub(crate) use checkpoints::snapshot::{
    PublicationClaim, PublicationPurpose, RetainedPublication,
};
mod files;
pub(crate) use files::PreparedFilePublication;
mod guidance;
mod images;
pub(crate) use images::PreparedImagePublication;
mod initialization;
mod inspection;
mod library;
mod permissions;
mod projection;
mod replay;
mod replies;
pub(crate) use replies::{
    PreparedPublicReply, PublishedPublicReply, open_public_reply_artifact,
    reply_publication_claim,
};
mod reservations;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// One canonical authority identity. Sets are acquired atomically in this enum's stable order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum AuthorityKey {
    Conversation(ConversationId),
    Run(RunId),
    Workspace(WorkspaceId),
}

/// The complete authority set declared before entering a C0 lane.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AuthoritySet(BTreeSet<AuthorityKey>);

impl AuthoritySet {
    #[must_use]
    pub(crate) fn new(keys: impl IntoIterator<Item = AuthorityKey>) -> Self {
        Self(keys.into_iter().collect())
    }

    #[must_use]
    pub(crate) fn contains(&self, key: AuthorityKey) -> bool {
        self.0.contains(&key)
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GenerationPhase {
    Bootstrapping,
    Running,
    Draining,
    Closed,
}

struct GenerationState {
    phase: GenerationPhase,
    owners_registered: bool,
    active: BTreeSet<AuthorityKey>,
    index_readers: usize,
    waiters: VecDeque<AuthorityWaiter>,
    reconciliation: BTreeMap<u64, AuthoritySet>,
    next_registration: u64,
}

struct AuthorityWaiter {
    token: Arc<()>,
    authorities: AuthoritySet,
}

struct ControlGenerationInner {
    root: PathBuf,
    store: StoreId,
    checkpoint_config: peritus_artifact_store::StoreConfig,
    publications: Arc<checkpoints::snapshot::PublicationCoordinator>,
    commits: Arc<ControlCommitCoordinator>,
    inventory: Mutex<Option<checkpoints::snapshot::PublicationInventory>>,
    state: Mutex<GenerationState>,
    ready: Condvar,
    // Declared last so all generation-owned state is dropped before the process ownership lock.
    _owner: ControlOwner,
}

/// Process-owned C0 generation. It owns the lock, configuration, authority lanes, and marker scan.
#[derive(Clone)]
pub(crate) struct ControlGeneration(Arc<ControlGenerationInner>);

/// Linear startup capability for exact recovery and marker adoption before foreground admission.
pub(crate) struct BootstrapRecoveryToken {
    generation: Option<ControlGeneration>,
}

/// One atomically acquired authority set. Dropping it releases the whole set together.
pub(crate) struct ControlAuthority {
    generation: ControlGeneration,
    authorities: AuthoritySet,
}

/// Move-only exact scope used to construct one lane-local [`ControlStore`].
pub(crate) struct ControlScope(ControlAuthority);

struct ControlIndexRead(ControlGeneration);

/// A generation-bound, pre-registered exact scope that remains admitted while draining.
pub(crate) struct ControlReconciliation {
    generation: ControlGeneration,
    registration: u64,
    authorities: AuthoritySet,
}

#[derive(Clone, Debug)]
enum StoreScope {
    Bootstrap,
    Scoped(AuthoritySet),
    IndexRead,
    Compatibility,
}

pub struct ControlStore {
    journal: SqliteJournal,
    store: StoreId,
    projections: Option<projection::ConversationProjections>,
    checkpoint_artifacts: peritus_artifact_store::ArtifactStore,
    checkpoint_config: peritus_artifact_store::StoreConfig,
    cancellation: JournalCancellation,
    scope: StoreScope,
    // Fields are dropped in declaration order: close lane connections before releasing authority
    // and release generation ownership only after every connection has closed.
    _scope: Option<ControlScope>,
    _index_read: Option<ControlIndexRead>,
    generation: ControlGeneration,
}

struct ControlOwner(File);

#[derive(Default)]
struct ControlCommitCoordinator {
    active: Mutex<bool>,
    ready: Condvar,
}

struct ControlCommitLease {
    coordinator: Arc<ControlCommitCoordinator>,
}

impl ControlCommitCoordinator {
    fn acquire(
        self: &Arc<Self>,
        cancellation: &JournalCancellation,
    ) -> Result<ControlCommitLease, Error> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| Error::Corrupt("control commit coordinator lock poisoned"))?;
        while *active {
            if cancellation.is_cancelled() {
                return Err(Error::ContentionCancelled);
            }
            let wake = self
                .ready
                .wait_timeout(active, Duration::from_millis(1))
                .map_err(|_| Error::Corrupt("control commit coordinator wait poisoned"))?;
            active = wake.0;
        }
        *active = true;
        Ok(ControlCommitLease { coordinator: Arc::clone(self) })
    }

    fn is_quiescent(&self) -> Result<bool, Error> {
        self.active
            .lock()
            .map(|active| !*active)
            .map_err(|_| Error::Corrupt("control commit coordinator lock poisoned"))
    }
}

impl Drop for ControlCommitLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.coordinator.active.lock() {
            *active = false;
            self.coordinator.ready.notify_all();
        }
    }
}

impl Drop for ControlOwner {
    fn drop(&mut self) {
        if let Err(error) = self.0.unlock() {
            use std::io::Write as _;
            let _ = std::io::stderr()
                .lock()
                .write_all(format!("control-store ownership unlock failed: {error}\n").as_bytes());
        }
    }
}

impl ControlGeneration {
    /// Opens the single process generation in bootstrap mode and captures its marker inventory.
    pub(crate) fn open_bootstrapping(
        root: &Path,
        store: StoreId,
        cancellation: &JournalCancellation,
    ) -> Result<(Self, BootstrapRecoveryToken), Error> {
        std::fs::create_dir_all(root)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        loop {
            match owner.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    if cancellation.is_cancelled() {
                        return Err(Error::ContentionCancelled);
                    }
                    contention_pause();
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(Error::Io(error)),
            }
        }
        let owner = ControlOwner(owner);
        // Validate the durable store identity while the process lock is already held.
        open_journal(root, store, cancellation).map(drop)?;
        let checkpoint_config = ControlStore::reply_artifact_config(root)?;
        peritus_artifact_store::ArtifactStore::open(checkpoint_config.clone())
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        let inventory =
            checkpoints::snapshot::PublicationInventory::scan(checkpoint_config.root())?;
        let generation = Self(Arc::new(ControlGenerationInner {
            root: root.to_path_buf(),
            store,
            checkpoint_config,
            publications: Arc::new(checkpoints::snapshot::PublicationCoordinator::default()),
            commits: Arc::new(ControlCommitCoordinator::default()),
            inventory: Mutex::new(Some(inventory)),
            state: Mutex::new(GenerationState {
                phase: GenerationPhase::Bootstrapping,
                owners_registered: false,
                active: BTreeSet::new(),
                index_readers: 0,
                waiters: VecDeque::new(),
                reconciliation: BTreeMap::new(),
                next_registration: 1,
            }),
            ready: Condvar::new(),
            _owner: owner,
        }));
        Ok((generation.clone(), BootstrapRecoveryToken { generation: Some(generation) }))
    }

    #[must_use]
    pub(crate) fn root(&self) -> &Path {
        &self.0.root
    }

    #[must_use]
    pub(crate) fn store_id(&self) -> StoreId {
        self.0.store
    }

    #[must_use]
    pub(crate) fn reply_artifact_config(&self) -> &peritus_artifact_store::StoreConfig {
        &self.0.checkpoint_config
    }

    /// Acquires an ordinary foreground set. Admission is possible only in Running.
    pub(crate) fn acquire(
        &self,
        authorities: AuthoritySet,
        cancellation: &JournalCancellation,
    ) -> Result<ControlAuthority, Error> {
        self.acquire_inner(authorities, None, cancellation)
    }

    fn acquire_inner(
        &self,
        authorities: AuthoritySet,
        reconciliation: Option<u64>,
        cancellation: &JournalCancellation,
    ) -> Result<ControlAuthority, Error> {
        if authorities.is_empty() {
            return Err(ControlError::InvalidInput.into());
        }
        let token = Arc::new(());
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        validate_admission(&state, reconciliation, &authorities)?;
        state.waiters.push_back(AuthorityWaiter {
            token: Arc::clone(&token),
            authorities: authorities.clone(),
        });
        loop {
            if cancellation.is_cancelled()
                || validate_admission(&state, reconciliation, &authorities).is_err()
            {
                remove_authority_waiter(&mut state, &token);
                self.0.ready.notify_all();
                return Err(Error::ContentionCancelled);
            }
            let at_front = state
                .waiters
                .front()
                .is_some_and(|waiter| Arc::ptr_eq(&waiter.token, &token));
            if at_front && !state.active.iter().any(|key| authorities.0.contains(key)) {
                let waiter = state
                    .waiters
                    .pop_front()
                    .ok_or(Error::Corrupt("control authority waiter disappeared"))?;
                if waiter.authorities != authorities {
                    return Err(Error::Corrupt("control authority waiter changed scope"));
                }
                state.active.extend(authorities.0.iter().copied());
                self.0.ready.notify_all();
                return Ok(ControlAuthority { generation: self.clone(), authorities });
            }
            let wake = self
                .0
                .ready
                .wait_timeout(state, Duration::from_millis(1))
                .map_err(|_| Error::Corrupt("control generation wait poisoned"))?;
            state = wake.0;
        }
    }

    /// Registers one exact owner while admission is open. The handle cannot broaden its set.
    pub(crate) fn register_reconciliation(
        &self,
        authorities: AuthoritySet,
    ) -> Result<ControlReconciliation, Error> {
        if authorities.is_empty() {
            return Err(ControlError::InvalidInput.into());
        }
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        if !matches!(state.phase, GenerationPhase::Bootstrapping | GenerationPhase::Running)
            || (state.phase == GenerationPhase::Bootstrapping && state.owners_registered)
        {
            return Err(Error::ContentionCancelled);
        }
        let registration = state.next_registration;
        state.next_registration = registration.checked_add(1).ok_or(ControlError::Capacity)?;
        state.reconciliation.insert(registration, authorities.clone());
        Ok(ControlReconciliation { generation: self.clone(), registration, authorities })
    }

    /// Closes new foreground admission. Pre-registered reconciliation handles remain valid.
    pub(crate) fn begin_draining(&self) -> Result<(), Error> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        match state.phase {
            GenerationPhase::Running => state.phase = GenerationPhase::Draining,
            GenerationPhase::Draining => {}
            GenerationPhase::Bootstrapping | GenerationPhase::Closed => {
                return Err(Error::Corrupt("control generation cannot begin draining"));
            }
        }
        self.0.ready.notify_all();
        Ok(())
    }

    /// Waits for all scopes and registered owners, then seals the generation permanently.
    pub(crate) fn finish_draining(
        &self,
        cancellation: &JournalCancellation,
    ) -> Result<(), Error> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        loop {
            if state.phase != GenerationPhase::Draining {
                return Err(Error::Corrupt("control generation is not draining"));
            }
            if state.active.is_empty()
                && state.waiters.is_empty()
                && state.index_readers == 0
                && state.reconciliation.is_empty()
                && self.publications_quiescent()?
                && self.0.commits.is_quiescent()?
            {
                state.phase = GenerationPhase::Closed;
                self.0.ready.notify_all();
                return Ok(());
            }
            if cancellation.is_cancelled() {
                return Err(Error::ContentionCancelled);
            }
            let wake = self
                .0
                .ready
                .wait_timeout(state, Duration::from_millis(1))
                .map_err(|_| Error::Corrupt("control generation wait poisoned"))?;
            state = wake.0;
        }
    }

    pub(crate) fn open_scope(
        &self,
        scope: ControlScope,
        cancellation: &JournalCancellation,
    ) -> Result<ControlStore, Error> {
        if !Arc::ptr_eq(&self.0, &(scope.0.generation).0) {
            return Err(ControlError::ScopeMismatch.into());
        }
        let authorities = scope.0.authorities.clone();
        self.open_lane(StoreScope::Scoped(authorities), Some(scope), cancellation)
    }

    /// Opens the only authority-free lane: a read-only catalog metadata snapshot.
    pub(crate) fn open_index_read(
        &self,
        cancellation: &JournalCancellation,
    ) -> Result<ControlStore, Error> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        if state.phase != GenerationPhase::Running {
            return Err(Error::ContentionCancelled);
        }
        state.index_readers = state.index_readers.checked_add(1).ok_or(ControlError::Capacity)?;
        drop(state);
        let guard = ControlIndexRead(self.clone());
        let mut store = self.open_lane(StoreScope::IndexRead, None, cancellation)?;
        store._index_read = Some(guard);
        Ok(store)
    }

    fn open_lane(
        &self,
        scope: StoreScope,
        authority: Option<ControlScope>,
        cancellation: &JournalCancellation,
    ) -> Result<ControlStore, Error> {
        let journal = open_journal(self.root(), self.0.store, cancellation)?;
        let checkpoint_artifacts =
            peritus_artifact_store::ArtifactStore::open(self.0.checkpoint_config.clone())
                .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        let projections = projection::ConversationProjections::open(self.root(), self.0.store).ok();
        Ok(ControlStore {
            journal,
            store: self.0.store,
            projections,
            checkpoint_artifacts,
            checkpoint_config: self.0.checkpoint_config.clone(),
            cancellation: cancellation.clone(),
            scope,
            _scope: authority,
            _index_read: None,
            generation: self.clone(),
        })
    }

    fn open_bootstrap_lane(
        &self,
        cancellation: &JournalCancellation,
    ) -> Result<ControlStore, Error> {
        self.open_lane(StoreScope::Bootstrap, None, cancellation)
    }

    fn open_compatibility_lane(
        &self,
        cancellation: &JournalCancellation,
    ) -> Result<ControlStore, Error> {
        self.open_lane(StoreScope::Compatibility, None, cancellation)
    }

    fn release(&self, authorities: &AuthoritySet) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        for key in &authorities.0 {
            state.active.remove(key);
        }
        self.0.ready.notify_all();
    }

    fn publications_quiescent(&self) -> Result<bool, Error> {
        self.0.publications.is_quiescent()
    }

    fn acquire_publications(
        &self,
        claims: Vec<PublicationClaim>,
        cancellation: &JournalCancellation,
    ) -> Result<checkpoints::snapshot::PublicationLeaseSet, Error> {
        self.0.publications.acquire(claims, cancellation)
    }

    fn acquire_commit(
        &self,
        cancellation: &JournalCancellation,
    ) -> Result<ControlCommitLease, Error> {
        self.0.commits.acquire(cancellation)
    }
}

impl BootstrapRecoveryToken {
    fn generation(&self) -> Result<&ControlGeneration, Error> {
        self.generation
            .as_ref()
            .ok_or(Error::Corrupt("bootstrap recovery token already consumed"))
    }

    /// Runs an exact bootstrap read, resolve, or recovery operation before foreground admission.
    pub(crate) fn with_control<T>(
        &self,
        cancellation: &JournalCancellation,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut store = self.generation()?.open_bootstrap_lane(cancellation)?;
        cancellation.run(|| operation(&mut store))
    }

    pub(crate) fn inspect_publication(
        &self,
        namespace: u16,
        id: &[u8; 16],
    ) -> Result<Option<RetainedPublication>, Error> {
        let inventory = self
            .generation()?
            .0
            .inventory
            .lock()
            .map_err(|_| Error::Corrupt("publication inventory lock poisoned"))?;
        let inventory = inventory
            .as_ref()
            .ok_or(Error::Corrupt("publication inventory already consumed"))?;
        Ok(inventory.inspect(namespace, id))
    }

    pub(crate) fn claim_publication(
        &mut self,
        claim: &PublicationClaim,
    ) -> Result<RetainedPublication, Error> {
        let generation = self.generation()?.clone();
        let mut inventory = generation
            .0
            .inventory
            .lock()
            .map_err(|_| Error::Corrupt("publication inventory lock poisoned"))?;
        inventory
            .as_mut()
            .ok_or(Error::Corrupt("publication inventory already consumed"))?
            .claim(claim)
    }

    /// Seals the startup owner registry. No later draining capability may be manufactured.
    pub(crate) fn mark_owners_registered(&mut self) -> Result<(), Error> {
        let generation = self.generation()?.clone();
        let mut state = generation
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        if state.phase != GenerationPhase::Bootstrapping {
            return Err(Error::Corrupt("control generation is not bootstrapping"));
        }
        state.owners_registered = true;
        Ok(())
    }

    /// Reconciles the complete marker inventory at quiescence, then opens foreground admission.
    pub(crate) fn finish_bootstrap(
        mut self,
        cancellation: &JournalCancellation,
    ) -> Result<(), Error> {
        let generation = self.generation()?.clone();
        {
            let state = generation
                .0
                .state
                .lock()
                .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
            if state.phase != GenerationPhase::Bootstrapping || !state.owners_registered {
                return Err(Error::Corrupt("control generation owners are not registered"));
            }
        }
        let mut inventory = generation
            .0
            .inventory
            .lock()
            .map_err(|_| Error::Corrupt("publication inventory lock poisoned"))?
            .take()
            .ok_or(Error::Corrupt("publication inventory already consumed"))?;
        let finish = match generation.open_bootstrap_lane(cancellation) {
            Ok(mut store) => {
                cancellation.run(|| store.finish_publication_inventory(&mut inventory))
            }
            Err(error) => Err(error),
        };
        if let Err(error) = finish {
            *generation
                .0
                .inventory
                .lock()
                .map_err(|_| Error::Corrupt("publication inventory lock poisoned"))? =
                Some(inventory);
            return Err(error);
        }
        let mut state = generation
            .0
            .state
            .lock()
            .map_err(|_| Error::Corrupt("control generation lock poisoned"))?;
        if !state.active.is_empty()
            || !state.waiters.is_empty()
            || state.index_readers != 0
            || !generation.publications_quiescent()?
            || !generation.0.commits.is_quiescent()?
        {
            return Err(Error::Corrupt("bootstrap completion is not quiescent"));
        }
        state.phase = GenerationPhase::Running;
        generation.0.ready.notify_all();
        self.generation.take();
        Ok(())
    }
}

impl ControlAuthority {
    #[must_use]
    pub(crate) fn authorities(&self) -> &AuthoritySet {
        &self.authorities
    }

    #[must_use]
    pub(crate) fn into_scope(self) -> ControlScope {
        ControlScope(self)
    }
}

impl Drop for ControlAuthority {
    fn drop(&mut self) {
        self.generation.release(&self.authorities);
    }
}

impl Drop for ControlIndexRead {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.0.state.lock() {
            state.index_readers = state.index_readers.saturating_sub(1);
            self.0.0.ready.notify_all();
        }
    }
}

impl ControlReconciliation {
    #[must_use]
    pub(crate) fn authorities(&self) -> &AuthoritySet {
        &self.authorities
    }

    pub(crate) fn acquire(
        &self,
        cancellation: &JournalCancellation,
    ) -> Result<ControlAuthority, Error> {
        self.generation.acquire_inner(
            self.authorities.clone(),
            Some(self.registration),
            cancellation,
        )
    }
}

impl Drop for ControlReconciliation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.generation.0.state.lock() {
            state.reconciliation.remove(&self.registration);
            self.generation.0.ready.notify_all();
        }
    }
}

fn validate_admission(
    state: &GenerationState,
    reconciliation: Option<u64>,
    authorities: &AuthoritySet,
) -> Result<(), Error> {
    match reconciliation {
        None if state.phase == GenerationPhase::Running => Ok(()),
        Some(registration)
            if matches!(state.phase, GenerationPhase::Running | GenerationPhase::Draining)
                && state.reconciliation.get(&registration) == Some(authorities) =>
        {
            Ok(())
        }
        _ => Err(Error::ContentionCancelled),
    }
}

fn remove_authority_waiter(state: &mut GenerationState, token: &Arc<()>) {
    if let Some(index) = state
        .waiters
        .iter()
        .position(|waiter| Arc::ptr_eq(&waiter.token, token))
    {
        state.waiters.remove(index);
    }
}

fn open_journal(
    root: &Path,
    store: StoreId,
    cancellation: &JournalCancellation,
) -> Result<SqliteJournal, Error> {
    loop {
        match SqliteJournal::open_waiting(root.join("control.sqlite3"), store, cancellation) {
            Ok(journal) => return Ok(journal),
            Err(error) if error.is_contention() => {
                if cancellation.is_cancelled() {
                    return Err(Error::ContentionCancelled);
                }
                contention_pause();
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// Fully planned immutable journal append plus every full publication claim it requires.
pub(crate) struct PreparedControlAppend {
    operation: ControlOperation,
    receipt: ControlReceipt,
    plan: AppendPlan,
    publications: checkpoints::snapshot::ControlPublications,
    _commit: ControlCommitLease,
}

impl PreparedControlAppend {
    #[must_use]
    pub(crate) const fn operation(&self) -> &ControlOperation {
        &self.operation
    }

    #[must_use]
    pub(crate) fn publication_claims(&self) -> Vec<PublicationClaim> {
        self.publications.claims()
    }
}

/// Post-commit exact receipt and authoritative successor root.
pub(crate) struct CommittedControlAppend {
    receipt: ControlReceipt,
    root: ConversationRecord,
}

impl CommittedControlAppend {
    #[must_use]
    pub(crate) const fn receipt(&self) -> &ControlReceipt {
        &self.receipt
    }

    #[must_use]
    pub(crate) const fn root(&self) -> &ConversationRecord {
        &self.root
    }

    #[must_use]
    pub(crate) fn into_receipt(self) -> ControlReceipt {
        self.receipt
    }
}

impl ControlStore {
    #[cfg(test)]
    pub(crate) fn storage_pages_for_test(
        &self,
    ) -> Result<peritus_journal::SqliteStoragePages, Error> {
        self.journal.storage_pages().map_err(Error::from)
    }

    #[cfg(test)]
    pub(crate) fn limit_storage_pages_for_test(&mut self, pages: u64) -> Result<(), Error> {
        self.journal.limit_storage_pages(pages).map(|_| ()).map_err(Error::from)
    }

    /// Compatibility constructor for isolated callers. Production uses [`ControlGeneration`].
    pub fn open(root: &Path, store: StoreId) -> Result<Self, Error> {
        Self::open_cancellable(root, store, &JournalCancellation::new())
    }

    pub(crate) fn open_cancellable(
        root: &Path,
        store: StoreId,
        cancellation: &JournalCancellation,
    ) -> Result<Self, Error> {
        let (generation, mut bootstrap) =
            ControlGeneration::open_bootstrapping(root, store, cancellation)?;
        bootstrap.mark_owners_registered()?;
        bootstrap.finish_bootstrap(cancellation)?;
        generation.open_compatibility_lane(cancellation)
    }

    #[must_use]
    pub(crate) fn generation(&self) -> &ControlGeneration {
        &self.generation
    }

    fn require_operation_scope(&self, operation: &ControlOperation) -> Result<(), Error> {
        match &self.scope {
            StoreScope::Bootstrap | StoreScope::Compatibility => Ok(()),
            StoreScope::IndexRead => Err(ControlError::ScopeMismatch.into()),
            StoreScope::Scoped(authorities) => {
                if !authorities.contains(AuthorityKey::Conversation(operation.conversation())) {
                    return Err(ControlError::ScopeMismatch.into());
                }
                if matches!(operation.intent(), ControlIntent::SetPermissions { .. }) {
                    let workspace = WorkspaceId::new(*operation.workspace_bytes())
                        .map_err(|_| ControlError::InvalidInput)?;
                    if !authorities.contains(AuthorityKey::Workspace(workspace)) {
                        return Err(ControlError::ScopeMismatch.into());
                    }
                }
                Ok(())
            }
        }
    }

    pub(crate) fn require_conversation_scope(
        &self,
        conversation: ConversationId,
    ) -> Result<(), Error> {
        match &self.scope {
            StoreScope::Bootstrap | StoreScope::Compatibility => Ok(()),
            StoreScope::IndexRead => Err(ControlError::ScopeMismatch.into()),
            StoreScope::Scoped(authorities)
                if authorities.contains(AuthorityKey::Conversation(conversation)) =>
            {
                Ok(())
            }
            StoreScope::Scoped(_) => Err(ControlError::ScopeMismatch.into()),
        }
    }

    pub(crate) fn require_run_scope(&self, run: RunId) -> Result<(), Error> {
        match &self.scope {
            StoreScope::Bootstrap | StoreScope::Compatibility => Ok(()),
            StoreScope::Scoped(authorities) if authorities.contains(AuthorityKey::Run(run)) => {
                Ok(())
            }
            StoreScope::Scoped(_) | StoreScope::IndexRead => {
                Err(ControlError::ScopeMismatch.into())
            }
        }
    }

    pub(crate) fn require_index_read_scope(&self) -> Result<(), Error> {
        match &self.scope {
            StoreScope::Bootstrap | StoreScope::IndexRead | StoreScope::Compatibility => Ok(()),
            StoreScope::Scoped(_) => Err(ControlError::ScopeMismatch.into()),
        }
    }

    /// Atomically publishes exact intent, successor state and original receipt through C0.
    pub fn accept(&mut self, operation: &ControlOperation) -> Result<ControlReceipt, Error> {
        if matches!(
            operation.intent(),
            ControlIntent::SetPermissions { .. }
                | ControlIntent::AttachImage { .. }
                | ControlIntent::UpdateGuidance(_)
                | ControlIntent::RecordInitialization { .. }
                | ControlIntent::Queue(
                    peritus_product_runner::control::QueueIntent::EnqueueSource { .. }
                        | peritus_product_runner::control::QueueIntent::EditSource { .. }
                        | peritus_product_runner::control::QueueIntent::CorrectSource { .. }
                )
        ) {
            return Err(ControlError::InvalidInput.into());
        }
        self.accept_archived(operation, None)
    }

    pub fn accept_request_source(
        &mut self,
        operation: &ControlOperation,
    ) -> Result<ControlReceipt, Error> {
        use peritus_product_runner::control::{ControlIntent, QueueIntent};
        let source = match operation.intent() {
            ControlIntent::Queue(
                QueueIntent::EnqueueSource { source, .. }
                | QueueIntent::EditSource { source, .. }
                | QueueIntent::CorrectSource { source, .. },
            ) => source,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        if source.artifact_bytes() != *operation.id().as_bytes() {
            return Err(ControlError::ScopeMismatch.into());
        }
        self.accept_archived(operation, None)
    }

    pub(super) fn host_goal_operation(
        &self,
        id: OperationId,
    ) -> Result<Option<ControlOperation>, Error> {
        let Some(record) =
            self.journal.state_record(HOST_GOAL_OPERATION_NAMESPACE, id.as_bytes())?
        else {
            return Ok(None);
        };
        if record.revision() != 1 {
            return Err(Error::Corrupt("invalid host goal operation revision"));
        }
        let operation = ControlOperation::parse(record.bytes())?;
        if operation.id() != id {
            return Err(Error::Corrupt("host goal operation identity mismatch"));
        }
        Ok(Some(operation))
    }

    pub(super) fn accept_host_goal_operation(
        &mut self,
        operation: &ControlOperation,
    ) -> Result<ControlReceipt, Error> {
        self.accept_installs(
            operation,
            vec![StateInstall::new(
                HOST_GOAL_OPERATION_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                operation.canonical_bytes()?,
            )?],
        )
    }

    pub(super) fn accept_archived(
        &mut self,
        operation: &ControlOperation,
        archive: Option<super::inputs::RequestArchive>,
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let archived = super::inputs::validate_archive(operation, archive)?;
        let mut installs = Vec::new();
        if let Some((request, manifest)) = archived {
            for (namespace, bytes) in [(REQUEST_NAMESPACE, request), (MANIFEST_NAMESPACE, manifest)]
            {
                installs.push(StateInstall::new(
                    namespace,
                    operation.id().as_bytes().to_vec(),
                    None,
                    1,
                    bytes,
                )?);
            }
        }
        self.accept_installs(operation, installs)
    }

    pub(in crate::product_control) fn prepare_installs(
        &mut self,
        operation: &ControlOperation,
        mut artifacts: Vec<StateInstall>,
    ) -> Result<PreparedControlAppend, Error> {
        self.require_operation_scope(operation)?;
        let commit = self.generation.acquire_commit(&self.cancellation)?;
        let mut replay = self.load_replay(operation.conversation())?;
        let current = replay.current().cloned();
        self.check_creation_reservation(operation, current.is_none())?;
        let receipt = replay.apply(operation)?;
        let next = replay.current().ok_or(Error::Corrupt("missing accepted successor"))?;
        let payload = operation.canonical_bytes()?;
        let mut publications = checkpoints::snapshot::ControlPublications::default();
        let aggregate = aggregate(operation.conversation())?;
        let head = self.journal.head(aggregate)?;
        let frame = self.control_event(operation, &payload, next, &mut publications)?;
        let event = EventDraft::new(
            aggregate,
            EventSequence::new(next.revision()).map_err(|_| ControlError::Capacity)?,
            event_id(operation)?,
            head.map(peritus_journal::AggregateHead::event_id),
            frame,
            sha256(operation.workspace_bytes()),
            Vec::new(),
        )?;
        let mut installs = vec![
            StateInstall::new(
                ROOT_NAMESPACE,
                operation.conversation().as_bytes().to_vec(),
                current.as_ref().map(ConversationRecord::revision),
                next.revision(),
                self.control_projection(operation, next, &replay, &mut publications)?,
            )?,
            StateInstall::new(
                RECEIPT_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                receipt.canonical_bytes()?,
            )?,
        ];
        installs.append(&mut artifacts);
        publications.append_installs(&mut installs);
        installs.sort_by(|left, right| {
            (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
        });
        let plan = AppendRequest::new(
            self.store,
            command_id(operation)?,
            sha256(&payload),
            vec![head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)],
            vec![event],
            installs,
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()?;
        Ok(PreparedControlAppend {
            operation: operation.clone(),
            receipt,
            plan,
            publications,
            _commit: commit,
        })
    }

    pub(in crate::product_control) fn commit_prepared(
        &mut self,
        mut prepared: PreparedControlAppend,
    ) -> Result<CommittedControlAppend, Error> {
        self.require_operation_scope(&prepared.operation)?;
        let _leases = self.generation.acquire_publications(
            prepared.publications.claims(),
            &self.cancellation,
        )?;
        prepared.publications.activate(self)?;
        #[cfg(test)]
        prepared.publications.before_root_publication()?;
        let result = match self.journal.append(prepared.plan) {
            Ok(_) => self
                .resolve(&prepared.operation)?
                .ok_or(Error::Corrupt("committed receipt missing")),
            Err(error) => match self.resolve(&prepared.operation)? {
                Some(receipt) => Ok(receipt),
                None => Err(error.into()),
            },
        };
        let receipt = result?;
        prepared.publications.finish()?;
        let root = self
            .load(prepared.operation.conversation())?
            .ok_or(Error::Corrupt("committed successor root missing"))?;
        if let Err(error) = self.refresh_replay_projection(prepared.operation.conversation()) {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "conversation projection refresh failed after accepted operation: {error}"
            );
        }
        if receipt != prepared.receipt {
            return Err(Error::Corrupt("resolved receipt differs from prepared receipt"));
        }
        Ok(CommittedControlAppend { receipt, root })
    }

    pub(in crate::product_control) fn accept_installs(
        &mut self,
        operation: &ControlOperation,
        artifacts: Vec<StateInstall>,
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.prepare_installs(operation, artifacts)?;
        self.commit_prepared(prepared).map(CommittedControlAppend::into_receipt)
    }

    pub fn resolve(&self, operation: &ControlOperation) -> Result<Option<ControlReceipt>, Error> {
        self.require_operation_scope(operation)?;
        match self
            .journal
            .resolve_command(command_id(operation)?, sha256(&operation.canonical_bytes()?))?
        {
            CommandResolution::DefinitelyAbsent => Ok(None),
            CommandResolution::Conflict { .. } => Err(ControlError::IdempotencyConflict.into()),
            CommandResolution::Committed(batch) => {
                let row = self
                    .journal
                    .state_record(RECEIPT_NAMESPACE, operation.id().as_bytes())?
                    .ok_or(Error::Corrupt("accepted operation has no durable receipt"))?;
                if row.producing_position() != batch.last_position() || row.revision() != 1 {
                    return Err(Error::Corrupt("receipt was not published with its operation"));
                }
                self.verify_request_archive(operation, batch.last_position(), None)?;
                Ok(Some(ControlReceipt::resolve(row.bytes(), operation)?))
            }
        }
    }
}

fn contention_pause() {
    std::thread::sleep(Duration::from_millis(1));
}

fn aggregate(id: ConversationId) -> Result<AggregateKey, Error> {
    Ok(AggregateKey::new(AggregateKind::Application, AggregateId::new(*id.as_bytes())?))
}
fn command_id(operation: &ControlOperation) -> Result<CommandId, Error> {
    CommandId::new(*operation.id().as_bytes()).map_err(|_| ControlError::InvalidInput.into())
}
fn event_id(operation: &ControlOperation) -> Result<EventId, Error> {
    let mut binding = b"peritus-product-control/event/v1".to_vec();
    binding.extend_from_slice(operation.id().as_bytes());
    let digest = sha256(&binding);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    EventId::new(bytes).map_err(|_| ControlError::InvalidInput.into())
}
