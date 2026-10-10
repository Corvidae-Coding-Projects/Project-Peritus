//! Truthful terminal work outcomes.

use crate::{DispatchId, WorkId};
use peritus_types::Sha256Digest;

#[doc = " Truthful terminal work outcome."]
#[derive(Debug, Eq, PartialEq)]
pub enum WorkTerminal {
    #[doc = " Work completed with exact inert result digest."]
    Succeeded {
        #[doc = " Digest of the inert result."]
        result_digest: Sha256Digest,
    },
    #[doc = " Work failed with exact inert failure digest."]
    Failed {
        #[doc = " Digest of the inert failure record."]
        failure_digest: Sha256Digest,
    },
    #[doc = " A canonical prerequisite could not succeed."]
    DependencyFailed {
        #[doc = " Canonical prerequisite that could not succeed."]
        dependency: WorkId,
    },
    #[doc = " Work was cancelled before or during execution."]
    Cancelled,
    #[doc = " Lost ownership left an unknowable external outcome."]
    Ambiguous {
        #[doc = " Dispatch whose external outcome is unknowable."]
        dispatch_id: DispatchId,
    },
    #[doc = " Bounded attempts were exhausted."]
    Exhausted {
        #[doc = " Digest explaining attempt exhaustion."]
        cause_digest: Sha256Digest,
    },
    #[doc = " An active reservation was explicitly abandoned."]
    Abandoned {
        #[doc = " Digest explaining explicit abandonment."]
        cause_digest: Sha256Digest,
    },
}
