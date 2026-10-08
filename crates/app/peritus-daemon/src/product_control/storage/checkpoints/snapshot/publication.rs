//! Sealed publication ownership and restart reconciliation for staged artifact references.

use super::{
    ArtifactDigest, ControlError, ControlStore, Error, RESTORE_EVIDENCE_NAMESPACE,
    SNAPSHOT_NAMESPACE, artifact, reference_owner,
};
use crate::product_control::storage::{
    FILE_DESCRIPTOR_NAMESPACE, IMAGE_DESCRIPTOR_NAMESPACE, REPLY_DESCRIPTOR_NAMESPACE,
};
use peritus_artifact_store::ReferenceOwner;
use peritus_journal::JournalCancellation;
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Seek as _, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

const PENDING_MAGIC: &[u8; 8] = b"pcpub001";
const TYPED_PENDING_MAGIC: &[u8; 8] = b"pcpub002";
const CLAIM_PENDING_MAGIC: &[u8; 8] = b"pcpub003";
const CLAIM_HEADER_BYTES: usize = 68;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// The immutable operation served by one publication claim.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub(crate) enum PublicationPurpose {
    ControlStream = 1,
    Snapshot = 2,
    RestoreEvidence = 3,
    ReplyDescriptor = 4,
    ImageDescriptor = 5,
    FileDescriptor = 6,
}

impl PublicationPurpose {
    fn from_u16(value: u16) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::ControlStream),
            2 => Ok(Self::Snapshot),
            3 => Ok(Self::RestoreEvidence),
            4 => Ok(Self::ReplyDescriptor),
            5 => Ok(Self::ImageDescriptor),
            6 => Ok(Self::FileDescriptor),
            _ => Err(Error::Corrupt("unsupported publication claim purpose")),
        }
    }
}

/// A full immutable claim for one truncated publication key.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct PublicationClaim {
    namespace: u16,
    id: [u8; 16],
    digest: [u8; 32],
    bytes: u64,
    purpose: PublicationPurpose,
}

impl PublicationClaim {
    pub(crate) fn new(
        namespace: u16,
        id: [u8; 16],
        digest: [u8; 32],
        bytes: u64,
        purpose: PublicationPurpose,
    ) -> Result<Self, Error> {
        if !supported_namespace(namespace) || bytes == 0 {
            return Err(ControlError::InvalidInput.into());
        }
        let claim = Self { namespace, id, digest, bytes, purpose };
        if !claim.purpose_matches_namespace() {
            return Err(Error::Corrupt("publication claim purpose differs from its namespace"));
        }
        Ok(claim)
    }

    #[must_use]
    pub(crate) const fn namespace(&self) -> u16 {
        self.namespace
    }

    #[must_use]
    pub(crate) const fn id(&self) -> &[u8; 16] {
        &self.id
    }

    #[must_use]
    pub(crate) const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    #[must_use]
    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub(crate) const fn purpose(&self) -> PublicationPurpose {
        self.purpose
    }

    const fn key(&self) -> (u16, [u8; 16]) {
        (self.namespace, self.id)
    }

    const fn purpose_matches_namespace(&self) -> bool {
        match self.purpose {
            PublicationPurpose::ControlStream => control_namespace(self.namespace),
            PublicationPurpose::Snapshot => self.namespace == SNAPSHOT_NAMESPACE,
            PublicationPurpose::RestoreEvidence => self.namespace == RESTORE_EVIDENCE_NAMESPACE,
            PublicationPurpose::ReplyDescriptor => self.namespace == REPLY_DESCRIPTOR_NAMESPACE,
            PublicationPurpose::ImageDescriptor => self.namespace == IMAGE_DESCRIPTOR_NAMESPACE,
            PublicationPurpose::FileDescriptor => self.namespace == FILE_DESCRIPTOR_NAMESPACE,
        }
    }
}

/// Exact retained marker evidence available during bootstrap adoption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RetainedPublication {
    namespace: u16,
    id: [u8; 16],
    claim: Option<PublicationClaim>,
    artifacts: Vec<ArtifactDigest>,
}

