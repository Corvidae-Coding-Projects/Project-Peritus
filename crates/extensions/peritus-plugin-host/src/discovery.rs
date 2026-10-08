//! Incremental filesystem discovery and exact artifact identification.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
};

use peritus_plugin_sdk::{ManifestDigest, PluginId, PluginManifest, PluginVersion};
use sha2::{Digest as _, Sha256};

use crate::{HostCancellation, HostError, HostFailureClass, RecoveryDisposition};

const MANIFEST_NAME: &str = "peritus-plugin.toml";
const HASH_BUFFER_BYTES: usize = 64 * 1024;
const MAXIMUM_PAGE_ENTRIES: usize = 128;

/// Finite allocation admission for the SDK's version-selecting TOML manifest parser.
///
/// [`PluginManifest::parse_toml`] retains SDK-supported legacy and current manifest formats. The
/// byte limit controls the input allocation made before that parser runs; it does not redefine
/// which SDK manifests are valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdkTomlManifestAdmission(NonZeroU64);

impl SdkTomlManifestAdmission {
    /// Selects the SDK TOML parser with a caller-supplied finite input-allocation bound.
    ///
    /// # Errors
    ///
    /// Rejects a zero allocation bound.
    pub fn new(maximum_bytes: u64) -> Result<Self, HostError> {
        NonZeroU64::new(maximum_bytes).map(Self).ok_or_else(|| {
            discovery_policy_error("SDK TOML manifest admission must allow at least one byte")
        })
    }

    /// Returns the caller-selected manifest input bound.
    #[must_use]
    pub const fn maximum_bytes(self) -> u64 {
        self.0.get()
    }

    fn parse(self, input: &str) -> Result<PluginManifest, peritus_plugin_sdk::SdkError> {
        PluginManifest::parse_toml(input)
    }
}

/// Independent total-byte policy for streamed executable or module artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactAdmission {
    maximum_bytes: Option<NonZeroU64>,
}

impl ArtifactAdmission {
    /// Uses fixed-buffer streaming without an arbitrary total artifact-size cutoff.
    #[must_use]
    pub const fn streamed() -> Self {
        Self { maximum_bytes: None }
    }

    /// Uses fixed-buffer streaming plus an explicit caller-selected total-byte maximum.
    ///
    /// # Errors
    ///
    /// Rejects a zero maximum.
    pub fn maximum_bytes(maximum_bytes: u64) -> Result<Self, HostError> {
        NonZeroU64::new(maximum_bytes)
            .map(|maximum_bytes| Self { maximum_bytes: Some(maximum_bytes) })
            .ok_or_else(|| {
                discovery_policy_error("finite artifact admission must allow at least one byte")
            })
    }

    /// Returns the optional caller-selected total-byte maximum.
    #[must_use]
    pub const fn total_byte_limit(self) -> Option<u64> {
        match self.maximum_bytes {
            Some(maximum_bytes) => Some(maximum_bytes.get()),
            None => None,
        }
    }
}

/// Caller-selected manifest allocation and artifact streaming policies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryLimits {
    manifest: SdkTomlManifestAdmission,
    artifact: ArtifactAdmission,
}

impl DiscoveryLimits {
    /// Combines explicit manifest and artifact admission policies.
    #[must_use]
    pub const fn new(
        manifest: SdkTomlManifestAdmission,
        artifact: ArtifactAdmission,
    ) -> Self {
        Self { manifest, artifact }
    }

    /// Returns the selected SDK TOML manifest admission policy.
    #[must_use]
    pub const fn manifest(self) -> SdkTomlManifestAdmission {
        self.manifest
    }

    /// Returns the selected artifact streaming policy.
    #[must_use]
    pub const fn artifact(self) -> ArtifactAdmission {
        self.artifact
    }
}

/// Validated upper bound for one incremental discovery page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryPageSize(NonZeroUsize);

impl DiscoveryPageSize {
    /// Largest supported page. This bounds transient candidate and diagnostic allocation only.
    pub const MAXIMUM: usize = MAXIMUM_PAGE_ENTRIES;

