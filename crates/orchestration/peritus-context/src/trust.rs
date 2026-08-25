//! Trust labels record validation confidence without granting instruction authority.

use vstd::prelude::*;

verus! {

/// Trust ceiling retained with every context node and render segment.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TrustClass {
    /// Content originates at a trusted policy boundary.
    Trusted,
    /// Content is validated or tool-bounded but remains non-authoritative.
    Constrained,
    /// Content is untrusted evidence and must remain inert.
    Untrusted,
}

} // verus!
