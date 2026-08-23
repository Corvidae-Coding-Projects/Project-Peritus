//! Safe bounded rendering, truncation, and redacted-secret behavior.

mod support;

use peritus_approval::{
    ApprovalAggregate, ApprovalChoice, MAX_RENDERED_APPROVAL_BYTES, MAX_RENDERED_FIELD_BYTES,
    MAX_RENDERED_PARTICIPANTS, MAX_RENDERED_PERMISSIONS, render_approval, verify_signed_decision,
};
use peritus_policy::IndependenceRequirement;
use peritus_types::{ActorId, Sha256Digest};
use sha2::{Digest, Sha256};

fn lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[usize::from(byte >> 4)] as char);
        output.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    output
}

fn assert_field(rendered: &str, name: &str, value: &str) {
    assert!(
        rendered.contains(&format!("{name}={value};")),
        "missing exact field {name}={value} in {rendered}",
    );
}

fn assert_count_field(rendered: &str, name: &str, total: usize, omitted: usize) {
    assert_field(rendered, name, &format!("{total}/{omitted}"));
}

fn participants(count: usize, namespace: u8) -> Vec<ActorId> {
    (0..count)
        .map(|index| {
            let mut bytes = [0_u8; 16];
            bytes[0] = namespace;
            bytes[14..].copy_from_slice(
                &u16::try_from(index + 1).expect("bounded participant index").to_be_bytes(),
            );
            ActorId::new(bytes).expect("nonzero canonical participant")
        })
        .collect()
}

fn resolved(choice: ApprovalChoice) -> ApprovalAggregate {
    let fixture = support::signed_fixture(choice);
    let observation = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("strict fixture authentication");
    ApprovalAggregate::new(fixture.request)
        .resolve(observation, &fixture.registry)
        .expect("resolution")
        .into_parts()
        .0
}

#[test]
fn render_is_printable_ascii_bounded_and_deterministic() {
    let aggregate = ApprovalAggregate::new(support::request(1, Vec::new()));
    let first = render_approval(&aggregate).expect("safe typed render");
    let second = render_approval(&aggregate).expect("deterministic render");
    assert_eq!(first, second);
    assert!(first.as_str().is_ascii());
    assert!(first.as_str().bytes().all(|byte| (0x20..=0x7e).contains(&byte)));
    assert!(first.as_str().len() <= MAX_RENDERED_APPROVAL_BYTES);
    assert!(
        first.as_str().split_terminator(';').all(|field| field.len() < MAX_RENDERED_FIELD_BYTES)
    );
}

#[test]
fn mandatory_authority_and_time_facts_are_complete() {
    let request = support::request(1, vec![IndependenceRequirement::NotRequester]);
    let ids = support::ids();
    let rendered = render_approval(&ApprovalAggregate::new(request)).expect("complete render");
    let text = rendered.as_str();
    assert_field(text, "phase", "pending");
    assert_field(text, "scope-actor", &lower_hex(ids.requester.as_bytes()));
    assert_field(text, "scope-role", "writer");
    assert_field(text, "environment-id", &lower_hex(ids.environment.as_bytes()));
    assert_field(text, "scope-not-before-epoch", "1");
    assert_field(text, "scope-not-before-tick", "5");
    assert_field(text, "scope-expires-at-epoch", "1");
    assert_field(text, "scope-expires-at-tick", "95");
    assert_field(text, "scope-use-limit", "1");
    assert_field(text, "requirement-tier", "user");
    assert_field(text, "approver-roles", "human-authority");
    assert_field(text, "independence", "not-requester");
    assert_field(text, "requirement-not-before-epoch", "1");
    assert_field(text, "requirement-not-before-tick", "10");
    assert_field(text, "requirement-expires-at-epoch", "1");
    assert_field(text, "requirement-expires-at-tick", "90");
    assert_field(text, "request-not-before-epoch", "1");
    assert_field(text, "request-not-before-tick", "10");
    assert_field(text, "request-expires-at-epoch", "1");
    assert_field(text, "request-expires-at-tick", "90");
    assert_field(text, "evaluated-at-epoch", "1");
    assert_field(text, "evaluated-at-tick", "20");
    assert_field(text, "challenge-epoch", "1");
    assert_field(text, "challenge-tick", "20");
    assert_field(text, "authority-floor-epoch", "1");
    assert_field(text, "authority-floor-tick", "20");
    assert_field(text, "resolution-present", "false");
}

