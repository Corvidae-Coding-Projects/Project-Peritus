//! Durable exact-once ownership for native exact-target gate commands.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_process::{
    CancellationReason, CommandSpec, DeadlinePolicy, ExecutionPlan, IoMode, OsExitObservation,
    OutputCompleteness, OutputPolicy, OutputStream, ProcessResourcePolicy, StdinPolicy,
    TerminalDisposition, TerminalResult, WorkingDirectory, WorkspaceAccess,
};
use peritus_types::Sha256Digest;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest as _, Sha256};

use super::{
    CommandRuntime, RuntimeState, authority, contract,
    identity::{self, CommandIds},
    native_gate::{self, GateInvocationOutcome, GateInvocationRequest},
    ordinal,
};

const DATABASE_NAME: &str = "native-gates.sqlite3";
const OUTPUT_SEGMENT_BYTES: u64 = 8 * 1_024 * 1_024;
const OUTPUT_PREVIEW_BYTES: usize = 512 * 1_024;
const OUTPUT_WINDOW_BYTES: usize = OUTPUT_PREVIEW_BYTES / 2;
const RETRY_DELAY: Duration = Duration::from_millis(20);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS native_gate_operations_v1 (
    operation_key BLOB PRIMARY KEY NOT NULL CHECK(length(operation_key) = 32),
    request_digest BLOB NOT NULL CHECK(length(request_digest) = 32)
);
CREATE TABLE IF NOT EXISTS native_gate_attempts_v1 (
    operation_key BLOB NOT NULL CHECK(length(operation_key) = 32),
    request_digest BLOB NOT NULL CHECK(length(request_digest) = 32),
    attempt BLOB NOT NULL CHECK(length(attempt) = 8),
    ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
    action_id BLOB NOT NULL CHECK(length(action_id) = 16),
    process_id BLOB NOT NULL CHECK(length(process_id) = 16),
    replay_identity BLOB NOT NULL CHECK(length(replay_identity) = 32),
    plan_digest BLOB NOT NULL CHECK(length(plan_digest) = 32),
    PRIMARY KEY(operation_key, attempt),
    FOREIGN KEY(operation_key) REFERENCES native_gate_operations_v1(operation_key)
);
CREATE TABLE IF NOT EXISTS native_gate_events_v1 (
    operation_key BLOB NOT NULL CHECK(length(operation_key) = 32),
    attempt BLOB NOT NULL CHECK(length(attempt) = 8),
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    state INTEGER NOT NULL CHECK(state BETWEEN 0 AND 9),
    detail TEXT NOT NULL,
    PRIMARY KEY(operation_key, attempt, sequence),
    FOREIGN KEY(operation_key, attempt)
        REFERENCES native_gate_attempts_v1(operation_key, attempt)
);
CREATE TRIGGER IF NOT EXISTS native_gate_operations_v1_no_update
BEFORE UPDATE ON native_gate_operations_v1 BEGIN SELECT RAISE(ABORT, 'immutable gate operation'); END;
CREATE TRIGGER IF NOT EXISTS native_gate_operations_v1_no_delete
BEFORE DELETE ON native_gate_operations_v1 BEGIN SELECT RAISE(ABORT, 'immutable gate operation'); END;
CREATE TRIGGER IF NOT EXISTS native_gate_attempts_v1_no_update
BEFORE UPDATE ON native_gate_attempts_v1 BEGIN SELECT RAISE(ABORT, 'immutable gate attempt'); END;
CREATE TRIGGER IF NOT EXISTS native_gate_attempts_v1_no_delete
BEFORE DELETE ON native_gate_attempts_v1 BEGIN SELECT RAISE(ABORT, 'immutable gate attempt'); END;
CREATE TRIGGER IF NOT EXISTS native_gate_events_v1_no_update
BEFORE UPDATE ON native_gate_events_v1 BEGIN SELECT RAISE(ABORT, 'immutable gate event'); END;
CREATE TRIGGER IF NOT EXISTS native_gate_events_v1_no_delete
BEFORE DELETE ON native_gate_events_v1 BEGIN SELECT RAISE(ABORT, 'immutable gate event'); END;
";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i64)]
enum AttemptState {
    Reserved = 0,
    LaunchIntended = 1,
    Active = 2,
    RevocationRequested = 3,
    CancellationRequested = 4,
    PublicationPending = 5,
    RetryableBeforeEffect = 6,
    TerminalComplete = 7,
    TerminalRevoked = 8,
    TerminalCancelled = 9,
}

impl AttemptState {
    fn from_i64(value: i64) -> Result<Self, String> {
        match value {
            0 => Ok(Self::Reserved),
            1 => Ok(Self::LaunchIntended),
            2 => Ok(Self::Active),
            3 => Ok(Self::RevocationRequested),
            4 => Ok(Self::CancellationRequested),
            5 => Ok(Self::PublicationPending),
            6 => Ok(Self::RetryableBeforeEffect),
            7 => Ok(Self::TerminalComplete),
            8 => Ok(Self::TerminalRevoked),
            9 => Ok(Self::TerminalCancelled),
            _ => Err("native gate ledger contains an unknown attempt state".to_owned()),
        }
    }

    const fn permits_fresh_attempt(self) -> bool {
        matches!(
            self,
            Self::RetryableBeforeEffect | Self::TerminalRevoked | Self::TerminalCancelled
        )
    }

    const fn terminal(self) -> bool {
        matches!(
            self,
            Self::TerminalComplete | Self::TerminalRevoked | Self::TerminalCancelled
        )
    }

    const fn request_generation_settled(self) -> bool {
        self.terminal() || matches!(self, Self::RetryableBeforeEffect)
    }
}

#[derive(Clone, Copy)]
struct AttemptSnapshot {
    attempt: u64,
    ordinal: u64,
    action_id: [u8; 16],
    process_id: [u8; 16],
    replay_identity: Sha256Digest,
    plan_digest: Sha256Digest,
    state: AttemptState,
    revocation_requested: bool,
    cancellation_requested: bool,
}

#[derive(Clone, Copy)]
struct RetainedAttempt {
    request_digest: Sha256Digest,
    snapshot: AttemptSnapshot,
}

struct PreparedAttempt {
    snapshot: AttemptSnapshot,
    ids: CommandIds,
    contract: peritus_spec::AcceptanceContract,
    plan: ExecutionPlan,
    native: native_gate::PreparedNativeGate,
}

