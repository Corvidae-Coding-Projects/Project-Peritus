//! Actual decision compiler: independent first-occurrence table and exact archive locators.
use crate::local_context::{
    praxis_support::fixture::{Fixture, profile, text},
    read_reuse, tools,
};
use peritus_model_protocol::{ContentBlock, Role};
use peritus_role::HarnessRole;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::atomic::Ordering,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub reads: Vec<u8>,
    pub warm: bool,
    pub reopen: bool,
    #[serde(flatten)]
    pub modes: Modes,
    pub role: u8,
    pub restriction: u8,
    pub capacity: u32,
    pub output: u16,
    pub padding: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Modes {
    pub conservative: bool,
    pub plans: bool,
}
pub fn strategy(steps: usize) -> BoxedStrategy<Case> {
    (
        proptest::collection::vec(0u8..12, 1..=steps.clamp(1, 24)),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        0u8..3,
        0u8..3,
        proptest::sample::select(vec![4096, 8192, 16384, 200_000]),
        1u16..8192,
        proptest::sample::select(vec![0, 256, 2048, 8192]),
    )
        .prop_map(
            |(
                reads,
                warm,
                reopen,
                conservative,
                plans,
                role,
                restriction,
                capacity,
                output,
                padding,
            )| Case {
                reads,
                warm,
                reopen,
                modes: Modes { conservative, plans },
                role,
                restriction,
                capacity,
                output,
                padding,
            },
        )
        .boxed()
}
fn metric(m: &mut BTreeMap<String, u64>, key: &str) {
    *m.entry(key.into()).or_default() += 1;
}

pub fn check(case: &Case) -> Result<BTreeMap<String, u64>, String> {
    let role = match case.role % 3 {
        0 => HarnessRole::Writer,
        1 => HarnessRole::Fixer,
        _ => HarnessRole::Reviewer,
    };
    let mut f = Fixture::new(case.modes.conservative, case.modes.plans, role)?;
    let mut metrics = BTreeMap::new();
    let original = populate(&mut f, case, &mut metrics)?;
    let p = profile(u64::from(case.capacity), u64::from(case.output))?;
    let mut memory = f.memory()?.lock().map_err(|e| e.to_string())?;
    let mut tools = crate::developer_tools::read_only_definitions().map_err(|e| e.to_string())?;
    tools.truncate(2);
    let policy = text(Role::System, "CURRENT AUTHORITATIVE POLICY")?;
    let state = text(Role::Developer, "CURRENT HOST INVOCATION STATE literal")?;
    let input = text(Role::User, "CURRENT USER INPUT literal")?;
    let messages = memory
        .prepare_view_with_governing(&p, &tools, Some(&policy), Some(&state), Some(&input), None)
        .map_err(|e| e.to_string())?;
    let prepared = &memory.prepared.as_ref().ok_or("no prepared projection")?.validation;
    let estimate = peritus_agent::estimate_developer_request_tokens(&messages, &tools);
    if prepared.estimated_input_tokens != estimate
        || prepared.max_input_tokens != u64::from(case.capacity)
        || prepared.input_tokens_saved != prepared.uncompacted_input_tokens.saturating_sub(estimate)
    {
        return Err("complete request accounting disagrees with final delivered request".into());
    }
    // The profile's maximum input is already input capacity. Changing the separate
    // output allowance must leave this same projection byte-for-byte unchanged.
    let second = memory
        .prepare_view_with_governing(
            &profile(u64::from(case.capacity), 1)?,
            &tools,
            Some(&policy),
            Some(&state),
            Some(&input),
            None,
        )
        .map_err(|e| e.to_string())?;
    if messages != second {
        return Err(
            "output allowance changed an input-only view (double subtraction or unstable view)"
                .into(),
        );
    }
    for wanted in [&policy, &state, &input] {
        if !messages.contains(wanted) {
            return Err("governing literal message lost or transformed".into());
        }
    }
    verify_cards(&mut memory, &messages, &original, case, &mut metrics)?;
    if case.modes.conservative && case.restriction != 0 {
        let all = messages
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter_map(|b| {
                if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None }
            })
            .collect::<String>();
        if all.contains("CANARY_") {
            return Err("conservative view included excluded archived file bytes".into());
        }
    }
    read_reuse::observe_view(&mut memory, &messages).map_err(|e| e.to_string())?;
    drop(memory);
    metric(
        &mut metrics,
        if case.modes.conservative { "conservative_views" } else { "history_views" },
    );
    metric(
        &mut metrics,
        if case.modes.plans { "plan_enabled_views" } else { "plan_disabled_views" },
    );
    metric(
        &mut metrics,
        match role {
            HarnessRole::Reviewer => "reviewer_views",
            HarnessRole::Fixer => "fixer_views",
            _ => "writer_views",
        },
    );
    Ok(metrics)
}

