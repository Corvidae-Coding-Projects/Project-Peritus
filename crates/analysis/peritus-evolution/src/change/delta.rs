//! Exact evolvable E1 component operations.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    identity::{digest_parts, push_bytes},
};
use peritus_harness::domain::{
    ArtifactDigest, ComponentDeclaration, ComponentId, ComponentKind, ProtectionClass,
};
use peritus_types::Sha256Digest;

/// Declared schema/runtime compatibility effect of one component change.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CompatibilityEffect {
    /// Existing consumers remain compatible.
    Compatible,
    /// Promotion requires the cited migration artifact.
    RequiresMigration,
    /// Candidate cannot be promoted under the frozen policy.
    Incompatible,
}

/// Exact component-set operation represented by a delta.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ComponentDeltaOperation {
    /// A declaration absent from the baseline is present in the candidate.
    Add,
    /// A declaration present in the baseline is absent from the candidate.
    Remove,
    /// The same stable declaration identity and kind changes in place.
    Update,
}

impl ComponentDeltaOperation {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Add => 1,
            Self::Remove => 2,
            Self::Update => 3,
        }
    }

    pub(crate) const fn from_tag(tag: u8) -> Result<Self, EvolutionError> {
        match tag {
            1 => Ok(Self::Add),
            2 => Ok(Self::Remove),
            3 => Ok(Self::Update),
            _ => Err(persisted_error("persisted component operation tag is unknown")),
        }
    }
}

/// One exact declaration-side snapshot bound by a version-two component delta.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentDeltaSide {
    content: Sha256Digest,
    executable: Option<Sha256Digest>,
    dependencies: Option<Sha256Digest>,
    declaration: Option<Sha256Digest>,
}

impl ComponentDeltaSide {
    pub(crate) const fn from_exact_parts(
        content: Sha256Digest,
        executable: Option<Sha256Digest>,
        dependencies: Option<Sha256Digest>,
        declaration: Option<Sha256Digest>,
    ) -> Self {
        Self { content, executable, dependencies, declaration }
    }

    fn capture(declaration: &ComponentDeclaration) -> Self {
        Self {
            content: declaration.content_digest(),
            executable: executable(declaration),
            dependencies: Some(dependency_digest(declaration)),
            declaration: Some(declaration_digest(declaration)),
        }
    }

    /// Exact source-content digest.
    #[must_use]
    pub const fn content(self) -> Sha256Digest {
        self.content
    }
    /// Independently bound executable artifact, when declared.
    #[must_use]
    pub const fn executable(self) -> Option<Sha256Digest> {
        self.executable
    }
    /// Digest of every ordered dependency requirement, when version-two bound.
    #[must_use]
    pub const fn dependencies_digest(self) -> Option<Sha256Digest> {
        self.dependencies
    }
    /// Digest of the complete declaration semantics, when version-two bound.
    #[must_use]
    pub const fn declaration_digest(self) -> Option<Sha256Digest> {
        self.declaration
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeltaEncoding {
    LegacyReplace,
    OperationV2,
}

/// Exact add, remove, or update operation for one evolvable component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDelta {
    component_id: ComponentId,
    kind: ComponentKind,
    operation: ComponentDeltaOperation,
    before: Option<ComponentDeltaSide>,
    after: Option<ComponentDeltaSide>,
    semantic_diff_artifact: Sha256Digest,
    compatibility: CompatibilityEffect,
    migration_artifact: Option<Sha256Digest>,
    encoding: DeltaEncoding,
    digest: Sha256Digest,
}

impl ComponentDelta {
    /// Captures the legacy content-replacement schema byte-for-byte.
    ///
    /// New writers should use [`Self::capture_update`]. This constructor remains available so
    /// historical command construction and accepted receipts retain their exact version-one bytes.
    ///
    /// # Errors
    /// Rejects identity/kind drift, equal declarations, protected components, equal content, or a
    /// migration classification without a migration artifact.
    pub fn capture(
        before: &ComponentDeclaration,
        after: &ComponentDeclaration,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
    ) -> Result<Self, EvolutionError> {
        if before.id() != after.id()
            || before.kind() != after.kind()
            || before == after
            || before.protection_class() != ProtectionClass::Evolvable
            || after.protection_class() != ProtectionClass::Evolvable
            || before.content_digest() == after.content_digest()
            || !migration_matches(compatibility, migration_artifact)
        {
            return Err(input_error(
                "legacy component replacement is equal, protected, mismatched, or lacks migration evidence",
            ));
        }
        Self::from_exact_parts(
            before.id().clone(),
            before.kind(),
            before.content_digest(),
            after.content_digest(),
            executable(before),
            executable(after),
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
        )
    }

