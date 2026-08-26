//! Exact durable registry binding for authenticated approval transitions.

mod support;

use peritus_approval::{
    ApprovalAggregate, ApprovalChoice, ApprovalDecision, ApprovalKeyId, ApprovalPhase,
    ApprovalPublicKey, ApprovalSignature, ApproverCredential, CredentialRegistrySnapshot,
    CredentialStatus, SignedApprovalDecision, verify_signed_decision,
};
use peritus_codec::{CodecLimits, encode_frame};
use peritus_journal::{
    AggregateKind, AppendRequest, ApprovalCommitRequest, ApprovalUseCommitRequest,
    ApprovalUseResolution, ApprovalUseResolutionRequest, CredentialRegistryInstall, EventDraft,
    ExactFrame, HeadExpectation, JournalErrorKind,
};
use peritus_policy::{ActorRole, AuthorityTier, ValidityWindow};
use peritus_types::{ActorId, EventSequence, Generation, RevisionNumber};
use tempfile::TempDir;

use support::b1::{approval_request, consume_resolution, instant};
use support::{
    DomainIds, aggregate, append_request, command, digest, event, frame, open, registry_plan,
    store_id,
};

const SIGNATURE: ApprovalSignature = ApprovalSignature::new([
    0x62, 0x65, 0x3b, 0x6e, 0x09, 0x0a, 0x1a, 0x2b, 0x8f, 0x1b, 0xfc, 0x5b, 0xf3, 0x79, 0xa5, 0x63,
    0xc3, 0x09, 0xde, 0xe0, 0xde, 0xbd, 0xc8, 0x8c, 0x0e, 0xac, 0xc4, 0x3a, 0xd4, 0xc9, 0xf2, 0xb7,
    0x53, 0x2a, 0x4a, 0x1b, 0x2a, 0xb0, 0xb9, 0xf6, 0x4f, 0xd4, 0xe5, 0x36, 0xb5, 0x94, 0x07, 0xc0,
    0x74, 0xb5, 0xb2, 0x5d, 0xee, 0x91, 0xe4, 0x6d, 0x05, 0x5e, 0x3c, 0x73, 0x10, 0x24, 0x86, 0x08,
]);

fn approval_resolution(
    maximum_tier: AuthorityTier,
) -> (peritus_approval::ApprovalTransitionOutcome, CredentialRegistrySnapshot) {
    let mut ids = DomainIds::new(*b"approve3");
    let request = approval_request(&mut ids);
    let responder = ids.next(ActorId::new);
    let public_key = ApprovalPublicKey::new([
        0xea, 0x4a, 0x6c, 0x63, 0xe2, 0x9c, 0x52, 0x0a, 0xbe, 0xf5, 0x50, 0x7b, 0x13, 0x2e, 0xc5,
        0xf9, 0x95, 0x47, 0x76, 0xae, 0xbe, 0xbe, 0x7b, 0x92, 0x42, 0x1e, 0xea, 0x69, 0x14, 0x46,
        0xd2, 0x2c,
    ]);
    let key_id = ApprovalKeyId::compute(public_key).expect("approval key id");
    let registry = credential_registry(&ids, responder, key_id, public_key, maximum_tier);
    let decision = ApprovalDecision::new(
        command(70),
        responder,
        ActorRole::HumanAuthority,
        request.request_id(),
        request.digest(),
        ApprovalChoice::ApproveOnce,
        instant(75),
        key_id,
        Generation::new(2).expect("second credential generation"),
        RevisionNumber::new(2).expect("second registry revision"),
    )
    .expect("approval decision");
    let signed = SignedApprovalDecision::new(decision, SIGNATURE);
    let observation =
        verify_signed_decision(&request, &signed, &registry, instant(30)).expect("signed approval");
    let outcome = ApprovalAggregate::new(request)
        .resolve(observation, &registry)
        .expect("approval resolution");
    (outcome, registry)
}

fn credential_registry(
    ids: &DomainIds,
    responder: ActorId,
    key_id: ApprovalKeyId,
    public_key: ApprovalPublicKey,
    maximum_tier: AuthorityTier,
) -> CredentialRegistrySnapshot {
    let validity = ValidityWindow::new(instant(0), instant(100)).expect("credential validity");
    let credential = ApproverCredential::new(
        key_id,
        public_key,
        responder,
        ActorRole::HumanAuthority,
        ids.environment,
        ids.workspace,
        maximum_tier,
        vec![ActorRole::HumanAuthority],
        validity,
        Generation::new(2).expect("second credential generation"),
        CredentialStatus::Enabled,
    )
    .expect("approval credential");
    CredentialRegistrySnapshot::new(
        RevisionNumber::new(2).expect("second registry revision"),
        vec![credential],
    )
    .expect("credential registry")
}

