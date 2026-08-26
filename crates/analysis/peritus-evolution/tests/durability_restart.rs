//! Durable F0 commit and restart-recovery tests.

mod support;

use peritus_evolution::{
    CampaignCommandKind, CampaignPhase, EvolutionErrorKind, PointerCommandKind, PointerPhase,
    commit_campaign_transition, commit_pointer_transition, decide_campaign, decide_pointer,
    finalize_evolution_artifact, recover_campaign, recover_pointer,
};
use peritus_types::EventId;

use support::{
    HarnessFixture, Stores, bytes, campaign_genesis, campaign_id, digest, next_campaign,
    next_pointer, open_journal, pointer_genesis, project_id,
};

#[test]
fn ordinary_campaign_and_pointer_commits_replay_exact_checkpoints() {
    let fixture = HarnessFixture::new();
    let mut stores = Stores::open();
    let campaign = campaign_genesis(&fixture);
    let created = decide_campaign(None, &campaign).expect("create campaign");
    let first = commit_campaign_transition(&mut stores.journal, &campaign, &created)
        .expect("commit campaign genesis");
    let exact_retry = commit_campaign_transition(&mut stores.journal, &campaign, &created)
        .expect("exact retry before successor");
    assert_eq!(first.batch_hash(), exact_retry.batch_hash());

    let freeze = next_campaign(created.state(), 62, 63, CampaignCommandKind::FreezeCampaign);
    let frozen = decide_campaign(Some(created.state()), &freeze).expect("freeze campaign");
    commit_campaign_transition(&mut stores.journal, &freeze, &frozen)
        .expect("commit campaign freeze");

    let artifact = finalize_evolution_artifact(
        &stores.artifacts,
        b"production initialization evidence",
        digest(72),
        EventId::new(bytes(73)).expect("artifact event"),
    )
    .expect("finalize initialization evidence");
    let pointer =
        pointer_genesis(&fixture, artifact.artifact_digest().sha256(), artifact.semantic_digest());
    let initialized = decide_pointer(None, &pointer).expect("initialize pointer");
    commit_pointer_transition(&mut stores.journal, &pointer, &initialized)
        .expect("commit pointer genesis");

    let stale_cancel = next_pointer(
        initialized.state(),
        74,
        75,
        PointerCommandKind::CancelPending { reason_digest: digest(76) },
    );
    let error = decide_pointer(Some(initialized.state()), &stale_cancel)
        .expect_err("an active pointer has no pending action to cancel");
    assert_eq!(error.kind(), EvolutionErrorKind::IllegalTransition);

    let database = stores.database.clone();
    let store_id = stores.store_id;
    drop(stores.artifacts);
    drop(stores.journal);
    let journal = open_journal(&database, store_id);
    let campaign_replay = recover_campaign(&journal, campaign_id()).expect("campaign restart");
    assert_eq!(campaign_replay.events().len(), 2);
    assert_eq!(campaign_replay.state(), Some(frozen.state()));
    assert_eq!(campaign_replay.state().expect("campaign state").phase(), CampaignPhase::Frozen);
    let pointer_replay = recover_pointer(&journal, project_id()).expect("pointer restart");
    assert_eq!(pointer_replay.events().len(), 1);
    assert_eq!(pointer_replay.state(), Some(initialized.state()));
    assert_eq!(pointer_replay.state().expect("pointer state").phase(), PointerPhase::Active);
}

#[test]
fn stale_campaign_cas_does_not_replace_the_committed_head() {
    let fixture = HarnessFixture::new();
    let mut stores = Stores::open();
    let campaign = campaign_genesis(&fixture);
    let created = decide_campaign(None, &campaign).expect("create campaign");
    commit_campaign_transition(&mut stores.journal, &campaign, &created).expect("commit genesis");
    let winner = next_campaign(created.state(), 81, 82, CampaignCommandKind::FreezeCampaign);
    let winner_transition =
        decide_campaign(Some(created.state()), &winner).expect("winning transition");
    commit_campaign_transition(&mut stores.journal, &winner, &winner_transition)
        .expect("commit winner");
    let loser = next_campaign(created.state(), 83, 84, CampaignCommandKind::FreezeCampaign);
    let loser_transition =
        decide_campaign(Some(created.state()), &loser).expect("independent stale transition");
    let error = commit_campaign_transition(&mut stores.journal, &loser, &loser_transition)
        .expect_err("stale head must lose CAS");
    assert_eq!(error.kind(), EvolutionErrorKind::BindingDrift);
    assert_eq!(
        recover_campaign(&stores.journal, campaign_id()).expect("replay winner").state(),
        Some(winner_transition.state()),
    );
}
