//! Point-loaded radix indexes atomically installed with D2 review checkpoints.

use std::collections::{BTreeMap, BTreeSet};

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{SqliteJournal, StateInstall};
use peritus_types::{EventId, FindingId, ReviewCycleId, RunId};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::history::index::{
    IndexedCycle, IndexedFact, IndexedFinding, ReconstructedReviewIndex, ReviewIndexKind,
    ReviewIndexManifest, ReviewIndexNode, ReviewIndexNodePayload, ReviewIndexSelection,
    ReviewQuorumIndex,
};
use crate::wire::index::{
    ReviewIndexManifestFrame, ReviewIndexNodeFrame, ReviewQuorumIndexFrame,
};
use crate::{ReviewCommand, ReviewCommandKind, ReviewEvent, ReviewEventKind, ReviewRunState};

pub(super) const REVIEW_INDEX_NAMESPACE: u16 = 0xD202;
const INDEX_KEY_DOMAIN: &[u8] = b"peritus.review.index.v1\0";
const MANIFEST_KEY_TAG: u8 = 1;
const QUORUM_KEY_TAG: u8 = 2;
const NODE_KEY_TAG: u8 = 3;

struct LoadedNode {
    expected_revision: Option<u64>,
    node: ReviewIndexNode,
    dirty: bool,
}

#[derive(Clone, Copy)]
enum IdentityWrite {
    Insert,
    Update,
}

/// Exact predecessor index observation retained until one guarded C0 append.
pub(super) struct ReviewIndexWorkspace {
    manifest_revision: Option<u64>,
    quorum_revision: Option<u64>,
    manifest: ReviewIndexManifest,
    quorum: ReviewQuorumIndex,
    nodes: BTreeMap<Vec<u8>, LoadedNode>,
}

impl ReviewIndexWorkspace {
    pub(super) fn load(
        journal: &SqliteJournal,
        state: &ReviewRunState,
    ) -> Result<Option<Self>, ReviewError> {
        let manifest_key = manifest_key(state.run_id());
        let Some(record) = journal
            .state_record(REVIEW_INDEX_NAMESPACE, &manifest_key)
            .map_err(super::journal_error)?
        else {
            return Ok(None);
        };
        let frame = decode_message::<ReviewIndexManifestFrame>(
            record.bytes(),
            CodecLimits::PRODUCTION,
        )
        .map_err(super::codec_error)?;
        let manifest = frame.manifest();
        if manifest.sequence() > state.sequence().get() {
            return Err(stale("review index frontier advanced beyond the loaded checkpoint"));
        }
        manifest.validate(state)?;

        let quorum_key = quorum_key(state.run_id());
        let quorum_record = journal
            .state_record(REVIEW_INDEX_NAMESPACE, &quorum_key)
            .map_err(super::journal_error)?
            .ok_or_else(|| super::inconsistent("review index manifest has no quorum row"))?;
        let quorum_frame = decode_message::<ReviewQuorumIndexFrame>(
            quorum_record.bytes(),
            CodecLimits::PRODUCTION,
        )
        .map_err(super::codec_error)?;
        if quorum_frame.manifest() != manifest {
            return Err(stale("review quorum index and manifest frontiers differ"));
        }
        quorum_frame.quorum().validate(state.binding(), state.limits())?;
        if quorum_frame.quorum().report() != state.quorum() {
            return Err(super::inconsistent(
                "review quorum index differs from the exact checkpoint",
            ));
        }
        Ok(Some(Self {
            manifest_revision: Some(record.revision()),
            quorum_revision: Some(quorum_record.revision()),
            manifest,
            quorum: quorum_frame.into_quorum(),
            nodes: BTreeMap::new(),
        }))
    }

    pub(super) fn from_reconstructed(
        state: &ReviewRunState,
        reconstructed: ReconstructedReviewIndex,
    ) -> Result<Self, ReviewError> {
        reconstructed.validate_checkpoint(state)?;
        let (cycles, findings, facts, quorum) = reconstructed.into_parts()?;
        let manifest = ReviewIndexManifest::from_state(state);
        let mut nodes = BTreeMap::new();
        if !cycles.is_empty() {
            insert_bootstrap_node(
                &mut nodes,
                state,
                ReviewIndexKind::Cycles,
                ReviewIndexNodePayload::Cycles(cycles.into_values().collect()),
            );
        }
        if !findings.is_empty() {
            insert_bootstrap_node(
                &mut nodes,
                state,
                ReviewIndexKind::Findings,
                ReviewIndexNodePayload::Findings(findings.into_values().collect()),
            );
        }
        if !facts.is_empty() {
            insert_bootstrap_node(
                &mut nodes,
                state,
                ReviewIndexKind::Facts,
                ReviewIndexNodePayload::Facts(facts.into_values().collect()),
            );
        }
        Ok(Self {
            manifest_revision: None,
            quorum_revision: None,
            manifest,
            quorum,
            nodes,
        })
    }