#[test]
fn permission_truncation_is_whole_item_and_counted() {
    let aggregate = ApprovalAggregate::new(support::request(256, Vec::new()));
    let rendered = render_approval(&aggregate).expect("bounded maximum request render");
    assert!(rendered.was_truncated());
    assert!(rendered.omitted_permissions() >= 256 - MAX_RENDERED_PERMISSIONS);
    assert!(rendered.as_str().len() <= MAX_RENDERED_APPROVAL_BYTES);
    assert_count_field(rendered.as_str(), "permission-count", 256, rendered.omitted_permissions());
    assert!(rendered.as_str().contains("truncated=true;"));
    assert_eq!(rendered.as_str().matches("permission=").count(), MAX_RENDERED_PERMISSIONS);
    assert!(!rendered.as_str().contains('\n'));
    assert!(!rendered.as_str().contains('\r'));
}

#[test]
fn permission_boundaries_have_exact_whole_item_counts() {
    for count in [1, 63, 64, 65, 255, 256] {
        let rendered =
            render_approval(&ApprovalAggregate::new(support::request(count, Vec::new())))
                .expect("bounded permission render");
        let expected_rendered = count.min(MAX_RENDERED_PERMISSIONS);
        assert_eq!(rendered.as_str().matches("permission=").count(), expected_rendered);
        assert_eq!(rendered.omitted_permissions(), count - expected_rendered);
        assert_count_field(rendered.as_str(), "permission-count", count, count - expected_rendered);
        assert_eq!(rendered.was_truncated(), count > MAX_RENDERED_PERMISSIONS);
        assert!(rendered.as_str().len() <= MAX_RENDERED_APPROVAL_BYTES);
    }
}

#[test]
fn provenance_participant_boundaries_are_exact_and_bounded() {
    for count in [0, 1, 63, 64, 65, 256] {
        let request = support::request_with_participants(
            Vec::new(),
            participants(count, 0x20),
            participants(count, 0x30),
        );
        let rendered =
            render_approval(&ApprovalAggregate::new(request)).expect("bounded participant render");
        let expected_rendered = count.min(MAX_RENDERED_PARTICIPANTS);
        let expected_omitted = count - expected_rendered;
        assert_eq!(rendered.as_str().matches("producing-participant=").count(), expected_rendered,);
        assert_eq!(rendered.as_str().matches("review-participant=").count(), expected_rendered,);
        assert_eq!(rendered.omitted_producing_participants(), expected_omitted);
        assert_eq!(rendered.omitted_review_participants(), expected_omitted);
        assert_count_field(rendered.as_str(), "producing-count", count, expected_omitted);
        assert_count_field(rendered.as_str(), "review-count", count, expected_omitted);
        assert!(rendered.as_str().len() <= MAX_RENDERED_APPROVAL_BYTES);
    }
}