struct GateLedger {
    connection: Connection,
}

struct GateOperationOwner {
    file: File,
    path: PathBuf,
}

impl Drop for GateOperationOwner {
    fn drop(&mut self) {
        if let Err(error) = self.file.unlock() {
            crate::diagnostic::report(&format!(
                "peritus native gate: operation lock release failed at {}: {error}",
                self.path.display()
            ));
        }
    }
}

impl CommandRuntime {
    /// Runs or observes one exact native gate operation without ever redispatching an unsettled
    /// durable attempt.
    pub(crate) fn run_native_gate(
        &self,
        request: GateInvocationRequest,
    ) -> Result<GateInvocationOutcome, String> {
        if !authority_live(&request) {
            return Ok(revoked("gate authority was revoked before durable reservation"));
        }
        if request.cancellation.is_cancelled() {
            return Ok(pending("gate execution was cancelled before durable reservation"));
        }
        let request_digest = request.request_digest();
        let mut ledger = GateLedger::open(&self.inner.state_root, &request)?;
        let Some(_operation_owner) = GateOperationOwner::acquire(
            &self.inner.state_root,
            request.operation_key,
        )? else {
            return Ok(pending(
                "another retained owner is preparing or observing this exact native gate operation",
            ));
        };
        loop {
            if !authority_live(&request) {
                return Ok(revoked("gate authority was revoked before native admission"));
            }
            if request.cancellation.is_cancelled() {
                return Ok(pending("gate execution was cancelled before native admission"));
            }
            let retained = ledger.latest(request.operation_key)?;
            let current = retained.map(|retained| retained.snapshot);
            if let Some(retained) = retained {
                let current = retained.snapshot;
                validate_snapshot(
                    self.inner.run_id,
                    request.operation_key,
                    retained.request_digest,
                    current,
                )?;
                if retained.request_digest != request_digest {
                    if !current.state.request_generation_settled() {
                        return Ok(pending(
                            "this candidate command has an unsettled retained attempt under a different exact network authority; restore that authority to reconcile it",
                        ));
                    }
                    if !authority_live(&request) {
                        return Ok(revoked(
                            "gate authority is unavailable after the prior exact request settled",
                        ));
                    }
                    if request.cancellation.is_cancelled() {
                        return Ok(pending(
                            "gate cancellation remains active after the prior exact request settled",
                        ));
                    }
                } else if current.state == AttemptState::Reserved {
                    ledger.append(
                        request.operation_key,
                        current.attempt,
                        AttemptState::RetryableBeforeEffect,
                        "the kernel operation lock proved the reserved pre-effect owner exited",
                    )?;
                    continue;
                } else if current.state == AttemptState::RetryableBeforeEffect {
                    if !authority_live(&request) {
                        return Ok(revoked(
                            "gate authority is unavailable after a settled pre-effect attempt",
                        ));
                    }
                    if request.cancellation.is_cancelled() {
                        return Ok(pending(
                            "gate cancellation remains active after a settled pre-effect attempt",
                        ));
                    }
                } else {
                    if let Some(outcome) = self.reconcile_attempt(
                        &mut ledger,
                        &request,
                        request_digest,
                        current,
                    )? {
                        let retry_settled = matches!(
                            current.state,
                            AttemptState::TerminalRevoked | AttemptState::TerminalCancelled
                        ) && authority_live(&request)
                            && !request.cancellation.is_cancelled();
                        if !retry_settled {
                            return Ok(outcome);
                        }
                    }
                }
                if retained.request_digest == request_digest
                    && !current.state.permits_fresh_attempt()
                {
                    return Ok(pending(&format!(
                        "native gate attempt {} remains durably owned and will not be redispatched",
                        current.attempt
                    )));
                }
                if !authority_live(&request) {
                    return Ok(revoked(
                        "gate authority remains revoked after exact predecessor settlement",
                    ));
                }
                if request.cancellation.is_cancelled() {
                    return Ok(pending(
                        "gate cancellation remains active after exact predecessor settlement",
                    ));
                }
            }

            let prepared = match self.prepare_attempt(&request, request_digest, current) {
                Ok(prepared) => prepared,
                Err(PrepareFailure::Unavailable(detail)) => {
                    return Ok(GateInvocationOutcome::Unavailable { detail });
                }
                Err(PrepareFailure::Cancelled) => {
                    return Ok(pending("native gate preparation was cancelled"));
                }
                Err(PrepareFailure::Revoked) => {
                    return Ok(revoked("gate authority was revoked during native preparation"));
                }
                Err(PrepareFailure::Storage(detail)) => return Err(detail),
            };
            let published = ledger.publish_attempt(
                &request,
                request.operation_key,
                request_digest,
                retained.map(|value| {
                    (value.snapshot.attempt, value.snapshot.state, value.request_digest)
                }),
                prepared.snapshot,
            )?;
            if !published {
                continue;
            }
            return self.launch_prepared(&mut ledger, &request, prepared);
        }
    }

