use super::{ControlRejection, pause, request_cancellation, resume};
use crate::{ActivePhase, AgentPhase};

#[test]
fn resume_rejects_missing_recovery_before_missing_saved_phase_without_mutation() {
    let mut phase = AgentPhase::Paused;
    let mut saved = None;
    assert_eq!(resume(&mut phase, &mut saved, false), Err(ControlRejection::RecoveryUnchecked));
    assert_eq!((phase, saved), (AgentPhase::Paused, None));
    assert_eq!(resume(&mut phase, &mut saved, true), Err(ControlRejection::NoResumablePhase));
    assert_eq!((phase, saved), (AgentPhase::Paused, None));

    phase = AgentPhase::Cancelling;
    for checked in [false, true] {
        assert_eq!(resume(&mut phase, &mut saved, checked), Err(ControlRejection::IllegalPhase));
        assert_eq!((phase, saved), (AgentPhase::Cancelling, None));
    }
}

#[test]
fn control_updates_handle_stale_saved_phase_without_an_unstated_invariant() {
    let mut phase = AgentPhase::Active(ActivePhase::StreamingResponse);
    let mut saved = Some(ActivePhase::PreparingContext);
    pause(&mut phase, &mut saved).expect("active phase pauses");
    assert_eq!(saved, Some(ActivePhase::StreamingResponse));
    assert_eq!(pause(&mut phase, &mut saved), Err(ControlRejection::IllegalPhase));
    assert_eq!((phase, saved), (AgentPhase::Paused, Some(ActivePhase::StreamingResponse)));
    request_cancellation(&mut phase, &mut saved).expect("paused phase cancels");
    assert_eq!((phase, saved), (AgentPhase::Cancelling, None));
}
