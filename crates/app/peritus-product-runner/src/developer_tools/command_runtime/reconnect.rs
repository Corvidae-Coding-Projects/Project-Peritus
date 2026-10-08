//! Startup reconstruction from immutable C4 receipts and authoritative C2 ownership.

use std::{sync::{Arc, Mutex}, time::Instant};

use peritus_artifact_store::ArtifactStore;
use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_process::{
    ExecutionPlan, LifecyclePhase, ProcessClaimState, ProcessReceipt, StdinPolicy,
};
use peritus_tool_router::{ReplayRecord, ReplayRecordKind, ToolRouter};
use peritus_tools_shell::{RecoveredTerminalExecution, ShellExecution};
use peritus_types::ActionId;
use serde_json::Value;

use super::{
    ActiveCommand, CommandRouter, CommandRuntime, ProgressBatch, TerminalCommand,
    projections::ReconnectReceipt, result, with_execution_context,
};

impl CommandRuntime {
    pub(super) fn rebuild_reconnect(&self) -> Result<(), String> {
        let mut after = None;
        loop {
            let page = self
                .inner
                .gateway
                .store()
                .process_receipts_page(after)
                .map_err(|error| format!("page authoritative command processes: {error}"))?;
            if page.is_empty() {
                break;
            }
            for process in page {
                after = Some(process.process_id());
                let Some(action_id) = process.claim_action_id() else { continue };
                let Some(receipt) = self.inner.projections.receipt(action_id)? else {
                    if process
                        .identity()
                        .is_some_and(|identity| identity.run_id() == self.inner.run_id)
                    {
                        self.retain_unmapped(
                            action_id,
                            Some(&process),
                            "the authoritative process receipt predates exact command reconnect material",
                        )?;
                    }
                    continue;
                };
                let replay = self
                    .inner
                    .replay_index
                    .record(action_id)
                    .map_err(|error| format!("read command replay receipt: {error}"))?;
                self.rebuild_one(receipt, Some(process), replay)?;
            }
        }

        let mut after = None;
        loop {
            let page = self
                .inner
                .replay_index
                .records_page(after)
                .map_err(|error| format!("page command replay receipts: {error}"))?;
            if page.is_empty() {
                break;
            }
            for replay in page {
                after = Some(replay.action_id());
                let Some(receipt) = self.inner.projections.receipt(replay.action_id())? else {
                    self.retain_unmapped(
                        replay.action_id(),
                        None,
                        "the authoritative C4 replay receipt predates exact command reconnect material",
                    )?;
                    continue;
                };
                let process = self
                    .inner
                    .gateway
                    .store()
                    .process_receipt(receipt.process_id)
                    .map_err(|error| format!("read command process receipt: {error}"))?;
                if process
                    .as_ref()
                    .and_then(ProcessReceipt::claim_action_id)
                    != Some(replay.action_id())
                {
                    self.rebuild_one(receipt, process, Some(replay))?;
                }
            }
        }

        // The reconnect receipt is committed before replay reservation and process dispatch. A
        // crash in that narrow interval therefore has neither of the indexed owners above from
        // which to discover the handle. Page the immutable receipts as a final union arm, while
        // avoiding actions already selected by either authoritative index.
        let mut after = None;
        loop {
            let page = self.inner.projections.receipts_page(after)?;
            if page.is_empty() {
                break;
            }
            for receipt in page {
                after = Some(receipt.action_id);
                let replay = self
                    .inner
                    .replay_index
                    .record(receipt.action_id)
                    .map_err(|error| format!("read command replay receipt: {error}"))?;
                let process = self
                    .inner
                    .gateway
                    .store()
                    .process_receipt(receipt.process_id)
                    .map_err(|error| format!("read command process receipt: {error}"))?;
                if replay.is_none()
                    && process
                        .as_ref()
                        .and_then(ProcessReceipt::claim_action_id)
                        != Some(receipt.action_id)
                {
                    self.rebuild_one(receipt, process, replay)?;
                }
            }
        }
        Ok(())
    }

