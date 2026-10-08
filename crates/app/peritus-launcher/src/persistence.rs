//! Immutable-generation product state and atomic local publication.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_state::{InstallIdentity, ProductState, ProductStateError};
use sha2::{Digest as _, Sha256};

use crate::LauncherError;

pub struct ProductStateStore {
    root: PathBuf,
}

const FRONTIER_HEADER: &str = "peritus-product-state-frontier-v1";

#[derive(Clone)]
struct StateFrontier {
    store_id: String,
    actor_id: String,
    committed_generation: u64,
    committed_digest: Option<String>,
    reserved_generation: u64,
    reserved_digest: Option<String>,
}

struct StoredState {
    state: ProductState,
    bytes: Vec<u8>,
}

struct StateCandidate {
    generation: u64,
    path: PathBuf,
    pending: bool,
}

impl ProductStateStore {
    pub fn open(root: PathBuf) -> Result<Self, LauncherError> {
        fs::create_dir_all(&root).map_err(|error| {
            LauncherError::filesystem("create product-state directory", &root, error)
        })?;
        protect_directory(&root)?;
        Ok(Self { root })
    }

    pub fn load_or_initialize(&self) -> Result<ProductState, LauncherError> {
        let mut state = match self.load_from_frontier() {
            Ok(Some(state)) => self.reconcile_adjacent_generation(state)?,
            Ok(None) => self.recover_inventory()?,
            Err(error) => {
                eprintln!("peritus launcher: product-state frontier requires recovery: {error}");
                self.recover_inventory()?
            }
        };
        if state.migrate_legacy_storage()? {
            self.commit(&state)?;
        }
        Ok(state)
    }

