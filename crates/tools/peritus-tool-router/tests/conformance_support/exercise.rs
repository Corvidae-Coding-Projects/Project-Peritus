//! Scenario execution and direct observation extraction.

use std::sync::Arc;

use peritus_conformance::{
    ToolAuthorizationDrift, ToolConformanceError, ToolConformanceFixture,
    ToolConformanceObservation, ToolDescriptorObservation, ToolDisposition, ToolReplayMode,
    ToolReplayObservation, ToolScenario,
};
use peritus_policy::{
    ActorRole, CapabilityScope, Permission, PermissionSet, UseLimit, ValidityWindow,
};
use peritus_tool_protocol::{IdempotencySemantics, PreparedToolCall};
use peritus_tool_router::{
    DispatchOutcome, RecoveryOutcome, ReplayDisposition, ToolDispatcher, ToolRouter,
    tool_action_intent,
};
use peritus_types::Sha256Digest;

use super::dispatcher::{ActiveDispatcher, ActiveMode, Counters, SyncDispatcher, SyncMode};
use super::fixture;
use super::observation::{
    disposition, empty_result, observed, observed_with_effects, result_observation, with_result,
};
use crate::support::{Ids, TestRoot, authority_request, commit_authority, instant, open_journal};

pub fn run(
    request: &ToolConformanceFixture,
    seed: u8,
) -> Result<ToolConformanceObservation, ToolConformanceError> {
    match request.scenario() {
        ToolScenario::DescriptorSchema => Ok(descriptor_schema(request)),
        ToolScenario::SchemaRejection => Ok(schema_rejection(seed)),
        ToolScenario::Exposure => Ok(exposure(seed)),
        ToolScenario::Dispatch => dispatch(request, seed),
        ToolScenario::Authorization(drift) => authorization(request, seed, drift),
        ToolScenario::ResultTruth => result_truth(request, seed),
        ToolScenario::Cancellation => lifecycle(request, seed, ActiveMode::Cancel),
        ToolScenario::Deadline => lifecycle(request, seed, ActiveMode::Deadline),
        ToolScenario::Replay(mode) => replay(request, seed, mode),
    }
}

fn descriptor_schema(request: &ToolConformanceFixture) -> ToolConformanceObservation {
    let first = fixture::descriptor(IdempotencySemantics::ReplayTerminal);
    let second = fixture::descriptor(IdempotencySemantics::ReplayTerminal);
    let descriptor = ToolDescriptorObservation::new(
        first.name().as_str().to_owned(),
        *first.schema_digest().as_bytes(),
        *second.schema_digest().as_bytes(),
        first.operation().name() == first.name(),
        first.implementation_identity() == second.implementation_identity(),
        first
            .schema()
            .validate(
                &peritus_tool_protocol::BoundedJson::parse(
                    std::str::from_utf8(request.canonical_arguments()).unwrap(),
                    peritus_tool_protocol::JsonLimits::PRODUCTION,
                )
                .unwrap(),
            )
            .is_ok(),
    );
    observed(ToolDisposition::Succeeded, Some(descriptor), false, false, true)
}