impl RetainedPublication {
    #[must_use]
    pub(crate) const fn namespace(&self) -> u16 {
        self.namespace
    }

    #[must_use]
    pub(crate) const fn id(&self) -> &[u8; 16] {
        &self.id
    }

    #[must_use]
    pub(crate) fn claim(&self) -> Option<&PublicationClaim> {
        self.claim.as_ref()
    }

    #[must_use]
    pub(crate) fn artifacts(&self) -> &[ArtifactDigest] {
        &self.artifacts
    }
}

#[derive(Default)]
pub(in crate::product_control::storage) struct PublicationCoordinator {
    state: Mutex<BTreeMap<(u16, [u8; 16]), PublicationKeyState>>,
    ready: Condvar,
}

#[derive(Default)]
struct PublicationKeyState {
    active: Option<ActiveClaim>,
    waiters: VecDeque<PublicationWaiter>,
}

struct ActiveClaim {
    claim: PublicationClaim,
    holders: usize,
}

struct PublicationWaiter {
    token: Arc<()>,
}

pub(in crate::product_control::storage) struct PublicationLeaseSet {
    coordinator: Arc<PublicationCoordinator>,
    claims: Vec<PublicationClaim>,
}

impl Drop for PublicationLeaseSet {
    fn drop(&mut self) {
        self.coordinator.release(&self.claims);
    }
}

impl PublicationCoordinator {
    pub(in crate::product_control::storage) fn acquire(
        self: &Arc<Self>,
        mut claims: Vec<PublicationClaim>,
        cancellation: &JournalCancellation,
    ) -> Result<PublicationLeaseSet, Error> {
        claims.sort_by_key(PublicationClaim::key);
        let mut deduplicated = Vec::with_capacity(claims.len());
        for claim in claims {
            if let Some(prior) = deduplicated.last() {
                if prior.key() == claim.key() {
                    if prior != &claim {
                        return Err(Error::Corrupt("publication key has conflicting full claims"));
                    }
                    continue;
                }
            }
            deduplicated.push(claim);
        }

        let mut acquired = Vec::with_capacity(deduplicated.len());
        for claim in &deduplicated {
            if let Err(error) = self.acquire_one(claim, cancellation) {
                self.release(&acquired);
                return Err(error);
            }
            acquired.push(claim.clone());
        }
        Ok(PublicationLeaseSet { coordinator: Arc::clone(self), claims: deduplicated })
    }

    fn acquire_one(
        &self,
        claim: &PublicationClaim,
        cancellation: &JournalCancellation,
    ) -> Result<(), Error> {
        let token = Arc::new(());
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Corrupt("publication coordinator lock poisoned"))?;
        state.entry(claim.key()).or_default().waiters.push_back(PublicationWaiter {
            token: Arc::clone(&token),
        });
        loop {
            if cancellation.is_cancelled() {
                if let Some(key) = state.get_mut(&claim.key()) {
                    if let Some(index) = key
                        .waiters
                        .iter()
                        .position(|waiter| Arc::ptr_eq(&waiter.token, &token))
                    {
                        key.waiters.remove(index);
                    }
                }
                state.retain(|_, key| key.active.is_some() || !key.waiters.is_empty());
                self.ready.notify_all();
                return Err(Error::ContentionCancelled);
            }
            let key = state
                .get_mut(&claim.key())
                .ok_or(Error::Corrupt("publication waiter disappeared"))?;
            let at_front = key
                .waiters
                .front()
                .is_some_and(|waiter| Arc::ptr_eq(&waiter.token, &token));
            if at_front {
                match key.active.as_mut() {
                    None => {
                        key.active = Some(ActiveClaim { claim: claim.clone(), holders: 1 });
                        key.waiters.pop_front();
                        return Ok(());
                    }
                    Some(_) => {}
                }
            }
            let wake = self
                .ready
                .wait_timeout(state, Duration::from_millis(1))
                .map_err(|_| Error::Corrupt("publication coordinator wait poisoned"))?;
            state = wake.0;
        }
    }

    fn release(&self, claims: &[PublicationClaim]) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        for claim in claims.iter().rev() {
            let Some(key) = state.get_mut(&claim.key()) else {
                continue;
            };
            let Some(active) = key.active.as_mut() else {
                continue;
            };
            if active.claim != *claim || active.holders == 0 {
                continue;
            }
            active.holders -= 1;
            if active.holders == 0 {
                key.active = None;
            }
        }
        state.retain(|_, key| key.active.is_some() || !key.waiters.is_empty());
        self.ready.notify_all();
    }

    pub(in crate::product_control::storage) fn is_quiescent(&self) -> Result<bool, Error> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::Corrupt("publication coordinator lock poisoned"))?;
        Ok(state.values().all(|key| key.active.is_none() && key.waiters.is_empty()))
    }
}

