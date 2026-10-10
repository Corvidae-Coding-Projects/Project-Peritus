//! Verified semantic clone for retained completion data.

use super::CompletionProposal;

impl CompletionProposal {}
impl Clone for CompletionProposal {
    fn clone(&self) -> Self {
        let evidence = self.evidence.clone();
        let uncertainties = self.uncertainties.clone();
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
