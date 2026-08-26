//! Read-only bridge from the stable shared revision tuple to exact E1 revision identity.

use core::fmt;

use peritus_types::RevisionTuple;

use crate::{
    domain::{HarnessRevision, HarnessRevisionIdentity},
    materialization::{MaterializationReceipt, WorkspaceSnapshot},
};

/// Exact governing E1 identity carried beside the unchanged shared seven-field revision tuple.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoverningHarnessBinding {
    revision: RevisionTuple,
    harness_revision: HarnessRevisionIdentity,
    materialization: MaterializationReceipt,
}

impl GoverningHarnessBinding {
    /// Validates stable lineage, full revision digest/number, and installed C1 snapshot equality.
    ///
    /// # Errors
    /// Rejects a tuple, E1 revision, or retained receipt from another lineage, revision, or
    /// workspace snapshot.
    pub fn new(
        revision: RevisionTuple,
        harness_revision: &HarnessRevision,
        materialization: &MaterializationReceipt,
    ) -> Result<Self, GoverningHarnessBindingError> {
        let identity = harness_revision.identity();
        let installed = materialization.after();
        let mismatched = [
            revision.harness_id() != identity.harness_id(),
            revision.harness_id() != materialization.harness_id(),
            materialization.revision_digest() != identity.digest(),
            revision.workspace_id() != installed.workspace_id(),
            revision.workspace_generation() != installed.generation(),
            revision.workspace_revision() != installed.revision(),
        ]
        .into_iter()
        .any(core::convert::identity);
        if mismatched {
            return Err(GoverningHarnessBindingError);
        }
        Ok(Self { revision, harness_revision: identity, materialization: materialization.clone() })
    }

    /// Returns the unchanged shared authority/evidence tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the exact branch-distinguishing E1 harness revision.
    #[must_use]
    pub const fn harness_revision(&self) -> HarnessRevisionIdentity {
        self.harness_revision
    }
    /// Returns the complete exact materialization evidence.
    #[must_use]
    pub const fn materialization(&self) -> &MaterializationReceipt {
        &self.materialization
    }
    /// Returns the clean installed workspace snapshot.
    #[must_use]
    pub const fn installed_snapshot(&self) -> &WorkspaceSnapshot {
        self.materialization.after()
    }
}

/// Stable mismatch returned by [`GoverningHarnessBinding::new`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GoverningHarnessBindingError;

impl fmt::Display for GoverningHarnessBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("shared revision tuple does not match the exact installed E1 revision")
    }
}

impl std::error::Error for GoverningHarnessBindingError {}

#[cfg(test)]
mod tests {
    use peritus_types::{
        AcceptanceSpecId, Generation, PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple,
        WorkspaceId,
    };

    use crate::domain::{
        AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentContents,
        ComponentDeclaration, ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind,
        ComponentLocation, ComponentOwnership, ComponentRequirements, GraphEnvironment,
        HarnessLimits, HarnessRevision, LineageSeed, ManifestDigest, MediaType, Owner, Provenance,
        SchemaInterval, SchemaVersion, SourcePath, TargetPath, VerifiedComponentContent,
    };
    use crate::materialization::{MaterializationReceipt, WorkspaceSnapshot};

    use super::GoverningHarnessBinding;

    #[test]
    fn exact_binding_retains_the_shared_tuple_and_rejects_drift() {
        let content = b"binding fixture\n".to_vec();
        let version = SchemaVersion::new(1).expect("schema version");
        let declaration = ComponentDeclaration::new(
            ComponentIdentity::new(
                ComponentId::new("binding.fixture").expect("component ID"),
                ComponentKind::BaseInstructionFragment,
                version,
            ),
            ComponentLocation::new(
                SourcePath::new(".peritus-harness/components/binding.txt").expect("source path"),
                TargetPath::new("runtime/binding.txt").expect("target path"),
                MediaType::new("text/plain").expect("media type"),
            ),
            ComponentIntegrity::new(content.len() as u64, peritus_codec::sha256(&content), None),
            ComponentOwnership::new(
                Owner::new("runtime-test").expect("owner"),
                Provenance::new("checked unit fixture").expect("provenance"),
            ),
            ComponentRequirements::new(
                Vec::new(),
                CompatibilityContract::new(
                    SchemaInterval::new(version, version).expect("schema interval"),
                    Vec::new(),
                    Vec::new(),
                )
                .expect("compatibility"),
                AuthoritySet::empty(),
                ComponentKind::BaseInstructionFragment.protection_class(),
            ),
            HarnessLimits::compiled(),
        )
        .expect("component declaration");
        let graph = CheckedHarnessGraph::check(
            vec![declaration.clone()],
            &GraphEnvironment::new(Vec::new(), Vec::new()).expect("environment"),
            HarnessLimits::compiled(),
        )
        .expect("checked graph");
        let contents = ComponentContents::new(
            &graph,
            vec![VerifiedComponentContent::new(&declaration, content).expect("content")],
        )
        .expect("complete content");
        let revision = HarnessRevision::genesis(
            LineageSeed::new(peritus_codec::sha256(b"binding-lineage")),
            ManifestDigest::new(peritus_codec::sha256(b"binding-manifest")),
            graph,
            &contents,
        )
        .expect("genesis revision");

        let workspace_id = WorkspaceId::new([30; 16]).expect("workspace");
        let workspace_revision = RevisionNumber::new(2).expect("workspace revision");
        let installed =
            WorkspaceSnapshot::test_fixture(workspace_id, Generation::first(), workspace_revision);
        let receipt = MaterializationReceipt::test_fixture(&revision, installed);
        let shared = RevisionTuple::new(
            AcceptanceSpecId::new([31; 16]).expect("acceptance spec"),
            revision.harness_id(),
            workspace_id,
            Generation::first(),
            workspace_revision,
            PolicyId::new([32; 16]).expect("policy"),
            ProviderProfileId::new([33; 16]).expect("provider"),
        );

        let binding = GoverningHarnessBinding::new(shared, &revision, &receipt)
            .expect("exact governing binding");
        assert_eq!(binding.revision(), shared);
        assert_eq!(binding.harness_revision(), revision.identity());
        assert_eq!(binding.materialization(), &receipt);
        assert_eq!(binding.installed_snapshot(), receipt.after());

        let stale = RevisionTuple::new(
            shared.acceptance_spec_id(),
            shared.harness_id(),
            shared.workspace_id(),
            shared.workspace_generation(),
            RevisionNumber::first(),
            shared.policy_id(),
            shared.provider_profile_id(),
        );
        assert!(GoverningHarnessBinding::new(stale, &revision, &receipt).is_err());
    }
}