fn schema_rejection(seed: u8) -> ToolConformanceObservation {
    let ids = Ids::new_named(seed, "fs.read");
    let router = fixture::router(IdempotencySemantics::ReplayTerminal);
    let invalid =
        fixture::call(&ids, br#"{"path":"src/lib.rs","max_bytes":0,"extra":true}"#, "invalid");
    let rejected = router.prepare(invalid).is_err();
    if rejected {
        observed(ToolDisposition::Rejected, None, false, false, true)
    } else {
        observed(ToolDisposition::Succeeded, None, true, false, true)
    }
}

fn exposure(seed: u8) -> ToolConformanceObservation {
    let ids = Ids::new_named(seed, "fs.read");
    let router = fixture::router(IdempotencySemantics::ReplayTerminal);
    let scope = CapabilityScope::new(
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        PermissionSet::new(vec![Permission::new(ids.resource, ids.capability.clone())]).unwrap(),
        ids.revision,
        ValidityWindow::new(instant(10), instant(100)).unwrap(),
        UseLimit::limited(2).unwrap(),
    );
    let first = router.exposed(ActorRole::ProviderToolWorker, &scope).unwrap();
    let second = router.exposed(ActorRole::ProviderToolWorker, &scope).unwrap();
    let names = |tools: &peritus_tool_router::ExposedTools| {
        tools
            .descriptors()
            .iter()
            .map(|descriptor| descriptor.name().as_str().to_owned())
            .collect::<Vec<_>>()
    };
    let canonical = names(&first) == names(&second);
    if first.descriptors().len() == 1 {
        observed(ToolDisposition::Succeeded, None, false, true, canonical)
    } else {
        observed(ToolDisposition::Rejected, None, false, false, canonical)
    }
}

fn dispatch(
    request: &ToolConformanceFixture,
    seed: u8,
) -> Result<ToolConformanceObservation, ToolConformanceError> {
    let ids = Ids::new_named(seed, "fs.read");
    let mut router = fixture::router(IdempotencySemantics::ReplayTerminal);
    let prepared = router
        .prepare(fixture::call(&ids, request.canonical_arguments(), "dispatch"))
        .map_err(|_| ToolConformanceError::Infrastructure)?;
    let counters = Arc::new(Counters::default());
    let mut dispatcher = SyncDispatcher::new(&prepared, SyncMode::Success, Arc::clone(&counters));
    let outcome = authorized_dispatch(&mut router, prepared, &ids, &mut dispatcher, true, None)?;
    let DispatchOutcome::Completed(result) = outcome else {
        return Err(ToolConformanceError::Infrastructure);
    };
    Ok(with_result(ToolDisposition::Succeeded, true, counters.effects(), &result))
}

fn authorization(
    request: &ToolConformanceFixture,
    seed: u8,
    drift: ToolAuthorizationDrift,
) -> Result<ToolConformanceObservation, ToolConformanceError> {
    let ids = Ids::new_named(seed, "fs.read");
    let mut router = fixture::router(IdempotencySemantics::ReplayTerminal);
    let prepared = router
        .prepare(fixture::call(&ids, request.canonical_arguments(), "authority"))
        .map_err(|_| ToolConformanceError::Infrastructure)?;
    let counters = Arc::new(Counters::default());
    let mut dispatcher = SyncDispatcher::new(&prepared, SyncMode::Success, Arc::clone(&counters));
    let dispatch_committed = drift != ToolAuthorizationDrift::Dispatch;
    let expected =
        (drift != ToolAuthorizationDrift::Dispatch).then_some(Sha256Digest::new([0xa5; 32]));
    let rejected = authorized_dispatch(
        &mut router,
        prepared,
        &ids,
        &mut dispatcher,
        dispatch_committed,
        expected,
    )
    .is_err();
    let disposition = if rejected { ToolDisposition::Rejected } else { ToolDisposition::Succeeded };
    Ok(observed_with_effects(disposition, false, counters.effects()))
}

fn result_truth(
    request: &ToolConformanceFixture,
    seed: u8,
) -> Result<ToolConformanceObservation, ToolConformanceError> {
    let ids = Ids::new_named(seed, "fs.read");
    let mut router = fixture::router(IdempotencySemantics::ReplayTerminal);
    let prepared = router
        .prepare(fixture::call(&ids, request.canonical_arguments(), "failure"))
        .map_err(|_| ToolConformanceError::Infrastructure)?;
    let counters = Arc::new(Counters::default());
    let mut dispatcher = SyncDispatcher::new(&prepared, SyncMode::Failure, Arc::clone(&counters));
    let outcome = authorized_dispatch(&mut router, prepared, &ids, &mut dispatcher, true, None)?;
    let DispatchOutcome::Completed(result) = outcome else {
        return Err(ToolConformanceError::Infrastructure);
    };
    Ok(with_result(ToolDisposition::Failed, true, counters.effects(), &result))
}

fn lifecycle(
    request: &ToolConformanceFixture,
    seed: u8,
    mode: ActiveMode,
) -> Result<ToolConformanceObservation, ToolConformanceError> {
    let ids = Ids::new_named(seed, "fs.read");
    let mut router = fixture::router(IdempotencySemantics::ReplayTerminal);
    let prepared = router
        .prepare(fixture::call(&ids, request.canonical_arguments(), "lifecycle"))
        .map_err(|_| ToolConformanceError::Infrastructure)?;
    let counters = Arc::new(Counters::default());
    let mut dispatcher = ActiveDispatcher::new(&prepared, mode, Arc::clone(&counters));
    let outcome = authorized_dispatch(&mut router, prepared, &ids, &mut dispatcher, true, None)?;
    let DispatchOutcome::Active(handle) = outcome else {
        return Err(ToolConformanceError::Infrastructure);
    };
    let update = match mode {
        ActiveMode::Cancel => {
            router.cancel(handle, peritus_tool_protocol::CancellationReason::Requested, instant(21))
        }
        ActiveMode::Deadline => router.poll(handle, instant(30_000)),
        ActiveMode::Lost => return Err(ToolConformanceError::Infrastructure),
    }
    .map_err(|_| ToolConformanceError::Infrastructure)?;
    let result = update.terminal().ok_or(ToolConformanceError::Infrastructure)?;
    let disposition = disposition(result.status());
    let observation = ToolConformanceObservation::new(
        disposition,
        None,
        true,
        false,
        true,
        counters.effects(),
        result_observation(result),
        update.progress().iter().map(|event| u64::from(event.sequence()) + 1).collect(),
        counters.control_observed(),
        counters.joined(),
        ToolReplayObservation::default(),
    );
    Ok(observation)
}

fn replay(
    request: &ToolConformanceFixture,
    seed: u8,
    mode: ToolReplayMode,
) -> Result<ToolConformanceObservation, ToolConformanceError> {
    let semantics = if mode == ToolReplayMode::NonIdempotent {
        IdempotencySemantics::ReportPriorOutcome
    } else {
        IdempotencySemantics::ReplayTerminal
    };
    let ids = Ids::new_named(seed, "fs.read");
    let mut router = fixture::router(semantics);
    let prepared = router
        .prepare(fixture::call(&ids, request.canonical_arguments(), "replay"))
        .map_err(|_| ToolConformanceError::Infrastructure)?;
    let root = TestRoot::new();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = open_journal(&root);
    let receipts = commit_authority(&mut journal, &ids, &intent, 30_000, true);
    let authority = authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let counters = Arc::new(Counters::default());
    let (second, replay_observation) = if mode == ToolReplayMode::Indeterminate {
        let mut dispatcher =
            ActiveDispatcher::new(&prepared, ActiveMode::Lost, Arc::clone(&counters));
        let DispatchOutcome::Active(handle) =
            router.dispatch(prepared.clone(), &authority, &mut dispatcher).unwrap()
        else {
            return Err(ToolConformanceError::Infrastructure);
        };
        assert!(matches!(
            router.recover(handle, instant(21)).unwrap(),
            RecoveryOutcome::Indeterminate(_),
        ));
        let second = router.dispatch(prepared, &authority, &mut dispatcher).unwrap();
        (second, ToolReplayObservation::new(false, false, false, true))
    } else {
        let mut dispatcher =
            SyncDispatcher::new(&prepared, SyncMode::Success, Arc::clone(&counters));
        router.dispatch(prepared.clone(), &authority, &mut dispatcher).unwrap();
        let second = if mode == ToolReplayMode::Conflicting {
            let conflict = router
                .prepare(fixture::call(
                    &ids,
                    br#"{"path":"src/lib.rs","max_bytes":2048}"#,
                    "replay",
                ))
                .unwrap();
            router
                .dispatch(conflict, &authority, &mut dispatcher)
                .unwrap_or(DispatchOutcome::PriorOutcome(ReplayDisposition::NonIdempotentTerminal))
        } else {
            router.dispatch(prepared, &authority, &mut dispatcher).unwrap()
        };
        let exact = mode == ToolReplayMode::ExactIdempotent;
        (second, ToolReplayObservation::new(exact, false, !exact, false))
    };
    let disposition = match second {
        DispatchOutcome::Replayed(_) => ToolDisposition::Replayed,
        DispatchOutcome::PriorOutcome(ReplayDisposition::Indeterminate) => {
            ToolDisposition::Indeterminate
        }
        _ => ToolDisposition::Rejected,
    };
    Ok(ToolConformanceObservation::new(
        disposition,
        None,
        true,
        false,
        true,
        counters.effects(),
        empty_result(),
        Vec::new(),
        false,
        true,
        replay_observation,
    ))
}

fn authorized_dispatch(
    router: &mut ToolRouter,
    prepared: PreparedToolCall,
    ids: &Ids,
    dispatcher: &mut dyn ToolDispatcher,
    dispatch_committed: bool,
    expected: Option<Sha256Digest>,
) -> Result<DispatchOutcome, ToolConformanceError> {
    let root = TestRoot::new();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = open_journal(&root);
    let receipts = commit_authority(&mut journal, ids, &intent, 30_000, dispatch_committed);
    let digest = expected.unwrap_or_else(|| prepared.prepared_digest());
    let request = authority_request(ids, &intent, &receipts, digest);
    router
        .dispatch(prepared, &request, dispatcher)
        .map_err(|_| ToolConformanceError::Infrastructure)
}
