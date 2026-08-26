//! Isolated immutable candidate variants.

use crate::{
    ChangeManifest, ChangeManifestId, CompatibilityEffect, EvolutionError, EvolutionErrorKind,
    EvolutionLimits, EvolutionOperation, EvolutionRecovery, InteractionGroupId,
    ProductionHarnessBinding, VariantId, identity::digest_parts,
};
use peritus_harness::domain::{ComponentId, ComponentKind, HarnessRevision};
use peritus_types::Sha256Digest;

/// One materialized E1 candidate and its exact canonical manifest set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantDefinition {
    id: VariantId,
    baseline: ProductionHarnessBinding,
    candidate: ProductionHarnessBinding,
    manifest_ids: Vec<ChangeManifestId>,
    manifest_digests: Vec<Sha256Digest>,
    changed_components: Vec<ComponentId>,
    changed_kinds: Vec<ComponentKind>,
    changes_executable: bool,
    compatibility: CompatibilityEffect,
    interaction_group: Option<InteractionGroupId>,
    digest: Sha256Digest,
}

impl VariantDefinition {
    /// Constructs one isolated materialized candidate variant.
    ///
    /// # Errors
    /// Rejects empty/noncanonical/over-limit manifests, baseline/candidate drift, duplicate
    /// component ownership, or missing/unnecessary interaction identity.
    pub fn new(
        baseline_revision: &HarnessRevision,
        candidate_revision: &HarnessRevision,
        baseline: ProductionHarnessBinding,
        candidate: ProductionHarnessBinding,
        manifests: &[ChangeManifest],
        interaction_group: Option<InteractionGroupId>,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if manifests.is_empty()
            || manifests.len() > usize::from(limits.manifests())
            || manifests.windows(2).any(|pair| pair[0].id() >= pair[1].id())
            || baseline.harness_revision() != baseline_revision.identity()
            || candidate.harness_revision() != candidate_revision.identity()
            || baseline.harness_revision() == candidate.harness_revision()
            || (manifests.len() > 1) != interaction_group.is_some()
            || manifests.iter().any(|manifest| {
                manifest.baseline() != baseline.harness_revision()
                    || manifest.candidate() != candidate.harness_revision()
            })
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::BindingDrift,
                EvolutionOperation::AdmitVariant,
                EvolutionRecovery::CorrectInput,
                "variant manifests, arms, or interaction identity differ",
            ));
        }
        let mut changed_components = manifests
            .iter()
            .flat_map(|manifest| manifest.deltas().iter().map(|delta| delta.component_id().clone()))
            .collect::<Vec<_>>();
        changed_components.sort();
        if changed_components.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Contamination,
                EvolutionOperation::AdmitVariant,
                EvolutionRecovery::CorrectInput,
                "multiple manifests claim the same component delta",
            ));
        }
        if changed_components
            != super::manifest::changed_components(baseline_revision, candidate_revision)
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Contamination,
                EvolutionOperation::AdmitVariant,
                EvolutionRecovery::CorrectInput,
                "variant manifest union omits or invents a candidate component change",
            ));
        }
        let mut changed_kinds = manifests
            .iter()
            .flat_map(|manifest| manifest.deltas().iter().map(crate::ComponentDelta::kind))
            .collect::<Vec<_>>();
        changed_kinds.sort_unstable();
        changed_kinds.dedup();
        let changes_executable = manifests
            .iter()
            .flat_map(ChangeManifest::deltas)
            .any(|delta| delta.before_executable() != delta.after_executable());
        let compatibility = manifests
            .iter()
            .flat_map(ChangeManifest::deltas)
            .map(crate::ComponentDelta::compatibility)
            .max()
            .unwrap_or(CompatibilityEffect::Incompatible);
        let manifest_ids = manifests.iter().map(ChangeManifest::id).collect::<Vec<_>>();
        let manifest_digests = manifests.iter().map(ChangeManifest::digest).collect::<Vec<_>>();
        let digest = variant_digest(
            baseline,
            candidate,
            &manifest_ids,
            &manifest_digests,
            &changed_components,
            &changed_kinds,
            changes_executable,
            compatibility,
            interaction_group,
        );
        let id = VariantId::derive(b"peritus.f0.variant-id.v1\0", digest);
        Ok(Self {
            id,
            baseline,
            candidate,
            manifest_ids,
            manifest_digests,
            changed_components,
            changed_kinds,
            changes_executable,
            compatibility,
            interaction_group,
            digest,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "all persisted variant facts stay explicit")]
    pub(crate) fn from_exact_parts(
        baseline: ProductionHarnessBinding,
        candidate: ProductionHarnessBinding,
        manifest_ids: Vec<ChangeManifestId>,
        manifest_digests: Vec<Sha256Digest>,
        changed_components: Vec<ComponentId>,
        changed_kinds: Vec<ComponentKind>,
        changes_executable: bool,
        compatibility: CompatibilityEffect,
        interaction_group: Option<InteractionGroupId>,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if baseline == candidate
            || manifest_ids.is_empty()
            || manifest_ids.len() > usize::from(limits.manifests())
            || manifest_ids.len() != manifest_digests.len()
            || manifest_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || changed_components.is_empty()
            || changed_components.windows(2).any(|pair| pair[0] >= pair[1])
            || changed_kinds.is_empty()
            || changed_kinds.windows(2).any(|pair| pair[0] >= pair[1])
            || (manifest_ids.len() > 1) != interaction_group.is_some()
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::NonCanonical,
                EvolutionOperation::AdmitVariant,
                EvolutionRecovery::Quarantine,
                "persisted variant facts are empty, noncanonical, or inconsistent",
            ));
        }
        let digest = variant_digest(
            baseline,
            candidate,
            &manifest_ids,
            &manifest_digests,
            &changed_components,
            &changed_kinds,
            changes_executable,
            compatibility,
            interaction_group,
        );
        let id = VariantId::derive(b"peritus.f0.variant-id.v1\0", digest);
        Ok(Self {
            id,
            baseline,
            candidate,
            manifest_ids,
            manifest_digests,
            changed_components,
            changed_kinds,
            changes_executable,
            compatibility,
            interaction_group,
            digest,
        })
    }

    /// Returns the content-derived variant identity.
    #[must_use]
    pub const fn id(&self) -> VariantId {
        self.id
    }
    /// Returns the exact materialized baseline.
    #[must_use]
    pub const fn baseline(&self) -> ProductionHarnessBinding {
        self.baseline
    }
    /// Returns the exact materialized candidate.
    #[must_use]
    pub const fn candidate(&self) -> ProductionHarnessBinding {
        self.candidate
    }
    /// Borrows canonical change-manifest identities.
    #[must_use]
    pub fn manifest_ids(&self) -> &[ChangeManifestId] {
        &self.manifest_ids
    }
    /// Borrows exact change-manifest digests.
    #[must_use]
    pub fn manifest_digests(&self) -> &[Sha256Digest] {
        &self.manifest_digests
    }
    /// Borrows the unique changed component set.
    #[must_use]
    pub fn changed_components(&self) -> &[ComponentId] {
        &self.changed_components
    }
    /// Borrows changed component kinds in canonical order.
    #[must_use]
    pub fn changed_kinds(&self) -> &[ComponentKind] {
        &self.changed_kinds
    }
    /// Returns whether any exact before/after executable artifact digest changed.
    #[must_use]
    pub const fn changes_executable(&self) -> bool {
        self.changes_executable
    }
    /// Returns the worst declared compatibility effect across all component deltas.
    #[must_use]
    pub const fn compatibility(&self) -> CompatibilityEffect {
        self.compatibility
    }
    /// Returns an explicit interaction group for multi-change variants.
    #[must_use]
    pub const fn interaction_group(&self) -> Option<InteractionGroupId> {
        self.interaction_group
    }
    /// Returns the complete variant digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[allow(clippy::too_many_arguments, reason = "all identity-bearing variant facts stay explicit")]