    pub(super) fn selection_for_command(
        &mut self,
        journal: &SqliteJournal,
        command: &ReviewCommand,
    ) -> Result<ReviewIndexSelection, ReviewError> {
        let mut cycle_ids = BTreeSet::new();
        let mut finding_ids = BTreeSet::new();
        let mut facts = Vec::new();
        match command.kind() {
            ReviewCommandKind::AssignReviewer { assignment } => {
                cycle_ids.insert(assignment.cycle_id());
            }
            ReviewCommandKind::SubmitReview { submission } => {
                cycle_ids.insert(submission.cycle_id());
                finding_ids.extend(submission.findings().iter().map(crate::Finding::id));
            }
            ReviewCommandKind::ReconcileDuplicates { canonical, duplicates, .. } => {
                finding_ids.insert(*canonical);
                finding_ids.extend(duplicates.iter().copied());
            }
            ReviewCommandKind::RecordFixerResponse { finding_id, response } => {
                finding_ids.insert(*finding_id);
                if let Some(superseding) = response.superseding() {
                    finding_ids.insert(superseding);
                }
            }
            ReviewCommandKind::RequestWaiver { finding_id, request } => {
                finding_ids.insert(*finding_id);
                if let Some(superseding) = request.superseding() {
                    finding_ids.insert(superseding);
                }
                if let Some(approval_request_id) = request.approval_request_id() {
                    facts.push(IndexedFact::waiver_request(approval_request_id));
                }
            }
            ReviewCommandKind::ConfirmResolution { finding_id, reviewer_cycle, .. }
            | ReviewCommandKind::ConfirmInvalidation { finding_id, reviewer_cycle, .. } => {
                finding_ids.insert(*finding_id);
                cycle_ids.insert(*reviewer_cycle);
            }
            ReviewCommandKind::ConfirmSupersession {
                finding_id,
                superseding,
                reviewer_cycle,
                ..
            } => {
                finding_ids.insert(*finding_id);
                finding_ids.insert(*superseding);
                cycle_ids.insert(*reviewer_cycle);
            }
            ReviewCommandKind::ObserveWaiver { waiver } => {
                finding_ids.insert(waiver.finding_id());
                facts.push(IndexedFact::waiver_observation(
                    waiver.finding_id(),
                    waiver.revision(),
                ));
            }
            ReviewCommandKind::CancelCycle { cycle_id } => {
                cycle_ids.insert(*cycle_id);
            }
            ReviewCommandKind::StartRun { .. }
            | ReviewCommandKind::AdvanceRevision { .. }
            | ReviewCommandKind::CancelRun
            | ReviewCommandKind::PauseRun
            | ReviewCommandKind::ResumeRun
            | ReviewCommandKind::ExhaustBudget { .. }
            | ReviewCommandKind::FailRun { .. }
            | ReviewCommandKind::FinalizeRun => {}
        }

        let mut selection = ReviewIndexSelection::empty();
        for cycle_id in cycle_ids {
            if let Some(cycle) = self.get_cycle(journal, cycle_id)? {
                selection.cycles.insert(cycle_id, cycle);
            }
        }
        for finding_id in finding_ids {
            if let Some(finding) = self.get_finding(journal, finding_id)? {
                selection.findings.insert(finding_id, finding);
            }
        }
        for fact in facts {
            let exists = self.get_fact(journal, &fact)?.is_some();
            match fact {
                IndexedFact::WaiverRequest { .. } => {
                    selection.waiver_request_exists = exists;
                }
                IndexedFact::WaiverObservation { .. } => {
                    selection.waiver_observation_exists = exists;
                }
            }
        }
        Ok(selection)
    }

