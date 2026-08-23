//! Closed total amendment-preview result relation and executable refinement.

use super::{
    amended_layers_from, successor_revision, PolicyAmendmentProposal, PolicyRevisionCandidate,
};
use crate::{PolicyDefinition, PolicyError};
#[cfg(verus_only)]
use crate::RestrictionLayer;
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Returns whether amendment preview produced the exact accepted candidate or first typed error.
pub closed spec fn preview_amendment_result_is_exact(
    base: &PolicyDefinition,
    proposal: &PolicyAmendmentProposal,
    result: &Result<PolicyRevisionCandidate, PolicyError>,
) -> bool {
    let base_matches = crate::model::same_identifier(
        proposal.spec_base_policy_id(),
        base.spec_policy_id(),
    );
    if !base_matches {
        match result {
            Err(error) => {
                error.spec_kind() == crate::PolicyErrorKind::AmendmentBaseMismatch
                    && error.spec_collection().is_none()
                    && error.spec_dimension().is_none()
            }
            Ok(_) => false,
        }
    } else {
        match result {
            Ok(candidate) => candidate.spec_is_exact_amendment_of(base, proposal),
            Err(error) => {
                error.spec_collection().is_none()
                    && exists |revision: RevisionTuple,
                        ceiling: crate::AuthorityCeiling,
                        layers: Seq<RestrictionLayer>| {
                        &&& crate::amendment_model::revision_is_exact_successor(
                            base.spec_boundary_revision(),
                            revision,
                            proposal.spec_successor_policy_id(),
                        )
                        &&& ceiling.spec_is_revision_rebind_of(
                            &base.spec_ceiling_value(),
                            revision,
                        )
                        &&& crate::amendment_model::exact_amended_layers_from(
                            base.spec_layers(),
                            layers,
                            proposal.spec_tier(),
                            &proposal.spec_replacement(),
                            revision,
                            0,
                            false,
                        )
                        &&& crate::definition::construction::policy_definition_validation_error(
                            proposal.spec_successor_policy_id_value(),
                            &ceiling,
                            layers,
                        ) == Some((error.spec_kind(), error.spec_dimension()))
                    }
            }
        }
    }
}

proof fn exact_rejection_has_component_witness(
    base: &PolicyDefinition,
    proposal: &PolicyAmendmentProposal,
    revision: RevisionTuple,
    ceiling: &crate::AuthorityCeiling,
    layers: Seq<RestrictionLayer>,
    error: &PolicyError,
)
    requires
        crate::amendment_model::revision_is_exact_successor(
            base.spec_boundary_revision(),
            revision,
            proposal.spec_successor_policy_id(),
        ),
        ceiling.spec_is_revision_rebind_of(&base.spec_ceiling_value(), revision),
        crate::amendment_model::exact_amended_layers_from(
            base.spec_layers(),
            layers,
            proposal.spec_tier(),
            &proposal.spec_replacement(),
            revision,
            0,
            false,
        ),
        crate::definition::construction::policy_definition_validation_error(
            proposal.spec_successor_policy_id_value(),
            ceiling,
            layers,
        ) == Some((error.spec_kind(), error.spec_dimension())),
    ensures
        exists |witness_revision: RevisionTuple,
            witness_ceiling: crate::AuthorityCeiling,
            witness_layers: Seq<RestrictionLayer>| {
            &&& crate::amendment_model::revision_is_exact_successor(
                base.spec_boundary_revision(),
                witness_revision,
                proposal.spec_successor_policy_id(),
            )
            &&& witness_ceiling.spec_is_revision_rebind_of(
                &base.spec_ceiling_value(),
                witness_revision,
            )
            &&& crate::amendment_model::exact_amended_layers_from(
                base.spec_layers(),
                witness_layers,
                proposal.spec_tier(),
                &proposal.spec_replacement(),
                witness_revision,
                0,
                false,
            )
            &&& crate::definition::construction::policy_definition_validation_error(
                proposal.spec_successor_policy_id_value(),
                &witness_ceiling,
                witness_layers,
            ) == Some((error.spec_kind(), error.spec_dimension()))
        },
{
    assert(exists |witness_revision: RevisionTuple,
        witness_ceiling: crate::AuthorityCeiling,
        witness_layers: Seq<RestrictionLayer>| {
        &&& witness_revision == revision
        &&& witness_ceiling == *ceiling
        &&& witness_layers == layers
        &&& crate::amendment_model::revision_is_exact_successor(
            base.spec_boundary_revision(),
            witness_revision,
            proposal.spec_successor_policy_id(),
        )
        &&& witness_ceiling.spec_is_revision_rebind_of(
            &base.spec_ceiling_value(),
            witness_revision,
        )
        &&& crate::amendment_model::exact_amended_layers_from(
            base.spec_layers(),
            witness_layers,
            proposal.spec_tier(),
            &proposal.spec_replacement(),
            witness_revision,
            0,
            false,
        )
        &&& crate::definition::construction::policy_definition_validation_error(
            proposal.spec_successor_policy_id_value(),
            &witness_ceiling,
            witness_layers,
        ) == Some((error.spec_kind(), error.spec_dimension()))
    });
}

