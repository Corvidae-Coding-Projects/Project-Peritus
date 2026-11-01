//! Real C4 replay checks over committed synthetic B0/B1/Budget/epoch authority.
//!
//! This module is compiled as a child of `agent_driver` to reuse its disposable
//! committed-authority fixture. The root Stage 6 module re-exports it as `router`.

use std::sync::Arc;

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    IdempotencyKey, ImplementationIdentity, SchemaDigest, ToolCall, ToolDescriptor,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, DispatchOutcome, ReplayDisposition, RouterErrorKind,
    ToolDispatcher, ToolStart,
};
use peritus_types::{Generation, RunId, SessionId, Sha256Digest};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    authority, contract, descriptor, identity::CommandIds, tool_ids, tool_result, tool_setup,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Drift {
    Valid,
    BeforeCapabilityUse,
    Deadline,
    WrongEpoch,
    PreparedDigest,
    WorkspaceGeneration,
    Session,
    CallKey,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    #[serde(default)]
    pub deny_before_first: bool,
    pub attempts: Vec<Drift>,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let drift = prop::sample::select(vec![
        Drift::Valid,
        Drift::BeforeCapabilityUse,
        Drift::Deadline,
        Drift::WrongEpoch,
        Drift::PreparedDigest,
        Drift::WorkspaceGeneration,
        Drift::Session,
        Drift::CallKey,
    ]);
    let general = (any::<bool>(), prop::collection::vec(drift.clone(), 0..=max_steps.min(32)))
        .prop_map(|(deny_before_first, attempts)| Case { deny_before_first, attempts });
    let stale_then_valid =
        (any::<bool>(), prop::collection::vec(drift, 0..=max_steps.saturating_sub(2).min(30)))
            .prop_map(|(deny_before_first, mut tail)| {
                let mut attempts = vec![Drift::Deadline, Drift::Valid];
                attempts.append(&mut tail);
                Case { deny_before_first, attempts }
            });
    prop_oneof![2 => general, 1 => stale_then_valid].boxed()
}

pub fn calibration() -> Case {
    Case { deny_before_first: true, attempts: vec![Drift::Deadline, Drift::Valid] }
}

pub fn examples() -> Vec<Case> {
    let mut cases = vec![calibration()];
    for drift in [
        Drift::BeforeCapabilityUse,
        Drift::WrongEpoch,
        Drift::PreparedDigest,
        Drift::WorkspaceGeneration,
        Drift::Session,
        Drift::CallKey,
    ] {
        cases.push(Case { deny_before_first: true, attempts: vec![drift, Drift::Valid] });
    }
    cases
}

struct CountingDispatcher {
    descriptor: Arc<ToolDescriptor>,
    calls: usize,
}

impl ToolDispatcher for CountingDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        self.descriptor.implementation_identity()
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor.descriptor_digest()
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        self.calls += 1;
        Ok(ToolStart::Completed(
            tool_result(invocation.prepared()).expect("synthetic terminal result is constructible"),
        ))
    }
}

