use peritus_process::{
    CancellationReason, CommandSpec, NativeLaunchDescription, NativeSandboxSession,
    OsExitObservation, ProcessTreeIdentity,
};
use peritus_sandbox::{CapabilityDomain, ObservationKind};
use peritus_types::Sha256Digest;

use super::{MacosSession, ObservationEvent, SessionPhase, SessionResources, TerminationReason};

pub(super) fn session() -> MacosSession {
    let (exec_status, exec_status_handle) = crate::exec_status::prepare().unwrap();
    let descriptor = u32::try_from(exec_status_handle.raw_handle()).unwrap();
    let manifest = crate::test_support::manifest_with_exec_status(descriptor);
    let launch = NativeLaunchDescription::new(
        CommandSpec::new("/helper", std::iter::empty::<String>()).unwrap(),
        "peritus-macos-helper:test",
        manifest.canonical_bytes().to_vec(),
        manifest.digest(),
        manifest.preparation_digest(),
    )
    .and_then(|launch| launch.with_protected_handles(vec![exec_status_handle]))
    .unwrap();
    MacosSession::new(
        launch,
        manifest,
        Sha256Digest::new([9; 32]),
        None,
        16,
        SessionResources::new(exec_status, None, peritus_secrets::SecretDeliverySession::new()),
    )
    .unwrap()
}

#[test]
fn lifecycle_is_ordered_bounded_and_release_is_idempotent() {
    let mut session = session();
    assert!(session.record_activation(ProcessTreeIdentity::new(44, Some(1), None, true)).is_err());
    assert!(
        session.record_activation(ProcessTreeIdentity::new(44, Some(1), Some(45), true)).is_err()
    );
    session.record_activation(ProcessTreeIdentity::new(44, Some(1), Some(44), true)).unwrap();
    session.record_cancellation(CancellationReason::User).unwrap();
    session.record_cancellation(CancellationReason::User).unwrap();
    session.record_termination(&OsExitObservation::Code(0)).unwrap();
    assert!(!session.record_release().unwrap().already_released());
    assert!(session.record_release().unwrap().already_released());
    assert_eq!(session.phase(), SessionPhase::Released);
    assert_eq!(
        session.observations().iter().map(|event| event.kind()).collect::<Vec<_>>(),
        vec![
            ObservationKind::Prepared,
            ObservationKind::Activated,
            ObservationKind::Cancellation,
            ObservationKind::Terminated,
            ObservationKind::Released,
        ]
    );
    assert_eq!(
        session.observations().iter().map(|event| event.sequence()).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert!(session.recovery_record().cleanup().is_complete());
    assert!(
        session
            .native_observations()
            .windows(2)
            .all(|pair| pair[1].sequence() == pair[0].sequence() + 1)
    );
    assert_eq!(
        session
            .native_observations()
            .iter()
            .filter(|event| {
                event.event() == ObservationEvent::ResourceMapped
                    && event.domain() == Some(CapabilityDomain::Resource)
            })
            .count(),
        8
    );
}

#[test]
fn target_reserved_range_exit_remains_exact_after_activation() {
    for code in 120..=125 {
        let mut session = session();
        session.record_activation(ProcessTreeIdentity::new(44, Some(1), Some(44), true)).unwrap();
        session.record_termination(&OsExitObservation::Code(code)).unwrap();
        assert_eq!(session.termination(), Some(TerminationReason::TargetExit(code)));
        assert_eq!(session.phase(), SessionPhase::Terminated);
        session.record_release().unwrap();
    }
}

#[test]
fn prepared_abandonment_cleans_without_normal_release_observation() {
    let mut session = session();
    assert!(!session.record_release().unwrap().already_released());
    assert!(session.record_release().unwrap().already_released());
    assert_eq!(session.phase(), SessionPhase::Released);
    assert!(session.recovery_record().cleanup().is_complete());
    assert_eq!(session.observations().len(), 1);
    assert_eq!(session.observations()[0].kind(), ObservationKind::Prepared);
    assert!(
        !session
            .native_observations()
            .iter()
            .any(|observation| observation.event() == ObservationEvent::Released)
    );
}