    pub fn commit(&self, state: &ProductState) -> Result<(), LauncherError> {
        let bytes = state.canonical_json()?;
        let digest = digest_hex(&bytes);
        let mut frontier = match self.read_frontier()? {
            Some(frontier) => frontier,
            None => self.initialize_frontier(state, &bytes)?,
        };
        if !frontier.matches_identity(state.identity()) {
            return Err(LauncherError::PlatformPaths(
                "product-state frontier belongs to a different installation identity".to_owned(),
            ));
        }
        if state.generation() == frontier.committed_generation
            && frontier.committed_digest.as_deref() == Some(digest.as_str())
        {
            return Ok(());
        }
        if state.generation() <= frontier.reserved_generation {
            return Err(LauncherError::PlatformPaths(format!(
                "product-state generation {} does not advance beyond reserved generation {}",
                state.generation(), frontier.reserved_generation,
            )));
        }
        let final_path = self.generation_path(state.generation());
        match fs::read(&final_path) {
            Ok(existing) if existing == bytes => {
                frontier.settle(state, digest);
                self.write_frontier(&frontier)?;
                return Ok(());
            }
            Ok(_) => {
                return Err(LauncherError::PlatformPaths(format!(
                    "product-state generation {} already exists with different content",
                    state.generation()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(LauncherError::filesystem(
                    "read product-state generation",
                    &final_path,
                    error,
                ));
            }
        }
        frontier.reserve(state, digest.clone());
        self.write_frontier(&frontier)?;
        publish_new(&self.pending_generation_path(state.generation()), &final_path, &bytes)?;
        frontier.settle(state, digest);
        self.write_frontier(&frontier)?;
        Ok(())
    }

    fn load_from_frontier(&self) -> Result<Option<ProductState>, LauncherError> {
        let Some(mut frontier) = self.read_frontier()? else {
            return Ok(None);
        };
        let committed = if frontier.committed_generation == 0 {
            None
        } else {
            let Some(digest) = frontier.committed_digest.as_deref() else {
                return Err(LauncherError::PlatformPaths(
                    "product-state frontier has no committed digest".to_owned(),
                ));
            };
            self.read_matching_candidate(
                &self.generation_path(frontier.committed_generation),
                frontier.committed_generation,
                digest,
            )?
        };
        if frontier.committed_generation > 0 && committed.is_none() {
            return Ok(None);
        }
        if let Some(committed) = &committed
            && !frontier.matches_identity(committed.state.identity())
        {
            return Ok(None);
        }
        if frontier.reserved_generation == frontier.committed_generation {
            return Ok(committed.map(|stored| stored.state));
        }

        let mut reserved = None;
        let mut pending_basis = None;
        if let Some(digest) = frontier.reserved_digest.as_deref() {
            reserved = self.read_matching_candidate(
                &self.generation_path(frontier.reserved_generation),
                frontier.reserved_generation,
                digest,
            )?;
            if reserved.is_none() {
                let pending = self.pending_generation_path(frontier.reserved_generation);
                if let Some(candidate) = self.read_matching_candidate(
                    &pending,
                    frontier.reserved_generation,
                    digest,
                )? {
                    let final_path = self.generation_path(frontier.reserved_generation);
                    match publish_new(&pending, &final_path, &candidate.bytes) {
                        Ok(()) => reserved = Some(candidate),
                        Err(_) if final_path.exists() => pending_basis = Some(candidate),
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        if let Some(candidate) = reserved.filter(|candidate| {
            frontier.matches_identity(candidate.state.identity())
        }) {
            frontier.settle(&candidate.state, digest_hex(&candidate.bytes));
            self.write_frontier(&frontier)?;
            return Ok(Some(candidate.state));
        }

        let mut recovery = if let Some(pending) = pending_basis.filter(|candidate| {
            frontier.matches_identity(candidate.state.identity())
        }) {
            pending.state
        } else if let Some(committed) = committed {
            committed.state
        } else {
            ProductState::new(InstallIdentity::parse(&frontier.store_id, &frontier.actor_id)?)
        };
        let generation = next_generation(frontier.reserved_generation)?;
        recovery.advance_generation_to(generation)?;
        self.commit(&recovery)?;
        Ok(Some(recovery))
    }

    fn recover_inventory(&self) -> Result<ProductState, LauncherError> {
        let (observed, candidates) = self.inventory()?;
        let mut best_final = None;
        let mut best_pending = None;
        for candidate in candidates {
            if candidate.pending && best_pending.is_some()
                || !candidate.pending && best_final.is_some()
            {
                continue;
            }
            let Some(stored) = self.read_candidate(&candidate.path, candidate.generation)? else {
                continue;
            };
            if candidate.pending {
                best_pending = Some((candidate, stored));
            } else {
                best_final = Some((candidate, stored));
            }
            if best_final.is_some() && best_pending.is_some() {
                break;
            }
        }

        if let Some((pending, stored)) = best_pending
            && best_final.as_ref().is_none_or(|(_, current)| {
                stored.state.generation() > current.state.generation()
                    && stored.state.identity() == current.state.identity()
            })
        {
            let final_path = self.generation_path(stored.state.generation());
            if !final_path.exists() {
                publish_new(&pending.path, &final_path, &stored.bytes)?;
                best_final = Some((
                    StateCandidate {
                        generation: stored.state.generation(),
                        path: final_path,
                        pending: false,
                    },
                    stored,
                ));
            } else {
                let frontier = best_final.as_ref().map_or_else(
                    || StateFrontier::empty(stored.state.identity(), observed),
                    |(_, current)| {
                        StateFrontier::from_committed(
                            &current.state,
                            digest_hex(&current.bytes),
                            observed,
                        )
                    },
                );
                self.write_frontier(&frontier)?;
                let mut recovery = stored.state;
                recovery.advance_generation_to(next_generation(observed)?)?;
                self.commit(&recovery)?;
                return Ok(recovery);
            }
        }

        if let Some((_, stored)) = best_final {
            let frontier = StateFrontier::from_committed(
                &stored.state,
                digest_hex(&stored.bytes),
                observed,
            );
            self.write_frontier(&frontier)?;
            if observed > stored.state.generation() {
                let mut recovery = stored.state;
                recovery.advance_generation_to(next_generation(observed)?)?;
                self.commit(&recovery)?;
                return Ok(recovery);
            }
            return Ok(stored.state);
        }

        let mut state = ProductState::new(crate::identity::generate()?);
        let frontier = StateFrontier::empty(state.identity(), observed);
        self.write_frontier(&frontier)?;
        if observed >= state.generation() {
            state.advance_generation_to(next_generation(observed)?)?;
        }
        self.commit(&state)?;
        Ok(state)
    }

    fn reconcile_adjacent_generation(
        &self,
        mut state: ProductState,
    ) -> Result<ProductState, LauncherError> {
        loop {
            let adjacent = next_generation(state.generation())?;
            let final_path = self.generation_path(adjacent);
            let pending_path = self.pending_generation_path(adjacent);
            let final_exists = final_path.exists();
            let pending_exists = pending_path.exists();
            if !final_exists && !pending_exists {
                return Ok(state);
            }
            let mut candidate = if final_exists {
                self.read_candidate(&final_path, adjacent)?
            } else {
                None
            };
            if candidate.is_none() && pending_exists {
                candidate = self.read_candidate(&pending_path, adjacent)?;
            }
            let Some(candidate) = candidate.filter(|candidate| {
                candidate.state.identity() == state.identity()
            }) else {
                let mut frontier = self.read_frontier()?.ok_or_else(|| {
                    LauncherError::PlatformPaths(
                        "product-state frontier disappeared during recovery".to_owned(),
                    )
                })?;
                frontier.reserved_generation = adjacent;
                frontier.reserved_digest = None;
                self.write_frontier(&frontier)?;
                state.advance_generation_to(next_generation(adjacent)?)?;
                self.commit(&state)?;
                return Ok(state);
            };
            if !final_exists {
                publish_new(&pending_path, &final_path, &candidate.bytes)?;
            } else if self.read_candidate(&final_path, adjacent)?.is_none() {
                let mut frontier = self.read_frontier()?.ok_or_else(|| {
                    LauncherError::PlatformPaths(
                        "product-state frontier disappeared during recovery".to_owned(),
                    )
                })?;
                frontier.reserved_generation = adjacent;
                frontier.reserved_digest = Some(digest_hex(&candidate.bytes));
                self.write_frontier(&frontier)?;
                state = candidate.state;
                state.advance_generation_to(next_generation(adjacent)?)?;
                self.commit(&state)?;
                return Ok(state);
            }
            state = candidate.state;
            let mut frontier = self.read_frontier()?.ok_or_else(|| {
                LauncherError::PlatformPaths(
                    "product-state frontier disappeared during recovery".to_owned(),
                )
            })?;
            frontier.settle(&state, digest_hex(&candidate.bytes));
            self.write_frontier(&frontier)?;
        }
    }

    fn inventory(&self) -> Result<(u64, Vec<StateCandidate>), LauncherError> {
        let entries = fs::read_dir(&self.root).map_err(|error| {
            LauncherError::filesystem("list product-state generations", &self.root, error)
        })?;
        let mut observed = 0_u64;
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!(
                        "peritus launcher: skipped an unreadable product-state directory entry: {error}"
                    );
                    continue;
                }
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(generation) = parse_generation(&name) {
                observed = observed.max(generation);
                candidates.push(StateCandidate {
                    generation,
                    path: entry.path(),
                    pending: false,
                });
            } else if let Some(generation) = parse_pending_generation(&name) {
                observed = observed.max(generation);
                candidates.push(StateCandidate {
                    generation,
                    path: entry.path(),
                    pending: true,
                });
            } else if name == "state.pending"
                && let Some(stored) = self.read_unbound_candidate(&entry.path())?
            {
                observed = observed.max(stored.state.generation());
                candidates.push(StateCandidate {
                    generation: stored.state.generation(),
                    path: entry.path(),
                    pending: true,
                });
            }
        }
        candidates.sort_unstable_by(|left, right| {
            right
                .generation
                .cmp(&left.generation)
                .then_with(|| left.pending.cmp(&right.pending))
        });
        Ok((observed, candidates))
    }

    fn generation_path(&self, generation: u64) -> PathBuf {
        self.root.join(format!("state-{generation:020}.json"))
    }

    fn pending_generation_path(&self, generation: u64) -> PathBuf {
        self.root.join(format!("state-{generation:020}.pending"))
    }

    fn frontier_path(&self) -> PathBuf {
        self.root.join("frontier")
    }

    fn read_frontier(&self) -> Result<Option<StateFrontier>, LauncherError> {
        let path = self.frontier_path();
        match fs::read(&path) {
            Ok(bytes) => StateFrontier::parse(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(LauncherError::filesystem("read product-state frontier", path, error)),
        }
    }

    fn write_frontier(&self, frontier: &StateFrontier) -> Result<(), LauncherError> {
        let path = self.frontier_path();
        let bytes = frontier.encode();
        let actual = read_exact_or_publish(&path, &bytes)?;
        if actual != bytes {
            replace_recovery_file(&path, &bytes)?;
        }
        Ok(())
    }

    fn initialize_frontier(
        &self,
        state: &ProductState,
        bytes: &[u8],
    ) -> Result<StateFrontier, LauncherError> {
        let observed = self.observed_generation()?;
        let path = self.generation_path(state.generation());
        let frontier = match fs::read(&path) {
            Ok(existing) if existing == bytes => StateFrontier::from_committed(
                state,
                digest_hex(bytes),
                observed,
            ),
            Ok(_) => StateFrontier::empty(state.identity(), observed),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                StateFrontier::empty(state.identity(), observed)
            }
            Err(error) => {
                return Err(LauncherError::filesystem(
                    "read product-state generation",
                    path,
                    error,
                ));
            }
        };
        self.write_frontier(&frontier)?;
        Ok(frontier)
    }

    fn observed_generation(&self) -> Result<u64, LauncherError> {
        self.inventory().map(|(observed, _)| observed)
    }

    fn read_candidate(
        &self,
        path: &Path,
        generation: u64,
    ) -> Result<Option<StoredState>, LauncherError> {
        let Some(stored) = self.read_unbound_candidate(path)? else {
            return Ok(None);
        };
        if stored.state.generation() != generation {
            eprintln!(
                "peritus launcher: skipped product-state candidate {} because filename generation {generation} differs from payload generation {}",
                path.display(), stored.state.generation(),
            );
            return Ok(None);
        }
        Ok(Some(stored))
    }

    fn read_matching_candidate(
        &self,
        path: &Path,
        generation: u64,
        digest: &str,
    ) -> Result<Option<StoredState>, LauncherError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(LauncherError::filesystem(
                    "read product-state candidate",
                    path,
                    error,
                ));
            }
        };
        if digest_hex(&bytes) != digest {
            return Ok(None);
        }
        let state = match ProductState::parse_json(&bytes) {
            Ok(state) if state.generation() == generation => state,
            _ => return Ok(None),
        };
        Ok(Some(StoredState { state, bytes }))
    }

    fn read_unbound_candidate(
        &self,
        path: &Path,
    ) -> Result<Option<StoredState>, LauncherError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                eprintln!(
                    "peritus launcher: skipped unreadable product-state candidate {}: {error}",
                    path.display(),
                );
                return Ok(None);
            }
        };
        match ProductState::parse_json(&bytes) {
            Ok(state) => Ok(Some(StoredState { state, bytes })),
            Err(error) => {
                eprintln!(
                    "peritus launcher: skipped invalid product-state candidate {}: {error}",
                    path.display(),
                );
                Ok(None)
            }
        }
    }
}

