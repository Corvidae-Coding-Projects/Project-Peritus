//! Exact stable identity encoding and constructor-enforced deserialization.

use super::{CheckpointId, ConversationId, InputId, InvocationId, OperationId, RestoreId};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn check_identity<T>()
where
    T: DeserializeOwned + Serialize + TryFrom<[u8; 16]>,
    T::Error: std::fmt::Debug,
{
    let nonzero = serde_json::to_string(&[7_u8; 16]).expect("raw bytes");
    let parsed: T = serde_json::from_str(&nonzero).expect("valid identity");
    assert_eq!(serde_json::to_string(&parsed).expect("identity encoding"), nonzero);
    let constructed = T::try_from([7; 16]).expect("constructor");
    assert_eq!(serde_json::to_string(&constructed).expect("encoding"), nonzero);
    let zero = serde_json::to_string(&[0_u8; 16]).expect("zero bytes");
    assert!(serde_json::from_str::<T>(&zero).is_err());
    assert!(serde_json::from_str::<T>("[1,2]").is_err());
}

#[test]
fn exact_encoding_and_nonzero_validation_survive_explicit_deserialization() {
    check_identity::<ConversationId>();
    check_identity::<OperationId>();
    check_identity::<InputId>();
    check_identity::<InvocationId>();
    check_identity::<CheckpointId>();
    check_identity::<RestoreId>();
}