impl PolicyDefinition {
    /// Previews one complete, checked, fresh-policy successor without activating it.
    ///
    /// Every unchanged ceiling, immutable denial, and non-target layer is carried forward exactly,
    /// with only its sole revision-tuple policy identity rebound to the fresh successor. The
    /// replacement is revalidated beneath that unchanged higher authority ceiling.
    ///
    /// # Errors
    ///
    /// Returns a typed base mismatch or the exact checked successor-policy validation failure.
    pub fn preview_amendment(
        &self,
        proposal: &PolicyAmendmentProposal,
    ) -> (result: Result<PolicyRevisionCandidate, PolicyError>)
        ensures preview_amendment_result_is_exact(self, proposal, &result),
    {
        reveal(preview_amendment_result_is_exact);
        if !crate::identity::identifier_values_equal(
            *proposal.base_policy_id().as_bytes(),
            *self.policy_id().as_bytes(),
        ) {
            return Err(PolicyError::amendment_base_mismatch());
        }
        let revision = successor_revision(
            *self.ceiling().boundary().revision(),
            proposal.successor_policy_id(),
        );
        let ceiling = self.ceiling().rebind_revision(revision);
        let layers = amended_layers_from(
            self.layers(),
            proposal.tier(),
            proposal.replacement(),
            revision,
            0,
            false,
        );
        let operations = self.operations().duplicate();
        let ghost ceiling_view = &ceiling;
        let ghost layers_view = layers@;
        proof {
            assert(crate::amendment_model::revision_is_exact_successor(
                self.spec_boundary_revision(),
                revision,
                proposal.spec_successor_policy_id(),
            ));
            assert(ceiling_view.spec_is_revision_rebind_of(
                &self.spec_ceiling_value(),
                revision,
            ));
            assert(crate::amendment_model::exact_amended_layers_from(
                self.spec_layers(),
                layers_view,
                proposal.spec_tier(),
                &proposal.spec_replacement(),
                revision,
                0,
                false,
            ));
        }
        let successor_result = Self::new(
            proposal.successor_policy_id(),
            ceiling,
            operations,
            layers,
        );
        let successor_policy = match successor_result {
            Ok(policy) => policy,
            Err(error) => {
                proof {
                    assert(crate::definition::construction::policy_definition_validation_error(
                        proposal.spec_successor_policy_id_value(),
                        ceiling_view,
                        layers_view,
                    ) == Some((error.spec_kind(), error.spec_dimension())));
                    exact_rejection_has_component_witness(
                        self,
                        proposal,
                        revision,
                        ceiling_view,
                        layers_view,
                        &error,
                    );
                }
                return Err(error);
            }
        };
        proof {
            successor_policy.establish_amendment_component_views(self, revision);
        }
        let candidate = PolicyRevisionCandidate {
            base_policy_id: proposal.base_policy_id(),
            successor_policy,
            tier: proposal.tier(),
            amendment_digest: proposal.amendment_digest(),
        };
        reveal(PolicyRevisionCandidate::spec_is_exact_amendment_of);
        assert(candidate.spec_is_exact_amendment_of(self, proposal));
        Ok(candidate)
    }
}

} // verus!
