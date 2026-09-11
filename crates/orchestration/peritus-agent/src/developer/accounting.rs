//! Incremental accounting boundaries that survive failed or cancelled invocations.

use peritus_model_protocol::UsageCounters;

/// One observed unit of developer-loop work, independent of terminal success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperAccountingEvent {
    /// A provider attempt is about to be dispatched, including semantic compaction requests.
    ModelRequest {
        /// Whether this is an additional attempt in the same checked retry sequence.
        retry: bool,
    },
    /// One application tool returned an observation (including a tool-level error).
    ToolCall,
    /// One context replacement was applied.
    Compaction,
    /// The current request's normalized high-water usage, replacing its previous snapshot.
    /// Repeated snapshots must not be added together; a new `ModelRequest` starts a new response.
    Usage(UsageCounters),
}
