//! B0 commit and restart recovery through the reducer-driven replay port.

mod support;

use peritus_codec::CodecLimits;
use peritus_journal::{
    HeadExpectation, KernelCommitRequest, KernelReplayCapsule, KernelReplayDriver,
    KernelReplayFailure,
};
use peritus_kernel::{
    CommandEnvelope, KernelAggregate, KernelCommand, KernelGenesis, KernelTransition,
    ReducerInputs, SessionPhase,
};
use peritus_types::{CommandId, EventId};
use tempfile::TempDir;

use support::{DomainIds, contract_dto, kernel_append, kernel_key, open, revision_for_contract};

#[derive(Default)]
struct ReplayDriver {
    genesis_calls: usize,
    transition_calls: usize,
}

impl KernelReplayDriver for ReplayDriver {
    fn replay_genesis(
        &mut self,
        capsule: &KernelReplayCapsule,
    ) -> Result<KernelGenesis, KernelReplayFailure> {
        self.genesis_calls += 1;
        if !capsule.is_genesis() || capsule.command().is_some() || !capsule.inputs().is_empty() {
            return Err(KernelReplayFailure::new("unexpected genesis capsule"));
        }
        let contract = contract_dto()
            .try_into_domain(CodecLimits::PRODUCTION)
            .map_err(|_| KernelReplayFailure::new("contract fixture failed"))?;
        KernelAggregate::open(
            capsule.project_id(),
            capsule.session_id(),
            &contract,
            capsule.envelope().revision(),
            capsule.envelope(),
        )
        .map_err(|_| KernelReplayFailure::new("genesis reducer rejected replay"))
    }

    fn replay_transition(
        &mut self,
        before: KernelAggregate,
        capsule: &KernelReplayCapsule,
    ) -> Result<KernelTransition, KernelReplayFailure> {
        self.transition_calls += 1;
        if capsule.is_genesis() || !capsule.inputs().is_empty() {
            return Err(KernelReplayFailure::new("unexpected transition capsule"));
        }
        let command = capsule
            .command()
            .cloned()
            .ok_or_else(|| KernelReplayFailure::new("transition command missing"))?;
        let contract = contract_dto()
            .try_into_domain(CodecLimits::PRODUCTION)
            .map_err(|_| KernelReplayFailure::new("contract fixture failed"))?;
        before
            .reduce(capsule.envelope(), command, ReducerInputs::new(&contract))
            .into_result()
            .map_err(|_| KernelReplayFailure::new("transition reducer rejected replay"))
    }
}

#[test]
fn genesis_and_transition_recover_exactly_after_journal_restart() {
    let temp = TempDir::new().expect("temporary directory");
    let ids = DomainIds::new(*b"kernel01");
    let contract =
        contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("checked contract fixture");
    let revision = revision_for_contract(contract.id(), &ids);
    let key = kernel_key(ids.session);

    let genesis_envelope = CommandEnvelope::new(
        CommandId::new([0x31; 16]).expect("genesis command"),
        EventId::new([0x41; 16]).expect("genesis event"),
        None,
        revision,
    );
    let genesis =
        KernelAggregate::open(ids.project, ids.session, &contract, revision, genesis_envelope)
            .expect("kernel genesis");
    let genesis_event = genesis.event();

    let expected_after_pause = {
        let mut journal = open(&temp);
        let genesis_request = KernelCommitRequest::genesis(
            kernel_append(genesis_envelope, genesis_event, HeadExpectation::Absent(key)),
            genesis,
            genesis_envelope,
            Vec::new(),
        )
        .expect("bind genesis commit");
        let committed_genesis =
            journal.commit_kernel_transition(genesis_request).expect("commit genesis");
        assert_eq!(committed_genesis.batch().first_position(), 1);
        assert_eq!(committed_genesis.aggregate().session().phase(), SessionPhase::Open);

        let before = committed_genesis.aggregate().clone();
        let pause_envelope = CommandEnvelope::new(
            CommandId::new([0x32; 16]).expect("pause command"),
            EventId::new([0x42; 16]).expect("pause event"),
            Some(before.head_event_id()),
            before.revision(),
        );
        let command = KernelCommand::PauseSession;
        let transition = before
            .reduce(pause_envelope, command.clone(), ReducerInputs::new(&contract))
            .into_result()
            .expect("pause transition");
        let transition_event = transition.event();
        let head = journal.head(key).expect("genesis head").expect("present");
        let transition_request = KernelCommitRequest::transition(
            kernel_append(pause_envelope, transition_event, HeadExpectation::Present(head)),
            transition,
            pause_envelope,
            command,
            Vec::new(),
        )
        .expect("bind pause commit");
        let committed_pause =
            journal.commit_kernel_transition(transition_request).expect("commit pause");
        assert_eq!(committed_pause.batch().last_position(), 2);
        assert_eq!(committed_pause.aggregate().session().phase(), SessionPhase::Paused);
        committed_pause.aggregate().clone()
    };

    let reopened = open(&temp);
    let mut driver = ReplayDriver::default();
    let recovered = reopened.recover_kernel(key, &mut driver).expect("restart recovery");
    assert_eq!(recovered.transition_count(), 2);
    assert_eq!(recovered.aggregate(), &expected_after_pause);
    assert_eq!(recovered.aggregate().session().phase(), SessionPhase::Paused);
    assert_eq!(driver.genesis_calls, 1);
    assert_eq!(driver.transition_calls, 1);
}