    /// Captures a complete in-place update, including executable and dependency-only changes.
    ///
    /// # Errors
    /// Rejects identity/kind drift, equal declarations, protected components, or missing migration
    /// evidence.
    pub fn capture_update(
        before: &ComponentDeclaration,
        after: &ComponentDeclaration,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
    ) -> Result<Self, EvolutionError> {
        if before.id() != after.id()
            || before.kind() != after.kind()
            || before == after
            || before.protection_class() != ProtectionClass::Evolvable
            || after.protection_class() != ProtectionClass::Evolvable
            || !migration_matches(compatibility, migration_artifact)
        {
            return Err(input_error(
                "component update is equal, protected, mismatched, or lacks migration evidence",
            ));
        }
        Self::operation(
            before.id().clone(),
            before.kind(),
            ComponentDeltaOperation::Update,
            Some(ComponentDeltaSide::capture(before)),
            Some(ComponentDeltaSide::capture(after)),
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            false,
        )
    }

    /// Captures a declaration that is actually absent from the baseline graph.
    ///
    /// # Errors
    /// Rejects protected declarations or missing migration evidence.
    pub fn capture_addition(
        after: &ComponentDeclaration,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
    ) -> Result<Self, EvolutionError> {
        if after.protection_class() != ProtectionClass::Evolvable
            || !migration_matches(compatibility, migration_artifact)
        {
            return Err(input_error(
                "component addition is protected or lacks migration evidence",
            ));
        }
        Self::operation(
            after.id().clone(),
            after.kind(),
            ComponentDeltaOperation::Add,
            None,
            Some(ComponentDeltaSide::capture(after)),
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            false,
        )
    }

