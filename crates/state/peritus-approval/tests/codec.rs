//! Canonical approval authority-codec conformance evidence.

mod support;

use peritus_approval::{
    ApprovalChoice, ApprovalError, MAX_APPROVAL_REQUEST_PREIMAGE_BYTES,
    MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES, decode_approval_request, decode_credential_registry,
    decode_signed_decision, encode_approval_request, encode_signed_decision,
    verify_signed_decision,
};

fn field_payload(bytes: &[u8], wanted_tag: u16) -> core::ops::Range<usize> {
    let magic = b"PERITUS\0B1\0APPROVAL\0V1\0";
    let domain_length = usize::from(u16::from_be_bytes(
        bytes[magic.len()..magic.len() + 2].try_into().expect("canonical domain length"),
    ));
    let mut offset = magic.len() + 2 + domain_length;
    while offset < bytes.len() {
        let tag =
            u16::from_be_bytes(bytes[offset..offset + 2].try_into().expect("canonical field tag"));
        let length = usize::try_from(u64::from_be_bytes(
            bytes[offset + 2..offset + 10].try_into().expect("canonical field length"),
        ))
        .expect("test field length fits usize");
        let start = offset + 10;
        let end = start + length;
        if tag == wanted_tag {
            return start..end;
        }
        offset = end;
    }
    panic!("canonical test field is present")
}

#[test]
fn request_round_trip_preserves_value_and_exact_bytes() {
    let request = support::request(
        3,
        vec![
            peritus_policy::IndependenceRequirement::NotRequester,
            peritus_policy::IndependenceRequirement::NoReviewParticipation,
        ],
    );
    let bytes = encode_approval_request(&request).expect("canonical request");
    let decoded = decode_approval_request(&bytes).expect("decoded request");

    assert_eq!(decoded, request);
    assert_eq!(encode_approval_request(&decoded).expect("reencoded request"), bytes);
}

#[test]
fn all_decision_choices_round_trip_and_remain_authenticatable() {
    let (_, amendment) = support::amendment_candidate();
    for choice in
        [ApprovalChoice::Deny, ApprovalChoice::ApproveOnce, ApprovalChoice::Amend(amendment)]
    {
        let fixture = support::signed_fixture(choice);
        let bytes = encode_signed_decision(&fixture.signed).expect("canonical signed decision");
        let decoded = decode_signed_decision(&bytes).expect("decoded signed decision");

        assert_eq!(decoded, fixture.signed);
        assert_eq!(encode_signed_decision(&decoded).expect("reencoded signed decision"), bytes);
        verify_signed_decision(&fixture.request, &decoded, &fixture.registry, fixture.observed_at)
            .expect("decoded signature remains bound to the semantic decision");
    }
}

#[test]
fn credential_registry_round_trip_preserves_canonical_bytes_and_digest() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let bytes = fixture.registry.canonical_bytes().expect("canonical registry");
    let expected_digest = fixture.registry.digest().expect("registry digest");
    let decoded = decode_credential_registry(&bytes).expect("decoded registry");

    assert_eq!(decoded, fixture.registry);
    assert_eq!(decoded.canonical_bytes().expect("reencoded registry"), bytes);
    assert_eq!(decoded.digest().expect("decoded registry digest"), expected_digest);
}

#[test]
fn malformed_trailing_and_over_limit_frames_fail_closed() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let request = support::request(3, vec![peritus_policy::IndependenceRequirement::NotRequester]);
    let request_bytes = encode_approval_request(&request).expect("canonical request");
    let signed_bytes = encode_signed_decision(&fixture.signed).expect("canonical decision");
    let registry_bytes = fixture.registry.canonical_bytes().expect("canonical registry");

    let mut malformed = request_bytes.clone();
    malformed[0] ^= 1;
    assert_eq!(decode_approval_request(&malformed), Err(ApprovalError::InvalidCanonicalEncoding));
    assert_eq!(
        decode_approval_request(&request_bytes[..request_bytes.len() - 1]),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );

    let mut noncanonical = request_bytes.clone();
    let permissions_range = field_payload(&noncanonical, 9);
    let permissions = &mut noncanonical[permissions_range];
    let first_length = usize::try_from(u32::from_be_bytes(
        permissions[4..8].try_into().expect("first permission length"),
    ))
    .expect("test item length fits usize");
    let first_record_length = 4 + first_length;
    let second_length_offset = 4 + first_record_length;
    let second_length = usize::try_from(u32::from_be_bytes(
        permissions[second_length_offset..second_length_offset + 4]
            .try_into()
            .expect("second permission length"),
    ))
    .expect("test item length fits usize");
    let second_record_end = second_length_offset + 4 + second_length;
    permissions[4..second_record_end].rotate_left(first_record_length);
    assert_eq!(
        decode_approval_request(&noncanonical),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );

    let mut trailing_request = request_bytes;
    trailing_request.push(0);
    assert_eq!(
        decode_approval_request(&trailing_request),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );
    assert_eq!(
        decode_approval_request(&vec![0; MAX_APPROVAL_REQUEST_PREIMAGE_BYTES + 1]),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );

    let mut trailing_signed = signed_bytes;
    trailing_signed.push(0);
    assert_eq!(
        decode_signed_decision(&trailing_signed),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );

    let mut trailing_registry = registry_bytes;
    trailing_registry.push(0);
    assert_eq!(
        decode_credential_registry(&trailing_registry),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );
    assert_eq!(
        decode_credential_registry(&vec![0; MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES + 1]),
        Err(ApprovalError::InvalidCanonicalEncoding)
    );
}