fn verify_index(
    f: &Fixture,
    expected: &BTreeMap<u64, u64>,
    original: &BTreeMap<u64, (String, String, Vec<u8>, bool)>,
) -> Result<(), String> {
    let mut memory = f.memory()?.lock().map_err(|e| e.to_string())?;
    let mut incremental = std::mem::take(&mut memory.decision_index);
    incremental.refresh(&memory).map_err(|e| e.to_string())?;
    let mut rebuilt = super::Index::default();
    rebuilt.refresh(&memory).map_err(|e| e.to_string())?;
    let wanted = expected.values().copied().collect::<BTreeSet<_>>();
    for index in [&incremental, &rebuilt] {
        let actual = index.evidence().into_iter().map(|e| e.source).collect::<BTreeSet<_>>();
        if actual != wanted {
            return Err(format!(
                "distinct cards {actual:?} != first-success/error model {wanted:?}"
            ));
        }
        for (id, rep) in expected {
            if index.representative(*id) != *rep {
                return Err("alias differs from independent first-occurrence table".into());
            }
        }
        for evidence in index.evidence() {
            let original = original.get(&evidence.source).ok_or("unexpected evidence")?;
            if memory.store.read(evidence.artifact).map_err(|e| e.to_string())? != original.2
                || evidence.is_error != original.3
            {
                return Err("index changed exact evidence or error flag".into());
            }
        }
    }
    if incremental.root() != rebuilt.root() || incremental.processed() != memory.sources.len() {
        return Err("incremental/rebuilt source identity differs".into());
    }
    memory.decision_index = incremental;
    drop(memory);
    Ok(())
}
pub fn examples() -> Vec<Case> {
    let mut cases = Vec::new();
    for conservative in [false, true] {
        for plans in [false, true] {
            for role in 0..3 {
                for restriction in 0..3 {
                    cases.push(Case {
                        reads: vec![0, 0, 1, 4, 3, 3, 2, 2],
                        warm: true,
                        reopen: true,
                        modes: Modes { conservative, plans },
                        role,
                        restriction,
                        capacity: 8192,
                        output: 4096,
                        padding: 2048,
                    });
                }
            }
        }
    }
    cases.push(calibration_case());
    cases
}
pub fn calibration_case() -> Case {
    Case {
        reads: vec![0, 0],
        warm: true,
        reopen: false,
        modes: Modes { conservative: true, plans: true },
        role: 0,
        restriction: 0,
        capacity: 8192,
        output: 4096,
        padding: 256,
    }
}
#[test]
fn praxis_stage2_decision() {
    crate::local_context::praxis_support::run(
        "decision",
        strategy,
        check,
        examples,
        calibration_case,
    );
}

type Originals = BTreeMap<u64, (String, String, Vec<u8>, bool)>;
fn populate(
    f: &mut Fixture,
    case: &Case,
    metrics: &mut BTreeMap<String, u64>,
) -> Result<Originals, String> {
    let mut original = Originals::new();
    let mut expected = BTreeMap::<u64, u64>::new();
    let mut seen = BTreeMap::<(String, String, Vec<u8>), u64>::new();
    for (i, n) in case.reads.iter().enumerate() {
        let path =
            if n % 4 == 3 { "missing.txt".to_owned() } else { format!("file-{}.txt", n % 3) };
        if n % 4 != 3 {
            fs::write(
                f.workspace.join(&path),
                format!(
                    "CANARY_{}_{} μ🙂 {}\n",
                    n % 3,
                    (n / 4) % 2,
                    "x".repeat(case.padding as usize)
                ),
            )
            .map_err(|e| e.to_string())?;
        }
        let args = json!({"path":path,"max_bytes":32768});
        let (value, error, id) = f.execute("workspace_read", &args, false)?;
        let bytes = f
            .memory()?
            .lock()
            .map_err(|e| e.to_string())?
            .artifact(id)
            .map_err(|e| e.to_string())?;
        if !error
            && value["source_sha256"]
                != json!(tools::hex(
                    peritus_codec::sha256(
                        &fs::read(f.workspace.join(&path)).map_err(|e| e.to_string())?
                    )
                    .as_bytes()
                ))
        {
            return Err("ordinary reader hash differs from exact current file".into());
        }
        let key = ("workspace_read".to_owned(), args.to_string(), bytes.clone());
        let rep = if error { id } else { *seen.entry(key).or_insert(id) };
        expected.insert(id, rep);
        original.insert(id, (path, args.to_string(), bytes, error));
        if case.warm || i + 1 == case.reads.len() {
            verify_index(f, &expected, &original)?;
            metric(metrics, "index_observations_checked");
        }
        if rep != id {
            metric(metrics, "identical_success_aliases");
        }
        if error {
            metric(metrics, "error_cards");
        }
    }
    if case.reopen {
        f.reopen()?;
        verify_index(f, &expected, &original)?;
        metric(metrics, "reopened_indexes");
    }
    // Change bytes without changing their length after the index is warm.
    for path in ["file-0.txt", "file-1.txt", "file-2.txt"] {
        let p = f.workspace.join(path);
        if p.exists() {
            let mut b = fs::read(&p).map_err(|e| e.to_string())?;
            b[0] = b'Z';
            fs::write(p, b).map_err(|e| e.to_string())?;
            metric(metrics, "same_size_edits");
        }
    }
    if case.restriction == 1 {
        f.view.read.store(false, Ordering::Release);
        metric(metrics, "read_revocations");
    }
    if case.restriction == 2 {
        f.view.opaque.store(true, Ordering::Release);
        metric(metrics, "path_restrictions");
    }
    Ok(original)
}