    fn rebuild_one(
        &self,
        receipt: ReconnectReceipt,
        process: Option<ProcessReceipt>,
        replay: Option<ReplayRecord>,
    ) -> Result<(), String> {
        let handle = super::identity::action_hex(receipt.action_id);
        let Some(replay) = replay else {
            return self.retain_unresolved(
                &receipt,
                &handle,
                "the immutable command receipt has no authoritative C4 replay receipt; any process ownership was preserved",
            );
        };
        let (plan, prepared, progress) = match self.validate_reconnect(&receipt, process.as_ref(), &replay) {
            Ok(validated) => validated,
            Err(error) => return self.retain_unresolved(&receipt, &handle, &error),
        };
        let Some(process) = process else {
            return self.retain_unresolved(
                &receipt,
                &handle,
                "the accepted C4 invocation has no authoritative C2 process receipt",
            );
        };

        match replay.kind() {
            ReplayRecordKind::Reserved | ReplayRecordKind::Active => {
                if process.terminal_available() {
                    let retained = receipt.clone();
                    let recovered = if process.retained_owner_claimed() {
                        match self.adopt_retained(
                            receipt.clone(),
                            plan.clone(),
                            prepared.clone(),
                            progress.clone(),
                        ) {
                            Ok(()) => Ok(()),
                            Err(_) => self.adopt_terminal(
                                receipt,
                                plan,
                                prepared,
                                progress,
                                replay.terminal_bytes(),
                            ),
                        }
                    } else {
                        self.adopt_terminal(
                            receipt,
                            plan,
                            prepared,
                            progress,
                            replay.terminal_bytes(),
                        )
                    };
                    match recovered {
                        Ok(()) => Ok(()),
                        Err(error) => self.retain_unresolved(
                            &retained,
                            &handle,
                            &format!("terminal command recovery remains unresolved: {error}"),
                        ),
                    }
                } else if process.retained_owner_claimed()
                    && matches!(
                        process.phase(),
                        Some(
                            LifecyclePhase::Authorized
                                | LifecyclePhase::Starting
                                | LifecyclePhase::Running
                                | LifecyclePhase::Stopping
                                | LifecyclePhase::Exited
                                | LifecyclePhase::Closed
                        )
                    )
                {
                    let retained = receipt.clone();
                    match self.adopt_retained(receipt, plan, prepared, progress) {
                        Ok(()) => Ok(()),
                        Err(error) => self.retain_unresolved(
                            &retained,
                            &handle,
                            &format!("retained command owner could not be reattached: {error}"),
                        ),
                    }
                } else {
                    self.retain_unresolved(
                        &receipt,
                        &handle,
                        "the accepted command still has durable process ownership, but no exact retained owner can be reattached",
                    )
                }
            }
            ReplayRecordKind::NonIdempotentTerminal | ReplayRecordKind::ReplayTerminal => {
                if process.terminal_available() {
                    let retained = receipt.clone();
                    match self.install_settled_terminal(
                        receipt,
                        plan,
                        prepared,
                        progress,
                        replay.terminal_bytes(),
                    ) {
                        Ok(()) => Ok(()),
                        Err(error) => self.retain_unresolved(
                            &retained,
                            &handle,
                            &format!("settled command recovery remains unresolved: {error}"),
                        ),
                    }
                } else {
                    self.retain_unresolved(
                        &receipt,
                        &handle,
                        "the terminal C4 replay receipt has no matching authoritative C2 terminal result",
                    )
                }
            }
            ReplayRecordKind::Indeterminate => self.retain_unresolved(
                &receipt,
                &handle,
                "the accepted command has an indeterminate replay receipt; its process ownership was preserved and was not redispatched",
            ),
        }
    }