fn variant_digest(
    baseline: ProductionHarnessBinding,
    candidate: ProductionHarnessBinding,
    manifest_ids: &[ChangeManifestId],
    manifest_digests: &[Sha256Digest],
    changed_components: &[ComponentId],
    changed_kinds: &[ComponentKind],
    changes_executable: bool,
    compatibility: CompatibilityEffect,
    interaction: Option<InteractionGroupId>,
) -> Sha256Digest {
    let mut manifests = Vec::with_capacity(manifest_ids.len() * 48);
    for (identity, digest) in manifest_ids.iter().zip(manifest_digests) {
        manifests.extend_from_slice(identity.as_bytes());
        manifests.extend_from_slice(digest.as_bytes());
    }
    let mut changes = Vec::new();
    for component in changed_components {
        crate::identity::push_bytes(&mut changes, component.as_str().as_bytes());
    }
    changes.extend(changed_kinds.iter().map(|kind| kind.tag()));
    changes.push(u8::from(changes_executable));
    changes.push(match compatibility {
        CompatibilityEffect::Compatible => 1,
        CompatibilityEffect::RequiresMigration => 2,
        CompatibilityEffect::Incompatible => 3,
    });
    digest_parts(
        b"peritus.f0.variant.v1\0",
        &[
            baseline.digest().as_bytes(),
            candidate.digest().as_bytes(),
            &manifests,
            &changes,
            interaction.as_ref().map_or(&[][..], |value| value.as_bytes()),
        ],
    )
}