#[test]
fn every_resolution_choice_renders_its_exact_identity() {
    for (choice, expected) in
        [(ApprovalChoice::ApproveOnce, "approve-once"), (ApprovalChoice::Deny, "deny")]
    {
        let rendered = render_approval(&resolved(choice)).expect("resolved render");
        assert_field(rendered.as_str(), "resolution-present", "true");
        assert_field(rendered.as_str(), "resolution-choice", expected);
        assert_field(rendered.as_str(), "resolution-registry-revision", "1");
        assert_field(rendered.as_str(), "resolution-credential-generation", "1");
        assert_field(rendered.as_str(), "resolution-valid-until-epoch", "1");
        assert_field(rendered.as_str(), "resolution-valid-until-tick", "75");
        assert_field(rendered.as_str(), "resolution-command-id", &lower_hex(&[16; 16]));
    }

    let (_, identity) = support::amendment_candidate();
    let rendered = render_approval(&resolved(ApprovalChoice::Amend(identity)))
        .expect("amendment resolution render");
    assert_field(rendered.as_str(), "resolution-choice", "amend");
    assert_field(
        rendered.as_str(),
        "resolution-amend-base-policy-id",
        &lower_hex(identity.base_policy_id().as_bytes()),
    );
    assert_field(
        rendered.as_str(),
        "resolution-amend-successor-policy-id",
        &lower_hex(identity.successor_policy_id().as_bytes()),
    );
    assert_field(rendered.as_str(), "resolution-amend-tier", "project");
    assert_field(
        rendered.as_str(),
        "resolution-amendment-digest",
        &lower_hex(identity.amendment_digest().as_bytes()),
    );
}

#[test]
fn mandatory_fields_survive_worst_case_collection_truncation() {
    let request = support::request_with_permission_and_participants(
        256,
        Vec::new(),
        participants(256, 0x20),
        participants(256, 0x30),
    );
    let rendered = render_approval(&ApprovalAggregate::new(request)).expect("worst-case render");
    assert!(rendered.as_str().contains("request-digest="));
    assert_field(rendered.as_str(), "scope-actor", &lower_hex(support::ids().requester.as_bytes()));
    assert_field(rendered.as_str(), "requirement-tier", "user");
    assert_field(rendered.as_str(), "authority-floor-tick", "20");
    assert_field(rendered.as_str(), "resolution-present", "false");
    assert!(rendered.was_truncated());
    assert_eq!(rendered.omitted_permissions(), 256 - MAX_RENDERED_PERMISSIONS);
    assert_eq!(rendered.omitted_producing_participants(), 256 - MAX_RENDERED_PARTICIPANTS,);
    assert_eq!(rendered.omitted_review_participants(), 256 - MAX_RENDERED_PARTICIPANTS,);
    assert_count_field(rendered.as_str(), "permission-count", 256, 256 - MAX_RENDERED_PERMISSIONS);
    assert_count_field(rendered.as_str(), "producing-count", 256, 256 - MAX_RENDERED_PARTICIPANTS);
    assert_count_field(rendered.as_str(), "review-count", 256, 256 - MAX_RENDERED_PARTICIPANTS);
    assert!(rendered.as_str().len() <= MAX_RENDERED_APPROVAL_BYTES);
    assert!(rendered.as_str().is_ascii());
}

#[test]
fn seeded_secret_precursor_renders_only_its_digest() {
    let canary = b"B1-SECRET-CANARY-control-\n-unicode-\xf0\x9f\x94\x92";
    let digest_bytes: [u8; 32] = Sha256::digest(canary).into();
    let request = support::request_with_risk_digest(1, Vec::new(), Sha256Digest::new(digest_bytes));
    assert_eq!(request.risk_details_digest().as_bytes(), &digest_bytes);
    let rendered = render_approval(&ApprovalAggregate::new(request)).expect("redacted render");
    assert!(rendered.as_str().contains(&lower_hex(&digest_bytes)));
    assert!(!rendered.as_str().as_bytes().windows(canary.len()).any(|window| window == canary));
    assert!(!rendered.as_str().contains("B1-SECRET-CANARY"));
    assert!(rendered.as_str().is_ascii());
}

#[test]
fn unsafe_capability_controls_and_non_ascii_are_rejected_before_render() {
    assert!(peritus_types::CapabilityName::new("workspace.\ninspect".to_owned()).is_err());
    assert!(peritus_types::CapabilityName::new("workspace.🔒".to_owned()).is_err());
}
