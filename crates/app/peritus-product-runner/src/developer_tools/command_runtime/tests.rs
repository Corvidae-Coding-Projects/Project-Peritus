use super::identity::bounded_key;

#[test]
fn idempotency_keys_are_stable_and_fixed_width() {
    let first = bounded_key("provider-call-17");
    assert_eq!(first, bounded_key("provider-call-17"));
    assert_ne!(first, bounded_key("provider-call-18"));
    assert_eq!(first.len(), 64);
}
