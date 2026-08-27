use peritus_types::{ActorId, ArtifactId, SessionId, Sha256Digest, WorkspaceId};
use rusqlite::params;

use crate::{
    ApplicationArtifactState, ApplicationCommandAdmission, ApplicationCommandSettlement,
    ApplicationCommandState, ApplicationPrincipalKind, ApplicationRequestId,
    ApplicationSessionState, ApplicationWorkspaceState, HeadExpectation, NewApplicationArtifact,
    NewApplicationCommand, NewApplicationPrincipal, NewApplicationSession, NewApplicationWorkspace,
};

use super::{command, draft, event, key, open, plan};
use crate::AggregateKind;
use tempfile::TempDir;

fn id16(value: u8) -> [u8; 16] {
    [value; 16]
}

#[test]
fn global_tail_is_bounded_and_reports_retention_bounds() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let empty = journal.global_events_after(0, 2).expect("empty global window");
    assert_eq!((empty.earliest(), empty.latest()), (0, 0));
    assert!(empty.records().is_empty());

    for seed in 1..=3 {
        let aggregate = key(AggregateKind::Kernel, seed);
        journal
            .append(plan(
                command(seed),
                Sha256Digest::new([seed; 32]),
                HeadExpectation::Absent(aggregate),
                vec![draft(aggregate, 1, event(seed), None, seed)],
            ))
            .expect("append global event");
    }

    let first = journal.global_events_after(0, 2).expect("first bounded window");
    assert_eq!((first.earliest(), first.latest()), (1, 3));
    assert_eq!(
        first.records().iter().map(|record| record.global_position()).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(!first.has_retention_gap_after(0));

    let second = journal.global_events_after(2, 2).expect("second bounded window");
    assert_eq!(second.records().len(), 1);
    assert_eq!(second.records()[0].global_position(), 3);
    assert!(journal.global_events_after(0, 0).is_err());
}

#[test]
fn application_ledger_survives_idempotency_and_terminal_settlement() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let actor = ActorId::new(id16(10)).expect("actor");
    let session = SessionId::new(id16(11)).expect("session");
    let principal_digest = Sha256Digest::new([12; 32]);
    let principal = journal
        .bind_application_principal(NewApplicationPrincipal::new(
            principal_digest,
            ApplicationPrincipalKind::UnixPeer,
            actor,
            Sha256Digest::new([13; 32]),
        ))
        .expect("bind principal");
    assert_eq!(principal.actor_id(), actor);
    assert_eq!(
        journal
            .bind_application_principal(NewApplicationPrincipal::new(
                principal_digest,
                ApplicationPrincipalKind::UnixPeer,
                actor,
                Sha256Digest::new([13; 32]),
            ))
            .expect("repeat exact principal"),
        principal
    );

    let opened = journal
        .open_application_session(
            NewApplicationSession::new(session, actor, 1, 100, [4; 16], 1, 0)
                .expect("session facts"),
        )
        .expect("open session");
    assert_eq!(opened.state(), ApplicationSessionState::Active);

    let command_id = command(20);
    let request_digest = Sha256Digest::new([21; 32]);
    let new_command = || {
        NewApplicationCommand::new(
            actor,
            session,
            b"build-workspace".to_vec(),
            request_digest,
            request_digest,
            ApplicationRequestId::new(id16(22)).expect("request"),
            command_id,
        )
        .expect("command admission")
    };
    let inserted = journal.admit_application_command(new_command()).expect("admit command");
    assert!(matches!(inserted, ApplicationCommandAdmission::Inserted(_)));

    let aggregate = key(AggregateKind::Kernel, 23);
    let committed = journal
        .append(plan(
            command_id,
            request_digest,
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(24), None, 24)],
        ))
        .expect("append application command");
    let result_digest = Sha256Digest::new([25; 32]);
    let settled = journal
        .settle_application_command(
            command_id,
            request_digest,
            ApplicationCommandSettlement::committed(&committed, result_digest),
        )
        .expect("settle committed command");
    assert_eq!(settled.state(), ApplicationCommandState::Committed);
    assert_eq!((settled.first_position(), settled.last_position()), (Some(1), Some(1)));
    assert_eq!(settled.result_digest(), Some(result_digest));

    let replay = journal.admit_application_command(new_command()).expect("classify replay");
    assert!(
        matches!(replay, ApplicationCommandAdmission::Existing(record) if record.state() == ApplicationCommandState::Committed)
    );
    let conflicting = NewApplicationCommand::new(
        actor,
        session,
        b"build-workspace".to_vec(),
        Sha256Digest::new([99; 32]),
        Sha256Digest::new([98; 32]),
        ApplicationRequestId::new(id16(26)).expect("request"),
        command(27),
    )
    .expect("conflicting admission");
    assert!(matches!(
        journal.admit_application_command(conflicting).expect("classify conflict"),
        ApplicationCommandAdmission::Conflict(_)
    ));
}

