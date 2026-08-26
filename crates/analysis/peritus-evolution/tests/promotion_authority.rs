//! Promotion and rollback authority-binding tests.

mod support;

use peritus_evolution::{
    ActivationAuthorization, CampaignCommand, CampaignCommandKind, EvolutionErrorKind,
    EvolutionLimits, PointerCommand, PointerCommandKind,
};
use peritus_types::{CommandId, EventId};

use support::{bytes, campaign_id, digest, project_id};

#[test]
fn authorization_digest_binds_every_authority_observation() {
    let exact = ActivationAuthorization::new(digest(1), digest(2), digest(3), digest(4), digest(5));
    assert_eq!(exact.action_digest(), digest(1));
    assert_eq!(exact.dispatch_digest(), digest(2));
    assert_eq!(exact.capability_use_digest(), digest(3));
    assert_eq!(exact.approval_use_digest(), digest(4));
    assert_eq!(exact.authority_digest(), digest(5));
    for changed in [
        ActivationAuthorization::new(digest(9), digest(2), digest(3), digest(4), digest(5)),
        ActivationAuthorization::new(digest(1), digest(9), digest(3), digest(4), digest(5)),
        ActivationAuthorization::new(digest(1), digest(2), digest(9), digest(4), digest(5)),
        ActivationAuthorization::new(digest(1), digest(2), digest(3), digest(9), digest(5)),
        ActivationAuthorization::new(digest(1), digest(2), digest(3), digest(4), digest(9)),
    ] {
        assert_ne!(changed.digest(), exact.digest());
    }
}

#[test]
fn malformed_genesis_and_stale_shape_commands_fail_before_authority() {
    let campaign = CampaignCommand::new(
        CommandId::new(bytes(20)).expect("command"),
        EventId::new(bytes(21)).expect("event"),
        campaign_id(),
        0,
        Some(EventId::new(bytes(22)).expect("impossible genesis head")),
        digest(0),
        digest(23),
        CampaignCommandKind::FreezeCampaign,
    )
    .expect_err("campaign genesis cannot carry a predecessor");
    assert_eq!(campaign.kind(), EvolutionErrorKind::InvalidInput);

    let pointer = PointerCommand::new(
        CommandId::new(bytes(24)).expect("command"),
        EventId::new(bytes(25)).expect("event"),
        project_id(),
        1,
        Some(EventId::new(bytes(26)).expect("head")),
        0,
        digest(27),
        digest(28),
        PointerCommandKind::CancelPending { reason_digest: digest(29) },
    )
    .expect_err("non-genesis pointer sequence cannot claim generation zero");
    assert_eq!(pointer.kind(), EvolutionErrorKind::InvalidInput);
}

#[test]
fn independent_authority_and_history_limits_fail_closed() {
    let compiled = EvolutionLimits::compiled();
    assert!(
        EvolutionLimits::new(
            compiled.manifests(),
            compiled.variants(),
            compiled.citations_per_manifest(),
            compiled.deltas_per_manifest(),
            compiled.predictions_per_manifest(),
            compiled.attribution_entries(),
            compiled.criteria(),
            compiled.text_bytes(),
            compiled.activation_history(),
        )
        .is_ok()
    );
    let error = EvolutionLimits::new(1, 1, 1, 1, 1, 1, 1, 1, 0)
        .expect_err("zero activation history would erase authority evidence");
    assert_eq!(error.kind(), EvolutionErrorKind::LimitExceeded);
}
