//! Exact numeric hard-ceiling selection; clock/resource sampling remains in the adapter.

use super::ProductRunProgress;
use vstd::prelude::*;

verus! {

/// Maximum cumulative provider requests.
pub const PRODUCT_RUN_MAX_MODEL_REQUESTS: u32 = 4_096;
/// Maximum cumulative application tool calls.
pub const PRODUCT_RUN_MAX_TOOL_CALLS: u32 = 20_000;
/// Maximum cumulative provider tokens.
pub const PRODUCT_RUN_MAX_TOTAL_TOKENS: u64 = 100_000_000;
/// Maximum cumulative provider cost in microunits.
pub const PRODUCT_RUN_MAX_COST_MICROUNITS: u64 = 500_000_000;
/// Maximum resident memory observed at completed effect boundaries.
pub const PRODUCT_RUN_MAX_PEAK_RSS_BYTES: u64 = 12 * 1024 * 1024 * 1024;
/// Maximum regular-file growth beneath the managed workspace during one run.
pub const PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// First exhausted product-run ceiling in its stable diagnostic priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetViolation {
    /// The configured elapsed horizon was exceeded.
    Elapsed,
    /// The provider requests ceiling was exceeded.
    ModelRequests,
    /// The application tool calls ceiling was exceeded.
    ToolCalls,
    /// The provider tokens ceiling was exceeded.
    TotalTokens,
    /// The provider cost in microunits ceiling was exceeded.
    ProviderCost,
    /// The resident memory bytes ceiling was exceeded.
    PeakRss,
    /// The workspace growth bytes ceiling was exceeded.
    WorkspaceGrowth,
}

impl ProductRunProgress {
    pub(crate) closed spec fn spec_budget_violation(self, elapsed_exceeded: bool) -> Option<BudgetViolation> {
        if elapsed_exceeded {
            Some(BudgetViolation::Elapsed)
        } else if self.model_requests > PRODUCT_RUN_MAX_MODEL_REQUESTS {
            Some(BudgetViolation::ModelRequests)
        } else if self.tool_calls > PRODUCT_RUN_MAX_TOOL_CALLS {
            Some(BudgetViolation::ToolCalls)
        } else if self.total_tokens > PRODUCT_RUN_MAX_TOTAL_TOKENS {
            Some(BudgetViolation::TotalTokens)
        } else if self.provider_cost_microunits > PRODUCT_RUN_MAX_COST_MICROUNITS {
            Some(BudgetViolation::ProviderCost)
        } else if self.peak_rss_bytes > PRODUCT_RUN_MAX_PEAK_RSS_BYTES {
            Some(BudgetViolation::PeakRss)
        } else if self.workspace_growth_bytes > PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES {
            Some(BudgetViolation::WorkspaceGrowth)
        } else {
            None
        }
    }

    /// Selects the first exceeded ceiling; equality with every numeric ceiling is admitted.
    pub(crate) const fn budget_violation(self, elapsed_exceeded: bool) -> (violation: Option<BudgetViolation>)
        ensures violation == self.spec_budget_violation(elapsed_exceeded),
    {
        if elapsed_exceeded {
            Some(BudgetViolation::Elapsed)
        } else if self.model_requests > PRODUCT_RUN_MAX_MODEL_REQUESTS {
            Some(BudgetViolation::ModelRequests)
        } else if self.tool_calls > PRODUCT_RUN_MAX_TOOL_CALLS {
            Some(BudgetViolation::ToolCalls)
        } else if self.total_tokens > PRODUCT_RUN_MAX_TOTAL_TOKENS {
            Some(BudgetViolation::TotalTokens)
        } else if self.provider_cost_microunits > PRODUCT_RUN_MAX_COST_MICROUNITS {
            Some(BudgetViolation::ProviderCost)
        } else if self.peak_rss_bytes > PRODUCT_RUN_MAX_PEAK_RSS_BYTES {
            Some(BudgetViolation::PeakRss)
        } else if self.workspace_growth_bytes > PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES {
            Some(BudgetViolation::WorkspaceGrowth)
        } else {
            None
        }
    }
}

} // verus!
