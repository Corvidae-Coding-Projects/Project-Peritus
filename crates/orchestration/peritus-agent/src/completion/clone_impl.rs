//! Verified semantic clone for retained completion data.

use super::CompletionProposal;
use vstd::prelude::*;

verus! {

impl CompletionProposal {
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.summary == right.summary
            && left.evidence@ == right.evidence@
            && left.uncertainties@ == right.uncertainties@
            && left.revision == right.revision
            && left.transcripts == right.transcripts
            && left.requested == right.requested
    }
}

impl Clone for CompletionProposal {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        let evidence = self.evidence.clone();
        let uncertainties = self.uncertainties.clone();
        proof {
            assert(evidence@ =~= self.evidence@);
            assert(uncertainties@ =~= self.uncertainties@);
        }
        Self {
            summary: self.summary.clone(),
            evidence,
            uncertainties,
            revision: self.revision,
            transcripts: self.transcripts,
            requested: self.requested,
        }
    }
}

} // verus!