#[test]
fn application_catalogs_retain_exact_metadata() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Kernel, 30);
    journal
        .append(plan(
            command(30),
            Sha256Digest::new([30; 32]),
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(30), None, 30)],
        ))
        .expect("producing event");

    let artifact_id = ArtifactId::new(id16(31)).expect("artifact");
    journal
        .begin_application_artifact(
            NewApplicationArtifact::new(
                artifact_id,
                Sha256Digest::new([32; 32]),
                512,
                "application/octet-stream".to_owned(),
            )
            .expect("artifact metadata"),
        )
        .expect("begin artifact");
    let artifact =
        journal.complete_application_artifact(artifact_id, 1).expect("complete artifact");
    assert_eq!(artifact.state(), ApplicationArtifactState::Available);
    assert_eq!(artifact.producing_position(), Some(1));

    let workspace_id = WorkspaceId::new(id16(33)).expect("workspace");
    let registration = vec![4, 5, 6];
    let registration_digest = peritus_codec::sha256(&registration);
    let workspace = journal
        .register_application_workspace(
            NewApplicationWorkspace::new(workspace_id, registration.clone(), registration_digest)
                .expect("workspace registration"),
        )
        .expect("register workspace");
    assert_eq!(workspace.registration_bytes(), registration);
    let unavailable = journal
        .set_application_workspace_state(workspace_id, ApplicationWorkspaceState::Unavailable)
        .expect("make workspace unavailable");
    assert_eq!(unavailable.state(), ApplicationWorkspaceState::Unavailable);

    let earlier_id = WorkspaceId::new(id16(32)).expect("earlier workspace");
    let earlier_registration = vec![1, 2, 3];
    journal
        .register_application_workspace(
            NewApplicationWorkspace::new(
                earlier_id,
                earlier_registration.clone(),
                peritus_codec::sha256(&earlier_registration),
            )
            .expect("earlier workspace registration"),
        )
        .expect("register earlier workspace");
    let first = journal.application_workspace_page(None, 1).expect("first workspace page");
    assert_eq!(first.workspaces().len(), 1);
    assert_eq!(first.workspaces()[0].workspace_id(), earlier_id);
    assert_eq!(first.next_after(), Some(earlier_id));
    let second =
        journal.application_workspace_page(first.next_after(), 1).expect("second workspace page");
    assert_eq!(second.workspaces().len(), 1);
    assert_eq!(second.workspaces()[0].workspace_id(), workspace_id);
    assert_eq!(second.workspaces()[0].state(), ApplicationWorkspaceState::Unavailable);
    assert_eq!(second.next_after(), None);
    assert_eq!(
        journal
            .application_workspace_page(None, 0)
            .expect_err("zero recovery page must fail")
            .kind(),
        crate::JournalErrorKind::InvalidInput,
    );
}

#[test]
fn application_workspace_digest_is_checked_on_input_and_recovery() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let workspace_id = WorkspaceId::new(id16(40)).expect("workspace");
    let registration = vec![9, 8, 7];
    assert_eq!(
        NewApplicationWorkspace::new(
            workspace_id,
            registration.clone(),
            Sha256Digest::new([1; 32]),
        )
        .expect_err("mismatched registration digest must fail")
        .kind(),
        crate::JournalErrorKind::InvalidInput,
    );
    journal
        .register_application_workspace(
            NewApplicationWorkspace::new(
                workspace_id,
                registration.clone(),
                peritus_codec::sha256(&registration),
            )
            .expect("checked workspace registration"),
        )
        .expect("register workspace");
    journal
        .connection
        .execute(
            "UPDATE app_workspaces SET registration_digest = ?1 WHERE workspace_id = ?2",
            params![[2_u8; 32].as_slice(), workspace_id.as_bytes().as_slice()],
        )
        .expect("inject durable digest corruption");
    assert_eq!(
        journal
            .application_workspace_page(None, 8)
            .expect_err("recovery must reject corrupt registration")
            .kind(),
        crate::JournalErrorKind::CorruptJournal,
    );
}
