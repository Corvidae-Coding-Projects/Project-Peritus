//! Strict production-limited semantic codecs shared by all six F0 frame families.

mod attribution;
mod binding;
mod campaign;
mod change;
mod evaluation;
mod pointer;
mod proposal;
mod selection;
mod state;

pub(super) use campaign::{
    decode_kind as decode_campaign_kind, encode_kind as encode_campaign_kind,
};
pub(super) use pointer::{decode_kind as decode_pointer_kind, encode_kind as encode_pointer_kind};
pub(super) use state::{
    decode_campaign_state, decode_pointer_state, encode_campaign_state, encode_pointer_state,
};