impl StateFrontier {
    fn empty(identity: &InstallIdentity, reserved_generation: u64) -> Self {
        Self {
            store_id: identity.store_id().to_owned(),
            actor_id: identity.actor_id().to_owned(),
            committed_generation: 0,
            committed_digest: None,
            reserved_generation,
            reserved_digest: None,
        }
    }

    fn from_committed(
        state: &ProductState,
        digest: String,
        observed_generation: u64,
    ) -> Self {
        let mut frontier = Self::empty(state.identity(), observed_generation);
        frontier.committed_generation = state.generation();
        frontier.committed_digest = Some(digest.clone());
        if observed_generation == state.generation() {
            frontier.reserved_digest = Some(digest);
        }
        frontier
    }

    fn reserve(&mut self, state: &ProductState, digest: String) {
        self.reserved_generation = state.generation();
        self.reserved_digest = Some(digest);
    }

    fn settle(&mut self, state: &ProductState, digest: String) {
        self.store_id = state.identity().store_id().to_owned();
        self.actor_id = state.identity().actor_id().to_owned();
        self.committed_generation = state.generation();
        self.committed_digest = Some(digest.clone());
        self.reserved_generation = state.generation();
        self.reserved_digest = Some(digest);
    }

    fn matches_identity(&self, identity: &InstallIdentity) -> bool {
        self.store_id == identity.store_id() && self.actor_id == identity.actor_id()
    }

