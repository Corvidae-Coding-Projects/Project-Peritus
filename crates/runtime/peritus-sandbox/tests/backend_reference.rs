//! Backend admission and executable reference-session conformance tests.

mod support;

use peritus_sandbox::{
    AdmissionProfile, BackendDescriptor, BackendKind, BackendName, BackendVersion,
    CancellationAcceptance, CancellationReason, CheckedSandboxPlan, DescendantPolicy, DnsName,
    EnvironmentContract, EnvironmentMode, EnvironmentName, EnvironmentRequirements, FeatureSet,
    FileOperation, FileRequirement, FilesystemContract, InputPermission, IsolationRequirement,
    NetworkContract, NetworkHost, NetworkTarget, PathSemantics, ProbeDecision, ProcessContract,
    ProcessRequirements, ReferenceBackend, ReferenceFault, ReferenceFaultPlan, ReferenceProbe,
    RequestedTerminalOperation, ResizePermission, ResourceDecision, ResourceFidelity,
    SandboxContract, SandboxErrorKind, SandboxFeature, SandboxOperation, SandboxOperationClass,
    SandboxPath, SandboxPhase, SandboxPreparation, SandboxRequirements, SandboxResourceKind,
    SecretContract, SecretDelivery, SecretGrant, SecretReference, SignalPolicy,
    TeardownCompleteness, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TerminalSize, TerminationKind, Transport,
    TreeContainment, admit_backend, compile_sandbox,
};
use peritus_types::{ResourceId, ResourceQuantity, Sha256Digest};

#[test]
fn admission_is_fail_closed_and_bound_to_backend_identity() {
    let plan = support::checked_plan();
    let incomplete = BackendDescriptor::new(
        BackendName::new("incomplete").unwrap(),
        BackendVersion::new("1.0.0").unwrap(),
        BackendKind::Native,
        PathSemantics::UnixNative,
        ResourceFidelity::Hard,
        FeatureSet::empty(),
    );
    let error = admit_backend(&plan, &incomplete, AdmissionProfile::Production).unwrap_err();
    assert_eq!(error.kind(), SandboxErrorKind::UnsupportedBackend);
    assert_eq!(error.missing_features(), plan.required_features());

    let backend = ReferenceBackend::default();
    let production_error =
        admit_backend(&plan, backend.descriptor(), AdmissionProfile::Production).unwrap_err();
    assert_eq!(production_error.kind(), SandboxErrorKind::UnsupportedBackend);

    let admission =
        admit_backend(&plan, backend.descriptor(), AdmissionProfile::Conformance).unwrap();
    assert_eq!(admission.plan_digest(), plan.digest());
    assert_eq!(admission.descriptor_digest(), backend.descriptor().digest());
    assert_ne!(admission.preparation_digest(), plan.digest());
}

#[test]
fn admission_requires_default_filesystem_and_descendant_denial_controls() {
    let plan = checked_default_denial_plan();
    for required in [SandboxFeature::FilesystemRemove, SandboxFeature::ProcessDescendants] {
        assert!(plan.required_features().contains(required));
        let supported = FeatureSet::from_features(
            plan.required_features().iter().filter(|feature| *feature != required),
        );
        let descriptor = BackendDescriptor::new(
            BackendName::new("missing-denial-control").unwrap(),
            BackendVersion::new("1").unwrap(),
            BackendKind::Native,
            PathSemantics::LogicalUtf8,
            ResourceFidelity::Hard,
            supported,
        );
        let error = admit_backend(&plan, &descriptor, AdmissionProfile::Conformance).unwrap_err();
        assert_eq!(error.missing_features(), FeatureSet::from_features([required]));
    }
}