fn verify_cards(
    memory: &mut crate::local_context::memory::LocalMemory,
    messages: &[peritus_model_protocol::Message],
    original: &Originals,
    case: &Case,
    metrics: &mut BTreeMap<String, u64>,
) -> Result<(), String> {
    for message in messages {
        for block in message.content() {
            let ContentBlock::Text(body) = block else { continue };
            let body = body.expose_for_wire();
            if !body.starts_with("DECISION TOOL EVIDENCE V1\n") {
                continue;
            }
            metric(metrics, "exact_locator_cards");
            if message.role() != Role::User {
                return Err("archive card gained instruction authority".into());
            }
            let parts = body.splitn(6, '\n').collect::<Vec<_>>();
            if parts.len() != 6 {
                return Err("card framing".into());
            }
            let id = parts[1]
                .strip_prefix("sequence=")
                .ok_or("card sequence")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?;
            let bytes = &original.get(&id).ok_or("card names unknown original")?.2;
            let source = parts[2]
                .strip_prefix("source=")
                .and_then(|s| s.split_whitespace().next())
                .ok_or("advertised locator absent")?;
            if tools::sequence(memory, source).map_err(|e| e.to_string())? != id {
                return Err("advertised locator resolves to a different original".into());
            }
            let metadata = original.get(&id).ok_or("card origin")?;
            let prefix = format!(
                "source={source} tool=workspace_read is_error={} source_bytes={} retrieval=context_read original_arguments={} ",
                metadata.3,
                bytes.len(),
                metadata.1
            );
            if !parts[2].starts_with(&prefix) {
                return Err(
                    "advertised evidence metadata/arguments differ from actual original proposal"
                        .into(),
                );
            }
            if !metadata.3
                && case.restriction == 0
                && !parts[2].ends_with("current_content=changed")
            {
                return Err("same-size edit was not recognized in current evidence bindings".into());
            }
            if !bytes.starts_with(parts[5].as_bytes()) {
                return Err("card bytes differ from exact original canonical JSON prefix".into());
            }
            let end = parts[5].len();
            if !parts[3].contains(&format!("visible_bytes={end} ")) {
                return Err("card delivered-byte accounting".into());
            }
            if end < bytes.len() {
                metric(metrics, "partial_locator_cards");
                if !parts[3].ends_with(&format!("next_offset={end}")) {
                    return Err("card continuation differs from delivered byte count".into());
                }
            } else if !parts[3].ends_with("next_offset=null") {
                return Err("complete card has continuation".into());
            }
            let mut recovered = parts[5].as_bytes().to_vec();
            while recovered.len() < bytes.len() {
                let req = json!({"observation_ids":[source],"query":null,"cursor":null,"offset":recovered.len(),"max_bytes":memory.config.max_read_bytes});
                let page = tools::read::execute(
                    memory,
                    &serde_json::to_vec(&req).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                let source = page["sources"]
                    .as_array()
                    .and_then(|s| s.first())
                    .ok_or_else(|| format!("locator did not resolve: {page}"))?;
                if source["offset"] != json!(recovered.len()) || source["encoding"] != "utf8" {
                    return Err("retrieval offset/encoding".into());
                }
                let data = source["data"].as_str().ok_or("retrieval data")?;
                if data.is_empty() {
                    return Err("nonprogressing exact retrieval".into());
                }
                recovered.extend_from_slice(data.as_bytes());
                metric(metrics, "exact_retrieval_pages");
            }
            if &recovered != bytes {
                return Err("locator reconstruction differs from original bytes".into());
            }
        }
    }
    Ok(())
}