    fn encode(&self) -> Vec<u8> {
        format!(
            "{FRONTIER_HEADER}\nstore={}\nactor={}\ncommitted-generation={}\ncommitted-sha256={}\nreserved-generation={}\nreserved-sha256={}\n",
            self.store_id,
            self.actor_id,
            self.committed_generation,
            self.committed_digest.as_deref().unwrap_or("-"),
            self.reserved_generation,
            self.reserved_digest.as_deref().unwrap_or("-"),
        )
        .into_bytes()
    }

    fn parse(bytes: &[u8]) -> Result<Self, LauncherError> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            LauncherError::PlatformPaths("product-state frontier is not UTF-8".to_owned())
        })?;
        let mut lines = text.lines();
        if lines.next() != Some(FRONTIER_HEADER) {
            return Err(LauncherError::PlatformPaths(
                "product-state frontier has an unsupported header".to_owned(),
            ));
        }
        let store_id = frontier_value(lines.next(), "store=")?.to_owned();
        let actor_id = frontier_value(lines.next(), "actor=")?.to_owned();
        let committed_generation = frontier_value(lines.next(), "committed-generation=")?
            .parse::<u64>()
            .map_err(|_| frontier_invalid())?;
        let committed_digest = frontier_digest(frontier_value(
            lines.next(),
            "committed-sha256=",
        )?)?;
        let reserved_generation = frontier_value(lines.next(), "reserved-generation=")?
            .parse::<u64>()
            .map_err(|_| frontier_invalid())?;
        let reserved_digest = frontier_digest(frontier_value(
            lines.next(),
            "reserved-sha256=",
        )?)?;
        if lines.next().is_some()
            || InstallIdentity::parse(&store_id, &actor_id).is_err()
            || committed_generation > reserved_generation
            || (committed_generation == 0) != committed_digest.is_none()
            || (reserved_generation == 0 && reserved_digest.is_some())
            || (committed_generation == reserved_generation
                && committed_digest != reserved_digest)
        {
            return Err(frontier_invalid());
        }
        Ok(Self {
            store_id,
            actor_id,
            committed_generation,
            committed_digest,
            reserved_generation,
            reserved_digest,
        })
    }
}

