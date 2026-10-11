//! Closed total amendment-preview result relation and executable refinement.

use super::{
    PolicyAmendmentProposal, PolicyRevisionCandidate, amended_layers_from, successor_revision,
};
use crate::{PolicyDefinition, PolicyError};

impl PolicyDefinition {
    #[doc = " Previews one complete, checked, fresh-policy successor without activating it."]
    #[doc = ""]
    #[doc = " Every unchanged ceiling, immutable denial, and non-target layer is carried forward exactly,"]
    #[doc = " with only its sole revision-tuple policy identity rebound to the fresh successor. The"]
    #[doc = " replacement is revalidated beneath that unchanged higher authority ceiling."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed base mismatch or the exact checked successor-policy validation failure."]
    pub fn preview_amendment(
        &self,
        proposal: &PolicyAmendmentProposal,
    ) -> Result<PolicyRevisionCandidate, PolicyError> {
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
        let successor_result =
            Self::new(proposal.successor_policy_id(), ceiling, operations, layers);
        let successor_policy = match successor_result {
            Ok(policy) => policy,
            Err(error) => {
                return Err(error);
            }
        };
        let candidate = PolicyRevisionCandidate {
            base_policy_id: proposal.base_policy_id(),
            successor_policy,
            tier: proposal.tier(),
            amendment_digest: proposal.amendment_digest(),
        };
        Ok(candidate)
    }
}