    /// Creates a nonzero bounded page size.
    ///
    /// # Errors
    ///
    /// Rejects zero or a value above [`Self::MAXIMUM`].
    pub fn new(value: usize) -> Result<Self, HostError> {
        if value == 0 || value > Self::MAXIMUM {
            return Err(HostError::new(
                HostFailureClass::Discovery,
                RecoveryDisposition::CorrectRequest,
                "configure plugin discovery page",
                format!(
                    "discovery page size must be between one and {} entries",
                    Self::MAXIMUM
                ),
            ));
        }
        let value = NonZeroUsize::new(value).ok_or_else(|| {
            HostError::new(
                HostFailureClass::Discovery,
                RecoveryDisposition::CorrectRequest,
                "configure plugin discovery page",
                "discovery page size must be nonzero",
            )
        })?;
        Ok(Self(value))
    }

    /// Returns the admitted entry count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

/// Filesystem scope associated with a recoverable discovery diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryDiagnosticScope {
    /// A configured discovery root could not be inspected reliably.
    Root,
    /// One candidate beneath a usable root was rejected.
    Plugin,
}

/// Bounded failure information retained alongside successful discoveries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryDiagnostic {
    scope: DiscoveryDiagnosticScope,
    root: PathBuf,
    candidate: Option<PathBuf>,
    class: HostFailureClass,
    recovery: RecoveryDisposition,
    operation: &'static str,
    detail: String,
}

impl DiscoveryDiagnostic {
    /// Returns whether the diagnostic applies to a root or one plugin candidate.
    #[must_use]
    pub const fn scope(&self) -> DiscoveryDiagnosticScope {
        self.scope
    }

    /// Borrows the configured root associated with the failure.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Borrows the candidate path when the failure applies to one entry.
    #[must_use]
    pub fn candidate(&self) -> Option<&Path> {
        self.candidate.as_deref()
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn class(&self) -> HostFailureClass {
        self.class
    }

    /// Returns the safe recovery disposition.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryDisposition {
        self.recovery
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    fn from_error(
        scope: DiscoveryDiagnosticScope,
        root: PathBuf,
        candidate: Option<PathBuf>,
        error: &HostError,
    ) -> Self {
        Self {
            scope,
            root,
            candidate,
            class: error.class(),
            recovery: error.recovery(),
            operation: error.operation(),
            detail: error.detail().to_owned(),
        }
    }
}

/// Continuation state after one discovery page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryStatus {
    /// More roots or entries remain.
    More,
    /// Every configured root was examined.
    Complete,
    /// Cancellation was observed; calling again with an active token resumes at the same entry.
    Cancelled,
}

/// Progress and new diagnostics produced by one bounded discovery step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryPage {
    status: DiscoveryStatus,
    examined: usize,
    discovered: usize,
    diagnostics: Vec<DiscoveryDiagnostic>,
}

impl DiscoveryPage {
    /// Returns whether discovery can continue, completed, or observed cancellation.
    #[must_use]
    pub const fn status(&self) -> DiscoveryStatus {
        self.status
    }

    /// Returns the number of roots and directory entries charged to this page.
    #[must_use]
    pub const fn examined(&self) -> usize {
        self.examined
    }

    /// Returns the number of plugin versions newly accepted by this page.
    #[must_use]
    pub const fn discovered(&self) -> usize {
        self.discovered
    }

    /// Borrows diagnostics first observed by this page.
    #[must_use]
    pub fn diagnostics(&self) -> &[DiscoveryDiagnostic] {
        &self.diagnostics
    }
}

/// Validated manifest plus the already-open artifact whose bytes were identified.
#[derive(Clone, Debug)]
pub struct DiscoveredPlugin {
    manifest: PluginManifest,
    manifest_path: PathBuf,
    root: PathBuf,
    artifact_path: PathBuf,
    manifest_digest: ManifestDigest,
    artifact_sha256: [u8; 32],
    artifact_bytes: u64,
    artifact: Arc<StdMutex<File>>,
}

