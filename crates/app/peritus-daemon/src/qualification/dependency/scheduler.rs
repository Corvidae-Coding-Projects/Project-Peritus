//! Deterministic C5 scheduler fixture and C0 durability for dependency qualification.

use peritus_journal::{SqliteJournal, StoreId};
use peritus_scheduler::{
    AttemptNumber, DispatchId, ExecutionClass, FailureDisposition, RecoveryPolicy, ResourceEntry,
    ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding, SchedulerCommand,
    SchedulerCommandKind, SchedulerId, SchedulerLimits, SchedulerReservation, SchedulerState,
    WorkId, WorkPhase, WorkSpec, WorkerDescriptor, WorkerId, commit_scheduler_transition, decide,
    load_scheduler_replay, start,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{DependencyFault, DependencyKind, dependency_error};

pub(super) struct DependencyCampaign {
    identity: Identity,
    state: SchedulerState,
    next_ordinal: u16,
}

impl DependencyCampaign {
    pub(super) fn start(
        journal: &mut SqliteJournal,
        store_id: StoreId,
        dependency: DependencyKind,
        fault: DependencyFault,
        maximum_attempts: u16,
    ) -> Result<Self, DaemonError> {
        let identity = Identity::new(store_id, dependency, fault, maximum_attempts)?;
        let report = journal.integrity_scan().map_err(journal_error)?;
        if report.event_count() != 0 || report.aggregate_count() != 0 {
            return Err(dependency_error(
                "start dependency scheduler",
                "qualification journal is not empty",
            ));
        }
        let command = identity.command(
            None,
            1,
            SchedulerCommandKind::StartScheduler { binding: identity.binding.clone() },
        )?;
        let transition = start(&command).map_err(scheduler_error)?;
        commit_scheduler_transition(journal, &command, &transition).map_err(scheduler_error)?;
        let mut campaign = Self { identity, state: transition.into_state(), next_ordinal: 2 };
        campaign.commit(
            journal,
            SchedulerCommandKind::RegisterWorker { descriptor: campaign.identity.worker()? },
        )?;
        campaign.commit(
            journal,
            SchedulerCommandKind::AdmitWork { spec: campaign.identity.work(maximum_attempts)? },
        )?;
        campaign.dispatch(journal)?;
        Ok(campaign)
    }

    pub(super) fn reopen(
        journal: &SqliteJournal,
        store_id: StoreId,
        dependency: DependencyKind,
        fault: DependencyFault,
        maximum_attempts: u16,
    ) -> Result<Self, DaemonError> {
        let identity = Identity::new(store_id, dependency, fault, maximum_attempts)?;
        let replay =
            load_scheduler_replay(journal, identity.binding.run_id()).map_err(scheduler_error)?;
        let state = replay
            .rebuild()
            .map_err(scheduler_error)?
            .ok_or_else(|| dependency_error("reopen dependency scheduler", "state is absent"))?;
        let next_ordinal = u16::try_from(state.sequence().get())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| dependency_error("reopen dependency scheduler", "sequence overflow"))?;
        Ok(Self { identity, state, next_ordinal })
    }

    pub(super) const fn state(&self) -> &SchedulerState {
        &self.state
    }

    pub(super) fn active_reservation(&self) -> Result<SchedulerReservation, DaemonError> {
        match self.state.reservations() {
            [reservation] if reservation.started() => Ok(reservation.clone()),
            _ => Err(dependency_error(
                "observe dependency reservation",
                "scheduler does not retain exactly one started reservation",
            )),
        }
    }

    pub(super) fn fail_retryable(
        &mut self,
        journal: &mut SqliteJournal,
        failure_digest: Sha256Digest,
    ) -> Result<(), DaemonError> {
        let dispatch_id = self.active_reservation()?.dispatch_id();
        self.commit(
            journal,
            SchedulerCommandKind::FailWork {
                dispatch_id,
                failure_digest,
                disposition: FailureDisposition::Retryable,
            },
        )
    }

    pub(super) fn retry_and_dispatch(
        &mut self,
        journal: &mut SqliteJournal,
    ) -> Result<(), DaemonError> {
        self.retry_pending(journal)?;
        self.dispatch(journal)
    }

    pub(super) fn retry_pending(&mut self, journal: &mut SqliteJournal) -> Result<(), DaemonError> {
        let work = self.state.work().first().ok_or_else(|| {
            dependency_error("retry dependency work", "qualification work is absent")
        })?;
        if work.phase() != WorkPhase::RetryPending {
            return Err(dependency_error(
                "retry dependency work",
                "qualification work is not retry-pending",
            ));
        }
        self.commit(journal, SchedulerCommandKind::RetryWork { work_id: self.identity.work_id })
    }

    pub(super) fn verify_stage(
        &self,
        fault: DependencyFault,
        expected_attempts: u16,
    ) -> Result<(), DaemonError> {
        let work = self.state.work().first().ok_or_else(|| {
            dependency_error("verify dependency stage", "qualification work is absent")
        })?;
        let correct = self.state.reservations().is_empty()
            && work.attempts_started() == expected_attempts
            && match fault {
                DependencyFault::Death => work.phase() == WorkPhase::RetryPending,
                DependencyFault::RetryExhaustion => {
                    work.phase() == WorkPhase::Terminal
                        && matches!(
                            work.terminal(),
                            Some(peritus_scheduler::WorkTerminal::Exhausted { .. })
                        )
                }
            };
        if !correct {
            return Err(dependency_error(
                "verify dependency stage",
                "scheduler did not retain the expected retry or exhaustion truth",
            ));
        }
        Ok(())
    }

    fn dispatch(&mut self, journal: &mut SqliteJournal) -> Result<(), DaemonError> {
        let ordinal = self.next_ordinal;
        let dispatch_id = DispatchId::new(self.identity.id(b"dispatch", ordinal))
            .map_err(|_| dependency_error("derive dependency dispatch", "identity is invalid"))?;
        let token = self.identity.digest(b"dispatch-token", ordinal);
        self.commit(
            journal,
            SchedulerCommandKind::DispatchNext { dispatch_id, dispatch_token: token },
        )?;
        self.commit(journal, SchedulerCommandKind::AcknowledgeStart { dispatch_id })
    }

    fn commit(
        &mut self,
        journal: &mut SqliteJournal,
        kind: SchedulerCommandKind,
    ) -> Result<(), DaemonError> {
        let ordinal = self.next_ordinal;
        let command = self.identity.command(Some(&self.state), ordinal, kind)?;
        let transition = decide(&self.state, &command).map_err(scheduler_error)?;
        commit_scheduler_transition(journal, &command, &transition).map_err(scheduler_error)?;
        self.state = transition.into_state();
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| dependency_error("advance dependency scheduler", "ordinal overflow"))?;
        Ok(())
    }
}