fn bump(metrics: &mut crate::Metrics, key: &str) {
    *metrics.entry(key.to_owned()).or_default() += 1;
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.attempts.len() > 32 {
        return Err("setup-incomplete: router replay length bound".into());
    }
    let run = RunId::new([93; 16]).map_err(|error| format!("run id: {error:?}"))?;
    let acceptance = contract::command_contract(run, 0)?;
    let ids = CommandIds::new(run, 1, &acceptance)?;
    let action_ids = tool_ids(&ids, 0)?;
    let (mut router, coordinator) = tool_setup(&ids, 1)?;
    let prepared = coordinator.slots()[0].prepared().clone();
    let directory = tempfile::tempdir().map_err(|error| format!("router fixture: {error}"))?;
    let authority = authority::commit_tool(
        &directory.path().join("authority.sqlite"),
        &action_ids,
        &acceptance,
        &prepared,
        100,
    )?;
    let mut dispatcher = CountingDispatcher { descriptor: descriptor()?, calls: 0 };
    let mut metrics = crate::Metrics::new();
    if case.deny_before_first {
        let stale = authority.request_variant(
            &action_ids,
            authority::instant(120),
            prepared.prepared_digest(),
            action_ids.revision.workspace_generation(),
            action_ids.session,
        );
        let denied = router.dispatch(prepared.clone(), &stale, &mut dispatcher);
        if !matches!(denied, Err(ref error) if error.kind() == RouterErrorKind::Authorization)
            || dispatcher.calls != 0
        {
            return Err(format!("stale first dispatch was not rejected before effect: {denied:?}"));
        }
        bump(&mut metrics, "router_initial_authority_denials");
    }
    let request = authority.request(&action_ids, &prepared);
    let first = router
        .dispatch(prepared.clone(), &request, &mut dispatcher)
        .map_err(|error| format!("initial committed tool dispatch: {error:?}"))?;
    if !matches!(first, DispatchOutcome::Completed(_)) || dispatcher.calls != 1 {
        return Err(format!("first dispatch did not execute exactly once: {first:?}"));
    }
    bump(&mut metrics, "router_first_effects");
    for (index, drift) in case.attempts.iter().copied().enumerate() {
        let mut observed_at = authority::instant(20);
        let mut expected_digest = prepared.prepared_digest();
        let mut expected_generation = action_ids.revision.workspace_generation();
        let mut session_id = action_ids.session;
        let mut replay_prepared = prepared.clone();
        match drift {
            Drift::Valid | Drift::CallKey => {}
            Drift::BeforeCapabilityUse => observed_at = authority::instant(19),
            Drift::Deadline => observed_at = authority::instant(120),
            Drift::WrongEpoch => {
                observed_at = AuthorityInstant::new(Generation::new(2).expect("nonzero epoch"), 20)
            }
            Drift::PreparedDigest => expected_digest = Sha256Digest::new([201; 32]),
            Drift::WorkspaceGeneration => {
                expected_generation = Generation::new(2).expect("nonzero generation")
            }
            Drift::Session => session_id = SessionId::new([202; 16]).expect("nonzero session id"),
        }
        if matches!(drift, Drift::CallKey) {
            let original = prepared.call();
            let altered = ToolCall::new(
                original.action_id(),
                original.name().clone(),
                original.version(),
                original.arguments().clone(),
                original.limits(),
                original.revision(),
                original.deadline().expect("tool fixture uses a deadline"),
                IdempotencyKey::new("changed-key".to_owned())
                    .map_err(|error| format!("altered key: {error:?}"))?,
            );
            replay_prepared = router
                .prepare(altered)
                .map_err(|error| format!("altered call preparation: {error:?}"))?;
        }
        let request = authority.request_variant(
            &action_ids,
            observed_at,
            expected_digest,
            expected_generation,
            session_id,
        );
        let result = router.dispatch(replay_prepared, &request, &mut dispatcher);
        match drift {
            Drift::Valid => {
                if !matches!(
                    result,
                    Ok(DispatchOutcome::PriorOutcome(ReplayDisposition::NonIdempotentTerminal))
                ) {
                    return Err(format!("valid non-idempotent replay {index}: {result:?}"));
                }
                bump(&mut metrics, "router_valid_prior_outcomes");
            }
            Drift::CallKey => {
                if !matches!(result, Err(ref error) if error.kind() == RouterErrorKind::ReplayConflict)
                {
                    return Err(format!("changed replay identity {index}: {result:?}"));
                }
                bump(&mut metrics, "router_replay_conflicts");
            }
            _ => {
                if !matches!(result, Err(ref error) if error.kind() == RouterErrorKind::Authorization)
                {
                    return Err(format!("stale authority replay {index} {drift:?}: {result:?}"));
                }
                bump(&mut metrics, "router_stale_authority_rejections");
            }
        }
        if dispatcher.calls != 1 {
            return Err(format!("replay {index} dispatched an extra effect"));
        }
    }
    Ok(metrics)
}