fn frontier_value<'a>(line: Option<&'a str>, prefix: &str) -> Result<&'a str, LauncherError> {
    line.and_then(|line| line.strip_prefix(prefix)).ok_or_else(frontier_invalid)
}

fn frontier_digest(value: &str) -> Result<Option<String>, LauncherError> {
    if value == "-" {
        return Ok(None);
    }
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(Some(value.to_owned()))
    } else {
        Err(frontier_invalid())
    }
}

fn frontier_invalid() -> LauncherError {
    LauncherError::PlatformPaths("product-state frontier is malformed".to_owned())
}

fn next_generation(generation: u64) -> Result<u64, LauncherError> {
    generation.checked_add(1).ok_or_else(|| {
        ProductStateError::GenerationExhausted { generation }.into()
    })
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut value, byte| {
        use std::fmt::Write as _;
        write!(value, "{byte:02x}").expect("writing to String cannot fail");
        value
    })
}

pub fn publish_new(
    pending_path: &Path,
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), LauncherError> {
    let _lock = PublicationLock::acquire(final_path)?;
    publish_new_locked(pending_path, final_path, bytes)
}

pub fn read_exact_or_publish(path: &Path, bytes: &[u8]) -> Result<Vec<u8>, LauncherError> {
    let _lock = PublicationLock::acquire(path)?;
    match fs::read(path) {
        Ok(existing) => Ok(existing),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let pending = path.with_extension("pending");
            publish_new_locked(&pending, path, bytes)?;
            Ok(bytes.to_vec())
        }
        Err(error) => Err(LauncherError::filesystem("read durable file", path, error)),
    }
}

