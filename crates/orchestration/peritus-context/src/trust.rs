//! Trust labels record validation confidence without granting instruction authority.

#[doc = " Trust ceiling retained with every context node and render segment."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TrustClass {
    #[doc = " Content originates at a trusted policy boundary."]
    Trusted,
    #[doc = " Content is validated or tool-bounded but remains non-authoritative."]
    Constrained,
    #[doc = " Content is untrusted evidence and must remain inert."]
    Untrusted,
}
