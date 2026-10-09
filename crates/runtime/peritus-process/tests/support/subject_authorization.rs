use std::fs;

use peritus_conformance::{
    ProcessAuthorizationDrift, ProcessConformanceError, ProcessConformanceObservation,
    ProcessDisposition, ProcessEffectObservation, ProcessInvocationObservation,
    ProcessOutputObservation, ProcessOwnershipObservation,
};
use peritus_policy::{ActorRole, AuthorityInstant, OperationClass};
use peritus_process::{
    ExecutionAuthorizationRequest, ExecutionGateway, IoMode, ProcessStore, StdinPolicy,
};
use peritus_types::{
    ActionId, CapabilityName, EnvironmentId, Generation, ResourceId, RevisionNumber, RevisionTuple,
    SessionId, Sha256Digest,
};

use super::{
    Ids, PlanOptions, TestRoot, commit_authority, commit_authority_with_lease,
    commit_authority_without_dispatch, intent, open_journal, plan,
};

pub fn exercise(
    root: &TestRoot,
    ids: &Ids,
    drift: ProcessAuthorizationDrift,
) -> Result<ProcessConformanceObservation, ProcessConformanceError> {
    let execution = execution_plan(root, ids)?;
    let original = intent(ids, &execution);
    let mut journal = open_journal(root);
    if drift == ProcessAuthorizationDrift::HolderLease {
        return surplus_lease_rejection(root, ids, execution, &original, &mut journal);
    }
    let reserve = if drift == ProcessAuthorizationDrift::Budget {
        execution.resource_policy().wall_millis() - 1
    } else {
        execution.resource_policy().wall_millis()
    };
    let receipts = if drift == ProcessAuthorizationDrift::Dispatch {
        commit_authority_without_dispatch(&mut journal, ids, &original, reserve)
    } else {
        commit_authority(&mut journal, ids, &original, reserve)
    };
    let mut altered = original.clone();
    alter_intent(&mut altered, drift);
    let revision = if drift == ProcessAuthorizationDrift::Revision {
        RevisionTuple::new(
            ids.revision.acceptance_spec_id(),
            ids.revision.harness_id(),
            ids.revision.workspace_id(),
            ids.revision.workspace_generation(),
            RevisionNumber::new(2).expect("second revision"),
            ids.revision.policy_id(),
            ids.revision.provider_profile_id(),
        )
    } else {
        ids.revision
    };
    let session = if matches!(
        drift,
        ProcessAuthorizationDrift::OwnerLineage
            | ProcessAuthorizationDrift::Dispatch
            | ProcessAuthorizationDrift::HolderLease
    ) {
        SessionId::new([250; 16]).expect("mismatched session")
    } else {
        ids.session
    };
    let generation = if drift == ProcessAuthorizationDrift::Generation {
        Generation::new(2).expect("second generation")
    } else {
        ids.revision.workspace_generation()
    };
    let observed_at = if drift == ProcessAuthorizationDrift::AuthorityTime {
        AuthorityInstant::new(Generation::first(), 101)
    } else {
        receipts.observed_at
    };
    let expected_digest = if drift == ProcessAuthorizationDrift::BackendPreparation {
        Sha256Digest::new([0; 32])
    } else {
        execution.digest()
    };
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).map_err(|_| infrastructure())?,
    );
    let request = ExecutionAuthorizationRequest::new(
        &altered,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        revision,
        session,
        generation,
        ids.revision.workspace_revision(),
        observed_at,
        expected_digest,
    );
    if gateway.launch(&request, execution).is_ok() {
        return Err(infrastructure());
    }
    let effects = ProcessEffectObservation::new(
        0,
        0,
        count_files(&root.registry().join("spools-v1")),
        count_files(&root.registry().join("claims-v1")) != 0,
    );
    Ok(rejected(effects))
}