struct Identity {
    store_id: StoreId,
    dependency: DependencyKind,
    fault: DependencyFault,
    binding: SchedulerBinding,
    owner: ActorId,
    work_id: WorkId,
    limits: SchedulerLimits,
}

impl Identity {
    fn new(
        store_id: StoreId,
        dependency: DependencyKind,
        fault: DependencyFault,
        maximum_attempts: u16,
    ) -> Result<Self, DaemonError> {
        let limits =
            SchedulerLimits::new(8, 16, 2, 1, 2, 1, maximum_attempts, 32, 1, 65_536, 1_048_576)
                .map_err(scheduler_error)?;
        let seed = Seed { store_id, dependency, fault };
        let revision = RevisionTuple::new(
            AcceptanceSpecId::new(seed.id(b"acceptance", 0))
                .map_err(|_| identity_error("acceptance"))?,
            HarnessId::new(seed.id(b"harness", 0)).map_err(|_| identity_error("harness"))?,
            WorkspaceId::new(seed.id(b"workspace", 0)).map_err(|_| identity_error("workspace"))?,
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new(seed.id(b"policy", 0)).map_err(|_| identity_error("policy"))?,
            ProviderProfileId::new(seed.id(b"provider-profile", 0))
                .map_err(|_| identity_error("provider profile"))?,
        );
        let binding = SchedulerBinding::new(
            RunId::new(seed.id(b"run", 0)).map_err(|_| identity_error("run"))?,
            SchedulerId::new(seed.id(b"scheduler", 0)).map_err(scheduler_error)?,
            revision,
            limits,
            resources(8, 65_536, limits)?,
        )
        .map_err(scheduler_error)?;
        Ok(Self {
            store_id,
            dependency,
            fault,
            binding,
            owner: ActorId::new(seed.id(b"owner", 0)).map_err(|_| identity_error("owner"))?,
            work_id: WorkId::new(seed.id(b"work", 0)).map_err(scheduler_error)?,
            limits,
        })
    }