/// Replaces one application-owned mutable recovery file and synchronizes it before returning.
pub fn replace_recovery_file(path: &Path, bytes: &[u8]) -> Result<(), LauncherError> {
    let _lock = PublicationLock::acquire(path)?;
    let existing = fs::read(path)
        .map_err(|error| LauncherError::filesystem("open recovery file", path, error))?;
    if existing == bytes {
        return Ok(());
    }
    let pending = sidecar(path, "replacement.pending")?;
    prepare_pending(&pending, bytes)?;
    atomic_replace(&pending, path)
        .map_err(|error| LauncherError::filesystem("replace recovery file", path, error))?;
    sync_parent(path)
}

fn publish_new_locked(
    pending_path: &Path,
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), LauncherError> {
    match fs::read(final_path) {
        Ok(existing) if existing == bytes => {
            remove_matching_pending(pending_path, bytes)?;
            return Ok(());
        }
        Ok(_) => {
            return Err(LauncherError::PlatformPaths(format!(
                "durable file already exists with different content: {}",
                final_path.display(),
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(LauncherError::filesystem(
                "read durable publication target",
                final_path,
                error,
            ));
        }
    }
    prepare_pending(pending_path, bytes)?;
    match fs::hard_link(pending_path, final_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(final_path).map_err(|read_error| {
                LauncherError::filesystem(
                    "read concurrently published durable file",
                    final_path,
                    read_error,
                )
            })?;
            if existing != bytes {
                return Err(LauncherError::PlatformPaths(format!(
                    "durable file was concurrently published with different content: {}",
                    final_path.display(),
                )));
            }
        }
        Err(error) => {
            return Err(LauncherError::filesystem(
                "publish durable file",
                final_path,
                error,
            ));
        }
    }
    fs::remove_file(pending_path).map_err(|error| {
        LauncherError::filesystem("retire completed pending publication", pending_path, error)
    })?;
    sync_parent(final_path)
}

fn prepare_pending(path: &Path, bytes: &[u8]) -> Result<(), LauncherError> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|error| {
                    LauncherError::filesystem("open pending publication", path, error)
                })?;
            protect_file(&file, path)?;
            file.sync_all().map_err(|error| {
                LauncherError::filesystem("synchronize pending publication", path, error)
            })?;
            return Ok(());
        }
        Ok(existing) => quarantine_pending(path, &existing)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(LauncherError::filesystem(
                "read pending publication",
                path,
                error,
            ));
        }
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| LauncherError::filesystem("create pending publication", path, error))?;
    protect_file(&file, path)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| LauncherError::filesystem("write pending publication", path, error))
}

