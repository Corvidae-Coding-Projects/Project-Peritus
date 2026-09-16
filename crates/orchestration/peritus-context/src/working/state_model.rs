//! Logical model and public reducer frames for working state.

#[cfg(verus_only)]
use super::{ObservationSource, WorkingEntry, WorkingEnvironment, WorkingLimits};
use super::WorkingState;
use vstd::prelude::*;

verus! {
impl WorkingState {
    /// Complete semantic equality used to frame reducers and replay.
    pub open spec fn spec_same(&self, other: &Self) -> bool {
        &&& self.spec_environment().spec_binding()
            == other.spec_environment().spec_binding()
        &&& self.spec_environment().spec_candidate()
            == other.spec_environment().spec_candidate()
        &&& self.spec_environment().spec_files()
            == other.spec_environment().spec_files()
        &&& self.spec_revision() == other.spec_revision()
        &&& self.spec_observations() == other.spec_observations()
        &&& WorkingEntry::sequence_clone_equivalent(
            self.spec_entries(),
            other.spec_entries(),
        )
        &&& self.spec_limits() == other.spec_limits()
        &&& self.spec_protocol().spec_requirements()
            == other.spec_protocol().spec_requirements()
        &&& self.spec_protocol().spec_pending()
            == other.spec_protocol().spec_pending()
    }

    /// Complete successful observation-ingestion result.
    pub open spec fn spec_observation_result(
        &self,
        source: ObservationSource,
        next: &Self,
    ) -> bool {
        &&& next.spec_environment().spec_binding()
            == self.spec_environment().spec_binding()
        &&& next.spec_environment().spec_candidate()
            == self.spec_environment().spec_candidate()
        &&& next.spec_environment().spec_files()
            == self.spec_environment().spec_files()
        &&& next.spec_revision() >= self.spec_revision()
        &&& next.spec_revision() as int <= self.spec_revision() as int + 1
        &&& next.spec_observations().len() >= self.spec_observations().len()
        &&& next.spec_observations().len() <= self.spec_observations().len() + 1
        &&& (source.spec_id().spec_value() <= self.spec_observations().len()
            ==> next.spec_revision() == self.spec_revision()
                && next.spec_observations() == self.spec_observations())
        &&& (source.spec_id().spec_value() > self.spec_observations().len()
            ==> next.spec_revision() as int == self.spec_revision() as int + 1
                && next.spec_observations() == self.spec_observations().push(source))
        &&& WorkingEntry::sequence_clone_equivalent(
            self.spec_entries(), next.spec_entries(),
        )
        &&& next.spec_limits() == self.spec_limits()
        &&& next.spec_protocol().spec_requirements()
            == self.spec_protocol().spec_requirements()
        &&& next.spec_protocol().spec_pending()
            == self.spec_protocol().spec_pending()
    }
}
}