/// Finalized immutable artifacts that will gain durable references only while their claim is held.
pub(in crate::product_control::storage) struct PublicationPlan {
    claim: PublicationClaim,
    owner: ReferenceOwner,
    artifacts: Vec<ArtifactDigest>,
}

impl PublicationPlan {
    pub(in crate::product_control::storage) fn new(
        claim: PublicationClaim,
        owner: ReferenceOwner,
        mut artifacts: Vec<ArtifactDigest>,
    ) -> Result<Self, Error> {
        if owner != reference_owner(claim.namespace, &claim.id) {
            return Err(Error::Corrupt("publication plan owner differs from its full claim"));
        }
        artifacts.sort();
        artifacts.dedup();
        Ok(Self { claim, owner, artifacts })
    }

    #[must_use]
    pub(in crate::product_control::storage) const fn claim(&self) -> &PublicationClaim {
        &self.claim
    }
}

pub(in crate::product_control::storage) struct PendingPublication {
    file: Option<File>,
    path: PathBuf,
}

impl PendingPublication {
    pub(in crate::product_control::storage) fn new(
        root: &Path,
        claim: &PublicationClaim,
    ) -> Result<Self, Error> {
        let directory = publication_directory(root)?;
        let path = directory.join(publication_name(claim.namespace, &claim.id));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        write_publication_header(&mut file, claim)?;
        file.sync_all()?;
        sync_directory(&directory)?;
        sync_directory(root)?;
        Ok(Self { file: Some(file), path })
    }

    /// Creates or resumes the exact same unpublished operation after an interrupted retry.
    pub(in crate::product_control::storage) fn new_or_resume(
        root: &Path,
        claim: &PublicationClaim,
        reconcile_incomplete: impl FnOnce() -> Result<(), Error>,
    ) -> Result<Self, Error> {
        match Self::new(root, claim) {
            Ok(marker) => Ok(marker),
            Err(Error::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => {
                let directory = publication_directory(root)?;
                let path = directory.join(publication_name(claim.namespace, &claim.id));
                if !fs::symlink_metadata(&path)?.file_type().is_file() {
                    return Err(Error::Corrupt("unsafe checkpoint publication marker"));
                }
                let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
                match publication_header(&mut file) {
                    Ok(MarkerHeader::Full(stored)) if stored != *claim => {
                        return Err(Error::Corrupt("publication retry full claim differs"));
                    }
                    Ok(MarkerHeader::Full(_)) => {}
                    Ok(MarkerHeader::Legacy { namespace, id })
                        if (namespace, id) == claim.key() =>
                    {
                        let artifacts = read_artifact_digests(&mut file)?;
                        file.set_len(0)?;
                        file.seek(io::SeekFrom::Start(0))?;
                        write_publication_header(&mut file, claim)?;
                        for digest in artifacts {
                            file.write_all(digest.as_bytes())?;
                        }
                    }
                    Ok(MarkerHeader::Legacy { .. }) => {
                        return Err(Error::Corrupt("publication retry binding differs"));
                    }
                    Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                        if !incomplete_publication_header_matches(&mut file, claim)? {
                            return Err(Error::Corrupt(
                                "incomplete publication marker is not a valid claim prefix",
                            ));
                        }
                        reconcile_incomplete()?;
                        file.set_len(0)?;
                        file.seek(io::SeekFrom::Start(0))?;
                        write_publication_header(&mut file, claim)?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                        return Err(Error::Corrupt("checkpoint publication marker invalid"));
                    }
                    Err(error) => return Err(error.into()),
                }
                file.sync_all()?;
                sync_directory(&directory)?;
                sync_directory(root)?;
                file.seek(io::SeekFrom::End(0))?;
                Ok(Self { file: Some(file), path })
            }
            Err(error) => Err(error),
        }
    }

