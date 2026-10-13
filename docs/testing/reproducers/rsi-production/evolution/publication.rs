//! Read back actual published F0 owner evidence; never claim or acknowledge an outbox row here.
#[path = "f0.rs"]
mod fixture;

use fixture::{hex, id, ids, key, parse, store};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_daemon::rsi::evolution::EvolutionOperator;
use peritus_evidence::EvidenceStore;
use peritus_evolution::{EVOLUTION_PUBLICATION_DESTINATION, EvolutionCampaignId};
use peritus_journal::{OutboxId, OutboxState, SqliteJournal};
use peritus_types::{EvidenceId, ProjectId, RevisionTuple, Sha256Digest};
use std::{error::Error, fs, path::PathBuf};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn short_digest(bytes: &[u8]) -> [u8; 16] {
    let digest = peritus_codec::sha256(bytes);
    let mut value = [0_u8; 16];
    value.copy_from_slice(&digest.as_bytes()[..16]);
    if value == [0; 16] {
        value[15] = 1;
    }
    value
}

// These bytes are an inert lookup key reconstructed from actual replayed owner fields. The
// retained C0 row must equal them and already be Acknowledged; this program grants no authority.
fn directive(
    kind: u8,
    project: ProjectId,
    campaign: Option<EvolutionCampaignId>,
    revision: RevisionTuple,
    action: Sha256Digest,
    artifact: Sha256Digest,
    evidence: Sha256Digest,
) -> Vec<u8> {
    let mut bytes = b"PERITUS-F0-PUBLICATION-DIRECTIVE\0".to_vec();
    bytes.push(kind);
    bytes.extend_from_slice(project.as_bytes());
    bytes.push(u8::from(campaign.is_some()));
    if let Some(campaign) = campaign {
        bytes.extend_from_slice(campaign.as_bytes());
    }
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
    bytes.extend_from_slice(action.as_bytes());
    bytes.extend_from_slice(artifact.as_bytes());
    bytes.extend_from_slice(evidence.as_bytes());
    bytes
}

