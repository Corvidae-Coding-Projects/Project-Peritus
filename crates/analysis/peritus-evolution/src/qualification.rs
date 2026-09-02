//! Feature-gated production qualification for F0 commit and replay boundaries.
//!
//! These fixtures build valid domain predecessors, then exercise the same public reducers and
//! C0 commit functions used by production. They are enabled only by the product's administrative
//! qualification surface and are not part of ordinary evolution planning.

mod approval;
mod evidence;
mod harness;
mod identity;
mod promotion;

pub use promotion::{
    CommittedPromotion, PreparedPromotion, PromotionQualificationIdentity,
    PromotionQualificationObservation, observe_promotion, prepare_promotion,
};