    fn prepare_attempt(
        &self,
        request: &GateInvocationRequest,
        request_digest: Sha256Digest,
        predecessor: Option<AttemptSnapshot>,
    ) -> Result<PreparedAttempt, PrepareFailure> {
        check_request(request)?;
        let attempt = predecessor
            .map_or(Ok(1), |value| value.attempt.checked_add(1).ok_or_else(|| {
                PrepareFailure::Storage("native gate attempt counter overflowed".to_owned())
            }))?;
        let after = self
            .inner
            .state
            .lock()
            .map_err(|_| PrepareFailure::Storage("command runtime is poisoned".to_owned()))?
            .next_ordinal;
        let ordinal = match ordinal::reserve_cancellable(
            &self.inner.state_root,
            self.inner.run_id,
            after,
            &request.cancellation,
        ) {
            Ok(value) => value,
            Err(ordinal::ReserveError::Cancelled) => return Err(PrepareFailure::Cancelled),
            Err(ordinal::ReserveError::Storage(error)) => {
                return Err(PrepareFailure::Storage(error));
            }
        };
        check_request(request)?;
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| PrepareFailure::Storage("command runtime is poisoned".to_owned()))?;
            update_ordinal_frontier(&mut state, ordinal);
        }
        let contract = contract::command_contract(self.inner.run_id, ordinal)
            .map_err(PrepareFailure::Storage)?;
        let ids = CommandIds::new(self.inner.run_id, ordinal, &contract)
            .map_err(PrepareFailure::Storage)?;
        let cwd = crate::developer_tools::path::canonical_command_cwd(
            &self.inner.workspace_root,
            &request.cwd,
        )
        .map_err(|error| PrepareFailure::Unavailable(error.to_string()))?;
        let executable = native_gate::resolve_executable(&request.program, &cwd)
            .map_err(PrepareFailure::Unavailable)?;
        let command = CommandSpec::new(executable, request.arguments.iter().cloned())
            .map_err(|error| {
                PrepareFailure::Unavailable(format!("construct native gate command: {error}"))
            })?;
        let managed_cache = request
            .network
            .as_ref()
            .map(|grant| grant.open_run_owned_cache(&self.inner.state_root))
            .transpose()
            .map_err(|error| PrepareFailure::Storage(error.to_string()))?;
        let environment = native_gate::environment(&request.program, managed_cache.as_deref())
            .map_err(PrepareFailure::Unavailable)?;
        let resources = ProcessResourcePolicy::with_optional_limits(
            None, None, None, None, None, None, None, 1,
        )
        .map_err(|error| {
            PrepareFailure::Storage(format!("construct native gate resource policy: {error}"))
        })?;
        let native = native_gate::prepare(
            &ids,
            &command,
            &self.inner.workspace_root,
            &cwd,
            &environment,
            resources,
            &self.inner.state_root,
            request.network.as_ref(),
            managed_cache.as_deref(),
            &request.cancellation,
        )
        .map_err(|detail| classify_preparation(request, detail))?;
        check_request(request)?;
        let working_directory = WorkingDirectory::open(
            &cwd,
            ids.workspace,
            ids.resource,
            ids.environment,
            ids.revision.workspace_generation(),
            ids.revision.workspace_revision(),
            WorkspaceAccess::Writable,
        )
        .map_err(|error| {
            PrepareFailure::Unavailable(format!("open native gate working directory: {error}"))
        })?;
        let output = OutputPolicy::streaming(
            16 * 1_024,
            OUTPUT_PREVIEW_BYTES as u64,
            OUTPUT_SEGMENT_BYTES,
            16_384,
        )
        .map_err(|error| {
            PrepareFailure::Storage(format!("construct native gate output policy: {error}"))
        })?;
        let deadlines = DeadlinePolicy::force(None).map_err(|error| {
            PrepareFailure::Storage(format!("construct native gate deadline policy: {error}"))
        })?;
        let plan = ExecutionPlan::new(
            ids.execution_identity(),
            command,
            working_directory,
            environment,
            IoMode::Pipes,
            StdinPolicy::Closed,
            output,
            deadlines,
            resources,
            &native.checked,
            &native.admission,
        )
        .map_err(|error| {
            PrepareFailure::Unavailable(format!("compile native gate execution plan: {error}"))
        })?;
        let replay_identity = replay_identity(
            request.operation_key,
            request_digest,
            attempt,
            ids.action.as_bytes(),
            ids.process.as_bytes(),
            plan.digest(),
        );
        let snapshot = AttemptSnapshot {
            attempt,
            ordinal,
            action_id: *ids.action.as_bytes(),
            process_id: *ids.process.as_bytes(),
            replay_identity,
            plan_digest: plan.digest(),
            state: AttemptState::Reserved,
            revocation_requested: false,
            cancellation_requested: false,
        };
        Ok(PreparedAttempt { snapshot, ids, contract, plan, native })
    }

    fn launch_prepared(
        &self,
        ledger: &mut GateLedger,
        request: &GateInvocationRequest,
        prepared: PreparedAttempt,
    ) -> Result<GateInvocationOutcome, String> {
        if !authority_live(request) {
            ledger.append(
                request.operation_key,
                prepared.snapshot.attempt,
                AttemptState::RetryableBeforeEffect,
                "authority revoked after reservation and before process authority",
            )?;
            return Ok(revoked("gate authority was revoked before process admission"));
        }
        if request.cancellation.is_cancelled() {
            ledger.append(
                request.operation_key,
                prepared.snapshot.attempt,
                AttemptState::RetryableBeforeEffect,
                "caller cancelled after reservation and before process authority",
            )?;
            return Ok(pending("gate execution was cancelled before process admission"));
        }
        let authority_root = self
            .inner
            .state_root
            .join("authority")
            .join(identity::action_hex(prepared.ids.action));
        let process_authority = match authority::commit_process(
            &authority_root.join("c2.sqlite3"),
            &prepared.ids,
            &prepared.contract,
            &prepared.plan,
        ) {
            Ok(authority) => authority,
            Err(error) => {
                ledger.append(
                    request.operation_key,
                    prepared.snapshot.attempt,
                    AttemptState::RetryableBeforeEffect,
                    &format!("process authority was not completed: {error}"),
                )?;
                return Ok(GateInvocationOutcome::Unavailable {
                    detail: format!("commit native gate process authority: {error}"),
                });
            }
        };
        if !authority_live(request) {
            ledger.append(
                request.operation_key,
                prepared.snapshot.attempt,
                AttemptState::RetryableBeforeEffect,
                "authority revoked after authority commit and before durable process consumption",
            )?;
            return Ok(revoked("gate authority was revoked before native launch"));
        }
        if request.cancellation.is_cancelled() {
            ledger.append(
                request.operation_key,
                prepared.snapshot.attempt,
                AttemptState::RetryableBeforeEffect,
                "caller cancelled after authority commit and before durable process consumption",
            )?;
            return Ok(pending("gate execution was cancelled before native launch"));
        }
        let artifacts = match ArtifactStore::open(self.inner.artifacts.clone()) {
            Ok(artifacts) => artifacts,
            Err(error) => {
                ledger.append(
                    request.operation_key,
                    prepared.snapshot.attempt,
                    AttemptState::RetryableBeforeEffect,
                    &format!("artifact store unavailable before native launch: {error}"),
                )?;
                return Ok(GateInvocationOutcome::Unavailable {
                    detail: format!("open native gate artifact store: {error}"),
                });
            }
        };
        let creating_event = prepared.ids.event("native-gate-output")?;
        ledger.append(
            request.operation_key,
            prepared.snapshot.attempt,
            AttemptState::LaunchIntended,
            "exact native launch identity is durable",
        )?;
        let process_request = process_authority.request(&prepared.ids, &prepared.plan);
        let native_gate::PreparedNativeGate { checked, admission, backend } = prepared.native;
        let process = self.inner.gateway.launch_with_backend(
            &process_request,
            prepared.plan,
            &checked,
            &admission,
            backend,
        );
        let process = match process {
            Ok(process) => process,
            Err(error) => {
                let current = AttemptSnapshot {
                    state: AttemptState::LaunchIntended,
                    ..prepared.snapshot
                };
                if let Some(outcome) = self.reconcile_attempt(
                    ledger,
                    request,
                    request.request_digest(),
                    current,
                )? {
                    return Ok(outcome);
                }
                return Ok(pending(&format!(
                    "native gate launch ownership is indeterminate after durable admission: {error}"
                )));
            }
        };
        ledger.append(
            request.operation_key,
            prepared.snapshot.attempt,
            AttemptState::Active,
            "native process owner admitted",
        )
        .unwrap_or_else(|error| {
            crate::diagnostic::report(&format!(
                "peritus native gate: active ownership event publication deferred: {error}"
            ));
        });
        let revoked_requested = AtomicBool::new(false);
        let cancellation_requested = AtomicBool::new(false);
        let result = process.wait_and_publish_with_cancellation(&artifacts, creating_event, || {
            if !authority_live(request) {
                if !revoked_requested.swap(true, Ordering::AcqRel) {
                    let _ = ledger.try_append(
                        request.operation_key,
                        prepared.snapshot.attempt,
                        AttemptState::RevocationRequested,
                        "live gate authority was revoked after process admission",
                    );
                }
                return Some(CancellationReason::LeaseFence);
            }
            if request.cancellation.is_cancelled() {
                if !cancellation_requested.swap(true, Ordering::AcqRel) {
                    let _ = ledger.try_append(
                        request.operation_key,
                        prepared.snapshot.attempt,
                        AttemptState::CancellationRequested,
                        "caller cancellation was observed after process admission",
                    );
                }
                return Some(CancellationReason::User);
            }
            None
        });
        match result {
            Ok(terminal) => self.settle_terminal(
                ledger,
                request,
                prepared.snapshot,
                terminal,
                revoked_requested.load(Ordering::Acquire),
                cancellation_requested.load(Ordering::Acquire),
            ),
            Err(error) => {
                if let Some(terminal) = error.terminal_result().cloned() {
                    ledger.append(
                        request.operation_key,
                        prepared.snapshot.attempt,
                        AttemptState::PublicationPending,
                        &format!("native gate artifact publication is pending: {error}"),
                    )?;
                    let state = self.inner.gateway.store().retry_artifact_publication(
                        terminal.process_id(),
                        &artifacts,
                        creating_event,
                    );
                    match state {
                        Ok(terminal) => self.settle_terminal(
                            ledger,
                            request,
                            prepared.snapshot,
                            terminal,
                            revoked_requested.load(Ordering::Acquire),
                            cancellation_requested.load(Ordering::Acquire),
                        ),
                        Err(retry) => Ok(pending(&format!(
                            "native gate completed but retained output publication is pending: {retry}"
                        ))),
                    }
                } else {
                    Ok(pending(&format!(
                        "native gate owner did not publish a complete terminal result: {error}"
                    )))
                }
            }
        }
    }

    fn reconcile_attempt(
        &self,
        ledger: &mut GateLedger,
        request: &GateInvocationRequest,
        _request_digest: Sha256Digest,
        current: AttemptSnapshot,
    ) -> Result<Option<GateInvocationOutcome>, String> {
        let process_id = peritus_types::ProcessId::new(current.process_id)
            .map_err(|error| format!("reconstruct native gate process identity: {error:?}"))?;
        let terminal = match self.inner.gateway.store().terminal_result(process_id) {
            Ok(terminal) => Some(terminal),
            Err(_) => {
                if current.state == AttemptState::TerminalRevoked {
                    return Ok(Some(revoked(
                        "revoked native gate predecessor lost its durable terminal result",
                    )));
                }
                if current.state.terminal() {
                    return Ok(Some(pending(
                        "settled native gate predecessor lost its durable terminal result",
                    )));
                }
                None
            }
        };
        if terminal.is_none() {
            if current.state == AttemptState::LaunchIntended {
                match self.inner.gateway.store().retained_owner_reservation(process_id) {
                    Ok(None)
                        if !self
                            .inner
                            .gateway
                            .store()
                            .process_identity_recorded(process_id)
                            .map_err(|error| {
                                format!("inspect native gate process identity: {error}")
                            })? =>
                    {
                        ledger.append(
                            request.operation_key,
                            current.attempt,
                            AttemptState::RetryableBeforeEffect,
                            "durable process registry proves launch intent ended before consumption",
                        )?;
                        return Ok(Some(pending(
                            "native gate predecessor was settled before process consumption",
                        )));
                    }
                    Ok(_) => {}
                    Err(error) => {
                        return Ok(Some(pending(&format!(
                            "native gate process consumption cannot yet be reconciled: {error}"
                        ))));
                    }
                }
            }
            let process = match self.inner.gateway.reattach_retained(process_id) {
                Ok(process) => process,
                Err(error) => {
                    return Ok(Some(pending(&format!(
                        "retained native gate owner is not yet attachable: {error}"
                    ))));
                }
            };
            if current.state == AttemptState::LaunchIntended {
                ledger.append(
                    request.operation_key,
                    current.attempt,
                    AttemptState::Active,
                    "native process owner reattached after daemon restart",
                )?;
            }
            let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
                .map_err(|error| format!("open native gate artifact store: {error}"))?;
            let ids = attempt_ids(self.inner.run_id, current)?;
            let creating_event = ids.event("native-gate-output")?;
            let revoked_requested = AtomicBool::new(current.revocation_requested);
            let cancellation_requested = AtomicBool::new(current.cancellation_requested);
            let result = process.wait_and_publish_with_cancellation(
                &artifacts,
                creating_event,
                || {
                    if !authority_live(request) {
                        if !revoked_requested.swap(true, Ordering::AcqRel) {
                            let _ = ledger.try_append(
                                request.operation_key,
                                current.attempt,
                                AttemptState::RevocationRequested,
                                "live gate authority was revoked after retained owner reattachment",
                            );
                        }
                        return Some(CancellationReason::LeaseFence);
                    }
                    if request.cancellation.is_cancelled() {
                        if !cancellation_requested.swap(true, Ordering::AcqRel) {
                            let _ = ledger.try_append(
                                request.operation_key,
                                current.attempt,
                                AttemptState::CancellationRequested,
                                "caller cancellation was observed after retained owner reattachment",
                            );
                        }
                        return Some(CancellationReason::User);
                    }
                    None
                },
            );
            let outcome = match result {
                Ok(terminal) => self.settle_terminal(
                    ledger,
                    request,
                    current,
                    terminal,
                    revoked_requested.load(Ordering::Acquire),
                    cancellation_requested.load(Ordering::Acquire),
                )?,
                Err(error) => {
                    if let Some(terminal) = error.terminal_result().cloned() {
                        ledger.append(
                            request.operation_key,
                            current.attempt,
                            AttemptState::PublicationPending,
                            &format!(
                                "reattached native gate artifact publication is pending: {error}"
                            ),
                        )?;
                        match self.inner.gateway.store().retry_artifact_publication(
                            terminal.process_id(),
                            &artifacts,
                            creating_event,
                        ) {
                            Ok(terminal) => self.settle_terminal(
                                ledger,
                                request,
                                current,
                                terminal,
                                revoked_requested.load(Ordering::Acquire),
                                cancellation_requested.load(Ordering::Acquire),
                            )?,
                            Err(retry) => pending(&format!(
                                "reattached native gate output publication remains pending: {retry}"
                            )),
                        }
                    } else {
                        pending(&format!(
                            "reattached native gate owner has no complete terminal result: {error}"
                        ))
                    }
                }
            };
            return Ok(Some(outcome));
        }
        let terminal = terminal.expect("checked terminal result");
        if terminal.process_id().as_bytes() != &current.process_id
            || terminal.plan_digest() != current.plan_digest
        {
            return Err("native gate terminal result differs from its durable attempt".to_owned());
        }
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| format!("open native gate artifact store: {error}"))?;
        let ids = attempt_ids(self.inner.run_id, current)?;
        let creating_event = ids.event("native-gate-output")?;
        let terminal = if terminal.artifact_publication_complete() {
            terminal
        } else {
            match self.inner.gateway.store().retry_artifact_publication(
                terminal.process_id(),
                &artifacts,
                creating_event,
            ) {
                Ok(terminal) => terminal,
                Err(error) => {
                    ledger.append(
                        request.operation_key,
                        current.attempt,
                        AttemptState::PublicationPending,
                        &format!("retained native gate artifact publication is pending: {error}"),
                    )?;
                    return Ok(Some(pending(&format!(
                        "native gate output publication remains pending: {error}"
                    ))));
                }
            }
        };
        let revoked_requested = current.revocation_requested;
        let cancellation_requested = current.cancellation_requested;
        self.settle_terminal(
            ledger,
            request,
            current,
            terminal,
            revoked_requested,
            cancellation_requested,
        )
        .map(Some)
    }

    fn settle_terminal(
        &self,
        ledger: &mut GateLedger,
        request: &GateInvocationRequest,
        current: AttemptSnapshot,
        terminal: TerminalResult,
        revoked_requested: bool,
        cancellation_requested: bool,
    ) -> Result<GateInvocationOutcome, String> {
        if terminal.process_id().as_bytes() != &current.process_id
            || terminal.plan_digest() != current.plan_digest
        {
            return Err("native gate terminal result differs from its durable attempt".to_owned());
        }
        if !terminal.artifact_publication_complete() {
            ledger.append(
                request.operation_key,
                current.attempt,
                AttemptState::PublicationPending,
                "terminal native gate output is not fully published",
            )?;
            return Ok(pending("native gate output publication is incomplete"));
        }
        let cancellation_reason = terminal
            .first_trigger()
            .map(peritus_process::StopTrigger::reason);
        if revoked_requested && terminal.disposition() == TerminalDisposition::Cancelled {
            ledger.append(
                request.operation_key,
                current.attempt,
                AttemptState::TerminalRevoked,
                &format!(
                    "revoked native gate attempt reached exact terminal settlement with trigger {cancellation_reason:?}"
                ),
            )?;
            return Ok(revoked(
                "gate authority was revoked; the exact admitted process was cancelled and settled",
            ));
        }
        if terminal.disposition() == TerminalDisposition::Cancelled {
            ledger.append(
                request.operation_key,
                current.attempt,
                AttemptState::TerminalCancelled,
                &format!(
                    "cancelled native gate attempt reached exact terminal settlement with trigger {cancellation_reason:?}; caller_requested={cancellation_requested}"
                ),
            )?;
            return Ok(pending(
                "native gate cancellation reached exact process settlement",
            ));
        }
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| format!("open native gate artifact store: {error}"))?;
        let preview = terminal_preview(&artifacts, &terminal)?;
        ledger.append(
            request.operation_key,
            current.attempt,
            AttemptState::TerminalComplete,
            "native gate terminal result and artifacts are complete",
        )?;
        let exit_code = match terminal.os_exit() {
            OsExitObservation::Code(code) => Some(*code),
            _ => None,
        };
        Ok(GateInvocationOutcome::Complete { exit_code, preview })
    }
}