impl DiscoveredPlugin {
    /// Borrows the validated manifest.
    #[must_use]
    pub const fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Borrows the exact manifest path used for discovery.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Borrows the canonical plugin directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Borrows the discovered artifact path for provenance.
    ///
    /// Launches use a separately retained custody object copied from the already-open artifact.
    #[must_use]
    pub fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    /// Returns the complete canonical manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> ManifestDigest {
        self.manifest_digest
    }

    /// Returns the exact artifact SHA-256 observed through the retained file handle.
    #[must_use]
    pub const fn artifact_sha256(&self) -> [u8; 32] {
        self.artifact_sha256
    }

    pub(crate) const fn artifact_bytes(&self) -> u64 {
        self.artifact_bytes
    }

    pub(crate) fn copy_verified_artifact(&self, destination: &mut File) -> Result<(), HostError> {
        let mut source = self.artifact.lock().map_err(|_| {
            artifact_changed("retained plugin artifact handle is unavailable")
        })?;
        source
            .seek(SeekFrom::Start(0))
            .map_err(|error| artifact_source("rewind retained plugin artifact", error))?;
        destination.set_len(0).map_err(|error| {
            custody_source("truncate staged plugin artifact", error)
        })?;
        destination.seek(SeekFrom::Start(0)).map_err(|error| {
            custody_source("rewind staged plugin artifact", error)
        })?;

        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = vec![0_u8; HASH_BUFFER_BYTES].into_boxed_slice();
        loop {
            let count = source
                .read(&mut buffer)
                .map_err(|error| artifact_source("read retained plugin artifact", error))?;
            if count == 0 {
                break;
            }
            let count_u64 = u64::try_from(count)
                .map_err(|_| artifact_changed("plugin artifact read size is not representable"))?;
            total = total
                .checked_add(count_u64)
                .ok_or_else(|| artifact_changed("plugin artifact size overflowed"))?;
            if total > self.artifact_bytes {
                return Err(artifact_changed(
                    "plugin artifact grew after its discovery identity was established",
                ));
            }
            destination.write_all(&buffer[..count]).map_err(|error| {
                custody_source("write staged plugin artifact", error)
            })?;
            hasher.update(&buffer[..count]);
        }
        destination
            .flush()
            .map_err(|error| custody_source("flush staged plugin artifact", error))?;
        source
            .seek(SeekFrom::Start(0))
            .map_err(|error| artifact_source("restore retained plugin artifact", error))?;
        let digest: [u8; 32] = hasher.finalize().into();
        if total != self.artifact_bytes || digest != self.artifact_sha256 {
            return Err(artifact_changed(
                "plugin artifact bytes changed after discovery and before custody",
            ));
        }
        Ok(())
    }
}

/// Canonical duplicate-free discovered plugin catalog plus recoverable diagnostics.
#[derive(Clone, Debug, Default)]
pub struct PluginCatalog {
    entries: BTreeMap<(PluginId, PluginVersion), DiscoveredPlugin>,
    diagnostics: Vec<DiscoveryDiagnostic>,
}

impl PluginCatalog {
    /// Looks up one exact plugin version.
    #[must_use]
    pub fn get(&self, id: &PluginId, version: PluginVersion) -> Option<&DiscoveredPlugin> {
        self.entries.get(&(id.clone(), version))
    }

    /// Iterates plugins in canonical identity/version order.
    pub fn iter(
        &self,
    ) -> std::collections::btree_map::Values<'_, (PluginId, PluginVersion), DiscoveredPlugin> {
        self.entries.values()
    }

    /// Borrows all root and candidate diagnostics in stable discovery order.
    #[must_use]
    pub fn diagnostics(&self) -> &[DiscoveryDiagnostic] {
        &self.diagnostics
    }

    /// Returns the number of discovered versions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no plugins were discovered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<'catalog> IntoIterator for &'catalog PluginCatalog {
    type Item = &'catalog DiscoveredPlugin;
    type IntoIter =
        std::collections::btree_map::Values<'catalog, (PluginId, PluginVersion), DiscoveredPlugin>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.values()
    }
}

struct ActiveRoot {
    configured: PathBuf,
    canonical: PathBuf,
    cursor: Option<OsString>,
}

struct CandidateSelection {
    entries: Vec<(OsString, PathBuf)>,
    has_more: bool,
}

enum ScanFailure {
    Cancelled,
    Failed(HostError),
}

