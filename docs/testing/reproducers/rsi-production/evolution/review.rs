//! Imports an independently executed review into its actual D2 owner.
//! This consumer accepts only the fresh delegated review's final scoped result.
//! It never manufactures a review, provider identity, finding, or approval.
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_context::*;
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_quality_policy::{ReviewCycleOrdinal, ReviewerIdentity};
use peritus_review::*;
use peritus_role::{ContextClass, HarnessRole, RoleProfile};
use peritus_types::*;
use std::{error::Error, fs, path::PathBuf};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn parse<const N: usize>(s: &str) -> Result<[u8; N]> {
    if s.len() != N * 2 {
        return Err("hex width differs".into());
    }
    let mut b = [0; N];
    for (i, x) in b.iter_mut().enumerate() {
        *x = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(b)
}
fn str_key<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    v[key].as_str().ok_or_else(|| format!("missing {key}").into())
}
fn digest(v: &serde_json::Value, key: &str) -> Result<Sha256Digest> {
    Ok(Sha256Digest::new(parse(str_key(v, key)?)?))
}
fn id(label: &str, input: Sha256Digest) -> [u8; 16] {
    let mut p = b"peritus-rsi-actual-review-host-v1\0".to_vec();
    p.extend_from_slice(label.as_bytes());
    p.extend_from_slice(input.as_bytes());
    let d = peritus_codec::sha256(&p);
    let mut b = [0; 16];
    b.copy_from_slice(&d.as_bytes()[..16]);
    b
}
fn event(label: &str, input: Sha256Digest) -> Result<EventId> {
    EventId::new(id(label, input)).map_err(|_| "event identity rejected".into())
}
fn save(a: &ArtifactStore, bytes: &[u8], label: &str) -> Result<ArtifactDigest> {
    let d = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let mut w = a.begin_write(WriteRequest::new(
        d,
        bytes.len() as u64,
        bytes.len().max(1) as u64,
        MediaType::new("application/json")?,
        EncryptionMetadata::unencrypted(),
        event(label, d.sha256())?,
    ))?;
    w.write_chunk(bytes)?;
    Ok(w.finalize()?.digest())
}
fn command(
    s: &ReviewRunState,
    label: &str,
    input: Sha256Digest,
    kind: ReviewCommandKind,
) -> Result<ReviewCommand> {
    Ok(ReviewCommand::new(
        CommandId::new(id(label, input)).map_err(|_| "command identity rejected")?,
        event(&format!("event-{label}"), input)?,
        s.run_id(),
        s.sequence().get(),
        Some(s.last_event_id()),
        s.state_digest(),
        s.binding().revision(),
        kind,
    )?)
}
fn commit(
    j: &mut SqliteJournal,
    s: &ReviewRunState,
    label: &str,
    input: Sha256Digest,
    kind: ReviewCommandKind,
    roots: &[Sha256Digest],
) -> Result<ReviewRunState> {
    let c = command(s, label, input, kind)?;
    let t = decide(s, &c)?;
    commit_review_transition_with_artifacts(j, &c, &t, roots)?;
    Ok(t.into_state())
}
fn run() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 && args.len() != 5 {
        return Err("usage: review-driver FIXTURE_ROOT REVIEW_JSON INPUT_JSON [EXPECTED_FRESH_REVIEWER_TASK]".into());
    }
    let expected_reviewer = args.get(4).map_or("/root/rsi_candidate_review", String::as_str);
    let root = PathBuf::from(&args[1]);
    let raw = fs::read(&args[2])?;
    let input = fs::read(&args[3])?;
    let v: serde_json::Value = serde_json::from_slice(&raw)?;
    let capsule: serde_json::Value = serde_json::from_slice(&input)?;
    if str_key(&v, "schema")? != "peritus-independent-candidate-review-v1"
        || str_key(&v, "reviewer_task_identity")? != expected_reviewer
        || str_key(&capsule, "reviewer_task_identity")? != expected_reviewer
        || str_key(&v, "status")? != "PASS"
        || v["input_capsule"] != capsule
        || !v["findings"].as_array().is_some_and(Vec::is_empty)
    {
        return Err("review is not the actual final blocker-free independent result".into());
    }
    if !capsule["provider"].is_null() || !capsule["model_family"].is_null() {
        return Err("unknown provider provenance was replaced with unsupported identity".into());
    }
    let input_digest = peritus_codec::sha256(&input);
    if digest(&v, "input_context_digest")? != input_digest {
        return Err("exact canonical review input digest differs".into());
    }
    let facts: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let db = root.join("peritus.sqlite3");
    let mut j = SqliteJournal::open(
        &db,
        StoreId::new(parse(str_key(&facts, "store_id")?)?)?,
        SqliteJournalOptions::default(),
    )?;
    let a = ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 64 * 1024 * 1024, 512 * 1024 * 1024)?
            .with_database_path(&db)?,
    )?;
    let run =
        RunId::new(parse(str_key(&v, "review_run")?)?).map_err(|_| "review identity rejected")?;
    let mut s = load_review_replay(&j, run)?.rebuild()?.ok_or("actual bound D2 owner absent")?;
    if v["candidate_revision_tuple"] != facts["candidate"]["revision"]
        || s.binding().digest() != digest(&v, "binding_digest")?
    {
        return Err("review does not bind the retained candidate".into());
    }
    let categories = v["categories"].as_array().ok_or("category response absent")?;
    if categories.len() != s.binding().required_categories().len() {
        return Err("review omits required category".into());
    }
    for (r, required) in categories.iter().zip(s.binding().required_categories()) {
        if str_key(r, "status")? != "PASS"
            || digest(r, "category_id")? != required.digest()
            || !r["findings"].as_array().is_some_and(Vec::is_empty)
        {
            return Err("required category has unresolved or absent verdict".into());
        }
    }
    // The external review adapter preserves an observed input capsule, not a C5 request transcript.
    // C6 IDs are caller-computed. This graph records the exact attested external input bytes.
    let plan_id = ContextPlanId::new(input_digest);
    let render = (|| -> std::result::Result<RenderPlan, ContextError> {
        let limits = ContextLimits::new(1, 1024 * 1024, 1, 1)?;
        let role = RoleProfile::for_harness_role(HarnessRole::Reviewer);
        let estimate = (input.len() as u64).div_ceil(4);
        let node = ContextNode::new(
            ContextNodeMetadata::new(
                ContextNodeId::new(id("input-node", input_digest))?,
                Provenance::Review,
                AuthorityClass::NonAuthoritative,
                TrustClass::Untrusted,
                ContextClass::ToolObservation,
                ContentKind::ToolObservation,
                estimate,
                1,
                RequirementMode::Required,
                0,
                RoleVisibility::new(vec![role.actor_role()], limits)?,
                vec![],
                limits,
            )?,
            bind_context_content(input.clone(), input_digest, limits)?,
        );
        let graph = ContextGraph::new(vec![node], limits)?;
        let plan = select_context(
            &graph,
            &SelectionPolicy::new(role, TokenBudget::new(estimate + 2, 1, 0)?, 1, 1024 * 1024)?,
            plan_id,
        )?;
        let render = build_render_plan(&graph, &plan)?;
        Ok(render)
    })()
    .map_err(|e| format!("actual context capture: {e:?}"))?;
    if render.segments().len() != 1 || render.segments()[0].content() != input {
        return Err("C6 dropped or changed observed review input".into());
    }
    let input_artifact = save(&a, &input, "observed-review-input")?;
    let response_artifact = save(&a, &raw, "actual-review-response")?;
    let mut roots = vec![input_artifact.sha256(), response_artifact.sha256()];
    roots.sort();
    roots.dedup();
    let reviewer_task = str_key(&v, "reviewer_task_identity")?;
    let prompt = peritus_codec::sha256(str_key(&capsule, "parent_task_text")?.as_bytes());
    let ancestry = peritus_codec::sha256(reviewer_task.as_bytes());
    let actor = ActorId::new(id("external-reviewer", ancestry))
        .map_err(|_| "reviewer identity rejected")?;
    let unavailable = Sha256Digest::new([0; 32]);
    let reviewer = ReviewerIdentity::new(
        actor,
        unavailable,
        unavailable,
        prompt,
        plan_id.digest(),
        ancestry,
        true,
    );
    let cycle = ReviewCycleId::new(id("actual-cycle", input_digest))
        .map_err(|_| "review cycle rejected")?;
    if s.phase() == ReviewRunPhase::Terminal
        && s.cycle(cycle).is_none_or(|value| {
            value.assignment().reviewer() != &reviewer
                || value.assignment().context_plan_id() != plan_id
                || value.submission().is_none()
        })
    {
        return Err(
            "retained completed review belongs to another observed input or reviewer".into()
        );
    }
    if s.phase() == ReviewRunPhase::Terminal {
        let export = j.integrity_export()?;
        let submitted = export.records().iter().find_map(|record| {
            if record.aggregate().kind() != peritus_journal::AggregateKind::Review
                || record.aggregate().id().as_bytes() != run.as_bytes()
            {
                return None;
            }
            let event = peritus_codec::decode_message::<ReviewEventFrame>(
                record.frame_bytes(),
                peritus_codec::CodecLimits::PRODUCTION,
            )
            .ok()?
            .into_event();
            matches!(event.kind(), ReviewEventKind::ReviewSubmitted { submission }
                if submission.cycle_id() == cycle)
            .then_some(record.global_position())
        });
        let position = submitted.ok_or("completed review has no actual submission event")?;
        if roots.iter().any(|digest| {
            !export.artifact_references().iter().any(|reference| {
                reference.artifact_digest() == *digest
                    && reference.first_position() <= position
                    && reference.last_position() >= position
            })
        }) {
            return Err(
                "completed review retry input or response differs from its retained submission artifacts"
                    .into(),
            );
        }
    }
    if s.phase() != ReviewRunPhase::Terminal {
        if s.cycle(cycle).is_none() {
            let assignment = ReviewAssignment::new(
                cycle,
                ReviewCycleOrdinal::new(1).map_err(|_| "ordinal rejected")?,
                s.binding(),
                reviewer,
                s.binding().required_categories().to_vec(),
                plan_id,
                true,
                s.limits(),
            )?;
            s = commit(
                &mut j,
                &s,
                "assign",
                input_digest,
                ReviewCommandKind::AssignReviewer { assignment },
                &roots,
            )?;
        }
        if s.cycle(cycle).ok_or("actual assignment absent")?.submission().is_none() {
            let submission = ReviewSubmission::new(
                cycle,
                s.binding().revision(),
                s.binding().required_categories().to_vec(),
                vec![],
                s.binding().blocking_severity(),
                s.limits(),
            )?;
            s = commit(
                &mut j,
                &s,
                "submit",
                input_digest,
                ReviewCommandKind::SubmitReview { submission },
                &roots,
            )?;
        }
        s = commit(&mut j, &s, "finalize", input_digest, ReviewCommandKind::FinalizeRun, &roots)?;
    }
    if s.terminal().map(ReviewTerminal::kind) != Some(ReviewTerminalKind::Completed)
        || !findings_are_conserved(&s)
        || !no_implicit_success(&s)
        || !quorum_is_complete(&s)
    {
        return Err("actual review did not satisfy bound completion obligations".into());
    }
    let output = serde_json::json!({"status":"PASS","run":hex(run.as_bytes()),"binding":hex(s.binding().digest().as_bytes()),"terminal":format!("{:?}",s.terminal().unwrap().kind()),"state_digest":hex(s.state_digest().as_bytes()),"input_artifact":hex(input_artifact.as_bytes()),"response_artifact":hex(response_artifact.as_bytes()),"external_host_attestation":true,"context_kind":"observed external review input capsule; not a C5 request transcript","provider":null,"model_family":null,"legacy_provider_model_digest_semantics":"zero unavailable sentinels, no diversity claim"});
    fs::write(root.join("actual-d2-independent-review.json"), serde_json::to_vec_pretty(&output)?)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    // Fresh owner readback exercises actual C0 durability.
    drop(j);
    let reopened = SqliteJournal::open(
        &db,
        StoreId::new(parse(str_key(&facts, "store_id")?)?)?,
        SqliteJournalOptions::default(),
    )?;
    let replayed = load_review_replay(&reopened, run)?.rebuild()?.ok_or("review lost on reopen")?;
    if replayed != s {
        return Err("review durability readback differs".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("actual D2 review consumer failed: {e:?}");
        std::process::exit(1)
    }
}
