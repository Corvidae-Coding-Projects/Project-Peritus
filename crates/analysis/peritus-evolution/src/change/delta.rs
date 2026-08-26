//! Exact evolvable E1 component deltas.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    identity::digest_parts,
};
use peritus_harness::domain::{ComponentDeclaration, ComponentId, ComponentKind, ProtectionClass};
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

/// Exact before/after content and executable identity for one evolvable component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDelta {
    component_id: ComponentId,
    kind: ComponentKind,
    before_content: Sha256Digest,
    after_content: Sha256Digest,
    before_executable: Option<Sha256Digest>,
    after_executable: Option<Sha256Digest>,
    semantic_diff_artifact: Sha256Digest,
    compatibility: CompatibilityEffect,
    migration_artifact: Option<Sha256Digest>,
    digest: Sha256Digest,
}

impl ComponentDelta {
    /// Captures one exact changed evolvable E1 declaration.
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
            || matches!(compatibility, CompatibilityEffect::RequiresMigration)
                != migration_artifact.is_some()
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Contamination,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "component delta is equal, protected, mismatched, or lacks migration evidence",
            ));
        }
        let before_executable = before
            .executable_artifact_digest()
            .map(peritus_harness::domain::ArtifactDigest::digest);
        let after_executable =
            after.executable_artifact_digest().map(peritus_harness::domain::ArtifactDigest::digest);
        let digest = delta_digest(
            before.id(),
            before.kind(),
            before.content_digest(),
            after.content_digest(),
            before_executable,
            after_executable,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
        );
        Ok(Self {
            component_id: before.id().clone(),
            kind: before.kind(),
            before_content: before.content_digest(),
            after_content: after.content_digest(),
            before_executable,
            after_executable,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            digest,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "all persisted component delta facts stay explicit"
    )]
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
            || matches!(compatibility, CompatibilityEffect::RequiresMigration)
                != migration_artifact.is_some()
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Corruption,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::Quarantine,
                "persisted component delta is protected, equal, or lacks migration evidence",
            ));
        }
        let digest = delta_digest(
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
            before_content,
            after_content,
            before_executable,
            after_executable,
            semantic_diff_artifact,
            compatibility,
            migration_artifact,
            digest,
        })
    }

    pub(crate) fn matches(
        &self,
        before: &ComponentDeclaration,
        after: &ComponentDeclaration,
    ) -> bool {
        before.id() == &self.component_id
            && after.id() == &self.component_id
            && before.kind() == self.kind
            && after.kind() == self.kind
            && before.content_digest() == self.before_content
            && after.content_digest() == self.after_content
            && before
                .executable_artifact_digest()
                .map(peritus_harness::domain::ArtifactDigest::digest)
                == self.before_executable
            && after
                .executable_artifact_digest()
                .map(peritus_harness::domain::ArtifactDigest::digest)
                == self.after_executable
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
    /// Returns the prior source-content digest.
    #[must_use]
    pub const fn before_content(&self) -> Sha256Digest {
        self.before_content
    }
    /// Returns the candidate source-content digest.
    #[must_use]
    pub const fn after_content(&self) -> Sha256Digest {
        self.after_content
    }
    /// Returns the prior executable artifact, when declared.
    #[must_use]
    pub const fn before_executable(&self) -> Option<Sha256Digest> {
        self.before_executable
    }
    /// Returns the candidate executable artifact, when declared.
    #[must_use]
    pub const fn after_executable(&self) -> Option<Sha256Digest> {
        self.after_executable
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

fn delta_digest(
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
    let compatibility = [match compatibility {
        CompatibilityEffect::Compatible => 1,
        CompatibilityEffect::RequiresMigration => 2,
        CompatibilityEffect::Incompatible => 3,
    }];
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