    fn validate_reconnect(
        &self,
        receipt: &ReconnectReceipt,
        process: Option<&ProcessReceipt>,
        replay: &ReplayRecord,
    ) -> Result<(
        ExecutionPlan,
        peritus_tool_protocol::PreparedToolCall,
        super::replay_index::RecoveryProgress,
    ), String> {
        let plan = ExecutionPlan::restore_canonical(receipt.plan.clone())
            .map_err(|error| format!("restore retained command plan: {error}"))?;
        let identity = plan.identity();
        let binding = plan
            .caller_binding()
            .ok_or_else(|| "retained command plan has no C4 caller binding".to_owned())?;
        if plan.digest() != receipt.plan_digest
            || plan.canonical_bytes() != receipt.plan
            || identity.action_id() != receipt.action_id
            || identity.process_id() != receipt.process_id
            || identity.run_id() != self.inner.run_id
            || binding.action_id() != receipt.action_id
            || binding.descriptor_digest() != receipt.descriptor_digest.get()
            || binding.prepared_digest() != receipt.prepared_digest
            || binding.actor_id() != identity.actor_id()
            || binding.role() != ActorRole::ProviderToolWorker
            || binding.environment_id() != identity.environment_id()
            || binding.resource_id() != identity.resource_id()
            || !plan
                .working_directory()
                .path()
                .starts_with(&self.inner.workspace_root)
            || super::CommandExecutionMode::from_access(plan.working_directory().access())
                != receipt.mode
            || (!matches!(plan.stdin_policy(), StdinPolicy::Closed) != receipt.interactive)
        {
            return Err("retained command receipt differs from its exact caller-bound plan".to_owned());
        }
        let prepared = super::plan::recover_prepared(
            &self.inner.registry,
            &plan,
            receipt.idempotency_key.clone(),
        )?;
        if prepared.call().action_id() != receipt.action_id
            || prepared.replay_identity().digest() != receipt.replay_identity
            || prepared.prepared_digest() != receipt.prepared_digest
            || prepared.descriptor_digest() != receipt.descriptor_digest
            || replay.action_id() != receipt.action_id
            || replay.replay_identity() != receipt.replay_identity
        {
            return Err("retained command C4 preparation or replay identity differs".to_owned());
        }
        if let Some(process) = process {
            if process.process_id() != receipt.process_id
                || process.claim_state() != ProcessClaimState::Matching
                || process.claim_action_id() != Some(receipt.action_id)
                || process.claim_action_digest() != Some(receipt.process_action_digest)
                || process.claim_plan_digest() != Some(receipt.plan_digest)
                || process.identity() != Some(identity)
                || process.manifest_action_digest() != Some(receipt.process_action_digest)
                || process.manifest_plan_digest() != Some(receipt.plan_digest)
            {
                return Err(
                    "authoritative process claim or manifest differs from the command receipt"
                        .to_owned(),
                );
            }
        }
        let progress = self
            .inner
            .replay_index
            .recovery_progress(receipt.action_id)
            .map_err(|error| format!("replay command progress receipts: {error}"))?;
        if progress.latest().is_some_and(|page| {
            page.action_id() != receipt.action_id
                || page.replay_identity() != receipt.replay_identity
                || page.prepared_digest() != receipt.prepared_digest
        }) {
            return Err("retained command progress chain differs from its C4 receipt".to_owned());
        }
        Ok((plan, prepared, progress))
    }