fn quarantine_pending(path: &Path, bytes: &[u8]) -> Result<(), LauncherError> {
    let quarantine = sidecar(path, &format!("rejected-{}", digest_hex(bytes)))?;
    match fs::read(&quarantine) {
        Ok(existing) if existing == bytes => fs::remove_file(path).map_err(|error| {
            LauncherError::filesystem("retire duplicate rejected publication", path, error)
        })?,
        Ok(_) => {
            return Err(LauncherError::PlatformPaths(format!(
                "rejected publication digest conflicts at {}",
                quarantine.display(),
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::rename(path, &quarantine).map_err(|rename_error| {
                LauncherError::filesystem(
                    "retain rejected pending publication",
                    &quarantine,
                    rename_error,
                )
            })?;
        }
        Err(error) => {
            return Err(LauncherError::filesystem(
                "inspect rejected pending publication",
                quarantine,
                error,
            ));
        }
    }
    sync_parent(path)
}

fn remove_matching_pending(path: &Path, bytes: &[u8]) -> Result<(), LauncherError> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => {
            fs::remove_file(path).map_err(|error| {
                LauncherError::filesystem("retire completed pending publication", path, error)
            })?;
            sync_parent(path)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(LauncherError::filesystem(
            "inspect completed pending publication",
            path,
            error,
        )),
    }
}

struct PublicationLock {
    file: File,
}

impl PublicationLock {
    fn acquire(path: &Path) -> Result<Self, LauncherError> {
        let lock_path = sidecar(path, "publication.lock")?;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| {
                LauncherError::filesystem("open durable publication lock", &lock_path, error)
            })?;
        protect_file(&file, &lock_path)?;
        fs4::FileExt::lock(&file).map_err(|error| {
            LauncherError::filesystem("lock durable publication", &lock_path, error)
        })?;
        Ok(Self { file })
    }
}

impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

fn sidecar(path: &Path, suffix: &str) -> Result<PathBuf, LauncherError> {
    let name = path.file_name().ok_or_else(|| {
        LauncherError::PlatformPaths("durable publication path has no file name".to_owned())
    })?;
    let mut sidecar = name.to_os_string();
    sidecar.push(".");
    sidecar.push(suffix);
    Ok(path.with_file_name(sidecar))
}

#[cfg(unix)]
fn atomic_replace(candidate: &Path, current: &Path) -> std::io::Result<()> {
    fs::rename(candidate, current)
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "atomic recovery-file replacement uses the documented Windows move primitive"
)]
fn atomic_replace(candidate: &Path, current: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let candidate = candidate.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let current = current.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: both encoded paths are NUL-terminated and remain live for this synchronous call.
    if unsafe {
        MoveFileExW(
            candidate.as_ptr(),
            current.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn parse_generation(name: &str) -> Option<u64> {
    name.strip_prefix("state-")?.strip_suffix(".json")?.parse().ok()
}

fn parse_pending_generation(name: &str) -> Option<u64> {
    name.strip_prefix("state-")?.strip_suffix(".pending")?.parse().ok()
}

#[cfg(unix)]
fn protect_directory(path: &Path) -> Result<(), LauncherError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| LauncherError::filesystem("protect product-state directory", path, error))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the platform implementations behind one fallible directory-protection contract"
)]
const fn protect_directory(_path: &Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(unix)]
pub fn protect_file(file: &File, path: &Path) -> Result<(), LauncherError> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| LauncherError::filesystem("protect durable file", path, error))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the platform implementations behind one fallible file-protection contract"
)]
pub const fn protect_file(_file: &File, _path: &Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), LauncherError> {
    let parent = path.parent().ok_or_else(|| {
        LauncherError::PlatformPaths("durable publication has no parent directory".to_owned())
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| LauncherError::filesystem("synchronize durable directory", parent, error))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the platform implementations behind one fallible durable-sync contract"
)]
const fn sync_parent(_path: &Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_invalid_generation_falls_back_to_last_valid_state() {
        let root = tempfile::tempdir().expect("state root");
        let store = ProductStateStore::open(root.path().to_owned()).expect("store");
        let expected = ProductState::new(crate::identity::generate().expect("identity"));
        store.commit(&expected).expect("valid generation");
        fs::write(root.path().join("state-18000000000000000000.json"), b"{broken")
            .expect("newer corrupt generation");

        let recovered = store.load_or_initialize().expect("fallback state");

        assert_eq!(recovered.generation(), expected.generation());
        assert_eq!(recovered.identity(), expected.identity());
    }
}
