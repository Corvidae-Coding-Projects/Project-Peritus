//! Canonical replay-derived memory index.

use vstd::prelude::*;

verus! {

mod canonical;
mod rebuild;
mod types;

pub use types::{ClaimPosting, FeaturePosting, MemoryIndex, ScopePosting};

} // verus!