impl GateOperationOwner {
    fn acquire(root: &Path, operation_key: Sha256Digest) -> Result<Option<Self>, String> {
        let directory = root.join("native-gate-locks-v1");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("create native gate operation lock directory: {error}"))?;
        let metadata = fs::symlink_metadata(&directory)
            .map_err(|error| format!("inspect native gate operation lock directory: {error}"))?;
        if !metadata.file_type().is_dir() {
            return Err("native gate operation lock root is not a directory".to_owned());
        }
        let path = directory.join(format!("{}.lock", digest_hex(operation_key)));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|error| format!("open native gate operation lock: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect native gate operation lock: {error}"))?;
        if !metadata.file_type().is_file() {
            return Err("native gate operation lock is not a regular file".to_owned());
        }
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { file, path })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(format!("acquire native gate operation lock: {error}")),
        }
    }
}

impl GateLedger {
    fn open(root: &Path, request: &GateInvocationRequest) -> Result<Self, String> {
        let connection = Connection::open(root.join(DATABASE_NAME))
            .map_err(|error| format!("open native gate ledger: {error}"))?;
        connection
            .busy_timeout(Duration::ZERO)
            .map_err(|error| format!("configure native gate ledger contention: {error}"))?;
        let mut ledger = Self { connection };
        ledger.retry(request, "initialize native gate ledger", |connection| {
            connection.pragma_update(None, "synchronous", "EXTRA")?;
            connection.pragma_update(None, "foreign_keys", true)?;
            connection.execute_batch(SCHEMA)
        })?;
        Ok(ledger)
    }