/// Resumable, deterministic discovery over caller-supplied roots.
///
/// Roots are ordered once. Each page rescans only the active root while retaining at most the
/// requested number of lexically smallest entries beyond its cursor, so directory cardinality
/// does not become a catalog-wide allocation or admission ceiling.
pub struct PluginDiscovery {
    roots: Vec<PathBuf>,
    limits: DiscoveryLimits,
    next_root: usize,
    active_root: Option<ActiveRoot>,
    catalog: PluginCatalog,
    complete: bool,
}

impl PluginDiscovery {
    /// Creates an incremental discovery session in stable root order.
    #[must_use]
    pub fn new(
        roots: impl IntoIterator<Item = PathBuf>,
        limits: DiscoveryLimits,
    ) -> Self {
        let mut roots: Vec<_> = roots.into_iter().collect();
        roots.sort_unstable();
        let complete = roots.is_empty();
        Self {
            roots,
            limits,
            next_root: 0,
            active_root: None,
            catalog: PluginCatalog::default(),
            complete,
        }
    }

    /// Borrows the recoverable catalog accumulated so far.
    #[must_use]
    pub const fn catalog(&self) -> &PluginCatalog {
        &self.catalog
    }

    /// Consumes the session and returns every accepted entry and diagnostic accumulated so far.
    #[must_use]
    pub fn into_catalog(self) -> PluginCatalog {
        self.catalog
    }

    /// Examines at most one admitted page of roots and entries.
    ///
    /// Cancellation preserves the current entry cursor, allowing a later call with an active token
    /// to resume. Root, I/O, manifest, duplicate, and artifact failures become diagnostics; other
    /// roots and candidates remain discoverable.
    #[must_use]
    pub fn next_page(
        &mut self,
        page_size: DiscoveryPageSize,
        cancellation: &HostCancellation,
    ) -> DiscoveryPage {
        if self.complete {
            return DiscoveryPage {
                status: DiscoveryStatus::Complete,
                examined: 0,
                discovered: 0,
                diagnostics: Vec::new(),
            };
        }
        if cancellation.is_cancelled() {
            return DiscoveryPage {
                status: DiscoveryStatus::Cancelled,
                examined: 0,
                discovered: 0,
                diagnostics: Vec::new(),
            };
        }

        let maximum = page_size.get();
        let mut examined = 0_usize;
        let mut discovered = 0_usize;
        let mut diagnostics = Vec::new();

        while examined < maximum {
            if cancellation.is_cancelled() {
                return DiscoveryPage {
                    status: DiscoveryStatus::Cancelled,
                    examined,
                    discovered,
                    diagnostics,
                };
            }

            if self.active_root.is_none() {
                let Some(configured) = self.roots.get(self.next_root).cloned() else {
                    self.complete = true;
                    break;
                };
                self.next_root += 1;
                examined += 1;
                match activate_root(&configured) {
                    Ok(active) => self.active_root = Some(active),
                    Err(error) => {
                        self.record_error(
                            &mut diagnostics,
                            DiscoveryDiagnosticScope::Root,
                            configured,
                            None,
                            &error,
                        );
                    }
                }
                if examined == maximum {
                    break;
                }
                if self.active_root.is_none() {
                    continue;
                }
            }

            let capacity = maximum - examined;
            let selection = {
                let active = self.active_root.as_ref().expect("active discovery root");
                select_candidates(active, capacity, cancellation)
            };
            let selection = match selection {
                Ok(selection) => selection,
                Err(ScanFailure::Cancelled) => {
                    return DiscoveryPage {
                        status: DiscoveryStatus::Cancelled,
                        examined,
                        discovered,
                        diagnostics,
                    };
                }
                Err(ScanFailure::Failed(error)) => {
                    let active = self.active_root.take().expect("active discovery root");
                    self.record_error(
                        &mut diagnostics,
                        DiscoveryDiagnosticScope::Root,
                        active.configured,
                        None,
                        &error,
                    );
                    continue;
                }
            };
            let root_complete = !selection.has_more;
            let configured = self
                .active_root
                .as_ref()
                .expect("active discovery root")
                .configured
                .clone();
            let canonical = self
                .active_root
                .as_ref()
                .expect("active discovery root")
                .canonical
                .clone();

            for (name, candidate) in selection.entries {
                match load_candidate(&canonical, &candidate, self.limits, cancellation) {
                    Ok(Some(plugin)) => {
                        let key = (
                            plugin.manifest().id().clone(),
                            plugin.manifest().version(),
                        );
                        if let Some(existing) = self.catalog.entries.get(&key) {
                            let error = discovery_error(format!(
                                "duplicate plugin identity and version; first manifest is {}",
                                existing.manifest_path().display()
                            ));
                            self.record_error(
                                &mut diagnostics,
                                DiscoveryDiagnosticScope::Plugin,
                                configured.clone(),
                                Some(candidate.clone()),
                                &error,
                            );
                        } else {
                            self.catalog.entries.insert(key, plugin);
                            discovered += 1;
                        }
                    }
                    Ok(None) => {}
                    Err(ScanFailure::Cancelled) => {
                        return DiscoveryPage {
                            status: DiscoveryStatus::Cancelled,
                            examined,
                            discovered,
                            diagnostics,
                        };
                    }
                    Err(ScanFailure::Failed(error)) => {
                        self.record_error(
                            &mut diagnostics,
                            DiscoveryDiagnosticScope::Plugin,
                            configured.clone(),
                            Some(candidate.clone()),
                            &error,
                        );
                    }
                }
                self.active_root
                    .as_mut()
                    .expect("active discovery root")
                    .cursor = Some(name);
                examined += 1;
            }
            if root_complete {
                self.active_root = None;
            }
        }

        if self.active_root.is_none() && self.next_root == self.roots.len() {
            self.complete = true;
        }
        DiscoveryPage {
            status: if self.complete {
                DiscoveryStatus::Complete
            } else {
                DiscoveryStatus::More
            },
            examined,
            discovered,
            diagnostics,
        }
    }