    fn adopt_retained(
        &self,
        receipt: ReconnectReceipt,
        plan: ExecutionPlan,
        prepared: peritus_tool_protocol::PreparedToolCall,
        progress: super::replay_index::RecoveryProgress,
    ) -> Result<(), String> {
        let owner = self
            .inner
            .gateway
            .reattach_retained(receipt.process_id)
            .map_err(|error| format!("reattach retained command owner: {error}"))?;
        let control = owner.control();
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| format!("open recovered command artifacts: {error}"))?;
        let next_frontier = progress.next_frontier();
        let process_cursor = progress.process_cursor();
        let observed_at = progress.observed_at();
        let truncated = progress.truncated();
        let mut router = ToolRouter::with_replay_store(
            self.inner.registry.clone(),
            self.inner.router_limits,
            self.inner.replay_index.clone(),
        );
        let process_store = self.inner.gateway.store().clone();
        let process_id = receipt.process_id;
        let creating_event = receipt.creating_event;
        let invocation = router
            .adopt_active(prepared.clone(), observed_at, move |prepared, frontier| {
                if frontier != next_frontier {
                    return Err(peritus_tools_shell::failure::adapter(
                        "shell-reconnect-frontier",
                        "router and command recovery progress frontiers differ",
                    ));
                }
                Ok(Box::new(ShellExecution::reattached(
                    prepared.clone(),
                    owner,
                    process_store,
                    process_id,
                    artifacts,
                    creating_event,
                    frontier,
                    process_cursor,
                    observed_at,
                    truncated,
                )))
            })
            .map_err(|error| format!("adopt retained command owner: {error}"))?;
        self.install_active(
            receipt,
            plan,
            Arc::new(Mutex::new(router)),
            invocation,
            Some(control.clone()),
            observed_at,
        )?;
        let handle = super::identity::action_hex(invocation.action_id());
        if let Err(error) = self.reconcile_observer(&handle) {
            crate::diagnostic::report(&format!(
                "peritus command runtime: retained command {handle} reattached, but its initial recovery observation is pending: {error}"
            ));
            self.enqueue_observer_reconciliation(&handle, Some(control));
        }
        Ok(())
    }

    fn adopt_terminal(
        &self,
        receipt: ReconnectReceipt,
        plan: ExecutionPlan,
        prepared: peritus_tool_protocol::PreparedToolCall,
        progress: super::replay_index::RecoveryProgress,
        expected_terminal: Option<&[u8]>,
    ) -> Result<(), String> {
        let execution = self.recovered_terminal(&receipt, &plan, prepared.clone(), &progress)?;
        if expected_terminal.is_some_and(|expected| execution.result().canonical_bytes() != expected)
        {
            return self.retain_unresolved(
                &receipt,
                &super::identity::action_hex(receipt.action_id),
                "the authoritative C2 terminal projection differs from the settled C4 receipt",
            );
        }
        let next_frontier = progress.next_frontier();
        let observed_at = progress.observed_at();
        let mut router = ToolRouter::with_replay_store(
            self.inner.registry.clone(),
            self.inner.router_limits,
            self.inner.replay_index.clone(),
        );
        let invocation = router
            .adopt_active(prepared, observed_at, move |_prepared, frontier| {
                if frontier != next_frontier {
                    return Err(peritus_tools_shell::failure::adapter(
                        "shell-reconnect-frontier",
                        "router and command recovery progress frontiers differ",
                    ));
                }
                Ok(Box::new(execution))
            })
            .map_err(|error| format!("adopt terminal command owner: {error}"))?;
        self.install_active(
            receipt,
            plan,
            Arc::new(Mutex::new(router)),
            invocation,
            None,
            observed_at,
        )?;
        self.reconcile_observer(&super::identity::action_hex(invocation.action_id()))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn install_settled_terminal(
        &self,
        receipt: ReconnectReceipt,
        plan: ExecutionPlan,
        prepared: peritus_tool_protocol::PreparedToolCall,
        progress: super::replay_index::RecoveryProgress,
        expected_terminal: Option<&[u8]>,
    ) -> Result<(), String> {
        let execution = self.recovered_terminal(&receipt, &plan, prepared, &progress)?;
        let result = execution.result().clone();
        if expected_terminal.is_none_or(|expected| result.canonical_bytes() != expected) {
            return self.retain_unresolved(
                &receipt,
                &super::identity::action_hex(receipt.action_id),
                "the authoritative C2 terminal projection differs from the settled C4 receipt",
            );
        }
        let handle = super::identity::action_hex(receipt.action_id);
        let progress = ProgressBatch {
            events: Vec::new(),
            page: progress.latest().cloned(),
        };
        let terminal = TerminalCommand {
            result,
            progress,
            mode: receipt.mode,
            resource_evidence: receipt.resource_evidence.clone(),
        };
        let projection = with_execution_context(
            result::terminal_deferred(&handle, &terminal.result, &terminal.progress),
            Some(receipt.mode),
            receipt.resource_evidence.as_ref(),
        );
        self.inner
            .state
            .lock()
            .map_err(|_| "command runtime is poisoned".to_owned())?
            .terminal
            .insert(handle.clone(), terminal);
        self.repair_projection(&handle, projection)
            .map_err(|error| error.to_string())
    }

    fn recovered_terminal(
        &self,
        receipt: &ReconnectReceipt,
        plan: &ExecutionPlan,
        prepared: peritus_tool_protocol::PreparedToolCall,
        progress: &super::replay_index::RecoveryProgress,
    ) -> Result<RecoveredTerminalExecution, String> {
        let retained_error = match self.inner.gateway.reattach_retained(receipt.process_id) {
            Ok(owner) => {
                let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
                    .map_err(|error| format!("open retained command artifacts: {error}"))?;
                match RecoveredTerminalExecution::from_retained(
                    prepared.clone(),
                    owner,
                    self.inner.gateway.store().clone(),
                    receipt.process_id,
                    receipt.plan_digest,
                    artifacts,
                    receipt.creating_event,
                    progress.observed_at(),
                    progress.next_frontier(),
                    progress.truncated(),
                ) {
                    Ok(execution) => return Ok(execution),
                    Err(error) => Some(error.to_string()),
                }
            }
            Err(_) => None,
        };
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| format!("open recovered command artifacts: {error}"))?;
        RecoveredTerminalExecution::new(
            prepared,
            self.inner.gateway.store().clone(),
            receipt.process_id,
            receipt.plan_digest,
            artifacts,
            receipt.creating_event,
            plan.output_policy().retained_window_bytes(),
            progress.observed_at(),
            progress.next_frontier(),
            progress.truncated(),
        )
        .map_err(|error| match retained_error {
            Some(retained) => format!(
                "recover command terminal result: retained owner failed ({retained}); artifact fallback failed ({error})"
            ),
            None => format!("recover command terminal result: {error}"),
        })
    }

    fn install_active(
        &self,
        receipt: ReconnectReceipt,
        plan: ExecutionPlan,
        router: CommandRouter,
        invocation: peritus_tool_router::InvocationHandle,
        control: Option<peritus_process::ProcessControl>,
        observed_base: AuthorityInstant,
    ) -> Result<(), String> {
        let handle = super::identity::action_hex(receipt.action_id);
        self.inner
            .state
            .lock()
            .map_err(|_| "command runtime is poisoned".to_owned())?
            .active
            .insert(
                handle.clone(),
                ActiveCommand {
                    plan,
                    control,
                    router,
                    invocation,
                    started: Instant::now(),
                    observed_base,
                    interactive: receipt.interactive,
                    resource_evidence: receipt.resource_evidence.clone(),
                    protected_paths: receipt.protected_paths.clone(),
                },
            );
        let projection = with_execution_context(
            result::active(&handle, &ProgressBatch::empty()),
            Some(receipt.mode),
            receipt.resource_evidence.as_ref(),
        );
        self.repair_projection(&handle, projection)
            .map_err(|error| error.to_string())
    }

    fn retain_unresolved(
        &self,
        receipt: &ReconnectReceipt,
        handle: &str,
        detail: &str,
    ) -> Result<(), String> {
        let process = self
            .inner
            .gateway
            .store()
            .process_receipt(receipt.process_id)
            .map_err(|error| format!("read unresolved command process receipt: {error}"))?;
        let ownership_unresolved = process.as_ref().is_none_or(|process| {
            process.claim_state() != ProcessClaimState::Matching
                || !process.manifest_ownership_settled()
        });
        let mut projection = with_execution_context(
            result::indeterminate(handle, detail),
            Some(receipt.mode),
            receipt.resource_evidence.as_ref(),
        );
        if let Some(object) = projection.as_object_mut() {
            object.insert("reconnect_unresolved".to_owned(), Value::Bool(true));
            object.insert(
                "ownership_unresolved".to_owned(),
                Value::Bool(ownership_unresolved),
            );
            object.insert(
                "process_id".to_owned(),
                Value::String(super::identity::process_hex(receipt.process_id)),
            );
        }
        self.repair_projection(handle, projection)
            .map_err(|error| error.to_string())
    }

    fn retain_unmapped(
        &self,
        action_id: ActionId,
        process: Option<&ProcessReceipt>,
        detail: &str,
    ) -> Result<(), String> {
        let handle = super::identity::action_hex(action_id);
        if self.inner.projections.lookup(&handle)?.is_some() {
            return Ok(());
        }
        let ownership_unresolved = process.is_none_or(|process| {
            process.claim_state() != ProcessClaimState::Matching
                || !process.manifest_ownership_settled()
        });
        let mut projection = result::indeterminate(&handle, detail);
        if let Some(object) = projection.as_object_mut() {
            object.insert("reconnect_unresolved".to_owned(), Value::Bool(true));
            object.insert(
                "ownership_unresolved".to_owned(),
                Value::Bool(ownership_unresolved),
            );
            if let Some(process) = process {
                object.insert(
                    "process_id".to_owned(),
                    Value::String(super::identity::process_hex(process.process_id())),
                );
            }
        }
        self.repair_projection(&handle, projection)
            .map_err(|error| error.to_string())
    }
}