    pub(super) fn apply_transition(
        &mut self,
        journal: &SqliteJournal,
        event: &ReviewEvent,
        successor: &ReviewRunState,
    ) -> Result<(), ReviewError> {
        let sequence = successor.sequence().get();
        let event_id = successor.last_event_id();
        match event.kind() {
            ReviewEventKind::RunStarted { binding, .. }
            | ReviewEventKind::RevisionAdvanced { binding } => {
                self.quorum.reset(binding);
            }
            ReviewEventKind::ReviewerAssigned { assignment } => {
                self.insert_cycle(
                    journal,
                    IndexedCycle::assigned(assignment.clone()),
                    sequence,
                    event_id,
                )?;
            }
            ReviewEventKind::ReviewSubmitted { submission } => {
                let mut cycle = self.get_cycle(journal, submission.cycle_id())?.ok_or_else(|| {
                    super::inconsistent("submitted review cycle is absent from its identity index")
                })?;
                cycle.submit(submission);
                self.update_cycle(journal, cycle.clone(), sequence, event_id)?;
                self.quorum.observe_submission(successor.binding(), cycle);
                for finding in submission.findings() {
                    self.insert_successor_finding(
                        journal,
                        successor,
                        finding.id(),
                        sequence,
                        event_id,
                    )?;
                }
            }
            ReviewEventKind::DuplicatesReconciled { canonical, duplicates, .. } => {
                self.update_successor_finding(journal, successor, *canonical, sequence, event_id)?;
                for duplicate in duplicates {
                    self.update_successor_finding(
                        journal,
                        successor,
                        *duplicate,
                        sequence,
                        event_id,
                    )?;
                }
            }
            ReviewEventKind::FixerResponseRecorded { finding_id, .. }
            | ReviewEventKind::ResolutionConfirmed { finding_id, .. }
            | ReviewEventKind::InvalidationConfirmed { finding_id, .. }
            | ReviewEventKind::WaiverRequested { finding_id, .. } => {
                self.update_successor_finding(
                    journal,
                    successor,
                    *finding_id,
                    sequence,
                    event_id,
                )?;
                if let ReviewEventKind::WaiverRequested { request, .. } = event.kind()
                    && let Some(approval_request_id) = request.approval_request_id()
                {
                    self.put_fact(
                        journal,
                        IndexedFact::waiver_request(approval_request_id),
                        sequence,
                        event_id,
                    )?;
                }
            }
            ReviewEventKind::SupersessionConfirmed { finding_id, superseding, .. } => {
                self.update_successor_finding(
                    journal,
                    successor,
                    *finding_id,
                    sequence,
                    event_id,
                )?;
                self.update_successor_finding(
                    journal,
                    successor,
                    *superseding,
                    sequence,
                    event_id,
                )?;
            }
            ReviewEventKind::WaiverObserved { waiver } => {
                self.update_successor_finding(
                    journal,
                    successor,
                    waiver.finding_id(),
                    sequence,
                    event_id,
                )?;
                self.put_fact(
                    journal,
                    IndexedFact::waiver_observation(waiver.finding_id(), waiver.revision()),
                    sequence,
                    event_id,
                )?;
            }
            ReviewEventKind::CycleCancelled { cycle_id } => {
                let mut cycle = self.get_cycle(journal, *cycle_id)?.ok_or_else(|| {
                    super::inconsistent("cancelled review cycle is absent from its identity index")
                })?;
                cycle.set_phase(crate::history::index::IndexedCyclePhase::Cancelled);
                self.update_cycle(journal, cycle, sequence, event_id)?;
                self.quorum.observe_cancellation(successor.binding(), *cycle_id);
            }
            ReviewEventKind::RunCancelled
            | ReviewEventKind::RunPaused
            | ReviewEventKind::RunResumed
            | ReviewEventKind::BudgetExhausted { .. }
            | ReviewEventKind::RunFailed { .. }
            | ReviewEventKind::RunFinalized => {}
        }
        if self.quorum.report() != successor.quorum() {
            return Err(super::inconsistent(
                "successor review quorum differs from its durable index update",
            ));
        }
        self.manifest = ReviewIndexManifest::from_state(successor);
        Ok(())
    }