    fn record_error(
        &mut self,
        page: &mut Vec<DiscoveryDiagnostic>,
        scope: DiscoveryDiagnosticScope,
        root: PathBuf,
        candidate: Option<PathBuf>,
        error: &HostError,
    ) {
        let diagnostic = DiscoveryDiagnostic::from_error(scope, root, candidate, error);
        self.catalog.diagnostics.push(diagnostic.clone());
        page.push(diagnostic);
    }
}

fn activate_root(configured: &Path) -> Result<ActiveRoot, HostError> {
    reject_symlink(configured, "plugin discovery root is a symbolic link")?;
    let canonical = fs::canonicalize(configured)
        .map_err(|error| discovery_source("canonicalize plugin discovery root", error))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| discovery_source("inspect plugin discovery root", error))?;
    if !metadata.is_dir() {
        return Err(discovery_error("plugin discovery root is not a directory"));
    }
    Ok(ActiveRoot {
        configured: configured.to_path_buf(),
        canonical,
        cursor: None,
    })
}

fn select_candidates(
    root: &ActiveRoot,
    maximum: usize,
    cancellation: &HostCancellation,
) -> Result<CandidateSelection, ScanFailure> {
    let entries = fs::read_dir(&root.canonical).map_err(|error| {
        ScanFailure::Failed(discovery_source("read plugin discovery root", error))
    })?;
    let mut selected = BTreeMap::new();
    let mut has_more = false;
    for entry in entries {
        if cancellation.is_cancelled() {
            return Err(ScanFailure::Cancelled);
        }
        let entry = entry.map_err(|error| {
            ScanFailure::Failed(discovery_source("enumerate plugin discovery root", error))
        })?;
        let name = entry.file_name();
        if root.cursor.as_ref().is_some_and(|cursor| &name <= cursor) {
            continue;
        }
        selected.insert(name, entry.path());
        if selected.len() > maximum {
            let _ = selected.pop_last();
            has_more = true;
        }
    }
    Ok(CandidateSelection { entries: selected.into_iter().collect(), has_more })
}

