//! Stable bounded identities for normalized OpenAI output coordinates.

use std::fmt::Write as _;

use peritus_model_protocol::ItemId;
use peritus_provider_core::ProviderCoreError;
use sha2::{Digest as _, Sha256};

use crate::error;

const MAX_NORMALIZED_ID_BYTES: usize = 512;
const DERIVED_PREFIX: &str = "openai-item-v1-";

pub(super) fn item_id(base: &str, suffix: &str) -> Result<ItemId, ProviderCoreError> {
    validate_source(base)?;
    if base
        .len()
        .checked_add(suffix.len())
        .is_some_and(|length| length <= MAX_NORMALIZED_ID_BYTES)
    {
        let mut value = String::with_capacity(base.len() + suffix.len());
        value.push_str(base);
        value.push_str(suffix);
        return ItemId::new(value)
            .map_err(|_| error::malformed("OpenAI normalized item identity was invalid"));
    }
    derived_item_id(base, suffix)
}

pub(super) fn derived_item_id(
    base: &str,
    suffix: &str,
) -> Result<ItemId, ProviderCoreError> {
    validate_source(base)?;
    let base_length = u64::try_from(base.len())
        .map_err(|_| error::limit("OpenAI provider identity length exceeded u64"))?;
    let suffix_length = u64::try_from(suffix.len())
        .map_err(|_| error::limit("OpenAI identity suffix length exceeded u64"))?;
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.openai.item.v1\0");
    hasher.update(base_length.to_be_bytes());
    hasher.update(base.as_bytes());
    hasher.update(suffix_length.to_be_bytes());
    hasher.update(suffix.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut value = String::with_capacity(DERIVED_PREFIX.len() + digest.len() * 2);
    value.push_str(DERIVED_PREFIX);
    for byte in digest {
        write!(value, "{byte:02x}").expect("writing to String cannot fail");
    }
    ItemId::new(value)
        .map_err(|_| error::malformed("OpenAI derived item identity was invalid"))
}

fn validate_source(value: &str) -> Result<(), ProviderCoreError> {
    ItemId::new(value.to_owned())
        .map(|_| ())
        .map_err(|_| error::malformed("OpenAI provider item identity was invalid"))
}
