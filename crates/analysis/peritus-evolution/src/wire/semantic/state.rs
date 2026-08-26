//! Complete canonical campaign and production-pointer checkpoint semantics.

mod campaign;
mod pointer;
mod shared;

pub(crate) use campaign::{decode_campaign_state, encode_campaign_state};
pub(crate) use pointer::{decode_pointer_state, encode_pointer_state};