fn execution_plan(
    root: &TestRoot,
    ids: &Ids,
) -> Result<peritus_process::ExecutionPlan, ProcessConformanceError> {
    plan(
        root,
        ids,
        PlanOptions {
            arguments: vec!["control".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::Closed,
            output_limit: 64,
            wall_timeout: None,
            graceful: peritus_process::GracefulAction::Terminate,
            grace_millis: 100,
            process_count: 1,
            descendants: 0,
            workspace_access: peritus_process::WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .map_err(|_| infrastructure())
}

fn surplus_lease_rejection(
    root: &TestRoot,
    ids: &Ids,
    execution: peritus_process::ExecutionPlan,
    original: &peritus_protocol::ActionIntentDto,
    journal: &mut peritus_journal::SqliteJournal,
) -> Result<ProcessConformanceObservation, ProcessConformanceError> {
    let receipts = commit_authority_with_lease(
        journal,
        ids,
        original,
        execution.resource_policy().wall_millis(),
    );
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).map_err(|_| infrastructure())?,
    );
    let request = ExecutionAuthorizationRequest::new(
        original,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        Some(&receipts.lease),
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        execution.digest(),
    );
    if gateway.launch(&request, execution).is_ok() {
        return Err(infrastructure());
    }
    Ok(rejected(ProcessEffectObservation::new(
        0,
        0,
        count_files(&root.registry().join("spools-v1")),
        count_files(&root.registry().join("claims-v1")) != 0,
    )))
}

fn alter_intent(intent: &mut peritus_protocol::ActionIntentDto, drift: ProcessAuthorizationDrift) {
    match drift {
        ProcessAuthorizationDrift::Action => {
            intent.action_id = ActionId::new([251; 16]).expect("mismatched action");
        }
        ProcessAuthorizationDrift::IntentPayload => intent.payload.push(0),
        ProcessAuthorizationDrift::SandboxDigest => {
            let payload = peritus_process::ExecutionIntentPayload::decode(&intent.payload)
                .expect("execution payload");
            intent.payload = peritus_process::ExecutionIntentPayload::new(
                payload.process_id(),
                payload.execution_plan_digest(),
                Sha256Digest::new([254; 32]),
                payload.backend_descriptor_digest(),
            )
            .encode();
        }
        ProcessAuthorizationDrift::BackendPreparation => {
            let payload = peritus_process::ExecutionIntentPayload::decode(&intent.payload)
                .expect("execution payload");
            intent.payload = peritus_process::ExecutionIntentPayload::new(
                payload.process_id(),
                payload.execution_plan_digest(),
                payload.sandbox_plan_digest(),
                Sha256Digest::new([255; 32]),
            )
            .encode();
        }
        ProcessAuthorizationDrift::MediaType => {
            "application/octet-stream".clone_into(&mut intent.media_type);
        }
        ProcessAuthorizationDrift::ActorRole => intent.role = ActorRole::Reviewer,
        ProcessAuthorizationDrift::Environment => {
            intent.environment_id = EnvironmentId::new([252; 16]).expect("mismatched environment");
        }
        ProcessAuthorizationDrift::Resource => {
            intent.resource_id = ResourceId::new([253; 16]).expect("mismatched resource");
        }
        ProcessAuthorizationDrift::Capability => {
            intent.capability_name =
                CapabilityName::new("process.other".to_owned()).expect("capability");
        }
        ProcessAuthorizationDrift::OperationClass => {
            intent.operation_class = OperationClass::Execution;
        }
        ProcessAuthorizationDrift::OwnerLineage
        | ProcessAuthorizationDrift::Budget
        | ProcessAuthorizationDrift::Dispatch
        | ProcessAuthorizationDrift::Revision
        | ProcessAuthorizationDrift::Generation
        | ProcessAuthorizationDrift::HolderLease
        | ProcessAuthorizationDrift::AuthorityTime => {}
    }
}

fn count_files(path: &std::path::Path) -> u64 {
    fs::read_dir(path).map_or(0, |entries| {
        u64::try_from(entries.filter_map(Result::ok).filter(|entry| entry.path().is_file()).count())
            .unwrap_or(u64::MAX)
    })
}

const fn rejected(effects: ProcessEffectObservation) -> ProcessConformanceObservation {
    ProcessConformanceObservation::new(
        ProcessDisposition::Unauthorized,
        None,
        ProcessInvocationObservation::new(Vec::new(), String::new(), Vec::new(), false),
        ProcessOutputObservation::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
            0,
            0,
            true,
            false,
            false,
        ),
        ProcessOwnershipObservation::new(0, false, false, 0, false, false),
        effects,
        None,
        false,
        false,
    )
}

const fn infrastructure() -> ProcessConformanceError {
    ProcessConformanceError::Infrastructure
}