    pub(in crate::product_control::storage) fn record(
        &mut self,
        digest: ArtifactDigest,
    ) -> Result<(), Error> {
        let file = self
            .file
            .as_mut()
            .ok_or(Error::Corrupt("checkpoint publication already closed"))?;
        file.write_all(digest.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    pub(in crate::product_control::storage) fn finish(mut self) -> Result<(), Error> {
        self.file.take();
        fs::remove_file(&self.path)?;
        sync_directory(
            self.path.parent().ok_or(Error::Corrupt("checkpoint publication parent missing"))?,
        )?;
        Ok(())
    }
}

pub(in crate::product_control::storage) struct PublicationInventory {
    entries: BTreeMap<(u16, [u8; 16]), InventoryEntry>,
}

struct InventoryEntry {
    path: PathBuf,
    retained: RetainedPublication,
    claimed: Option<PublicationClaim>,
}

impl PublicationInventory {
    pub(in crate::product_control::storage) fn scan(root: &Path) -> Result<Self, Error> {
        let directory = root.join("publishing");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self { entries: BTreeMap::new() });
            }
            Err(error) => return Err(error.into()),
        };
        if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
            return Err(Error::Corrupt("unsafe checkpoint publication directory"));
        }
        let mut inventory = BTreeMap::new();
        for entry in entries {
            let path = entry?.path();
            if !fs::symlink_metadata(&path)?.file_type().is_file() {
                return Err(Error::Corrupt("unsafe checkpoint publication marker"));
            }
            let named = publication_identity(&path)?;
            let mut file = File::open(&path)?;
            let (claim, artifacts) = match publication_header(&mut file) {
                Ok(MarkerHeader::Full(claim)) if claim.key() == named => {
                    let artifacts = read_artifact_digests(&mut file)?;
                    (Some(claim), artifacts)
                }
                Ok(MarkerHeader::Legacy { namespace, id }) if (namespace, id) == named => {
                    let artifacts = read_artifact_digests(&mut file)?;
                    (None, artifacts)
                }
                Ok(_) => {
                    return Err(Error::Corrupt(
                        "checkpoint publication marker name and header differ",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                    if !incomplete_named_header_matches(&mut file, named.0, &named.1)? {
                        return Err(Error::Corrupt(
                            "incomplete checkpoint publication marker is not a valid prefix",
                        ));
                    }
                    (None, Vec::new())
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    return Err(Error::Corrupt("checkpoint publication marker invalid"));
                }
                Err(error) => return Err(error.into()),
            };
            let retained = RetainedPublication {
                namespace: named.0,
                id: named.1,
                claim,
                artifacts,
            };
            if inventory
                .insert(named, InventoryEntry { path, retained, claimed: None })
                .is_some()
            {
                return Err(Error::Corrupt("duplicate checkpoint publication marker"));
            }
        }
        Ok(Self { entries: inventory })
    }

    pub(in crate::product_control::storage) fn inspect(
        &self,
        namespace: u16,
        id: &[u8; 16],
    ) -> Option<RetainedPublication> {
        self.entries.get(&(namespace, *id)).map(|entry| entry.retained.clone())
    }

    pub(in crate::product_control::storage) fn claim(
        &mut self,
        expected: &PublicationClaim,
    ) -> Result<RetainedPublication, Error> {
        let entry = self
            .entries
            .get_mut(&expected.key())
            .ok_or(ControlError::NotFound)?;
        if entry
            .retained
            .claim
            .as_ref()
            .is_some_and(|claim| claim != expected)
            || entry.claimed.as_ref().is_some_and(|claim| claim != expected)
        {
            return Err(ControlError::IdempotencyConflict.into());
        }
        entry.claimed = Some(expected.clone());
        Ok(entry.retained.clone())
    }
}