#[test]
fn signed_approval_rejects_same_revision_wrong_digest_and_commits_exact_snapshot() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let exact_registry = install_exact_registry(&mut journal);
    let current = journal.current_credential_registry().expect("current exact registry");
    let current_digest = exact_registry.digest().expect("exact registry digest");
    assert_eq!(current.digest(), current_digest);
    assert_eq!((current.revision(), current.generation()), (2, 7));

    let (wrong_outcome, wrong_registry) = approval_resolution(AuthorityTier::User);
    assert_eq!(wrong_registry.revision(), exact_registry.revision());
    assert_ne!(wrong_registry.digest().expect("wrong registry digest"), current_digest);
    let Err(error) =
        ApprovalCommitRequest::new(approval_append(), wrong_outcome, None, Some(&current))
    else {
        panic!("same-revision approval from a different snapshot must be stale");
    };
    assert_eq!(error.kind(), JournalErrorKind::StaleRegistry);

    let (exact_outcome, reproduced_registry) = approval_resolution(AuthorityTier::Organization);
    assert_eq!(reproduced_registry.digest().expect("reproduced digest"), current_digest);
    let committed = journal
        .commit_approval_transition(
            ApprovalCommitRequest::new(approval_append(), exact_outcome, None, Some(&current))
                .expect("bind exact authenticated registry"),
        )
        .expect("commit exact authenticated approval");
    assert_eq!(committed.aggregate().phase(), ApprovalPhase::ApprovedOnce);
    assert_eq!(
        committed
            .aggregate()
            .resolution()
            .expect("approval resolution")
            .credential_generation()
            .get(),
        2
    );
    assert_eq!(committed.registry_binding(), Some((2, 7, current_digest)));
    assert_eq!(committed.state_revision(), 1);
}

#[test]
fn approval_use_augments_a_multi_aggregate_append_atomically() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let exact_registry = install_exact_registry(&mut journal);
    let current = journal.current_credential_registry().expect("current exact registry");
    assert_eq!(exact_registry.digest().expect("registry digest"), current.digest());

    let (resolution, reproduced_registry) = approval_resolution(AuthorityTier::Organization);
    assert_eq!(reproduced_registry.digest().expect("reproduced registry digest"), current.digest());
    let approved = journal
        .commit_approval_transition(
            ApprovalCommitRequest::new(approval_append(), resolution, None, Some(&current))
                .expect("bind approval resolution"),
        )
        .expect("commit approval resolution")
        .into_parts()
        .1;
    let request_id = approved.request().request_id();
    let action_id = approved.request().action_id();
    let action_digest = approved.request().action_digest();
    let outcome = approved
        .consume_once(action_id, action_digest, instant(40))
        .expect("consume exact approved action");

    let campaign = aggregate(AggregateKind::EvolutionCampaign, 70);
    let pointer = aggregate(AggregateKind::ProductionHarness, 71);
    let committed = journal
        .commit_approval_use(
            ApprovalUseCommitRequest::new(f0_activation_append(), outcome, 1, &current)
                .expect("bind approval use"),
        )
        .expect("commit atomic approval use");

    assert_eq!(committed.batch().records().len(), 2);
    assert_eq!(committed.batch().heads().len(), 2);
    assert_eq!(committed.aggregate().phase(), ApprovalPhase::Consumed);
    assert_eq!(committed.state_revision(), 2);
    assert_eq!(committed.registry_binding(), (2, 7, current.digest()));
    assert!(journal.head(campaign).expect("campaign head query").is_some());
    assert!(journal.head(pointer).expect("pointer head query").is_some());
    let durable = journal
        .state_record(104, request_id.as_bytes())
        .expect("approval state query")
        .expect("consumed approval state");
    assert_eq!(durable.revision(), 2);
    assert_eq!(durable.digest(), committed.state_digest());
    assert_eq!(durable.producing_position(), committed.batch().last_position());
}

#[test]
fn approval_use_rejects_stale_registry_and_state_without_partial_activation() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    install_exact_registry(&mut journal);
    let current = journal.current_credential_registry().expect("current exact registry");

    let (wrong_resolution, _) = approval_resolution(AuthorityTier::User);
    let wrong_use = consume_resolution(wrong_resolution);
    let Err(error) = ApprovalUseCommitRequest::new(f0_activation_append(), wrong_use, 1, &current)
    else {
        panic!("wrong registry digest must be rejected before append");
    };
    assert_eq!(error.kind(), JournalErrorKind::StaleRegistry);

    let (resolution, _) = approval_resolution(AuthorityTier::Organization);
    let approved = journal
        .commit_approval_transition(
            ApprovalCommitRequest::new(approval_append(), resolution, None, Some(&current))
                .expect("bind approval resolution"),
        )
        .expect("commit approval resolution")
        .into_parts()
        .1;
    let request_id = approved.request().request_id();
    let action_id = approved.request().action_id();
    let action_digest = approved.request().action_digest();
    let use_outcome = approved
        .consume_once(action_id, action_digest, instant(40))
        .expect("consume approved action");
    let stale = ApprovalUseCommitRequest::new(f0_activation_append(), use_outcome, 2, &current)
        .expect("construct stale state CAS");
    let Err(error) = journal.commit_approval_use(stale) else {
        panic!("stale approval-state revision must reject the complete append");
    };
    assert_eq!(error.kind(), JournalErrorKind::StaleHead);
    assert!(
        journal
            .head(aggregate(AggregateKind::EvolutionCampaign, 70))
            .expect("campaign head query")
            .is_none()
    );
    assert!(
        journal
            .head(aggregate(AggregateKind::ProductionHarness, 71))
            .expect("pointer head query")
            .is_none()
    );
    assert_eq!(
        journal
            .state_record(104, request_id.as_bytes())
            .expect("approval state query")
            .expect("approved state remains")
            .revision(),
        1
    );
}

