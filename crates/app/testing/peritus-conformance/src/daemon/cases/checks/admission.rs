//! Session, command, readiness, framing, and non-authority checks.

use super::super::super::{
    DaemonCommandOutcome, DaemonConformanceFixture, DaemonConformanceObservation, DaemonReadiness,
    DaemonScenario, DaemonSessionOutcome,
};

pub(super) fn matches(
    fixture: &DaemonConformanceFixture,
    observed: &DaemonConformanceObservation,
) -> bool {
    use DaemonConformanceObservation as O;
    use DaemonScenario as D;
    match (fixture.scenario(), observed) {
        (D::CompatibleSession, O::Session(value)) => {
            value.outcome() == DaemonSessionOutcome::Established
                && value.session_stable_on_resume()
                && value.principal_binding_matches()
                && value.negotiated_context_matches()
                && value.external_effects() == 0
        }
        (D::IncompatibleSession, O::Session(value)) => {
            value.outcome() == DaemonSessionOutcome::Incompatible
                && !value.session_stable_on_resume()
                && value.durable_mutations() == 0
                && value.external_effects() == 0
        }
        (D::PeerActorMismatch, O::Session(value)) => {
            value.outcome() == DaemonSessionOutcome::Rejected
                && !value.principal_binding_matches()
                && value.durable_mutations() == 0
                && value.external_effects() == 0
        }
        (D::ContextMismatch, O::Session(value)) => {
            value.outcome() == DaemonSessionOutcome::Rejected
                && value.principal_binding_matches()
                && !value.negotiated_context_matches()
                && value.durable_mutations() == 0
                && value.external_effects() == 0
        }
        (D::NewCommand, O::Command(value)) => {
            value.outcome() == DaemonCommandOutcome::Committed
                && value.committed_events() > 0
                && value.response_range_exact()
                && value.replacement_commands() == 0
                && value.new_durable_appends() > 0
        }
        (D::ReplayCommand, O::Command(value)) => {
            value.outcome() == DaemonCommandOutcome::Replayed
                && value.committed_events() > 0
                && value.response_range_exact()
                && value.original_identity_reconciled()
                && value.replacement_commands() == 0
                && value.new_durable_appends() == 0
                && value.new_external_effects() == 0
        }
        (D::ConflictingCommand, O::Command(value)) => {
            value.outcome() == DaemonCommandOutcome::Conflict
                && value.committed_events() == 0
                && value.replacement_commands() == 0
                && value.new_durable_appends() == 0
                && value.new_external_effects() == 0
        }
        (D::IndeterminateCommand, O::Command(value)) => {
            value.outcome() == DaemonCommandOutcome::Indeterminate
                && value.original_identity_reconciled()
                && value.replacement_commands() == 0
                && value.new_external_effects() == 0
        }
        (D::StaleRevision, O::Command(value)) => {
            value.outcome() == DaemonCommandOutcome::Rejected
                && value.replacement_commands() == 0
                && value.new_durable_appends() == 0
                && value.new_external_effects() == 0
        }
        (D::ReadOnlyAdmission, O::Admission(value)) => {
            value.readiness() == DaemonReadiness::ReadyReadOnly
                && value.read_admitted()
                && !value.mutation_admitted()
                && value.effect_workers_started() == 0
        }
        (D::MalformedFrame, O::Frame(value)) => {
            value.rejected()
                && value.payload_bytes_allocated() == 0
                && value.requests_dispatched() == 0
                && value.external_effects() == 0
        }
        (D::NonAuthority, O::NonAuthority(value)) => {
            value.observation_reported()
                && value.authority_appends() == 0
                && value.external_effects() == 0
                && !value.acceptance_inferred()
        }
        _ => false,
    }
}