fn load_candidate(
    root: &Path,
    directory: &Path,
    limits: DiscoveryLimits,
    cancellation: &HostCancellation,
) -> Result<Option<DiscoveredPlugin>, ScanFailure> {
    check_cancellation(cancellation)?;
    let directory_metadata = fs::symlink_metadata(directory).map_err(|error| {
        ScanFailure::Failed(discovery_source("inspect plugin directory entry", error))
    })?;
    if directory_metadata.file_type().is_symlink() {
        return Err(ScanFailure::Failed(discovery_error(
            "plugin directory is a symbolic link",
        )));
    }
    if !directory_metadata.is_dir() {
        return Ok(None);
    }
    let canonical_directory = fs::canonicalize(directory).map_err(|error| {
        ScanFailure::Failed(discovery_source("canonicalize plugin directory", error))
    })?;
    if !canonical_directory.starts_with(root) {
        return Err(ScanFailure::Failed(discovery_error(
            "plugin directory escapes its discovery root",
        )));
    }

    let manifest_path = canonical_directory.join(MANIFEST_NAME);
    let manifest_metadata = match fs::symlink_metadata(&manifest_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ScanFailure::Failed(discovery_source(
                "inspect plugin manifest",
                error,
            )));
        }
    };
    if manifest_metadata.file_type().is_symlink() {
        return Err(ScanFailure::Failed(discovery_error(
            "plugin manifest is a symbolic link",
        )));
    }
    let mut manifest_file = File::open(&manifest_path).map_err(|error| {
        ScanFailure::Failed(discovery_source("open plugin manifest", error))
    })?;
    let opened_manifest = manifest_file.metadata().map_err(|error| {
        ScanFailure::Failed(discovery_source("inspect open plugin manifest", error))
    })?;
    if !opened_manifest.is_file()
        || opened_manifest.len() > limits.manifest().maximum_bytes()
    {
        return Err(ScanFailure::Failed(discovery_error(
            "plugin manifest is not a bounded regular file",
        )));
    }
    let manifest_bytes = read_bounded(
        &mut manifest_file,
        opened_manifest.len(),
        limits.manifest().maximum_bytes(),
        cancellation,
    )?;
    let manifest_text = String::from_utf8(manifest_bytes).map_err(|error| {
        ScanFailure::Failed(HostError::with_source(
            HostFailureClass::Discovery,
            RecoveryDisposition::CorrectRequest,
            "decode plugin manifest",
            error.to_string(),
            error,
        ))
    })?;
    let manifest = limits.manifest().parse(&manifest_text).map_err(|error| {
        ScanFailure::Failed(HostError::with_source(
            HostFailureClass::Discovery,
            RecoveryDisposition::CorrectRequest,
            "parse plugin manifest",
            error.to_string(),
            error,
        ))
    })?;
    let manifest_digest = manifest.digest().map_err(|error| {
        ScanFailure::Failed(HostError::with_source(
            HostFailureClass::Discovery,
            RecoveryDisposition::CorrectRequest,
            "digest plugin manifest",
            error.to_string(),
            error,
        ))
    })?;

    let unresolved_artifact = canonical_directory.join(manifest.entrypoint().artifact());
    reject_symlink(&unresolved_artifact, "plugin artifact is a symbolic link")
        .map_err(ScanFailure::Failed)?;
    let artifact_path = fs::canonicalize(&unresolved_artifact).map_err(|error| {
        ScanFailure::Failed(discovery_source("canonicalize plugin artifact", error))
    })?;
    if !artifact_path.starts_with(&canonical_directory) {
        return Err(ScanFailure::Failed(discovery_error(
            "plugin artifact escapes its plugin directory",
        )));
    }
    let mut artifact = File::open(&artifact_path).map_err(|error| {
        ScanFailure::Failed(discovery_source("open plugin artifact", error))
    })?;
    let artifact_metadata = artifact.metadata().map_err(|error| {
        ScanFailure::Failed(discovery_source("inspect open plugin artifact", error))
    })?;
    let artifact_maximum = limits.artifact().total_byte_limit();
    if !artifact_metadata.is_file()
        || artifact_maximum.is_some_and(|maximum| artifact_metadata.len() > maximum)
    {
        return Err(ScanFailure::Failed(discovery_error(
            "plugin artifact is not a bounded regular file",
        )));
    }
    let (artifact_sha256, artifact_bytes) =
        hash_artifact(&mut artifact, artifact_maximum, cancellation)?;
    artifact.seek(SeekFrom::Start(0)).map_err(|error| {
        ScanFailure::Failed(discovery_source("rewind plugin artifact", error))
    })?;

    Ok(Some(DiscoveredPlugin {
        manifest,
        manifest_path,
        root: canonical_directory,
        artifact_path,
        manifest_digest,
        artifact_sha256,
        artifact_bytes,
        artifact: Arc::new(StdMutex::new(artifact)),
    }))
}