fn checked_default_denial_plan() -> CheckedSandboxPlan {
    let terminal_limits =
        TerminalLimits::new(None, ResourceQuantity::new(16), ResourceQuantity::new(100)).unwrap();
    let contract = SandboxContract::new(
        FilesystemContract::deny_all(),
        ProcessContract::new(
            vec![SandboxPath::new("/bin/tool").unwrap()],
            DescendantPolicy::Denied,
            SignalPolicy::Denied,
            TreeContainment::Required,
            1,
        )
        .unwrap(),
        EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).unwrap(),
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        support::limits(100),
        TerminalContract::new(
            TerminalModes::from_modes([TerminalMode::Pipes]),
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Denied,
            terminal_limits,
        )
        .unwrap(),
    );
    let requirements = SandboxRequirements::new(
        Vec::new(),
        ProcessRequirements::new(SandboxPath::new("/bin/tool").unwrap(), 0, false),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).unwrap(),
        Vec::new(),
        Vec::new(),
        support::limits(50),
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Denied,
            None,
            ResourceQuantity::new(8),
            ResourceQuantity::new(50),
        )
        .unwrap(),
    )
    .unwrap();
    compile_sandbox(
        support::binding(11),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .unwrap()
}

#[test]
fn reference_backend_exercises_all_domains_and_complete_teardown() {
    let plan = support::checked_plan();
    let backend = ReferenceBackend::default();
    let admission =
        admit_backend(&plan, backend.descriptor(), AdmissionProfile::Conformance).unwrap();
    let mut session = backend.prepare(&plan, &admission).unwrap();
    assert_eq!(session.phase(), SandboxPhase::Prepared);
    session.activate().unwrap();

    let allowed_file = FileRequirement::new(
        SandboxPath::new("/workspace/answer.txt").unwrap(),
        FileOperation::Write,
    );
    assert_eq!(
        session.evaluate(&ReferenceProbe::Filesystem(allowed_file)).unwrap(),
        ProbeDecision::Allowed
    );
    assert_eq!(
        session
            .evaluate(&ReferenceProbe::RootProgram(SandboxPath::new("/bin/other").unwrap()))
            .unwrap(),
        ProbeDecision::Denied
    );
    assert_eq!(
        session
            .evaluate(&ReferenceProbe::LiteralEnvironment(EnvironmentName::new("MODE").unwrap(),))
            .unwrap(),
        ProbeDecision::Allowed
    );
    let denied_target = NetworkTarget::new(
        NetworkHost::Dns(DnsName::new("blocked.test").unwrap()),
        Transport::Tcp,
        443,
    )
    .unwrap();
    assert_eq!(
        session.evaluate(&ReferenceProbe::Network(denied_target)).unwrap(),
        ProbeDecision::Denied
    );
    let denied_secret = SecretGrant::new(
        SecretReference::new(ResourceId::new([77; 16]).unwrap(), Sha256Digest::new([78; 32])),
        SecretDelivery::Environment(EnvironmentName::new("TOKEN").unwrap()),
    );
    assert_eq!(
        session.evaluate(&ReferenceProbe::Secret(denied_secret)).unwrap(),
        ProbeDecision::Denied
    );
    assert_eq!(
        session
            .evaluate(&ReferenceProbe::Terminal(RequestedTerminalOperation::Resize(
                TerminalSize::new(80, 24).unwrap(),
            )))
            .unwrap(),
        ProbeDecision::Denied
    );
    assert_eq!(
        session.charge(SandboxResourceKind::Output, ResourceQuantity::new(60)).unwrap(),
        ResourceDecision::WithinLimit
    );
    assert_eq!(
        session.charge(SandboxResourceKind::Output, ResourceQuantity::new(50)).unwrap(),
        ResourceDecision::LimitExceeded(SandboxResourceKind::Output)
    );

    assert_eq!(
        session.cancel(CancellationReason::Requested).unwrap(),
        CancellationAcceptance::Accepted
    );
    assert_eq!(
        session.cancel(CancellationReason::Deadline).unwrap(),
        CancellationAcceptance::AlreadyAccepted
    );
    assert_eq!(session.cancellation().reason(), Some(CancellationReason::Requested));
    session.terminate(TerminationKind::Cancelled(CancellationReason::Requested)).unwrap();
    session.release().unwrap();
    assert_eq!(session.phase(), SandboxPhase::Released);
    assert_eq!(session.teardown_completeness(), TeardownCompleteness::Complete);
    assert!(
        session
            .observations()
            .windows(2)
            .all(|events| events[1].sequence() == events[0].sequence() + 1)
    );
}