    pub(super) fn prepare_installs(
        &mut self,
        journal: &SqliteJournal,
    ) -> Result<Vec<StateInstall>, ReviewError> {
        self.split_oversized_nodes(journal)?;
        let mut installs = Vec::new();
        for (key, loaded) in &self.nodes {
            if !loaded.dirty {
                continue;
            }
            let bytes = encode_message(
                &ReviewIndexNodeFrame(loaded.node.clone()),
                CodecLimits::PRODUCTION,
            )
            .map_err(super::codec_error)?;
            installs.push(
                StateInstall::new(
                    REVIEW_INDEX_NAMESPACE,
                    key.clone(),
                    loaded.expected_revision,
                    next_revision(loaded.expected_revision)?,
                    bytes,
                )
                .map_err(super::journal_error)?,
            );
        }
        let manifest_bytes = encode_message(
            &ReviewIndexManifestFrame(self.manifest),
            CodecLimits::PRODUCTION,
        )
        .map_err(super::codec_error)?;
        installs.push(
            StateInstall::new(
                REVIEW_INDEX_NAMESPACE,
                manifest_key(self.manifest.run_id()),
                self.manifest_revision,
                next_revision(self.manifest_revision)?,
                manifest_bytes,
            )
            .map_err(super::journal_error)?,
        );
        let quorum_bytes = encode_message(
            &ReviewQuorumIndexFrame::new(self.manifest, self.quorum.clone()),
            CodecLimits::PRODUCTION,
        )
        .map_err(super::codec_error)?;
        installs.push(
            StateInstall::new(
                REVIEW_INDEX_NAMESPACE,
                quorum_key(self.manifest.run_id()),
                self.quorum_revision,
                next_revision(self.quorum_revision)?,
                quorum_bytes,
            )
            .map_err(super::journal_error)?,
        );
        installs.sort_by(|left, right| {
            (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
        });
        Ok(installs)
    }

    fn insert_successor_finding(
        &mut self,
        journal: &SqliteJournal,
        successor: &ReviewRunState,
        finding_id: FindingId,
        sequence: u64,
        event_id: EventId,
    ) -> Result<(), ReviewError> {
        self.write_successor_finding(
            journal,
            successor,
            finding_id,
            sequence,
            event_id,
            IdentityWrite::Insert,
        )
    }

    fn update_successor_finding(
        &mut self,
        journal: &SqliteJournal,
        successor: &ReviewRunState,
        finding_id: FindingId,
        sequence: u64,
        event_id: EventId,
    ) -> Result<(), ReviewError> {
        self.write_successor_finding(
            journal,
            successor,
            finding_id,
            sequence,
            event_id,
            IdentityWrite::Update,
        )
    }

    fn write_successor_finding(
        &mut self,
        journal: &SqliteJournal,
        successor: &ReviewRunState,
        finding_id: FindingId,
        sequence: u64,
        event_id: EventId,
        write: IdentityWrite,
    ) -> Result<(), ReviewError> {
        let finding = successor.finding(finding_id).cloned().ok_or_else(|| {
            super::inconsistent("updated review finding is absent from the successor checkpoint")
        })?;
        self.write_finding(
            journal,
            IndexedFinding::new(successor.binding().digest(), finding),
            sequence,
            event_id,
            write,
        )
    }

    fn get_cycle(
        &mut self,
        journal: &SqliteJournal,
        cycle_id: ReviewCycleId,
    ) -> Result<Option<IndexedCycle>, ReviewError> {
        let route = cycle_id.as_bytes();
        let Some(key) = self.find_leaf(journal, ReviewIndexKind::Cycles, route, None)? else {
            return Ok(None);
        };
        let loaded = self.nodes.get(&key).ok_or_else(|| missing_node())?;
        let ReviewIndexNodePayload::Cycles(cycles) = loaded.node.payload() else {
            return Err(super::inconsistent("cycle index leaf has another payload kind"));
        };
        Ok(cycles
            .binary_search_by_key(&cycle_id, IndexedCycle::id)
            .ok()
            .map(|index| cycles[index].clone()))
    }

    fn get_finding(
        &mut self,
        journal: &SqliteJournal,
        finding_id: FindingId,
    ) -> Result<Option<IndexedFinding>, ReviewError> {
        let route = finding_id.as_bytes();
        let Some(key) = self.find_leaf(journal, ReviewIndexKind::Findings, route, None)? else {
            return Ok(None);
        };
        let loaded = self.nodes.get(&key).ok_or_else(|| missing_node())?;
        let ReviewIndexNodePayload::Findings(findings) = loaded.node.payload() else {
            return Err(super::inconsistent("finding index leaf has another payload kind"));
        };
        Ok(findings
            .binary_search_by_key(&finding_id, IndexedFinding::id)
            .ok()
            .map(|index| findings[index].clone()))
    }

    fn get_fact(
        &mut self,
        journal: &SqliteJournal,
        fact: &IndexedFact,
    ) -> Result<Option<IndexedFact>, ReviewError> {
        let route = fact.key_bytes();
        let Some(key) = self.find_leaf(journal, ReviewIndexKind::Facts, &route, None)? else {
            return Ok(None);
        };
        let loaded = self.nodes.get(&key).ok_or_else(|| missing_node())?;
        let ReviewIndexNodePayload::Facts(facts) = loaded.node.payload() else {
            return Err(super::inconsistent("fact index leaf has another payload kind"));
        };
        Ok(facts
            .binary_search_by(|candidate| candidate.key_bytes().cmp(&route))
            .ok()
            .map(|index| facts[index].clone()))
    }

    fn insert_cycle(
        &mut self,
        journal: &SqliteJournal,
        cycle: IndexedCycle,
        sequence: u64,
        event_id: EventId,
    ) -> Result<(), ReviewError> {
        self.write_cycle(journal, cycle, sequence, event_id, IdentityWrite::Insert)
    }

    fn update_cycle(
        &mut self,
        journal: &SqliteJournal,
        cycle: IndexedCycle,
        sequence: u64,
        event_id: EventId,
    ) -> Result<(), ReviewError> {
        self.write_cycle(journal, cycle, sequence, event_id, IdentityWrite::Update)
    }

    fn write_cycle(
        &mut self,
        journal: &SqliteJournal,
        cycle: IndexedCycle,
        sequence: u64,
        event_id: EventId,
        write: IdentityWrite,
    ) -> Result<(), ReviewError> {
        let route = *cycle.id().as_bytes();
        let create_frontier =
            matches!(write, IdentityWrite::Insert).then_some((sequence, event_id));
        let key = self
            .find_leaf(journal, ReviewIndexKind::Cycles, &route, create_frontier)?
            .ok_or_else(|| {
                super::inconsistent("updated review cycle is absent from its identity index")
            })?;
        let loaded = self.nodes.get_mut(&key).ok_or_else(missing_node)?;
        let ReviewIndexNodePayload::Cycles(cycles) = loaded.node.payload_mut() else {
            return Err(super::inconsistent("cycle index leaf has another payload kind"));
        };
        match (write, cycles.binary_search_by_key(&cycle.id(), IndexedCycle::id)) {
            (IdentityWrite::Insert, Ok(_)) => {
                return Err(reject(
                    ReviewErrorKind::IdentityConflict,
                    "review cycle identity was already indexed",
                ));
            }
            (IdentityWrite::Insert, Err(index)) => cycles.insert(index, cycle),
            (IdentityWrite::Update, Ok(index)) => cycles[index] = cycle,
            (IdentityWrite::Update, Err(_)) => {
                return Err(super::inconsistent(
                    "updated review cycle is absent from its identity index",
                ));
            }
        }
        loaded.node.advance(sequence, event_id);
        loaded.dirty = true;
        Ok(())
    }

    fn write_finding(
        &mut self,
        journal: &SqliteJournal,
        finding: IndexedFinding,
        sequence: u64,
        event_id: EventId,
        write: IdentityWrite,
    ) -> Result<(), ReviewError> {
        let route = *finding.id().as_bytes();
        let create_frontier =
            matches!(write, IdentityWrite::Insert).then_some((sequence, event_id));
        let key = self
            .find_leaf(journal, ReviewIndexKind::Findings, &route, create_frontier)?
            .ok_or_else(|| {
                super::inconsistent("updated review finding is absent from its identity index")
            })?;
        let loaded = self.nodes.get_mut(&key).ok_or_else(missing_node)?;
        let ReviewIndexNodePayload::Findings(findings) = loaded.node.payload_mut() else {
            return Err(super::inconsistent("finding index leaf has another payload kind"));
        };
        match (write, findings.binary_search_by_key(&finding.id(), IndexedFinding::id)) {
            (IdentityWrite::Insert, Ok(_)) => {
                return Err(reject(
                    ReviewErrorKind::IdentityConflict,
                    "review finding identity was already indexed",
                ));
            }
            (IdentityWrite::Insert, Err(index)) => findings.insert(index, finding),
            (IdentityWrite::Update, Ok(index)) => findings[index] = finding,
            (IdentityWrite::Update, Err(_)) => {
                return Err(super::inconsistent(
                    "updated review finding is absent from its identity index",
                ));
            }
        }
        loaded.node.advance(sequence, event_id);
        loaded.dirty = true;
        Ok(())
    }

    fn put_fact(
        &mut self,
        journal: &SqliteJournal,
        fact: IndexedFact,
        sequence: u64,
        event_id: EventId,
    ) -> Result<(), ReviewError> {
        let route = fact.key_bytes();
        let key = self
            .find_leaf(
                journal,
                ReviewIndexKind::Facts,
                &route,
                Some((sequence, event_id)),
            )?
            .ok_or_else(missing_node)?;
        let loaded = self.nodes.get_mut(&key).ok_or_else(missing_node)?;
        let ReviewIndexNodePayload::Facts(facts) = loaded.node.payload_mut() else {
            return Err(super::inconsistent("fact index leaf has another payload kind"));
        };
        match facts.binary_search_by(|candidate| candidate.key_bytes().cmp(&route)) {
            Ok(_) => {
                return Err(super::inconsistent(
                    "immutable review fact was inserted more than once",
                ));
            }
            Err(index) => facts.insert(index, fact),
        }
        loaded.node.advance(sequence, event_id);
        loaded.dirty = true;
        Ok(())
    }

    fn find_leaf(
        &mut self,
        journal: &SqliteJournal,
        kind: ReviewIndexKind,
        route: &[u8],
        create_frontier: Option<(u64, EventId)>,
    ) -> Result<Option<Vec<u8>>, ReviewError> {
        let mut prefix = Vec::new();
        let mut must_exist = false;
        loop {
            let key = node_key(self.manifest.run_id(), kind, &prefix);
            if !self.nodes.contains_key(&key) {
                let record = journal
                    .state_record(REVIEW_INDEX_NAMESPACE, &key)
                    .map_err(super::journal_error)?;
                match record {
                    Some(record) => {
                        let frame = decode_message::<ReviewIndexNodeFrame>(
                            record.bytes(),
                            CodecLimits::PRODUCTION,
                        )
                        .map_err(super::codec_error)?;
                        let node = frame.into_node();
                        validate_node(&node, self.manifest, kind, &prefix)?;
                        self.nodes.insert(
                            key.clone(),
                            LoadedNode {
                                expected_revision: Some(record.revision()),
                                node,
                                dirty: false,
                            },
                        );
                    }
                    None => {
                        if must_exist {
                            return Err(super::inconsistent(
                                "review radix branch names an absent child row",
                            ));
                        }
                        let Some((sequence, event_id)) = create_frontier else {
                            return Ok(None);
                        };
                        let payload = empty_leaf(kind);
                        self.nodes.insert(
                            key.clone(),
                            LoadedNode {
                                expected_revision: None,
                                node: ReviewIndexNode::from_parts(
                                    self.manifest.run_id(),
                                    kind,
                                    prefix.clone(),
                                    sequence,
                                    event_id,
                                    payload,
                                ),
                                dirty: true,
                            },
                        );
                    }
                }
            }
            let branch = match self.nodes.get(&key).ok_or_else(missing_node)?.node.payload() {
                ReviewIndexNodePayload::Branch(children) => Some(children.clone()),
                _ => None,
            };
            let Some(children) = branch else {
                return Ok(Some(key));
            };
            let child = *route.get(prefix.len()).ok_or_else(|| {
                super::inconsistent("review radix branch is deeper than its exact identity key")
            })?;
            if children.binary_search(&child).is_err() {
                let Some((sequence, event_id)) = create_frontier else {
                    return Ok(None);
                };
                let parent = self.nodes.get_mut(&key).ok_or_else(missing_node)?;
                let ReviewIndexNodePayload::Branch(children) = parent.node.payload_mut() else {
                    return Err(missing_node());
                };
                let index = children.binary_search(&child).unwrap_or_else(|index| index);
                children.insert(index, child);
                parent.node.advance(sequence, event_id);
                parent.dirty = true;
                must_exist = false;
            } else {
                must_exist = true;
            }
            prefix.push(child);
        }
    }

    fn split_oversized_nodes(&mut self, journal: &SqliteJournal) -> Result<(), ReviewError> {
        loop {
            let oversized = self
                .nodes
                .iter()
                .find_map(|(key, loaded)| {
                    if !loaded.dirty
                        || matches!(loaded.node.payload(), ReviewIndexNodePayload::Branch(_))
                    {
                        return None;
                    }
                    match encode_message(
                        &ReviewIndexNodeFrame(loaded.node.clone()),
                        CodecLimits::PRODUCTION,
                    ) {
                        Ok(bytes) if bytes.len() <= peritus_journal::MAX_STATE_BYTES => None,
                        Ok(_) | Err(_) => Some(key.clone()),
                    }
                });
            let Some(key) = oversized else {
                return Ok(());
            };
            self.split_leaf(journal, &key)?;
        }
    }

    fn split_leaf(&mut self, journal: &SqliteJournal, key: &[u8]) -> Result<(), ReviewError> {
        let loaded = self.nodes.get(key).ok_or_else(missing_node)?;
        let prefix = loaded.node.prefix().to_vec();
        let kind = loaded.node.kind();
        let sequence = loaded.node.through_sequence();
        let event_id = loaded.node.through_event();
        let payload = loaded.node.payload().clone();
        let groups = partition_payload(&payload, prefix.len())?;
        let children = groups.keys().copied().collect::<Vec<_>>();
        for (child, payload) in groups {
            let mut child_prefix = prefix.clone();
            child_prefix.push(child);
            let child_key = node_key(self.manifest.run_id(), kind, &child_prefix);
            if self.nodes.contains_key(&child_key)
                || journal
                    .state_record(REVIEW_INDEX_NAMESPACE, &child_key)
                    .map_err(super::journal_error)?
                    .is_some()
            {
                return Err(super::inconsistent(
                    "review radix split would overwrite a retained descendant row",
                ));
            }
            self.nodes.insert(
                child_key,
                LoadedNode {
                    expected_revision: None,
                    node: ReviewIndexNode::from_parts(
                        self.manifest.run_id(),
                        kind,
                        child_prefix,
                        sequence,
                        event_id,
                        payload,
                    ),
                    dirty: true,
                },
            );
        }
        let loaded = self.nodes.get_mut(key).ok_or_else(missing_node)?;
        loaded.node = ReviewIndexNode::from_parts(
            self.manifest.run_id(),
            kind,
            prefix,
            sequence,
            event_id,
            ReviewIndexNodePayload::Branch(children),
        );
        loaded.dirty = true;
        Ok(())
    }
}

fn insert_bootstrap_node(
    nodes: &mut BTreeMap<Vec<u8>, LoadedNode>,
    state: &ReviewRunState,
    kind: ReviewIndexKind,
    payload: ReviewIndexNodePayload,
) {
    let key = node_key(state.run_id(), kind, &[]);
    nodes.insert(
        key,
        LoadedNode {
            expected_revision: None,
            node: ReviewIndexNode::from_parts(
                state.run_id(),
                kind,
                Vec::new(),
                state.sequence().get(),
                state.last_event_id(),
                payload,
            ),
            dirty: true,
        },
    );
}

fn validate_node(
    node: &ReviewIndexNode,
    manifest: ReviewIndexManifest,
    kind: ReviewIndexKind,
    prefix: &[u8],
) -> Result<(), ReviewError> {
    if node.run_id() != manifest.run_id()
        || node.kind() != kind
        || node.prefix() != prefix
    {
        return Err(super::inconsistent(
            "review index node differs from its collision-free typed key",
        ));
    }
    if node.through_sequence() > manifest.sequence() {
        return Err(stale("review index node advanced beyond its observed manifest"));
    }
    if node.through_sequence() == manifest.sequence()
        && node.through_event() != manifest.event_id()
    {
        return Err(super::inconsistent(
            "review index node names another event at the manifest sequence",
        ));
    }
    let valid = match node.payload() {
        ReviewIndexNodePayload::Branch(_) => true,
        ReviewIndexNodePayload::Cycles(cycles) => cycles
            .iter()
            .all(|cycle| cycle.id().as_bytes().starts_with(prefix)),
        ReviewIndexNodePayload::Findings(findings) => findings
            .iter()
            .all(|finding| finding.id().as_bytes().starts_with(prefix)),
        ReviewIndexNodePayload::Facts(facts) => {
            facts.iter().all(|fact| fact.key_bytes().starts_with(prefix))
        }
    };
    if !valid {
        return Err(super::inconsistent(
            "review index leaf contains an entry outside its collision-free prefix",
        ));
    }
    Ok(())
}

fn partition_payload(
    payload: &ReviewIndexNodePayload,
    depth: usize,
) -> Result<BTreeMap<u8, ReviewIndexNodePayload>, ReviewError> {
    match payload {
        ReviewIndexNodePayload::Cycles(values) => partition_values(
            values,
            depth,
            |value| value.id().as_bytes().to_vec(),
            ReviewIndexNodePayload::Cycles,
        ),
        ReviewIndexNodePayload::Findings(values) => partition_values(
            values,
            depth,
            |value| value.id().as_bytes().to_vec(),
            ReviewIndexNodePayload::Findings,
        ),
        ReviewIndexNodePayload::Facts(values) => partition_values(
            values,
            depth,
            IndexedFact::key_bytes,
            ReviewIndexNodePayload::Facts,
        ),
        ReviewIndexNodePayload::Branch(_) => Err(super::inconsistent(
            "review radix branch cannot be split as a leaf",
        )),
    }
}

fn partition_values<T: Clone>(
    values: &[T],
    depth: usize,
    route: impl Fn(&T) -> Vec<u8>,
    wrap: impl Fn(Vec<T>) -> ReviewIndexNodePayload,
) -> Result<BTreeMap<u8, ReviewIndexNodePayload>, ReviewError> {
    let mut groups = BTreeMap::<u8, Vec<T>>::new();
    for value in values {
        let key = route(value);
        let child = *key.get(depth).ok_or_else(|| {
            reject(
                ReviewErrorKind::LimitExceeded,
                "one exact review index entry exceeds the C0 state-row bound",
            )
        })?;
        groups.entry(child).or_default().push(value.clone());
    }
    Ok(groups
        .into_iter()
        .map(|(child, values)| (child, wrap(values)))
        .collect())
}

fn empty_leaf(kind: ReviewIndexKind) -> ReviewIndexNodePayload {
    match kind {
        ReviewIndexKind::Cycles => ReviewIndexNodePayload::Cycles(Vec::new()),
        ReviewIndexKind::Findings => ReviewIndexNodePayload::Findings(Vec::new()),
        ReviewIndexKind::Facts => ReviewIndexNodePayload::Facts(Vec::new()),
    }
}

fn manifest_key(run_id: RunId) -> Vec<u8> {
    root_key(run_id, MANIFEST_KEY_TAG)
}

fn quorum_key(run_id: RunId) -> Vec<u8> {
    root_key(run_id, QUORUM_KEY_TAG)
}

fn root_key(run_id: RunId, tag: u8) -> Vec<u8> {
    let mut key = Vec::with_capacity(INDEX_KEY_DOMAIN.len() + 17);
    key.extend_from_slice(INDEX_KEY_DOMAIN);
    key.push(tag);
    key.extend_from_slice(run_id.as_bytes());
    key
}

fn node_key(run_id: RunId, kind: ReviewIndexKind, prefix: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(INDEX_KEY_DOMAIN.len() + 20 + prefix.len());
    key.extend_from_slice(INDEX_KEY_DOMAIN);
    key.push(NODE_KEY_TAG);
    key.extend_from_slice(run_id.as_bytes());
    key.push(match kind {
        ReviewIndexKind::Cycles => 1,
        ReviewIndexKind::Findings => 2,
        ReviewIndexKind::Facts => 3,
    });
    key.extend_from_slice(&(prefix.len() as u16).to_be_bytes());
    key.extend_from_slice(prefix);
    key
}

fn next_revision(previous: Option<u64>) -> Result<u64, ReviewError> {
    previous.map_or(Ok(1), |revision| {
        revision.checked_add(1).ok_or_else(|| {
            reject(
                ReviewErrorKind::LimitExceeded,
                "review index state revision overflowed",
            )
        })
    })
}

fn missing_node() -> ReviewError {
    super::inconsistent("review index node disappeared during one admission")
}

fn stale(detail: &'static str) -> ReviewError {
    reject(ReviewErrorKind::StaleFence, detail)
}