fn read_bounded(
    file: &mut File,
    advertised: u64,
    maximum: u64,
    cancellation: &HostCancellation,
) -> Result<Vec<u8>, ScanFailure> {
    let capacity = usize::try_from(advertised).map_err(|_| {
        ScanFailure::Failed(discovery_error(
            "plugin manifest allocation size is not representable",
        ))
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut total = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        check_cancellation(cancellation)?;
        let count = file.read(&mut buffer).map_err(|error| {
            ScanFailure::Failed(discovery_source("read plugin manifest", error))
        })?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| {
            ScanFailure::Failed(discovery_error(
                "plugin manifest read size is not representable",
            ))
        })?;
        total = total.checked_add(count_u64).ok_or_else(|| {
            ScanFailure::Failed(discovery_error("plugin manifest size overflowed"))
        })?;
        if total > maximum {
            return Err(ScanFailure::Failed(discovery_error(
                "plugin manifest exceeds its byte bound",
            )));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}

fn hash_artifact(
    file: &mut File,
    maximum: Option<u64>,
    cancellation: &HostCancellation,
) -> Result<([u8; 32], u64), ScanFailure> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES].into_boxed_slice();
    loop {
        check_cancellation(cancellation)?;
        let count = file.read(&mut buffer).map_err(|error| {
            ScanFailure::Failed(discovery_source("hash plugin artifact", error))
        })?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| {
            ScanFailure::Failed(discovery_error(
                "plugin artifact read size is not representable",
            ))
        })?;
        total = total.checked_add(count_u64).ok_or_else(|| {
            ScanFailure::Failed(discovery_error("plugin artifact size overflowed"))
        })?;
        if maximum.is_some_and(|maximum| total > maximum) {
            return Err(ScanFailure::Failed(discovery_error(
                "plugin artifact exceeds its byte bound",
            )));
        }
        hasher.update(&buffer[..count]);
    }
    Ok((hasher.finalize().into(), total))
}

fn check_cancellation(cancellation: &HostCancellation) -> Result<(), ScanFailure> {
    if cancellation.is_cancelled() { Err(ScanFailure::Cancelled) } else { Ok(()) }
}

fn reject_symlink(path: &Path, detail: &'static str) -> Result<(), HostError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| discovery_source("inspect plugin path", error))?;
    if metadata.file_type().is_symlink() {
        Err(discovery_error(detail))
    } else {
        Ok(())
    }
}

fn discovery_policy_error(detail: impl Into<String>) -> HostError {
    HostError::new(
        HostFailureClass::Discovery,
        RecoveryDisposition::CorrectRequest,
        "configure plugin discovery policy",
        detail,
    )
}

fn discovery_error(detail: impl Into<String>) -> HostError {
    HostError::new(
        HostFailureClass::Discovery,
        RecoveryDisposition::CorrectRequest,
        "discover plugins",
        detail,
    )
}

fn discovery_source(operation: &'static str, error: std::io::Error) -> HostError {
    HostError::with_source(
        HostFailureClass::Discovery,
        RecoveryDisposition::CorrectRequest,
        operation,
        error.to_string(),
        error,
    )
}

fn artifact_changed(detail: impl Into<String>) -> HostError {
    HostError::new(
        HostFailureClass::Trust,
        RecoveryDisposition::EstablishTrust,
        "bind plugin artifact",
        detail,
    )
}

fn artifact_source(operation: &'static str, error: std::io::Error) -> HostError {
    HostError::with_source(
        HostFailureClass::Trust,
        RecoveryDisposition::EstablishTrust,
        operation,
        error.to_string(),
        error,
    )
}

fn custody_source(operation: &'static str, error: std::io::Error) -> HostError {
    HostError::with_source(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::Reconcile,
        operation,
        error.to_string(),
        error,
    )
}
