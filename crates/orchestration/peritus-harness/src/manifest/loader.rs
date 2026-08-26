//! Exact manifest and component loading through `ReadOnlyWorkspace` only.

use std::collections::{BTreeMap, BTreeSet};

use peritus_artifact_store::{
    ArtifactDigest as StoreArtifactDigest, ArtifactStore, EncryptionMetadata, FinalizedArtifact,
    MediaType as StoreMediaType, WriteRequest,
};
use peritus_patch::WorkspacePath;
use peritus_types::EventId;
use peritus_workspace::{ReadOnlyWorkspace, WorkspaceEntryKind};

use crate::domain::{
    CheckedHarnessGraph, ComponentContents, ComponentId, HarnessLimits, HarnessRevision,
    VerifiedComponentContent,
};

use super::{HarnessManifest, ManifestError, ManifestErrorKind, inventory::component_inventory};

const MANIFEST_PATH: &str = ".peritus-harness/manifest.toml";

/// Strict loaded manifest plus exact verified opaque component bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedHarness {
    manifest: HarnessManifest,
    contents: BTreeMap<ComponentId, Vec<u8>>,
}

/// Exact loaded bytes paired with the fully validated graph and component-content witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedLoadedHarness {
    manifest: HarnessManifest,
    graph: CheckedHarnessGraph,
    contents: ComponentContents,
}

impl LoadedHarness {
    /// Returns the strict checked manifest.
    #[must_use]
    pub const fn manifest(&self) -> &HarnessManifest {
        &self.manifest
    }

    /// Returns exact verified opaque bytes for one declared component.
    #[must_use]
    pub fn component_bytes(&self, id: &ComponentId) -> Option<&[u8]> {
        self.contents.get(id).map(Vec::as_slice)
    }

    /// Returns the number of exact verified components.
    #[must_use]
    pub fn component_count(&self) -> usize {
        self.contents.len()
    }

    /// Runs complete graph validation and constructs the exact component-content witness.
    ///
    /// # Errors
    /// Rejects graph, dependency, compatibility, authority, feature, protection, or content
    /// relationships that cannot form a checked harness revision.
    pub fn check(mut self) -> Result<CheckedLoadedHarness, ManifestError> {
        let graph = CheckedHarnessGraph::check(
            self.manifest.declarations().to_vec(),
            self.manifest.environment(),
            self.manifest.limits(),
        )?;
        let verified = graph
            .declarations()
            .iter()
            .map(|declaration| {
                let bytes = self.contents.remove(declaration.id()).ok_or_else(|| {
                    ManifestError::new(
                        ManifestErrorKind::MissingEntry,
                        "verified component bytes disappeared before graph assembly",
                    )
                })?;
                VerifiedComponentContent::new(declaration, bytes).map_err(Into::into)
            })
            .collect::<Result<Vec<_>, ManifestError>>()?;
        let contents = ComponentContents::new(&graph, verified)?;
        Ok(CheckedLoadedHarness { manifest: self.manifest, graph, contents })
    }
}

impl CheckedLoadedHarness {
    /// Returns the strict exact-byte manifest.
    #[must_use]
    pub const fn manifest(&self) -> &HarnessManifest {
        &self.manifest
    }
    /// Returns the complete checked graph.
    #[must_use]
    pub const fn graph(&self) -> &CheckedHarnessGraph {
        &self.graph
    }
    /// Returns the exact component-content witness.
    #[must_use]
    pub const fn contents(&self) -> &ComponentContents {
        &self.contents
    }

    /// Finalizes every exact component payload and verifies all executable roots in C0.
    ///
    /// The creating event may be reserved before registration; an interrupted registration leaves
    /// only ordinary unreferenced C0 artifacts eligible for the artifact-store recovery/GC policy.
    ///
    /// # Errors
    /// Rejects any artifact metadata, streaming, digest, size, publication, or executable-root
    /// verification failure.
    pub fn finalize_artifacts(
        &self,
        store: &ArtifactStore,
        creating_event: EventId,
    ) -> Result<Vec<FinalizedArtifact>, ManifestError> {
        let mut finalized = Vec::with_capacity(self.graph.declarations().len());
        for declaration in self.graph.declarations() {
            let content = self.contents.content(declaration.id()).ok_or_else(|| {
                ManifestError::new(
                    ManifestErrorKind::MissingEntry,
                    "checked graph content is absent during artifact finalization",
                )
            })?;
            let digest = StoreArtifactDigest::from_sha256(declaration.content_digest());
            let media_type = StoreMediaType::new(declaration.media_type().as_str().to_owned())
                .map_err(artifact_store)?;
            let request = WriteRequest::new(
                digest,
                declaration.byte_length(),
                declaration.byte_length().max(1),
                media_type,
                EncryptionMetadata::unencrypted(),
                creating_event,
            );
            let mut writer = store.begin_write(request).map_err(artifact_store)?;
            writer.write_chunk(content.bytes()).map_err(artifact_store)?;
            finalized.push(writer.finalize().map_err(artifact_store)?);
            if let Some(executable) = declaration.executable_artifact_digest() {
                store
                    .verify(StoreArtifactDigest::from_sha256(executable.digest()))
                    .map_err(artifact_store)?;
            }
        }
        Ok(finalized)
    }

