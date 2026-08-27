//! Subscription, artifact, prompt, and terminal service checks.

use super::super::super::{
    DaemonArtifactIntegrity, DaemonArtifactOutcome, DaemonArtifactPublication,
    DaemonConformanceFixture, DaemonConformanceObservation, DaemonPromptRejection, DaemonScenario,
    DaemonSubscriptionOutcome,
};

pub(super) fn matches(
    fixture: &DaemonConformanceFixture,
    observed: &DaemonConformanceObservation,
) -> bool {
    use DaemonConformanceObservation as O;
    use DaemonScenario as D;
    match (fixture.scenario(), observed) {
        (D::SubscriptionResume, O::Subscription(value)) => {
            value.outcome() == DaemonSubscriptionOutcome::Active
                && value.supplied_cursor() == fixture.source_cursor()
                && value
                    .first_source_cursor()
                    .is_some_and(|cursor| cursor > fixture.source_cursor())
                && value.stable_event_identity()
                && value.peak_in_flight() <= fixture.maximum_in_flight()
        }
        (D::SubscriptionRedelivery, O::Subscription(value)) => {
            value.outcome() == DaemonSubscriptionOutcome::Active
                && value.redeliveries() > 0
                && value.stable_event_identity()
                && value.distinct_attempt_identity()
                && value.peak_in_flight() <= fixture.maximum_in_flight()
        }
        (D::SubscriptionAcknowledgement, O::Subscription(value)) => {
            value.outcome() == DaemonSubscriptionOutcome::Acknowledged
                && value.acknowledgement_contiguous()
                && value.released_capacity() > 0
                && value.journal_records_deleted() == 0
                && value.peak_in_flight() <= fixture.maximum_in_flight()
        }
        (D::SubscriptionGap, O::Subscription(value)) => {
            value.outcome() == DaemonSubscriptionOutcome::SnapshotRequired
                && value.first_source_cursor().is_none()
                && value.journal_records_deleted() == 0
        }
        (D::SubscriptionBackpressure, O::Subscription(value)) => {
            value.outcome() == DaemonSubscriptionOutcome::Backpressured
                && value.peak_in_flight() > 0
                && value.peak_in_flight() <= fixture.maximum_in_flight()
        }
        (D::ArtifactDownload, O::Artifact(value)) => {
            value.outcome() == DaemonArtifactOutcome::Downloaded
                && value.transferred_bytes() == fixture.artifact_size()
                && value.integrity() == DaemonArtifactIntegrity::Exact
                && value.publication() == DaemonArtifactPublication::Available
        }
        (D::ArtifactUpload, O::Artifact(value)) => {
            value.outcome() == DaemonArtifactOutcome::Uploaded
                && value.transferred_bytes() == fixture.artifact_size()
                && value.integrity() == DaemonArtifactIntegrity::Exact
                && value.publication() == DaemonArtifactPublication::Published
        }
        (D::ArtifactCorruption, O::Artifact(value)) => {
            value.outcome() == DaemonArtifactOutcome::CorruptRejected
                && value.integrity() == DaemonArtifactIntegrity::Mismatched
                && value.publication() == DaemonArtifactPublication::Withheld
        }
        (D::PromptFreshness, O::Prompt(value)) => {
            let rejected = value.rejected_attempts();
            value.current_response_settled()
                && rejected.len() == 3
                && rejected.contains(&DaemonPromptRejection::ActorSessionMismatch)
                && rejected.contains(&DaemonPromptRejection::StaleRevisionGeneration)
                && rejected.contains(&DaemonPromptRejection::UnsignedApproval)
                && value.terminal_settlements() == 1
        }
        (D::PtyOrdering, O::Terminal(value)) => {
            value.output_bytes() > 0
                && value.sequence_strictly_increasing()
                && value.offsets_conserved()
                && value.combined_stream_only()
                && value.exit_records() == 1
                && value.configured_buffer_limit() > 0
                && value.peak_buffered_bytes() <= value.configured_buffer_limit()
        }
        _ => false,
    }
}
