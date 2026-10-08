//! Durable adoption and acknowledgement of native observation deltas.

use peritus_types::ProcessId;

use super::{ProcessStore, store_error};
use crate::{
    ProcessError,
    native::{
        NativeObservationReceipt, NativeObservationTransport, NativeSandboxSession,
        native_observation_producer_binding,
        observation::{NativeObservationFrontier, NativeObservationState, StoredObservationPage},
        native_mismatch,
    },
    registry_storage::{
        acquire_retained_owner_transaction, load_manifest_record,
        persist_native_observation_page, persist_tombstone, write_manifest,
    },
};

impl ProcessStore {
    pub(crate) fn capture_native_observations(
        &self,
        process_id: ProcessId,
        session: &mut dyn NativeSandboxSession,
    ) -> Result<NativeObservationState, ProcessError> {
        if session.observation_transport() == NativeObservationTransport::LegacySnapshot {
            self.record_legacy_native_observations(process_id)?;
            return Ok(NativeObservationState::LegacyUnrecorded);
        }

        loop {
            let current = self.native_observation_state(process_id)?;
            let prior = match current {
                NativeObservationState::Untracked => None,
                NativeObservationState::Recorded(frontier) => {
                    let durable_receipt = receipt(frontier);
                    if session.acknowledged_observation_receipt() != Some(durable_receipt) {
                        self.reestablish_native_observation_frontier(process_id, frontier)?;
                        session.acknowledge_observations(durable_receipt)?;
                    }
                    Some(frontier)
                }
                NativeObservationState::LegacyUnrecorded => {
                    return Err(native_mismatch(
                        "native observation transport changed after legacy recording",
                    ));
                }
            };
            let after_sequence = prior.map_or(0, NativeObservationFrontier::through_sequence);
            let transfer = session.observation_page(after_sequence)?;
            if transfer.head_sequence() < after_sequence {
                return Err(native_mismatch("native observation producer regressed its stream head"));
            }
            if transfer.observations().is_empty() {
                if transfer.head_sequence() != after_sequence {
                    return Err(native_mismatch(
                        "native observation producer omitted an unacknowledged delta",
                    ));
                }
                return Ok(current);
            }
            let (plan_digest, backend_digest) = self.native_observation_binding(process_id)?;
            let producer_binding_digest = native_observation_producer_binding(
                session.launch_description().manifest_digest(),
                session.launch_description().preparation_digest(),
            );
            let observed_head = transfer.head_sequence();
            let (page, next) = StoredObservationPage::from_records(
                process_id,
                plan_digest,
                backend_digest,
                producer_binding_digest,
                prior,
                transfer.into_observations(),
            )?;
            persist_native_observation_page(&self.inner.native_observations, &page)?;
            self.update(process_id, |manifest| {
                if manifest.native_observations == NativeObservationState::Recorded(next) {
                    return Ok(());
                }
                if manifest.native_observations != current {
                    return Err(store_error(
                        "native observation manifest frontier changed concurrently",
                    ));
                }
                manifest.native_observations = NativeObservationState::Recorded(next);
                Ok(())
            })?;
            session.acknowledge_observations(receipt(next))?;
            if next.through_sequence() == observed_head {
                return Ok(NativeObservationState::Recorded(next));
            }
            if next.through_sequence() > observed_head {
                return Err(native_mismatch("native observation page exceeded its declared head"));
            }
        }
    }

    fn record_legacy_native_observations(
        &self,
        process_id: ProcessId,
    ) -> Result<(), ProcessError> {
        match self.native_observation_state(process_id)? {
            NativeObservationState::LegacyUnrecorded => return Ok(()),
            NativeObservationState::Recorded(_) => {
                return Err(native_mismatch(
                    "native observation transport changed after durable recording",
                ));
            }
            NativeObservationState::Untracked => {}
        }
        self.update(process_id, |manifest| match manifest.native_observations {
            NativeObservationState::Untracked => {
                manifest.native_observations = NativeObservationState::LegacyUnrecorded;
                Ok(())
            }
            NativeObservationState::LegacyUnrecorded => Ok(()),
            NativeObservationState::Recorded(_) => Err(native_mismatch(
                "native observation transport changed after durable recording",
            )),
        })
    }

    fn native_observation_state(
        &self,
        process_id: ProcessId,
    ) -> Result<NativeObservationState, ProcessError> {
        self.authoritative_record(process_id)?
            .and_then(|record| record.manifest)
            .map(|manifest| manifest.native_observations)
            .ok_or_else(|| store_error("process manifest is missing"))
    }

    fn native_observation_binding(
        &self,
        process_id: ProcessId,
    ) -> Result<(peritus_types::Sha256Digest, peritus_types::Sha256Digest), ProcessError> {
        let manifest = self
            .authoritative_record(process_id)?
            .and_then(|record| record.manifest)
            .ok_or_else(|| store_error("process manifest is missing"))?;
        Ok((manifest.sandbox_digest, manifest.backend_digest))
    }

    fn reestablish_native_observation_frontier(
        &self,
        process_id: ProcessId,
        expected: NativeObservationFrontier,
    ) -> Result<(), ProcessError> {
        let transaction =
            acquire_retained_owner_transaction(&self.inner.retained_owners, process_id)?;
        let mut state = self.lock_authoritative_identity(&transaction)?;
        let record = state
            .index
            .get(process_id)?
            .ok_or_else(|| store_error("process manifest is missing"))?;
        let manifest = record
            .manifest
            .ok_or_else(|| store_error("process manifest is missing"))?;
        if manifest.native_observations != NativeObservationState::Recorded(expected) {
            return Err(store_error(
                "native observation frontier changed before acknowledgement",
            ));
        }
        if record.retired {
            let claim = record
                .claim
                .ok_or_else(|| store_error("retired native observation frontier has no claim"))?;
            return persist_tombstone(&self.inner.tombstones, claim, &manifest)
                .inspect_err(|_| state.unresolved_io = true);
        }
        let written = write_manifest(&self.inner.manifests, &manifest);
        let actual = load_manifest_record(&self.inner.manifests, process_id).and_then(|manifest| {
            manifest.ok_or_else(|| store_error("native observation manifest disappeared"))
        });
        match actual {
            Ok(actual) if actual == manifest => {}
            Ok(_) => {
                state.unresolved_io = true;
                return Err(store_error(
                    "native observation manifest frontier changed during synchronization",
                ));
            }
            Err(error) => {
                state.unresolved_io = true;
                return Err(error);
            }
        }
        if let Err(error) = written {
            state.unresolved_io = true;
            return Err(error);
        }
        Ok(())
    }
}

const fn receipt(frontier: NativeObservationFrontier) -> NativeObservationReceipt {
    NativeObservationReceipt::new(
        frontier.through_sequence(),
        frontier.last_page_digest,
        frontier.producer_binding_digest,
        frontier.producer_prefix_digest,
    )
}