    /// Constructs the immutable genesis revision bound to the exact loaded manifest and content.
    ///
    /// # Errors
    /// Rejects any graph/content disagreement detected by the revision constructor.
    pub fn genesis(&self) -> Result<HarnessRevision, ManifestError> {
        HarnessRevision::genesis(
            self.manifest.lineage_seed(),
            self.manifest.digest(),
            self.graph.clone(),
            &self.contents,
        )
        .map_err(Into::into)
    }

    /// Constructs a protected-invariant direct successor from the loaded manifest and content.
    ///
    /// # Errors
    /// Rejects a lineage, revision, protected-asset, graph, or content disagreement.
    pub fn successor(
        &self,
        predecessor: &HarnessRevision,
    ) -> Result<HarnessRevision, ManifestError> {
        HarnessRevision::successor(
            predecessor,
            self.manifest.digest(),
            self.graph.clone(),
            &self.contents,
        )
        .map_err(Into::into)
    }
}

/// Loads the only harness entry point and requires exact declared/inventoried file equality.
///
/// Production loading uses only C1 no-follow metadata, directory listing, and bounded file reads.
///
/// # Errors
/// Rejects missing, symlinked, special, oversized, duplicate, undeclared, size-mismatched, or
/// digest-mismatched entries and every strict schema/domain error.
pub fn load_harness(
    workspace: &ReadOnlyWorkspace,
    compiled_limits: HarnessLimits,
) -> Result<LoadedHarness, ManifestError> {
    let manifest_path = WorkspacePath::new(MANIFEST_PATH.to_owned())
        .map_err(|_| invalid(MANIFEST_PATH, "manifest path is not representable by C1"))?;
    let metadata = workspace.metadata(&manifest_path).map_err(workspace_error)?;
    if metadata.kind() != WorkspaceEntryKind::File {
        return Err(invalid(MANIFEST_PATH, "manifest entry is not a no-follow regular file"));
    }
    if metadata.size() > compiled_limits.max_manifest_bytes() {
        return Err(invalid(MANIFEST_PATH, "manifest exceeds the compiled byte ceiling"));
    }
    let bytes =
        workspace.read_file(&manifest_path, metadata.size().max(1)).map_err(workspace_error)?;
    let manifest = HarnessManifest::parse(&bytes, compiled_limits)?;
    let inventory = component_inventory(workspace, manifest.limits())?;
    let declared = manifest
        .declarations()
        .iter()
        .map(|declaration| declaration.source_path().as_str())
        .collect::<BTreeSet<_>>();
    if declared.len() != manifest.declarations().len() {
        return Err(ManifestError::new(
            ManifestErrorKind::DuplicateEntry,
            "manifest repeats a component source path",
        ));
    }
    let observed = inventory.iter().map(WorkspacePath::as_str).collect::<BTreeSet<_>>();
    if let Some(path) = declared.difference(&observed).next() {
        return Err(ManifestError::at(
            ManifestErrorKind::MissingEntry,
            *path,
            "declared component file is absent from exact inventory",
        ));
    }
    if let Some(path) = observed.difference(&declared).next() {
        return Err(ManifestError::at(
            ManifestErrorKind::UndeclaredEntry,
            *path,
            "component inventory contains an undeclared file",
        ));
    }
    let mut contents = BTreeMap::new();
    for declaration in manifest.declarations() {
        let path =
            WorkspacePath::new(declaration.source_path().as_str().to_owned()).map_err(|_| {
                invalid(
                    declaration.source_path().as_str(),
                    "source path is not representable by C1",
                )
            })?;
        let bytes = workspace
            .read_file(&path, declaration.byte_length().max(1))
            .map_err(workspace_error)?;
        let actual_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if actual_size != declaration.byte_length() {
            return Err(ManifestError::at(
                ManifestErrorKind::SizeMismatch,
                path.as_str(),
                "component bytes disagree with the declared byte length",
            ));
        }
        if peritus_codec::sha256(&bytes) != declaration.content_digest() {
            return Err(ManifestError::at(
                ManifestErrorKind::DigestMismatch,
                path.as_str(),
                "component bytes disagree with the declared SHA-256 digest",
            ));
        }
        if contents.insert(declaration.id().clone(), bytes).is_some() {
            return Err(ManifestError::at(
                ManifestErrorKind::DuplicateEntry,
                path.as_str(),
                "manifest repeats a component identity",
            ));
        }
    }
    Ok(LoadedHarness { manifest, contents })
}

fn workspace_error(error: impl core::fmt::Display) -> ManifestError {
    ManifestError::new(ManifestErrorKind::Workspace, error.to_string())
}

fn artifact_store(error: impl core::fmt::Display) -> ManifestError {
    ManifestError::new(ManifestErrorKind::ArtifactStore, error.to_string())
}

fn invalid(path: &str, detail: &'static str) -> ManifestError {
    ManifestError::at(ManifestErrorKind::UnsafeEntry, path, detail)
}
