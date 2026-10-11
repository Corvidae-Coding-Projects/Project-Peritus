//! Contract-derived review categories, quorum, and supplied independence facts.

#[doc = " Supplied reviewer provenance dimension compared by the acceptance policy."]
#[derive(Clone, Copy)]
pub enum ReviewIndependenceDimension {
    #[doc = " Exact review-context digest."]
    Context,
    #[doc = " Exact model-family digest."]
    ModelFamily,
    #[doc = " Exact provider digest."]
    Provider,
    #[doc = " Exact declared ancestry digest."]
    Ancestry,
}
