use super::*;

#[test]
fn geometry_uses_the_full_portable_pty_width_without_clamping() {
    let encoded = resize_payload(65_535, 65_534, 65_533, 65_532).unwrap();
    assert_eq!(u16::from_be_bytes([encoded[0], encoded[1]]), 65_535);
    assert_eq!(u16::from_be_bytes([encoded[2], encoded[3]]), 65_534);
    assert!(resize_payload(65_536, 24, 0, 0).is_err());
}