    fn latest(&mut self, operation_key: Sha256Digest) -> Result<Option<RetainedAttempt>, String> {
        let raw = self
            .connection
            .query_row(
                "SELECT a.request_digest, a.attempt, a.ordinal, a.action_id, a.process_id,
                        a.replay_identity, a.plan_digest,
                        (SELECT e.state FROM native_gate_events_v1 e
                         WHERE e.operation_key = a.operation_key AND e.attempt = a.attempt
                         ORDER BY e.sequence DESC LIMIT 1),
                        EXISTS(SELECT 1 FROM native_gate_events_v1 e
                               WHERE e.operation_key = a.operation_key
                                 AND e.attempt = a.attempt AND e.state = 3),
                        EXISTS(SELECT 1 FROM native_gate_events_v1 e
                               WHERE e.operation_key = a.operation_key
                                 AND e.attempt = a.attempt AND e.state = 4)
                 FROM native_gate_attempts_v1 a
                 WHERE a.operation_key = ?1
                 ORDER BY a.attempt DESC LIMIT 1",
                params![operation_key.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, bool>(8)?,
                        row.get::<_, bool>(9)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("read native gate attempt: {error}"))?;
        raw.map(decode_retained_attempt).transpose()
    }

    fn publish_attempt(
        &mut self,
        request: &GateInvocationRequest,
        operation_key: Sha256Digest,
        request_digest: Sha256Digest,
        expected: Option<(u64, AttemptState, Sha256Digest)>,
        attempt: AttemptSnapshot,
    ) -> Result<bool, String> {
        let transaction = loop {
            if request.cancellation.is_cancelled() || !authority_live(request) {
                return Ok(false);
            }
            match self.connection.transaction_with_behavior(TransactionBehavior::Immediate) {
                Ok(transaction) => break transaction,
                Err(error) if recoverable(&error) => thread::sleep(RETRY_DELAY),
                Err(error) => return Err(format!("reserve native gate attempt: {error}")),
            }
        };
        let existing: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT request_digest FROM native_gate_operations_v1 WHERE operation_key = ?1",
                params![operation_key.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("read native gate operation while reserving: {error}"))?;
        match existing {
            None => {
                transaction
                    .execute(
                        "INSERT INTO native_gate_operations_v1(operation_key, request_digest)
                         VALUES (?1, ?2)",
                        params![
                            operation_key.as_bytes().as_slice(),
                            request_digest.as_bytes().as_slice(),
                        ],
                    )
                    .map_err(|error| format!("reserve native gate operation: {error}"))?;
            }
            Some(_) => {}
        }
        let current: Option<(Vec<u8>, i64, Vec<u8>)> = transaction
            .query_row(
                "SELECT a.attempt,
                        (SELECT e.state FROM native_gate_events_v1 e
                         WHERE e.operation_key = a.operation_key AND e.attempt = a.attempt
                         ORDER BY e.sequence DESC LIMIT 1),
                        a.request_digest
                 FROM native_gate_attempts_v1 a
                 WHERE a.operation_key = ?1 ORDER BY a.attempt DESC LIMIT 1",
                params![operation_key.as_bytes().as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| format!("read native gate attempt frontier: {error}"))?;
        let current = current
            .map(|(attempt, state, digest)| {
                Ok::<_, String>((
                    decode_u64(&attempt, "attempt")?,
                    AttemptState::from_i64(state)?,
                    Sha256Digest::new(decode_array(&digest, "request digest")?),
                ))
            })
            .transpose()?;
        if current != expected {
            return Ok(false);
        }
        if let Some((_, state, previous_digest)) = expected {
            let permitted = if previous_digest == request_digest {
                state.permits_fresh_attempt()
            } else {
                state.request_generation_settled()
            };
            if !permitted {
                return Err("native gate attempted to replace an unsettled predecessor".to_owned());
            }
        }
        transaction
            .execute(
                "INSERT INTO native_gate_attempts_v1(
                    operation_key, request_digest, attempt, ordinal, action_id, process_id,
                    replay_identity, plan_digest
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    operation_key.as_bytes().as_slice(),
                    request_digest.as_bytes().as_slice(),
                    encode_u64(attempt.attempt).as_slice(),
                    encode_u64(attempt.ordinal).as_slice(),
                    attempt.action_id.as_slice(),
                    attempt.process_id.as_slice(),
                    attempt.replay_identity.as_bytes().as_slice(),
                    attempt.plan_digest.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| format!("persist native gate attempt identity: {error}"))?;
        transaction
            .execute(
                "INSERT INTO native_gate_events_v1(operation_key, attempt, sequence, state, detail)
                 VALUES (?1, ?2, 1, ?3, ?4)",
                params![
                    operation_key.as_bytes().as_slice(),
                    encode_u64(attempt.attempt).as_slice(),
                    AttemptState::Reserved as i64,
                    "exact native gate attempt reserved before effect",
                ],
            )
            .map_err(|error| format!("persist native gate reservation event: {error}"))?;
        transaction.commit().map_err(|error| {
            format!(
                "native gate reservation publication is ambiguous; the exact identity will only be observed, never redispatched: {error}"
            )
        })?;
        Ok(true)
    }

    fn append(
        &mut self,
        operation_key: Sha256Digest,
        attempt: u64,
        state: AttemptState,
        detail: &str,
    ) -> Result<(), String> {
        self.append_with_contention(operation_key, attempt, state, detail, true)
    }

    fn try_append(
        &mut self,
        operation_key: Sha256Digest,
        attempt: u64,
        state: AttemptState,
        detail: &str,
    ) -> Result<(), String> {
        self.append_with_contention(operation_key, attempt, state, detail, false)
    }

    fn append_with_contention(
        &mut self,
        operation_key: Sha256Digest,
        attempt: u64,
        state: AttemptState,
        detail: &str,
        wait_for_owner: bool,
    ) -> Result<(), String> {
        let transaction = loop {
            match self.connection.transaction_with_behavior(TransactionBehavior::Immediate) {
                Ok(transaction) => break transaction,
                Err(error) if wait_for_owner && recoverable(&error) => {
                    thread::sleep(RETRY_DELAY);
                }
                Err(error) => {
                    return Err(format!("acquire native gate event owner: {error}"));
                }
            }
        };
        let (sequence, current): (i64, i64) = transaction
            .query_row(
                "SELECT sequence, state FROM native_gate_events_v1
                 WHERE operation_key = ?1 AND attempt = ?2
                 ORDER BY sequence DESC LIMIT 1",
                params![
                    operation_key.as_bytes().as_slice(),
                    encode_u64(attempt).as_slice(),
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| format!("read native gate event frontier: {error}"))?;
        let current = AttemptState::from_i64(current)?;
        if current == state || current.terminal() {
            return Ok(());
        }
        if !valid_transition(current, state) {
            return Err(format!(
                "native gate event transition is invalid: {current:?} -> {state:?}"
            ));
        }
        let next = sequence
            .checked_add(1)
            .ok_or_else(|| "native gate event sequence overflowed".to_owned())?;
        transaction
            .execute(
                "INSERT INTO native_gate_events_v1(operation_key, attempt, sequence, state, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    operation_key.as_bytes().as_slice(),
                    encode_u64(attempt).as_slice(),
                    next,
                    state as i64,
                    detail,
                ],
            )
            .map_err(|error| format!("append native gate event: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("publish native gate event: {error}"))
    }

    fn retry<T>(
        &mut self,
        request: &GateInvocationRequest,
        operation: &str,
        mut action: impl FnMut(&mut Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, String> {
        loop {
            if request.cancellation.is_cancelled() {
                return Err(format!("{operation}: cancelled while waiting for durable storage"));
            }
            if !authority_live(request) {
                return Err(format!("{operation}: authority revoked while waiting for durable storage"));
            }
            match action(&mut self.connection) {
                Ok(value) => return Ok(value),
                Err(error) if recoverable(&error) => thread::sleep(RETRY_DELAY),
                Err(error) => return Err(format!("{operation}: {error}")),
            }
        }
    }
}

enum PrepareFailure {
    Unavailable(String),
    Cancelled,
    Revoked,
    Storage(String),
}

fn check_request(request: &GateInvocationRequest) -> Result<(), PrepareFailure> {
    if request.cancellation.is_cancelled() {
        Err(PrepareFailure::Cancelled)
    } else if !authority_live(request) {
        Err(PrepareFailure::Revoked)
    } else {
        Ok(())
    }
}

fn classify_preparation(request: &GateInvocationRequest, detail: String) -> PrepareFailure {
    if request.cancellation.is_cancelled() {
        PrepareFailure::Cancelled
    } else if !authority_live(request) {
        PrepareFailure::Revoked
    } else {
        PrepareFailure::Unavailable(detail)
    }
}

fn update_ordinal_frontier(state: &mut RuntimeState, ordinal: u64) {
    state.next_ordinal = state.next_ordinal.max(ordinal);
}

fn attempt_ids(
    run_id: peritus_types::RunId,
    attempt: AttemptSnapshot,
) -> Result<CommandIds, String> {
    let contract = contract::command_contract(run_id, attempt.ordinal)?;
    let ids = CommandIds::new(run_id, attempt.ordinal, &contract)?;
    if ids.action.as_bytes() != &attempt.action_id || ids.process.as_bytes() != &attempt.process_id {
        return Err("native gate attempt identities differ from their durable ordinal".to_owned());
    }
    Ok(ids)
}

fn validate_snapshot(
    run_id: peritus_types::RunId,
    operation_key: Sha256Digest,
    request_digest: Sha256Digest,
    attempt: AttemptSnapshot,
) -> Result<(), String> {
    let ids = attempt_ids(run_id, attempt)?;
    let expected = replay_identity(
        operation_key,
        request_digest,
        attempt.attempt,
        ids.action.as_bytes(),
        ids.process.as_bytes(),
        attempt.plan_digest,
    );
    if expected != attempt.replay_identity {
        return Err("native gate replay identity differs from its durable attempt".to_owned());
    }
    Ok(())
}

fn decode_attempt(
    raw: (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, i64, bool, bool),
) -> Result<AttemptSnapshot, String> {
    Ok(AttemptSnapshot {
        attempt: decode_u64(&raw.0, "attempt")?,
        ordinal: decode_u64(&raw.1, "ordinal")?,
        action_id: decode_array(&raw.2, "action identity")?,
        process_id: decode_array(&raw.3, "process identity")?,
        replay_identity: Sha256Digest::new(decode_array(&raw.4, "replay identity")?),
        plan_digest: Sha256Digest::new(decode_array(&raw.5, "plan digest")?),
        state: AttemptState::from_i64(raw.6)?,
        revocation_requested: raw.7 || raw.6 == AttemptState::TerminalRevoked as i64,
        cancellation_requested: raw.8 || raw.6 == AttemptState::TerminalCancelled as i64,
    })
}

fn decode_retained_attempt(
    raw: (
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        i64,
        bool,
        bool,
    ),
) -> Result<RetainedAttempt, String> {
    Ok(RetainedAttempt {
        request_digest: Sha256Digest::new(decode_array(&raw.0, "request digest")?),
        snapshot: decode_attempt((
            raw.1, raw.2, raw.3, raw.4, raw.5, raw.6, raw.7, raw.8, raw.9,
        ))?,
    })
}

fn decode_array<const N: usize>(bytes: &[u8], name: &str) -> Result<[u8; N], String> {
    bytes
        .try_into()
        .map_err(|_| format!("native gate ledger contains a malformed {name}"))
}

const fn encode_u64(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

fn decode_u64(bytes: &[u8], name: &str) -> Result<u64, String> {
    Ok(u64::from_be_bytes(decode_array(bytes, name)?))
}

fn digest_hex(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(Sha256Digest::LENGTH * 2);
    for byte in digest.as_bytes() {
        use core::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing hexadecimal into String cannot fail");
    }
    output
}

fn replay_identity(
    operation_key: Sha256Digest,
    request_digest: Sha256Digest,
    attempt: u64,
    action_id: &[u8; 16],
    process_id: &[u8; 16],
    plan_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-native-gate-replay-v1\0");
    hasher.update(operation_key.as_bytes());
    hasher.update(request_digest.as_bytes());
    hasher.update(attempt.to_be_bytes());
    hasher.update(action_id);
    hasher.update(process_id);
    hasher.update(plan_digest.as_bytes());
    Sha256Digest::new(hasher.finalize().into())
}

fn valid_transition(before: AttemptState, after: AttemptState) -> bool {
    use AttemptState::{
        Active, CancellationRequested, LaunchIntended, PublicationPending, Reserved,
        RetryableBeforeEffect, RevocationRequested, TerminalCancelled, TerminalComplete,
        TerminalRevoked,
    };
    match before {
        Reserved => matches!(after, LaunchIntended | RetryableBeforeEffect),
        LaunchIntended => matches!(
            after,
            Active
                | RevocationRequested
                | CancellationRequested
                | PublicationPending
                | RetryableBeforeEffect
                | TerminalComplete
                | TerminalRevoked
                | TerminalCancelled
        ),
        Active => matches!(
            after,
            RevocationRequested
                | CancellationRequested
                | PublicationPending
                | TerminalComplete
                | TerminalRevoked
                | TerminalCancelled
        ),
        // Requesting cancellation does not prove that cancellation won the finish race. The
        // process store remains authoritative: a natural exit observed at the same boundary is
        // still a completed terminal, rather than a permanently unsettled request state.
        RevocationRequested => {
            matches!(after, PublicationPending | TerminalComplete | TerminalRevoked)
        }
        CancellationRequested => {
            matches!(after, PublicationPending | TerminalComplete | TerminalCancelled)
        }
        PublicationPending => {
            matches!(after, TerminalComplete | TerminalRevoked | TerminalCancelled)
        }
        RetryableBeforeEffect | TerminalComplete | TerminalRevoked | TerminalCancelled => false,
    }
}

fn authority_live(request: &GateInvocationRequest) -> bool {
    catch_unwind(AssertUnwindSafe(|| (request.authority_live)())).unwrap_or(false)
}

fn pending(detail: &str) -> GateInvocationOutcome {
    GateInvocationOutcome::Pending { detail: detail.to_owned() }
}

fn revoked(detail: &str) -> GateInvocationOutcome {
    GateInvocationOutcome::AuthorityRevoked { detail: detail.to_owned() }
}

fn terminal_preview(store: &ArtifactStore, terminal: &TerminalResult) -> Result<String, String> {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut terminal_output = String::new();
    for artifact in terminal.artifacts() {
        let preview = artifact_preview(
            store,
            artifact.digest(),
            artifact.size(),
            artifact.completeness(),
        )?;
        match artifact.stream() {
            OutputStream::Stdout => stdout = preview,
            OutputStream::Stderr => stderr = preview,
            OutputStream::Terminal => terminal_output = preview,
        }
    }
    let mut combined = if terminal_output.is_empty() {
        format!("{stdout}{stderr}")
    } else {
        terminal_output
    };
    if terminal.disposition() != TerminalDisposition::Exited
        || !terminal.tree_cleanup_complete()
        || !terminal.support_tasks_joined()
    {
        use core::fmt::Write as _;
        let _ = write!(
            combined,
            "\n[native gate disposition: {:?}; tree_cleanup_complete={}; support_tasks_joined={}]\n",
            terminal.disposition(),
            terminal.tree_cleanup_complete(),
            terminal.support_tasks_joined(),
        );
    }
    Ok(crate::bundle::limit_text(&combined, OUTPUT_PREVIEW_BYTES))
}

fn artifact_preview(
    store: &ArtifactStore,
    digest: Sha256Digest,
    size: u64,
    completeness: OutputCompleteness,
) -> Result<String, String> {
    if size == 0 && digest != peritus_codec::sha256(b"") {
        return Err("empty native gate artifact has a nonempty digest".to_owned());
    }
    let mut reader = store
        .open_read(ArtifactDigest::from_sha256(digest))
        .map_err(|error| format!("open native gate output artifact: {error}"))?;
    if reader.metadata().size() != size {
        return Err("native gate artifact length differs from terminal accounting".to_owned());
    }
    let incomplete = completeness != OutputCompleteness::Complete;
    if size <= OUTPUT_PREVIEW_BYTES as u64 {
        let maximum = usize::try_from(size)
            .map_err(|_| "native gate artifact length is unrepresentable".to_owned())?;
        let bytes = reader
            .read_chunk_at(0, maximum.max(1))
            .map_err(|error| format!("read native gate output artifact: {error}"))?
            .map_or_else(Vec::new, |chunk| chunk.bytes().to_vec());
        let mut output = String::from_utf8_lossy(&bytes).into_owned();
        if incomplete {
            output.push_str("\n[output incomplete]\n");
        }
        return Ok(output);
    }
    let head = reader
        .read_chunk_at(0, OUTPUT_WINDOW_BYTES)
        .map_err(|error| format!("read native gate output artifact head: {error}"))?
        .ok_or_else(|| "native gate output artifact head is missing".to_owned())?;
    let tail_offset = size
        .checked_sub(OUTPUT_WINDOW_BYTES as u64)
        .ok_or_else(|| "native gate output artifact tail offset underflowed".to_owned())?;
    let tail = reader
        .read_chunk_at(tail_offset, OUTPUT_WINDOW_BYTES)
        .map_err(|error| format!("read native gate output artifact tail: {error}"))?
        .ok_or_else(|| "native gate output artifact tail is missing".to_owned())?;
    let marker = if incomplete {
        "\n[output truncated; stream incomplete]\n"
    } else {
        "\n[output truncated]\n"
    };
    Ok(format!(
        "{}{}{}",
        String::from_utf8_lossy(head.bytes()),
        marker,
        String::from_utf8_lossy(tail.bytes()),
    ))
}

fn recoverable(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy
                    | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}
