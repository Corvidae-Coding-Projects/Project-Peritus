//! Child allocation and atomic operation begin transitions.

mod begin;
mod begin_apply;
mod child;

pub(super) use begin::begin;
pub(super) use child::allocate_child;
