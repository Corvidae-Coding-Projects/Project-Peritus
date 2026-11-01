//! Public-API regression: causal order is independent of opaque message identity order.
use peritus_collaboration::{
    CollaborationBinding, CollaborationCommand, CollaborationCommandKind, CollaborationId,
    CollaborationLimits, CollaborationMessage, CollaborationMessageId, CollaborationState,
    CollaborationTaskId, Delegation, JoinPolicy, ReservationObservation, decide, start,
};
use peritus_role::HarnessRole;
use peritus_scheduler::{DispatchId, SchedulerId, WorkId};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

fn debug(value: impl std::fmt::Debug) -> String {
    format!("{value:?}")
}
fn bytes(value: u8) -> [u8; 16] {
    u128::from(value).to_be_bytes()
}
fn command(
    state: Option<&CollaborationState>,
    serial: u8,
    revision: RevisionTuple,
    kind: CollaborationCommandKind,
) -> Result<CollaborationCommand, String> {
    CollaborationCommand::new(
        CommandId::new(bytes(serial)).map_err(debug)?,
        EventId::new(bytes(serial)).map_err(debug)?,
        RunId::new(bytes(1)).map_err(debug)?,
        state.map_or(0, |state| state.sequence().get()),
        state.map(CollaborationState::last_event_id),
        state.map_or(Sha256Digest::new([0; 32]), CollaborationState::state_digest),
        revision,
        kind,
    )
    .map_err(debug)
}
fn probe(ids: [u8; 3]) -> Result<(), String> {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new(bytes(1)).map_err(debug)?,
        HarnessId::new(bytes(1)).map_err(debug)?,
        WorkspaceId::new(bytes(1)).map_err(debug)?,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(1)).map_err(debug)?,
        ProviderProfileId::new(bytes(1)).map_err(debug)?,
    );
    let task = CollaborationTaskId::new(bytes(1)).map_err(debug)?;
    let owner = ActorId::new(bytes(1)).map_err(debug)?;
    let work = WorkId::new(bytes(1)).map_err(debug)?;
    let root = Delegation::root(
        task,
        owner,
        HarnessRole::Writer,
        work,
        Sha256Digest::new([1; 32]),
        JoinPolicy::NoChildren,
    )
    .map_err(debug)?;
    let limits =
        CollaborationLimits::new(8, 4, 4, 16, 4, 4096, 4, 1_048_576, 1_048_576).map_err(debug)?;
    let binding = CollaborationBinding::new(
        CollaborationId::new(bytes(1)).map_err(debug)?,
        RunId::new(bytes(1)).map_err(debug)?,
        revision,
        SchedulerId::new(bytes(1)).map_err(debug)?,
        limits,
        root,
    )
    .map_err(debug)?;
    let genesis = command(None, 1, revision, CollaborationCommandKind::Start { binding })?;
    let transition = start(&genesis).map_err(debug)?;
    let mut state = transition.into_state();
    let activate = command(
        Some(&state),
        2,
        revision,
        CollaborationCommandKind::ActivateTask {
            task_id: task,
            observation: ReservationObservation::new(
                work,
                DispatchId::new(bytes(1)).map_err(debug)?,
                owner,
                revision,
            ),
        },
    )?;
    let transition = decide(&state, &activate).map_err(debug)?;
    state = transition.into_state();
    for (index, id) in ids.into_iter().enumerate() {
        let ordinal = u32::try_from(index + 1).map_err(debug)?;
        let predecessor = if index == 0 {
            None
        } else {
            Some(CollaborationMessageId::new(bytes(ids[index - 1])).map_err(debug)?)
        };
        let message = CollaborationMessage::new(
            CollaborationMessageId::new(bytes(id)).map_err(debug)?,
            task,
            task,
            owner,
            owner,
            ordinal,
            predecessor,
            "text/plain",
            7,
            Sha256Digest::new([id; 32]),
            None,
            revision,
        )
        .map_err(debug)?;
        let request = command(
            Some(&state),
            u8::try_from(index + 3).map_err(debug)?,
            revision,
            CollaborationCommandKind::SendMessage { message },
        )?;
        let before = state.clone();
        match decide(&state, &request) {
            Ok(transition) => {
                state = transition.into_state();
                println!("ids={ids:?} ordinal={ordinal} id={id} result=accepted");
            }
            Err(error) => {
                if state != before {
                    return Err("rejected message changed accepted state".into());
                }
                return Err(format!(
                    "valid causal successor rejected for IDs {ids:?} at ordinal {ordinal}: {error}"
                ));
            }
        }
    }
    Ok(())
}
fn main() -> Result<(), String> {
    probe([10, 20, 30])?;
    probe([30, 20, 10])
}