    fn worker(&self) -> Result<WorkerDescriptor, DaemonError> {
        WorkerDescriptor::new(
            WorkerId::new(self.id(b"worker", 0)).map_err(scheduler_error)?,
            self.owner,
            vec![self.execution_class()],
            resources(4, 32_768, self.limits)?,
            1,
            self.limits,
        )
        .map_err(scheduler_error)
    }

    fn work(&self, attempts: u16) -> Result<WorkSpec, DaemonError> {
        WorkSpec::new(
            self.work_id,
            self.owner,
            self.binding.revision(),
            self.execution_class(),
            1,
            resources(1, 4_096, self.limits)?,
            None,
            Vec::new(),
            None,
            AttemptNumber::new(attempts).map_err(scheduler_error)?,
            RecoveryPolicy::RetrySafe,
            self.digest(b"payload", 0),
            self.limits,
        )
        .map_err(scheduler_error)
    }

    fn command(
        &self,
        state: Option<&SchedulerState>,
        ordinal: u16,
        kind: SchedulerCommandKind,
    ) -> Result<SchedulerCommand, DaemonError> {
        let (sequence, previous, digest) = state
            .map_or((0, None, Sha256Digest::new([0; 32])), |s| {
                (s.sequence().get(), Some(s.last_event_id()), s.state_digest())
            });
        SchedulerCommand::new(
            CommandId::new(self.id(b"command", ordinal)).map_err(|_| identity_error("command"))?,
            EventId::new(self.id(b"event", ordinal)).map_err(|_| identity_error("event"))?,
            self.binding.run_id(),
            sequence,
            previous,
            digest,
            self.binding.revision(),
            kind,
        )
        .map_err(scheduler_error)
    }

    const fn execution_class(&self) -> ExecutionClass {
        match self.dependency {
            DependencyKind::Provider => ExecutionClass::Model,
            DependencyKind::Tool => ExecutionClass::Tool,
            DependencyKind::Worker => ExecutionClass::Coordination,
        }
    }

    fn id(&self, domain: &[u8], ordinal: u16) -> [u8; 16] {
        Seed { store_id: self.store_id, dependency: self.dependency, fault: self.fault }
            .id(domain, ordinal)
    }

    fn digest(&self, domain: &[u8], ordinal: u16) -> Sha256Digest {
        Seed { store_id: self.store_id, dependency: self.dependency, fault: self.fault }
            .digest(domain, ordinal)
    }
}

#[derive(Clone, Copy)]
struct Seed {
    store_id: StoreId,
    dependency: DependencyKind,
    fault: DependencyFault,
}

impl Seed {
    fn id(self, domain: &[u8], ordinal: u16) -> [u8; 16] {
        let digest = self.digest(domain, ordinal);
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes
    }

    fn digest(self, domain: &[u8], ordinal: u16) -> Sha256Digest {
        let mut bytes = Vec::with_capacity(domain.len() + 64);
        bytes.extend_from_slice(b"peritus/h1/dependency-scheduler/v1\0");
        bytes.extend_from_slice(self.store_id.as_bytes());
        bytes.extend_from_slice(self.dependency.code().as_bytes());
        bytes.extend_from_slice(self.fault.code().as_bytes());
        bytes.extend_from_slice(domain);
        bytes.extend_from_slice(&ordinal.to_be_bytes());
        peritus_codec::sha256(&bytes)
    }
}

fn resources(
    cpu: u64,
    memory: u64,
    limits: SchedulerLimits,
) -> Result<ResourceVector, DaemonError> {
    ResourceVector::new(
        vec![
            ResourceEntry::new(
                ResourceKind::CPU,
                ResourceQuantity::new(cpu).map_err(scheduler_error)?,
            ),
            ResourceEntry::new(
                ResourceKind::MEMORY_BYTES,
                ResourceQuantity::new(memory).map_err(scheduler_error)?,
            ),
        ],
        limits.resource_dimensions(),
    )
    .map_err(scheduler_error)
}

pub(super) fn digest_hex(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn scheduler_error(error: peritus_scheduler::SchedulerError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify dependency scheduler",
        error.to_string(),
        error,
    )
}

fn journal_error(error: peritus_journal::JournalError) -> DaemonError {
    super::super::journal_error(error)
}

fn identity_error(kind: &'static str) -> DaemonError {
    dependency_error("derive dependency identity", format!("{kind} identity is invalid"))
}