impl ControlStore {
    pub(in crate::product_control::storage) fn activate_publication(
        &mut self,
        plan: &PublicationPlan,
    ) -> Result<PendingPublication, Error> {
        let root = self.checkpoint_config.root().to_path_buf();
        let claim = plan.claim.clone();
        let owner = plan.owner;
        let mut pending = PendingPublication::new_or_resume(&root, &claim, || {
            if self.publication_is_authoritative(claim.namespace, &claim.id)? {
                return Err(Error::Corrupt(
                    "authoritative root retains an unfinished publication marker",
                ));
            }
            artifact(self.checkpoint_artifacts.retire_reference_owner(owner))?;
            Ok(())
        })?;
        for digest in &plan.artifacts {
            let metadata = artifact(self.checkpoint_artifacts.verify(*digest))?;
            if metadata.digest() != *digest {
                return Err(Error::Corrupt("publication artifact digest differs"));
            }
            pending.record(*digest)?;
            artifact(self.checkpoint_artifacts.add_reference(owner, *digest))?;
        }
        Ok(pending)
    }

    pub(in crate::product_control::storage) fn finish_publication_inventory(
        &mut self,
        inventory: &mut PublicationInventory,
    ) -> Result<(), Error> {
        if !self.generation.publications_quiescent()? {
            return Err(Error::Corrupt("publication recovery requires quiescent leases"));
        }
        let mut abandoned = self
            .checkpoint_config
            .root()
            .join("collection-pending")
            .try_exists()?;
        let mut changed = false;
        let mut retained_claimed = BTreeMap::new();
        for (key, entry) in std::mem::take(&mut inventory.entries) {
            let authoritative = self.publication_is_authoritative(key.0, &key.1)?;
            if !authoritative && entry.claimed.is_some() {
                retained_claimed.insert(key, entry);
                continue;
            }
            if !authoritative {
                self.mark_snapshot_collection_pending()?;
                artifact(
                    self.checkpoint_artifacts
                        .retire_reference_owner(reference_owner(key.0, &key.1)),
                )?;
                abandoned = true;
            }
            fs::remove_file(entry.path)?;
            changed = true;
        }
        inventory.entries = retained_claimed;
        if changed {
            sync_directory(&self.checkpoint_config.root().join("publishing"))?;
        }
        if abandoned {
            self.collect_abandoned_snapshot_artifacts()?;
        }
        Ok(())
    }

    fn publication_is_authoritative(
        &self,
        namespace: u16,
        id: &[u8; 16],
    ) -> Result<bool, Error> {
        if let Some(record) = self.journal.state_record(namespace, id)? {
            if record.revision() != 1 {
                return Err(Error::Corrupt("immutable publication root has a mutable revision"));
            }
            return Ok(true);
        }
        Ok(self.journal.state_record_revision(namespace, id, 1)?.is_some())
    }

