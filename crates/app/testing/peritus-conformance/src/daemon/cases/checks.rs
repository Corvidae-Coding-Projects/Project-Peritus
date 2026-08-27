//! Scenario-specific assertions over direct daemon observations.

mod admission;
mod lifecycle;
mod services;

use super::super::{DaemonConformanceFixture, DaemonConformanceObservation, DaemonScenario};

pub(super) fn matches_contract(
    fixture: &DaemonConformanceFixture,
    observed: &DaemonConformanceObservation,
) -> bool {
    use DaemonScenario as D;
    match fixture.scenario() {
        D::CompatibleSession
        | D::IncompatibleSession
        | D::PeerActorMismatch
        | D::ContextMismatch
        | D::NewCommand
        | D::ReplayCommand
        | D::ConflictingCommand
        | D::IndeterminateCommand
        | D::StaleRevision
        | D::ReadOnlyAdmission
        | D::MalformedFrame
        | D::NonAuthority => admission::matches(fixture, observed),
        D::SubscriptionResume
        | D::SubscriptionRedelivery
        | D::SubscriptionAcknowledgement
        | D::SubscriptionGap
        | D::SubscriptionBackpressure
        | D::ArtifactDownload
        | D::ArtifactUpload
        | D::ArtifactCorruption
        | D::PromptFreshness
        | D::PtyOrdering => services::matches(fixture, observed),
        D::SecondInstance
        | D::StartupFailure
        | D::OutboxCrash
        | D::GracefulShutdown
        | D::ForcedRestart
        | D::Bounds => lifecycle::matches(fixture, observed),
    }
}