    /// Captures a declaration that is actually absent from the candidate graph.
    ///
    /// # Errors
    /// Rejects protected declarations or missing migration evidence.
    pub fn capture_removal(
        before: &ComponentDeclaration,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
    ) -> Result<Self, EvolutionError> {
        if before.protection_class() != ProtectionClass::Evolvable
            || !migration_matches(compatibility, migration_artifact)
        {
            return Err(input_error(
                "component removal is protected or lacks migration evidence",
            ));
        }
        Self::operation(
            before.id().clone(),
            before.kind(),
            ComponentDeltaOperation::Remove,
            Some(ComponentDeltaSide::capture(before)),
            None,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            false,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "legacy receipt fields stay exact")]
    pub(crate) fn from_exact_parts(
        component_id: ComponentId,
        kind: ComponentKind,
        before_content: Sha256Digest,
        after_content: Sha256Digest,
        before_executable: Option<Sha256Digest>,
        after_executable: Option<Sha256Digest>,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
    ) -> Result<Self, EvolutionError> {
        if kind.protection_class() != ProtectionClass::Evolvable
            || before_content == after_content
            || !migration_matches(compatibility, migration_artifact)
        {
            return Err(persisted_error(
                "persisted legacy component replacement is protected, equal, or lacks migration evidence",
            ));
        }
        let before = ComponentDeltaSide::from_exact_parts(
            before_content,
            before_executable,
            None,
            None,
        );
        let after = ComponentDeltaSide::from_exact_parts(
            after_content,
            after_executable,
            None,
            None,
        );
        let digest = legacy_delta_digest(
            &component_id,
            kind,
            before_content,
            after_content,
            before_executable,
            after_executable,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
        );
        Ok(Self {
            component_id,
            kind,
            operation: ComponentDeltaOperation::Update,
            before: Some(before),
            after: Some(after),
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            encoding: DeltaEncoding::LegacyReplace,
            digest,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "all operation and declaration facts stay explicit")]
    pub(crate) fn from_operation_parts(
        component_id: ComponentId,
        kind: ComponentKind,
        operation: ComponentDeltaOperation,
        before: Option<ComponentDeltaSide>,
        after: Option<ComponentDeltaSide>,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
    ) -> Result<Self, EvolutionError> {
        Self::operation(
            component_id,
            kind,
            operation,
            before,
            after,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn operation(
        component_id: ComponentId,
        kind: ComponentKind,
        operation: ComponentDeltaOperation,
        before: Option<ComponentDeltaSide>,
        after: Option<ComponentDeltaSide>,
        semantic_diff_artifact: Sha256Digest,
        compatibility: CompatibilityEffect,
        migration_artifact: Option<Sha256Digest>,
        persisted: bool,
    ) -> Result<Self, EvolutionError> {
        let shape_matches = matches!(
            (operation, before, after),
            (ComponentDeltaOperation::Add, None, Some(_))
                | (ComponentDeltaOperation::Remove, Some(_), None)
                | (ComponentDeltaOperation::Update, Some(_), Some(_))
        );
        let complete = before.into_iter().chain(after).all(|side| {
            side.dependencies_digest().is_some() && side.declaration_digest().is_some()
        });
        let changed = operation != ComponentDeltaOperation::Update || before != after;
        if kind.protection_class() != ProtectionClass::Evolvable
            || !shape_matches
            || !complete
            || !changed
            || !migration_matches(compatibility, migration_artifact)
        {
            return Err(if persisted {
                persisted_error(
                    "persisted component operation is protected, incomplete, equal, or malformed",
                )
            } else {
                input_error("component operation is protected, incomplete, equal, or malformed")
            });
        }
        let digest = operation_delta_digest(
            &component_id,
            kind,
            operation,
            before,
            after,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
        );
        Ok(Self {
            component_id,
            kind,
            operation,
            before,
            after,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            encoding: DeltaEncoding::OperationV2,
            digest,
        })
    }

    pub(crate) fn matches(
        &self,
        before: Option<&ComponentDeclaration>,
        after: Option<&ComponentDeclaration>,
    ) -> bool {
        let identity_matches = before.into_iter().chain(after).all(|declaration| {
            declaration.id() == &self.component_id
                && declaration.kind() == self.kind
                && declaration.protection_class() == ProtectionClass::Evolvable
        });
        if !identity_matches {
            return false;
        }
        match self.encoding {
            DeltaEncoding::LegacyReplace => {
                let (Some(before), Some(after), Some(expected_before), Some(expected_after)) =
                    (before, after, self.before, self.after)
                else {
                    return false;
                };
                before.content_digest() == expected_before.content()
                    && after.content_digest() == expected_after.content()
                    && executable(before) == expected_before.executable()
                    && executable(after) == expected_after.executable()
            }
            DeltaEncoding::OperationV2 => {
                operation_for(before, after) == Some(self.operation)
                    && before.map(ComponentDeltaSide::capture) == self.before
                    && after.map(ComponentDeltaSide::capture) == self.after
            }
        }
    }

    pub(crate) const fn legacy_encoding(&self) -> bool {
        matches!(self.encoding, DeltaEncoding::LegacyReplace)
    }
    /// Returns the stable E1 component identity.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }
    /// Returns the closed component kind.
    #[must_use]
    pub const fn kind(&self) -> ComponentKind {
        self.kind
    }
    /// Returns the exact component-set operation.
    #[must_use]
    pub const fn operation(&self) -> ComponentDeltaOperation {
        self.operation
    }
    /// Returns the exact baseline-side declaration snapshot when present.
    #[must_use]
    pub const fn before(&self) -> Option<ComponentDeltaSide> {
        self.before
    }
    /// Returns the exact candidate-side declaration snapshot when present.
    #[must_use]
    pub const fn after(&self) -> Option<ComponentDeltaSide> {
        self.after
    }
    /// Returns the prior source-content digest when the declaration existed.
    #[must_use]
    pub const fn before_content(&self) -> Option<Sha256Digest> {
        match self.before {
            Some(value) => Some(value.content()),
            None => None,
        }
    }
    /// Returns the candidate source-content digest when the declaration exists.
    #[must_use]
    pub const fn after_content(&self) -> Option<Sha256Digest> {
        match self.after {
            Some(value) => Some(value.content()),
            None => None,
        }
    }
    /// Returns the prior executable artifact, when declared.
    #[must_use]
    pub const fn before_executable(&self) -> Option<Sha256Digest> {
        match self.before {
            Some(value) => value.executable(),
            None => None,
        }
    }
    /// Returns the candidate executable artifact, when declared.
    #[must_use]
    pub const fn after_executable(&self) -> Option<Sha256Digest> {
        match self.after {
            Some(value) => value.executable(),
            None => None,
        }
    }
    /// Returns the baseline dependency-set digest when version-two bound.
    #[must_use]
    pub const fn before_dependencies(&self) -> Option<Sha256Digest> {
        match self.before {
            Some(value) => value.dependencies_digest(),
            None => None,
        }
    }
    /// Returns the candidate dependency-set digest when version-two bound.
    #[must_use]
    pub const fn after_dependencies(&self) -> Option<Sha256Digest> {
        match self.after {
            Some(value) => value.dependencies_digest(),
            None => None,
        }
    }
    /// Returns the content-addressed semantic diff artifact.
    #[must_use]
    pub const fn semantic_diff_artifact(&self) -> Sha256Digest {
        self.semantic_diff_artifact
    }
    /// Returns the declared compatibility effect.
    #[must_use]
    pub const fn compatibility(&self) -> CompatibilityEffect {
        self.compatibility
    }
    /// Returns the required migration artifact, when any.
    #[must_use]
    pub const fn migration_artifact(&self) -> Option<Sha256Digest> {
        self.migration_artifact
    }
    /// Returns the canonical delta digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn operation_for(
    before: Option<&ComponentDeclaration>,
    after: Option<&ComponentDeclaration>,
) -> Option<ComponentDeltaOperation> {
    match (before, after) {
        (None, Some(_)) => Some(ComponentDeltaOperation::Add),
        (Some(_), None) => Some(ComponentDeltaOperation::Remove),
        (Some(_), Some(_)) => Some(ComponentDeltaOperation::Update),
        (None, None) => None,
    }
}

fn executable(declaration: &ComponentDeclaration) -> Option<Sha256Digest> {
    declaration.executable_artifact_digest().map(ArtifactDigest::digest)
}

fn dependency_digest(declaration: &ComponentDeclaration) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &u64::try_from(declaration.dependencies().len()).unwrap_or(u64::MAX).to_be_bytes(),
    );
    for dependency in declaration.dependencies() {
        push_bytes(&mut bytes, dependency.component_id().as_str().as_bytes());
        bytes.push(dependency.required_kind().tag());
        bytes.extend_from_slice(&dependency.compatible_schema().minimum().get().to_be_bytes());
        bytes.extend_from_slice(&dependency.compatible_schema().maximum().get().to_be_bytes());
        push_optional_digest(&mut bytes, dependency.exact_content_digest());
    }
    digest_parts(b"peritus.f0.component-dependencies.v1\0", &[&bytes])
}

fn declaration_digest(declaration: &ComponentDeclaration) -> Sha256Digest {
    let mut bytes = Vec::new();
    push_bytes(&mut bytes, declaration.id().as_str().as_bytes());
    bytes.push(declaration.kind().tag());
    bytes.extend_from_slice(&declaration.schema_version().get().to_be_bytes());
    push_bytes(&mut bytes, declaration.source_path().as_str().as_bytes());
    push_bytes(&mut bytes, declaration.target_path().as_str().as_bytes());
    push_bytes(&mut bytes, declaration.media_type().as_str().as_bytes());
    bytes.extend_from_slice(&declaration.byte_length().to_be_bytes());
    bytes.extend_from_slice(declaration.content_digest().as_bytes());
    push_optional_digest(&mut bytes, executable(declaration));
    push_bytes(&mut bytes, declaration.owner().as_str().as_bytes());
    push_bytes(&mut bytes, declaration.provenance().as_str().as_bytes());
    bytes.extend_from_slice(dependency_digest(declaration).as_bytes());
    let compatibility = declaration.compatibility();
    bytes.extend_from_slice(&compatibility.supported_schema().minimum().get().to_be_bytes());
    bytes.extend_from_slice(&compatibility.supported_schema().maximum().get().to_be_bytes());
    bytes.extend_from_slice(
        &u64::try_from(compatibility.provider_features().len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for feature in compatibility.provider_features() {
        push_bytes(&mut bytes, feature.as_str().as_bytes());
    }
    bytes.extend_from_slice(
        &u64::try_from(compatibility.platform_features().len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for feature in compatibility.platform_features() {
        push_bytes(&mut bytes, feature.as_str().as_bytes());
    }
    bytes.extend_from_slice(&declaration.declared_authority().bits().to_be_bytes());
    bytes.push(declaration.protection_class().tag());
    digest_parts(b"peritus.f0.component-declaration.v1\0", &[&bytes])
}

#[allow(clippy::too_many_arguments)]
fn legacy_delta_digest(
    component_id: &ComponentId,
    kind: ComponentKind,
    before_content: Sha256Digest,
    after_content: Sha256Digest,
    before_exec: Option<Sha256Digest>,
    after_exec: Option<Sha256Digest>,
    semantic_diff: Sha256Digest,
    compatibility: CompatibilityEffect,
    migration: Option<Sha256Digest>,
) -> Sha256Digest {
    let compatibility = [compatibility_tag(compatibility)];
    digest_parts(
        b"peritus.f0.component-delta.v1\0",
        &[
            component_id.as_str().as_bytes(),
            &[kind.tag()],
            before_content.as_bytes(),
            after_content.as_bytes(),
            before_exec.as_ref().map_or(&[][..], |value| value.as_bytes()),
            after_exec.as_ref().map_or(&[][..], |value| value.as_bytes()),
            semantic_diff.as_bytes(),
            &compatibility,
            migration.as_ref().map_or(&[][..], |value| value.as_bytes()),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn operation_delta_digest(
    component_id: &ComponentId,
    kind: ComponentKind,
    operation: ComponentDeltaOperation,
    before: Option<ComponentDeltaSide>,
    after: Option<ComponentDeltaSide>,
    semantic_diff: Sha256Digest,
    compatibility: CompatibilityEffect,
    migration: Option<Sha256Digest>,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.push(operation.tag());
    push_bytes(&mut bytes, component_id.as_str().as_bytes());
    bytes.push(kind.tag());
    push_side(&mut bytes, before);
    push_side(&mut bytes, after);
    bytes.extend_from_slice(semantic_diff.as_bytes());
    bytes.push(compatibility_tag(compatibility));
    push_optional_digest(&mut bytes, migration);
    digest_parts(b"peritus.f0.component-delta.v2\0", &[&bytes])
}

fn push_side(bytes: &mut Vec<u8>, side: Option<ComponentDeltaSide>) {
    bytes.push(u8::from(side.is_some()));
    if let Some(side) = side {
        bytes.extend_from_slice(side.content().as_bytes());
        push_optional_digest(bytes, side.executable());
        bytes.extend_from_slice(
            side.dependencies_digest().unwrap_or(Sha256Digest::new([0; 32])).as_bytes(),
        );
        bytes.extend_from_slice(
            side.declaration_digest().unwrap_or(Sha256Digest::new([0; 32])).as_bytes(),
        );
    }
}

fn push_optional_digest(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(value.as_bytes());
    }
}

const fn compatibility_tag(value: CompatibilityEffect) -> u8 {
    match value {
        CompatibilityEffect::Compatible => 1,
        CompatibilityEffect::RequiresMigration => 2,
        CompatibilityEffect::Incompatible => 3,
    }
}

const fn migration_matches(
    compatibility: CompatibilityEffect,
    migration: Option<Sha256Digest>,
) -> bool {
    matches!(compatibility, CompatibilityEffect::RequiresMigration) == migration.is_some()
}

const fn input_error(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Contamination,
        EvolutionOperation::AdmitManifest,
        EvolutionRecovery::CorrectInput,
        detail,
    )
}

const fn persisted_error(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::AdmitManifest,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
