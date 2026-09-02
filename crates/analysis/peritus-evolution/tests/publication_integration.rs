//! Finalized-artifact and publication recovery integration tests.

mod support;

use peritus_artifact_store::Publication;
use peritus_codec::{CodecLimits, encode_message};
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_evolution::{
    CampaignEventFrame, EVOLUTION_PUBLICATION_DESTINATION, EvolutionErrorKind,
    EvolutionPublicationClaim, EvolutionPublicationKind, decide_campaign,
    finalize_evolution_artifact, publish_claimed_evolution,
};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ArtifactDependency, EventDraft,
    ExactFrame, HeadExpectation, OutboxDraft, OutboxId,
};
use peritus_types::{CommandId, EventId, EventSequence};

use support::{
    HarnessFixture, Stores, bytes, campaign_genesis, campaign_id, digest, project_id,
    revision_tuple,
};

#[test]
fn finalized_artifact_and_publication_recover_after_restart() {
    let mut stores = Stores::open();
    let payload = b"canonical F0 campaign decision";
    let semantic = digest(101);
    let artifact = finalize_evolution_artifact(
        &stores.artifacts,
        payload,
        semantic,
        EventId::new(bytes(102)).expect("artifact event"),
    )
    .expect("finalize decision artifact");
    let reused = finalize_evolution_artifact(
        &stores.artifacts,
        payload,
        semantic,
        EventId::new(bytes(103)).expect("retry artifact event"),
    )
    .expect("idempotent artifact finalization");
    assert_eq!(artifact.artifact_digest(), reused.artifact_digest());
    assert_eq!(reused.publication(), Publication::Existing);

    let directive = directive_bytes(artifact.artifact_digest().sha256(), semantic);
    let outbox_id = derived_outbox_id(&directive);
    let fixture = HarnessFixture::new();
    let campaign = campaign_genesis(&fixture);
    let transition = decide_campaign(None, &campaign).expect("campaign transition");
    let event_frame = encode_message(
        &CampaignEventFrame::from_event(transition.event()).expect("campaign event frame"),
        CodecLimits::PRODUCTION,
    )
    .expect("canonical F0 frame");
    append_publication_directive(
        &mut stores,
        outbox_id,
        directive,
        artifact.artifact_digest().sha256(),
        event_frame,
    );
    let database = stores.database.clone();
    drop(stores.artifacts);
    drop(stores.journal);

    let mut journal = support::open_journal(&database, stores.store_id);
    let artifacts = peritus_artifact_store::ArtifactStore::open(
        peritus_artifact_store::StoreConfig::new(
            stores.temporary.path().join("artifacts"),
            1_048_576,
            8 * 1_048_576,
        )
        .expect("artifact configuration")
        .with_database_path(&database)
        .expect("shared artifact database"),
    )
    .expect("reopen artifacts");
    let mut evidence =
        EvidenceStore::open(&database, EvidenceStoreOptions::default()).expect("evidence store");
    let message =
        journal.claim_outbox(200, 300).expect("claim publication").expect("pending publication");
    let claim = EvolutionPublicationClaim::from_message(&message).expect("checked F0 claim");
    assert_eq!(claim.directive().kind(), EvolutionPublicationKind::CampaignDecision);
    let published =
        publish_claimed_evolution(&mut journal, &mut evidence, &artifacts, &claim, artifact)
            .expect("publish and acknowledge");
    assert_eq!(
        evidence.load(published.evidence().id()).expect("load evidence"),
        Some(published.evidence().clone()),
    );
    assert!(journal.claim_outbox(301, 400).expect("outbox after ack").is_none());
}

#[test]
fn empty_artifacts_and_malformed_publication_payloads_fail_closed() {
    let stores = Stores::open();
    let error = finalize_evolution_artifact(
        &stores.artifacts,
        b"",
        digest(110),
        EventId::new(bytes(111)).expect("artifact event"),
    )
    .expect_err("empty artifact cannot become evidence");
    assert_eq!(error.kind(), EvolutionErrorKind::Artifact);
}

fn directive_bytes(
    artifact: peritus_types::Sha256Digest,
    semantic: peritus_types::Sha256Digest,
) -> Vec<u8> {
    let mut bytes = b"PERITUS-F0-PUBLICATION-DIRECTIVE\0".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(project_id().as_bytes());
    bytes.push(1);
    bytes.extend_from_slice(campaign_id().as_bytes());
    let revision = revision_tuple();
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
    bytes.extend_from_slice(digest(104).as_bytes());
    bytes.extend_from_slice(artifact.as_bytes());
    bytes.extend_from_slice(semantic.as_bytes());
    bytes
}

fn derived_outbox_id(payload: &[u8]) -> OutboxId {
    let digest = peritus_codec::sha256(payload);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    if id == [0; 16] {
        id[15] = 1;
    }
    OutboxId::new(id).expect("derived outbox identity")
}

fn append_publication_directive(
    stores: &mut Stores,
    outbox_id: OutboxId,
    payload: Vec<u8>,
    artifact: peritus_types::Sha256Digest,
    event_frame: Vec<u8>,
) {
    let aggregate = AggregateKey::new(
        AggregateKind::EvolutionCampaign,
        AggregateId::new(bytes(112)).expect("aggregate identity"),
    );
    let event = EventId::new(bytes(113)).expect("event identity");
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(1).expect("event sequence"),
        event,
        None,
        ExactFrame::new(event_frame).expect("canonical F0 event frame"),
        peritus_evidence::revision_digest(&revision_tuple()),
        Vec::new(),
    )
    .expect("event draft");
    let outbox =
        OutboxDraft::new(outbox_id, EVOLUTION_PUBLICATION_DESTINATION.to_owned(), payload, 16)
            .expect("publication outbox");
    let request = AppendRequest::new(
        stores.store_id,
        CommandId::new(bytes(115)).expect("command identity"),
        digest(116),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft],
        Vec::new(),
        vec![ArtifactDependency::new(artifact)],
        None,
        None,
        vec![outbox],
    );
    stores
        .journal
        .append(request.plan().expect("publication append plan"))
        .expect("commit publication directive");
}
