//! Paged checkpoint-bearing operations and independently published projection components.

use super::{
    ControlOperation, ControlStore, Error, EvidenceRoot, PendingPublication, PublicationClaim,
    PublicationPlan, PublicationPurpose, reference_owner,
};
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_journal::{ExactFrame, StateInstall};
use peritus_product_runner::control::{
    CheckpointFileMode, CheckpointFileVersion, ConversationRecord, ConversationReplay,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

pub(super) const OPERATION_PAGES: u16 = 3484;
pub(super) const CHECKPOINT_PROJECTION_PAGES: u16 = 3485;
pub(super) const CONTROL_CORE_PAGES: u16 = 3486;
pub(super) const REPLAY_PROJECTION_PAGES: u16 = 3488;
pub(super) const CONTROL_RECORD_PAGES: u16 = 3489;
const PROJECTION_MAGIC_V2: &[u8; 8] = b"pcstate2";
const PROJECTION_MAGIC_V3: &[u8; 8] = b"pcstate3";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StreamReference {
    namespace: u16,
    id: [u8; 16],
    stream: EvidenceRoot,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionRoot {
    schema: u16,
    digest: [u8; 32],
    core: StreamReference,
    checkpoints: StreamReference,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayProjectionRoot {
    schema: u16,
    digest: [u8; 32],
    record: StreamReference,
    replay: StreamReference,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PagedControlEvent {
    operation: StreamReference,
    successor_digest: [u8; 32],
}

/// All checkpoint control pages accepted with one journal transaction and original receipt.
#[derive(Default)]
pub(in crate::product_control::storage) struct ControlPublications {
    pending: Vec<PendingPublication>,
    plans: Vec<PublicationPlan>,
    installs: Vec<StateInstall>,
    staged: BTreeMap<(u16, [u8; 16]), StreamReference>,
}

impl ControlPublications {
    pub(in crate::product_control::storage) fn add_plan(&mut self, plan: PublicationPlan) {
        self.plans.push(plan);
    }

    pub(in crate::product_control::storage) fn append_installs(
        &mut self,
        installs: &mut Vec<StateInstall>,
    ) {
        installs.append(&mut self.installs);
    }

    #[must_use]
    pub(in crate::product_control::storage) fn claims(&self) -> Vec<PublicationClaim> {
        self.plans.iter().map(|plan| plan.claim().clone()).collect()
    }

    pub(in crate::product_control::storage) fn activate(
        &mut self,
        store: &mut ControlStore,
    ) -> Result<(), Error> {
        for plan in std::mem::take(&mut self.plans) {
            self.pending.push(store.activate_publication(&plan)?);
        }
        Ok(())
    }

    pub(in crate::product_control::storage) fn finish(self) -> Result<(), Error> {
        #[cfg(test)]
        if !self.pending.is_empty() {
            super::faults::check(super::SnapshotFaultPoint::AfterRootPublication)?;
        }
        for pending in self.pending {
            pending.finish()?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::product_control::storage) fn before_root_publication(&self) -> Result<(), Error> {
        if !self.pending.is_empty() {
            super::faults::check(super::SnapshotFaultPoint::BeforeRootPublication)?;
        }
        Ok(())
    }
}

impl ControlStore {
    pub(in crate::product_control::storage) fn control_event(
        &self,
        operation: &ControlOperation,
        payload: &[u8],
        successor: &ConversationRecord,
        publications: &mut ControlPublications,
    ) -> Result<ExactFrame, Error> {
        let successor_digest = sha256(&successor.canonical_bytes()?).into_bytes();
        let (schema, payload) = if payload
            .len()
            .checked_add(successor_digest.len())
            .is_none_or(|bytes| bytes > peritus_journal::MAX_STATE_BYTES)
        {
            (
                4,
                serde_json::to_vec(&PagedControlEvent {
                    operation: self.stage_control_stream(
                        operation,
                        OPERATION_PAGES,
                        payload,
                        publications,
                    )?,
                    successor_digest,
                })
                .map_err(|_| Error::Corrupt("cannot encode paged control event"))?,
            )
        } else {
            let mut event = Vec::with_capacity(successor_digest.len() + payload.len());
            event.extend_from_slice(&successor_digest);
            event.extend_from_slice(payload);
            (3, event)
        };
        ExactFrame::new(
            encode_frame(
                super::super::super::FRAME_FAMILY,
                schema,
                &payload,
                CodecLimits::PRODUCTION,
            )
            .map_err(|_| Error::Corrupt("cannot encode control event"))?,
        )
        .map_err(Into::into)
    }

    pub(in crate::product_control::storage) fn control_projection(
        &self,
        operation: &ControlOperation,
        record: &ConversationRecord,
        replay: &ConversationReplay,
        publications: &mut ControlPublications,
    ) -> Result<Vec<u8>, Error> {
        let record_bytes = record.canonical_bytes()?;
        let replay_bytes = replay.checkpoint_bytes()?;
        let root = ReplayProjectionRoot {
            schema: 3,
            digest: sha256(&record_bytes).into_bytes(),
            record: self.stage_control_stream(
                operation,
                CONTROL_RECORD_PAGES,
                &record_bytes,
                publications,
            )?,
            replay: self.stage_control_stream(
                operation,
                REPLAY_PROJECTION_PAGES,
                &replay_bytes,
                publications,
            )?,
        };
        let mut bytes = PROJECTION_MAGIC_V3.to_vec();
        bytes.extend(
            serde_json::to_vec(&root)
                .map_err(|_| Error::Corrupt("cannot encode control projection root"))?,
        );
        Ok(bytes)
    }

    pub(in crate::product_control::storage) fn decode_control_event(
        &self,
        bytes: &[u8],
    ) -> Result<ControlOperation, Error> {
        self.decode_control_event_evidence(bytes).map(|(operation, _)| operation)
    }

    pub(in crate::product_control::storage) fn decode_control_event_evidence(
        &self,
        bytes: &[u8],
    ) -> Result<(ControlOperation, Option<[u8; 32]>), Error> {
        let frame = decode_frame(bytes, CodecLimits::PRODUCTION)
            .map_err(|_| Error::Corrupt("invalid control event frame"))?;
        if frame.header().family() != super::super::super::FRAME_FAMILY {
            return Err(Error::Corrupt("unsupported control event family"));
        }
        match frame.header().schema_version() {
            1 => Ok((ControlOperation::parse(frame.payload())?, None)),
            2 => {
                let reference: StreamReference = serde_json::from_slice(frame.payload())
                    .map_err(|_| Error::Corrupt("invalid paged operation root"))?;
                let payload = self.read_control_stream(&reference, OPERATION_PAGES)?;
                Ok((ControlOperation::parse(&payload)?, None))
            }
            3 => {
                let payload = frame
                    .payload()
                    .get(32..)
                    .ok_or(Error::Corrupt("invalid successor-bound control event"))?;
                let mut successor_digest = [0; 32];
                successor_digest.copy_from_slice(&frame.payload()[..32]);
                Ok((ControlOperation::parse(payload)?, Some(successor_digest)))
            }
            4 => {
                let event: PagedControlEvent = serde_json::from_slice(frame.payload())
                    .map_err(|_| Error::Corrupt("invalid successor-bound paged event"))?;
                let payload = self.read_control_stream(&event.operation, OPERATION_PAGES)?;
                Ok((ControlOperation::parse(&payload)?, Some(event.successor_digest)))
            }
            _ => Err(Error::Corrupt("unsupported control event generation")),
        }
    }

    pub(in crate::product_control::storage) fn decode_control_projection_record(
        &self,
        bytes: &[u8],
    ) -> Result<ConversationRecord, Error> {
        if let Some(bytes) = bytes.strip_prefix(PROJECTION_MAGIC_V3) {
            let root: ReplayProjectionRoot = serde_json::from_slice(bytes)
                .map_err(|_| Error::Corrupt("invalid replay projection root"))?;
            return self.decode_replay_projection_record(&root);
        }
        let Some(bytes) = bytes.strip_prefix(PROJECTION_MAGIC_V2) else {
            return Ok(ConversationRecord::parse(bytes)?);
        };
        let root: ProjectionRoot = serde_json::from_slice(bytes)
            .map_err(|_| Error::Corrupt("invalid paged projection root"))?;
        if root.schema != 2 {
            return Err(Error::Corrupt("unsupported paged projection generation"));
        }
        let record = ConversationRecord::parse_checkpoint_parts(
            &self.read_control_stream(&root.core, CONTROL_CORE_PAGES)?,
            &self.read_control_stream(&root.checkpoints, CHECKPOINT_PROJECTION_PAGES)?,
        )?;
        if sha256(&record.canonical_bytes()?).as_bytes() != &root.digest {
            return Err(Error::Corrupt("paged projection differs from its canonical identity"));
        }
        Ok(record)
    }

    pub(in crate::product_control::storage) fn decode_control_projection_evidence(
        &self,
        bytes: &[u8],
    ) -> Result<(ConversationRecord, Option<Vec<u8>>), Error> {
        if let Some(bytes) = bytes.strip_prefix(PROJECTION_MAGIC_V3) {
            let root: ReplayProjectionRoot = serde_json::from_slice(bytes)
                .map_err(|_| Error::Corrupt("invalid replay projection root"))?;
            let record = self.decode_replay_projection_record(&root)?;
            let replay_bytes =
                self.read_control_stream(&root.replay, REPLAY_PROJECTION_PAGES)?;
            let replay = ConversationReplay::from_checkpoint_bytes(&replay_bytes)?;
            if replay.current() != Some(&record) {
                return Err(Error::Corrupt("replay witness differs from projection record"));
            }
            return Ok((record, Some(replay_bytes)));
        }
        self.decode_control_projection_record(bytes).map(|record| (record, None))
    }

    fn decode_replay_projection_record(
        &self,
        root: &ReplayProjectionRoot,
    ) -> Result<ConversationRecord, Error> {
        if root.schema != 3 {
            return Err(Error::Corrupt("unsupported replay projection generation"));
        }
        let record_bytes = self.read_control_stream(&root.record, CONTROL_RECORD_PAGES)?;
        let record = ConversationRecord::parse(&record_bytes)?;
        if sha256(&record.canonical_bytes()?).as_bytes() != &root.digest {
            return Err(Error::Corrupt(
                "replay projection differs from its canonical identity",
            ));
        }
        Ok(record)
    }

    fn stage_control_stream(
        &self,
        operation: &ControlOperation,
        namespace: u16,
        bytes: &[u8],
        publications: &mut ControlPublications,
    ) -> Result<StreamReference, Error> {
        let digest = sha256(bytes).into_bytes();
        let id = stream_id(namespace, &digest);
        if let Some(reference) = publications.staged.get(&(namespace, id)) {
            if reference.stream.digest != digest || reference.stream.bytes != bytes.len() as u64 {
                return Err(Error::Corrupt("staged control metadata identity conflict"));
            }
            return Ok(reference.clone());
        }
        if let Some(record) = self.journal.state_record(namespace, &id)? {
            let stream: EvidenceRoot = serde_json::from_slice(record.bytes())
                .map_err(|_| Error::Corrupt("invalid control metadata root"))?;
            if stream.schema != 1
                || stream.digest != digest
                || stream.bytes != bytes.len() as u64
                || record.revision() != 1
            {
                return Err(Error::Corrupt("control metadata identity conflict"));
            }
            return Ok(StreamReference { namespace, id, stream });
        }
        let version = CheckpointFileVersion::present(
            peritus_types::Sha256Digest::new(digest),
            bytes.len() as u64,
            CheckpointFileMode::Regular,
        );
        let (root, artifacts) =
            self.spool_stream(operation, &mut io::Cursor::new(bytes), version)?;
        let stream = EvidenceRoot { schema: 1, root, digest, bytes: bytes.len() as u64 };
        publications.installs.push(StateInstall::new(
            namespace,
            id.to_vec(),
            None,
            1,
            serde_json::to_vec(&stream)
                .map_err(|_| Error::Corrupt("cannot encode control metadata root"))?,
        )?);
        let claim = PublicationClaim::new(
            namespace,
            id,
            digest,
            bytes.len() as u64,
            PublicationPurpose::ControlStream,
        )?;
        publications.plans.push(PublicationPlan::new(
            claim,
            reference_owner(namespace, &id),
            artifacts,
        )?);
        let reference = StreamReference { namespace, id, stream };
        publications.staged.insert((namespace, id), reference.clone());
        Ok(reference)
    }

    fn read_control_stream(
        &self,
        reference: &StreamReference,
        namespace: u16,
    ) -> Result<Vec<u8>, Error> {
        if reference.namespace != namespace
            || reference.id != stream_id(namespace, &reference.stream.digest)
            || reference.stream.schema != 1
        {
            return Err(Error::Corrupt("control metadata reference identity differs"));
        }
        let stored = self
            .journal
            .state_record(namespace, &reference.id)?
            .ok_or(Error::Corrupt("control metadata publication missing"))?;
        let stream: EvidenceRoot = serde_json::from_slice(stored.bytes())
            .map_err(|_| Error::Corrupt("invalid control metadata root"))?;
        if stored.revision() != 1 || stream != reference.stream {
            return Err(Error::Corrupt("control metadata publication differs from its reference"));
        }
        self.manifest_bytes(&stream)
    }
}

fn stream_id(namespace: u16, digest: &[u8; 32]) -> [u8; 16] {
    let mut binding = b"peritus-checkpoint-control-pages-v1\0".to_vec();
    binding.extend(namespace.to_be_bytes());
    binding.extend(digest);
    let digest = sha256(&binding);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    id
}