#[test]
fn preparation_detects_cross_plan_admission_and_faults_are_deterministic() {
    let first = support::checked_plan();
    let (contract, requirements) = support::contract_and_requirements(false);
    let second = compile_sandbox(
        support::binding(2),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .unwrap();
    let backend = ReferenceBackend::default();
    let admission =
        admit_backend(&first, backend.descriptor(), AdmissionProfile::Conformance).unwrap();
    let mismatch = backend.prepare(&second, &admission).unwrap_err();
    assert_eq!(mismatch.kind(), SandboxErrorKind::BackendMismatch);

    let faulty =
        ReferenceBackend::new(ReferenceFaultPlan::from_faults([ReferenceFault::Prepare]), 32)
            .unwrap();
    let admission =
        admit_backend(&first, faulty.descriptor(), AdmissionProfile::Conformance).unwrap();
    let error = faulty.prepare(&first, &admission).unwrap_err();
    assert_eq!(error.kind(), SandboxErrorKind::InjectedFault);
    assert_eq!(error.operation(), SandboxOperation::Prepare);
}

#[test]
fn observation_capacity_drops_optional_events_but_retains_teardown() {
    let plan = support::checked_plan();
    let backend = ReferenceBackend::new(ReferenceFaultPlan::none(), 6).unwrap();
    let admission = backend.admit(&plan, AdmissionProfile::Conformance).unwrap();
    let mut session = backend.prepare(&plan, &admission).unwrap();
    session.activate().unwrap();
    let probe = ReferenceProbe::RootProgram(SandboxPath::new("/bin/tool").unwrap());
    assert_eq!(session.evaluate(&probe).unwrap(), ProbeDecision::Allowed);
    assert_eq!(session.evaluate(&probe).unwrap(), ProbeDecision::Allowed);
    assert_eq!(session.dropped_observations(), 1);
    session.cancel(CancellationReason::Requested).unwrap();
    session.terminate(TerminationKind::Exited(0)).unwrap();
    session.release().unwrap();
    assert_eq!(session.observations().len(), 6);
    assert_eq!(session.dropped_observations(), 1);
    assert_eq!(session.teardown_completeness(), TeardownCompleteness::Complete);
    assert_eq!(
        session.termination(),
        Some(TerminationKind::Cancelled(CancellationReason::Requested))
    );
    assert_eq!(session.underlying_exit_status(), Some(0));
    assert!(
        session.observations().windows(2).all(|events| events[0].sequence() < events[1].sequence())
    );
    assert!(ReferenceBackend::new(ReferenceFaultPlan::none(), 4).is_err());
}

#[test]
fn first_cancellation_reason_overrides_later_terminal_classification() {
    let plan = support::checked_plan();
    let backend = ReferenceBackend::default();
    let admission = backend.admit(&plan, AdmissionProfile::Conformance).unwrap();
    let mut session = backend.prepare(&plan, &admission).unwrap();
    session.activate().unwrap();
    assert_eq!(
        session.cancel(CancellationReason::Deadline).unwrap(),
        CancellationAcceptance::Accepted
    );
    assert_eq!(
        session.cancel(CancellationReason::BackendFailure).unwrap(),
        CancellationAcceptance::AlreadyAccepted
    );
    session.terminate(TerminationKind::Cancelled(CancellationReason::BackendFailure)).unwrap();
    assert_eq!(
        session.termination(),
        Some(TerminationKind::Cancelled(CancellationReason::Deadline))
    );
    assert_eq!(session.underlying_exit_status(), None);
}