    fn mark_snapshot_collection_pending(&self) -> Result<(), Error> {
        let path = self.checkpoint_config.root().join("collection-pending");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                file.sync_all()?;
                sync_directory(self.checkpoint_config.root())?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&path)?.file_type().is_file() {
                    return Err(Error::Corrupt("unsafe snapshot collection marker"));
                }
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    fn collect_abandoned_snapshot_artifacts(&mut self) -> Result<(), Error> {
        use peritus_artifact_store::CollectionGeneration;
        let path = self.checkpoint_config.root().join("collection-generation");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(Error::Corrupt("unsafe snapshot collection generation"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut generation = match fs::read(&path) {
            Ok(bytes) => u64::from_be_bytes(
                bytes
                    .try_into()
                    .map_err(|_| Error::Corrupt("checkpoint collection generation invalid"))?,
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        for _ in 0..2 {
            generation = generation.checked_add(1).ok_or(ControlError::Capacity)?;
            let mut file = tempfile::NamedTempFile::new_in(self.checkpoint_config.root())?;
            file.write_all(&generation.to_be_bytes())?;
            file.as_file().sync_all()?;
            file.persist(&path).map_err(|error| error.error)?;
            sync_directory(self.checkpoint_config.root())?;
            let plan = artifact(
                self.checkpoint_artifacts
                    .plan_gc(artifact(CollectionGeneration::new(generation))?),
            )?;
            artifact(self.checkpoint_artifacts.apply_gc_plan(&plan))?;
        }
        fs::remove_file(self.checkpoint_config.root().join("collection-pending"))?;
        sync_directory(self.checkpoint_config.root())?;
        Ok(())
    }
}

enum MarkerHeader {
    Full(PublicationClaim),
    Legacy { namespace: u16, id: [u8; 16] },
}

fn publication_header(file: &mut File) -> io::Result<MarkerHeader> {
    file.seek(io::SeekFrom::Start(0))?;
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)?;
    if &magic == CLAIM_PENDING_MAGIC {
        let mut namespace = [0_u8; 2];
        let mut id = [0_u8; 16];
        let mut digest = [0_u8; 32];
        let mut bytes = [0_u8; 8];
        let mut purpose = [0_u8; 2];
        file.read_exact(&mut namespace)?;
        file.read_exact(&mut id)?;
        file.read_exact(&mut digest)?;
        file.read_exact(&mut bytes)?;
        file.read_exact(&mut purpose)?;
        let claim = PublicationClaim::new(
            u16::from_be_bytes(namespace),
            id,
            digest,
            u64::from_be_bytes(bytes),
            PublicationPurpose::from_u16(u16::from_be_bytes(purpose))
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?,
        )
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        return Ok(MarkerHeader::Full(claim));
    }
    let namespace = if &magic == PENDING_MAGIC {
        SNAPSHOT_NAMESPACE
    } else if &magic == TYPED_PENDING_MAGIC {
        let mut bytes = [0_u8; 2];
        file.read_exact(&mut bytes)?;
        u16::from_be_bytes(bytes)
    } else {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    };
    if !supported_namespace(namespace) {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut id = [0_u8; 16];
    file.read_exact(&mut id)?;
    Ok(MarkerHeader::Legacy { namespace, id })
}

fn write_publication_header(file: &mut File, claim: &PublicationClaim) -> io::Result<()> {
    file.write_all(&claim_publication_header(claim))
}

fn claim_publication_header(claim: &PublicationClaim) -> [u8; CLAIM_HEADER_BYTES] {
    let mut header = [0_u8; CLAIM_HEADER_BYTES];
    header[..8].copy_from_slice(CLAIM_PENDING_MAGIC);
    header[8..10].copy_from_slice(&claim.namespace.to_be_bytes());
    header[10..26].copy_from_slice(&claim.id);
    header[26..58].copy_from_slice(&claim.digest);
    header[58..66].copy_from_slice(&claim.bytes.to_be_bytes());
    header[66..68].copy_from_slice(&(claim.purpose as u16).to_be_bytes());
    header
}

fn incomplete_publication_header_matches(
    file: &mut File,
    claim: &PublicationClaim,
) -> io::Result<bool> {
    let length = usize::try_from(file.metadata()?.len())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if length >= CLAIM_HEADER_BYTES {
        return Ok(false);
    }
    file.seek(io::SeekFrom::Start(0))?;
    let mut actual = vec![0_u8; length];
    file.read_exact(&mut actual)?;
    if claim_publication_header(claim).starts_with(&actual) {
        return Ok(true);
    }
    incomplete_named_bytes_match(&actual, claim.namespace, &claim.id)
}

fn incomplete_named_header_matches(
    file: &mut File,
    namespace: u16,
    id: &[u8; 16],
) -> io::Result<bool> {
    let length = usize::try_from(file.metadata()?.len())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if length >= CLAIM_HEADER_BYTES {
        return Ok(false);
    }
    file.seek(io::SeekFrom::Start(0))?;
    let mut actual = vec![0_u8; length];
    file.read_exact(&mut actual)?;
    let mut claim_prefix = [0_u8; 26];
    claim_prefix[..8].copy_from_slice(CLAIM_PENDING_MAGIC);
    claim_prefix[8..10].copy_from_slice(&namespace.to_be_bytes());
    claim_prefix[10..].copy_from_slice(id);
    if (actual.len() <= claim_prefix.len() && claim_prefix.starts_with(&actual))
        || (actual.len() > claim_prefix.len() && actual.starts_with(&claim_prefix))
    {
        return Ok(true);
    }
    incomplete_named_bytes_match(&actual, namespace, id)
}

fn incomplete_named_bytes_match(
    actual: &[u8],
    namespace: u16,
    id: &[u8; 16],
) -> io::Result<bool> {
    let mut typed = [0_u8; 26];
    typed[..8].copy_from_slice(TYPED_PENDING_MAGIC);
    typed[8..10].copy_from_slice(&namespace.to_be_bytes());
    typed[10..].copy_from_slice(id);
    if typed.starts_with(actual) {
        return Ok(true);
    }
    if namespace != SNAPSHOT_NAMESPACE {
        return Ok(false);
    }
    let mut legacy = [0_u8; 24];
    legacy[..8].copy_from_slice(PENDING_MAGIC);
    legacy[8..].copy_from_slice(id);
    Ok(legacy.starts_with(actual))
}

fn read_artifact_digests(file: &mut File) -> io::Result<Vec<ArtifactDigest>> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.len() % 32 != 0 {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    bytes
        .chunks_exact(32)
        .map(|chunk| {
            let digest: [u8; 32] = chunk
                .try_into()
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
            Ok(ArtifactDigest::from_sha256(peritus_types::Sha256Digest::new(digest)))
        })
        .collect()
}

fn publication_directory(root: &Path) -> Result<PathBuf, Error> {
    let directory = root.join("publishing");
    fs::create_dir_all(&directory)?;
    if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err(Error::Corrupt("unsafe checkpoint publication directory"));
    }
    Ok(directory)
}

fn publication_name(namespace: u16, id: &[u8; 16]) -> String {
    let identity = id
        .iter()
        .flat_map(|byte| {
            [char::from(HEX[usize::from(byte >> 4)]), char::from(HEX[usize::from(byte & 15)])]
        })
        .collect::<String>();
    format!("{namespace:04x}-{identity}")
}

fn publication_identity(path: &Path) -> Result<(u16, [u8; 16]), Error> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Error::Corrupt("checkpoint publication marker name invalid"))?;
    let (namespace, identity) = name
        .split_once('-')
        .ok_or(Error::Corrupt("checkpoint publication marker name invalid"))?;
    if namespace.len() != 4 || identity.len() != 32 {
        return Err(Error::Corrupt("checkpoint publication marker name invalid"));
    }
    let namespace = u16::from_str_radix(namespace, 16)
        .map_err(|_| Error::Corrupt("checkpoint publication marker namespace invalid"))?;
    if !supported_namespace(namespace) {
        return Err(Error::Corrupt("unsupported snapshot publication namespace"));
    }
    let mut id = [0_u8; 16];
    for (slot, pair) in id.iter_mut().zip(identity.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair)
            .map_err(|_| Error::Corrupt("checkpoint publication marker identity invalid"))?;
        *slot = u8::from_str_radix(pair, 16)
            .map_err(|_| Error::Corrupt("checkpoint publication marker identity invalid"))?;
    }
    if publication_name(namespace, &id) != name {
        return Err(Error::Corrupt("checkpoint publication marker name is not canonical"));
    }
    Ok((namespace, id))
}

pub(in crate::product_control::storage) const fn control_namespace(namespace: u16) -> bool {
    matches!(
        namespace,
        super::control::OPERATION_PAGES
            | super::control::CHECKPOINT_PROJECTION_PAGES
            | super::control::CONTROL_CORE_PAGES
            | super::control::REPLAY_PROJECTION_PAGES
            | super::control::CONTROL_RECORD_PAGES
    )
}

const fn supported_namespace(namespace: u16) -> bool {
    namespace == SNAPSHOT_NAMESPACE
        || namespace == RESTORE_EVIDENCE_NAMESPACE
        || namespace == REPLY_DESCRIPTOR_NAMESPACE
        || namespace == IMAGE_DESCRIPTOR_NAMESPACE
        || namespace == FILE_DESCRIPTOR_NAMESPACE
        || control_namespace(namespace)
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        OpenOptions::new().write(true).custom_flags(0x0200_0000).open(path)?.sync_all()
    }
}