fn published(
    journal: &SqliteJournal,
    artifacts: &ArtifactStore,
    evidence: &EvidenceStore,
    bytes: &[u8],
    kind: &str,
    revision: RevisionTuple,
    payload: Sha256Digest,
    artifact: Sha256Digest,
) -> Result<(EvidenceId, serde_json::Value)> {
    let outbox_id = OutboxId::new(short_digest(bytes))?;
    let row = journal.outbox_message(outbox_id)?.ok_or("actual F0 outbox row absent")?;
    if row.destination() != EVOLUTION_PUBLICATION_DESTINATION
        || row.payload() != bytes
        || row.state() != OutboxState::Acknowledged
    {
        return Err("normal daemon has not acknowledged this exact F0 publication".into());
    }
    let mut evidence_key = b"PERITUS-F0-EVIDENCE-ID\0".to_vec();
    evidence_key.extend_from_slice(outbox_id.as_bytes());
    evidence_key.extend_from_slice(&row.producing_position().to_be_bytes());
    let evidence_id =
        EvidenceId::new(short_digest(&evidence_key)).map_err(|_| "evidence ID rejected")?;
    let record = evidence.load(evidence_id)?.ok_or("published F0 evidence absent")?;
    let artifact = ArtifactDigest::from_sha256(artifact);
    if record.kind().as_str() != kind
        || record.source().as_str() != "peritus-evolution"
        || record.revision() != &revision
        || record.payload_digest() != payload
        || record.provenance().global_position() != row.producing_position()
        || !record.artifacts().contains(&artifact)
    {
        return Err("actual F0 evidence differs from exact replayed owning state".into());
    }
    artifacts.verify(artifact)?;
    let out = serde_json::json!({
        "kind": kind, "outbox": hex(outbox_id.as_bytes()), "outbox_state": "Acknowledged",
        "evidence": hex(evidence_id.as_bytes()), "record_digest": hex(record.record_digest().as_bytes()),
        "producing_position": row.producing_position(), "artifact": hex(artifact.sha256().as_bytes()),
        "semantic_digest": hex(payload.as_bytes())
    });
    Ok((evidence_id, out))
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 || args[1] != "verify" {
        return Err("usage: publication verify FIXTURE ACTUAL_E3_CAMPAIGN".into());
    }
    let root = PathBuf::from(&args[2]);
    let e3 = parse::<16>(&args[3])?;
    let campaign = EvolutionCampaignId::new(id(&format!("actual-f0-campaign:{}", hex(&e3))))
        .map_err(|_| "campaign ID rejected")?;
    let (mut journal, artifacts, mut evidence, facts) = store(&root)?;
    let project = ProjectId::new(key(&facts, "project_id")?).map_err(|_| "project ID rejected")?;
    let state =
        EvolutionOperator::new(&mut journal, &artifacts, &mut evidence).campaign(campaign)?;
    let proposal = state.proposal().ok_or("actual F0 promotion proposal absent")?;
    let pointer =
        EvolutionOperator::new(&mut journal, &artifacts, &mut evidence).pointer(project)?;
    let decision_bytes = directive(
        1,
        project,
        Some(campaign),
        proposal.candidate().revision(),
        proposal.digest(),
        proposal.evidence_bundle_artifact(),
        proposal.digest(),
    );
    let (decision_id, decision) = published(
        &journal,
        &artifacts,
        &evidence,
        &decision_bytes,
        "evolution-decision",
        proposal.candidate().revision(),
        proposal.digest(),
        proposal.evidence_bundle_artifact(),
    )?;
    let mut activations = Vec::new();
    for activation in pointer.history().iter().filter(|entry| entry.generation() > 1) {
        let bytes = directive(
            2,
            project,
            activation.campaign_id(),
            activation.successor().revision(),
            activation.action_digest(),
            activation.evidence_artifact(),
            activation.evidence_digest(),
        );
        let (_, observation) = published(
            &journal,
            &artifacts,
            &evidence,
            &bytes,
            "harness-activation",
            activation.successor().revision(),
            activation.evidence_digest(),
            activation.evidence_artifact(),
        )?;
        activations.push(observation);
    }
    if activations.is_empty() {
        return Err("no actual activation publication observed".into());
    }
    let before = journal.integrity_export()?.report().last_position();
    let wrong_evidence = EvidenceId::new(key(&facts, "baseline_evidence_id")?)
        .map_err(|_| "baseline ID rejected")?;
    let error = EvolutionOperator::new(&mut journal, &artifacts, &mut evidence)
        .record_decision_publication(campaign, wrong_evidence, ids("wrong-decision-publication")?)
        .expect_err("baseline evidence must not acknowledge a candidate decision");
    if journal.integrity_export()?.report().last_position() != before {
        return Err("wrong publication receipt changed the durable journal".into());
    }
    let acknowledged = EvolutionOperator::new(&mut journal, &artifacts, &mut evidence)
        .record_decision_publication(campaign, decision_id, ids("actual-decision-publication")?)?;
    if acknowledged.publication().is_none() {
        return Err("campaign owner did not retain actual publication".into());
    }
    let after = journal.integrity_export()?.report().last_position();
    let retry = EvolutionOperator::new(&mut journal, &artifacts, &mut evidence)
        .record_decision_publication(campaign, decision_id, ids("actual-decision-publication")?)?;
    if retry != acknowledged || journal.integrity_export()?.report().last_position() != after {
        return Err("exact publication acknowledgement retry was not idempotent".into());
    }
    let output = serde_json::json!({"decision": decision, "activations": activations,
        "wrong_receipt_rejected": format!("{error:?}"), "wrong_receipt_no_append": true,
        "exact_owner_acknowledged": true, "exact_retry_no_append": true});
    fs::write(
        root.join("actual-f0-publication-readback.json"),
        serde_json::to_vec_pretty(&output)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
