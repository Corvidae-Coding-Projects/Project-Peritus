//! Effect-free values shared by the ordinary and verification-only developer compositions.

/// One atomically captured governing conversation revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeveloperInput {
    /// Monotonic durable user-input revision.
    pub revision: u64,
    /// Current user-visible conversation, without private reasoning or credentials.
    pub conversation: String,
    /// Exact image inputs selected in the same revision as the conversation. The host resolves
    /// immutable bytes; D0 never discovers files from text. Empty means no live image selection.
    pub images: Vec<peritus_model_protocol::MediaInput>,
}

/// Host admission of the exact fully constructed request against current governing input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperRequestAdmission {
    /// The host durably bound the inspected revision before any provider call may begin.
    Accepted,
    /// Input changed during preparation; rebuild context without sending this stale request.
    Stale,
    /// Durable host control denied this request at a pause, budget, or inactive boundary.
    Stopped,
}