#[test]
fn exact_approval_use_retry_returns_the_original_atomic_batch() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    install_exact_registry(&mut journal);
    let current = journal.current_credential_registry().expect("current exact registry");
    let (resolution, _) = approval_resolution(AuthorityTier::Organization);
    let approved = journal
        .commit_approval_transition(
            ApprovalCommitRequest::new(approval_append(), resolution, None, Some(&current))
                .expect("bind approval resolution"),
        )
        .expect("commit approval resolution")
        .into_parts()
        .1;
    let action_id = approved.request().action_id();
    let action_digest = approved.request().action_digest();
    let first_use = approved
        .consume_once(action_id, action_digest, instant(40))
        .expect("consume approved action");
    let first = journal
        .commit_approval_use(
            ApprovalUseCommitRequest::new(f0_activation_append(), first_use, 1, &current)
                .expect("bind first approval use"),
        )
        .expect("commit first approval use");
    let first_position = first.batch().first_position();
    let last_position = first.batch().last_position();
    let batch_hash = first.batch().batch_hash();
    let state_digest = first.state_digest();

    let (replayed_resolution, _) = approval_resolution(AuthorityTier::Organization);
    let replayed_use = consume_resolution(replayed_resolution);
    let resolution = ApprovalUseResolutionRequest::new(
        command(61),
        digest(61),
        Vec::new(),
        &replayed_use,
        1,
        &current,
    )
    .expect("reconstruct exact approval-use request");
    let ApprovalUseResolution::Committed(replayed) = journal
        .resolve_approval_use(&resolution, replayed_use)
        .expect("resolve exact approval-use retry")
    else {
        panic!("committed approval use must resolve before mutable-head checks");
    };
    assert_eq!(replayed.batch().first_position(), first_position);
    assert_eq!(replayed.batch().last_position(), last_position);
    assert_eq!(replayed.batch().batch_hash(), batch_hash);
    assert_eq!(replayed.state_revision(), 2);
    assert_eq!(replayed.state_digest(), state_digest);
}

fn install_exact_registry(
    journal: &mut peritus_journal::SqliteJournal,
) -> CredentialRegistrySnapshot {
    let aggregate = aggregate(AggregateKind::CredentialRegistry, 50);
    let initial = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
        .expect("initial registry");
    let install = CredentialRegistryInstall::new(None, 6, &initial).expect("initial install");
    journal
        .append(registry_plan(
            aggregate,
            HeadExpectation::Absent(aggregate),
            command(50),
            event(50),
            None,
            1,
            install,
        ))
        .expect("install initial registry");
    let head = journal.head(aggregate).expect("registry head").expect("present");
    let (_, exact) = approval_resolution(AuthorityTier::Organization);
    let install = CredentialRegistryInstall::new(Some(1), 7, &exact).expect("exact install");
    journal
        .append(registry_plan(
            aggregate,
            HeadExpectation::Present(head),
            command(51),
            event(51),
            Some(event(50)),
            2,
            install,
        ))
        .expect("install exact registry");
    exact
}

fn approval_append() -> AppendRequest {
    let aggregate = aggregate(AggregateKind::Approval, 60);
    append_request(
        command(60),
        digest(60),
        HeadExpectation::Absent(aggregate),
        1,
        event(60),
        None,
        frame(60),
        digest(160),
    )
}

fn f0_activation_append() -> AppendRequest {
    let campaign = aggregate(AggregateKind::EvolutionCampaign, 70);
    let pointer = aggregate(AggregateKind::ProductionHarness, 71);
    let campaign_event = EventDraft::new(
        campaign,
        EventSequence::first(),
        event(71),
        None,
        ExactFrame::new(
            encode_frame(89, 1, &[1], CodecLimits::PRODUCTION).expect("campaign frame"),
        )
        .expect("exact campaign frame"),
        digest(171),
        Vec::new(),
    )
    .expect("campaign event");
    let pointer_event = EventDraft::new(
        pointer,
        EventSequence::first(),
        event(72),
        None,
        ExactFrame::new(encode_frame(92, 1, &[2], CodecLimits::PRODUCTION).expect("pointer frame"))
            .expect("exact pointer frame"),
        digest(172),
        Vec::new(),
    )
    .expect("pointer event");
    AppendRequest::new(
        store_id(),
        command(61),
        digest(61),
        vec![HeadExpectation::Absent(campaign), HeadExpectation::Absent(pointer)],
        vec![campaign_event, pointer_event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
}
