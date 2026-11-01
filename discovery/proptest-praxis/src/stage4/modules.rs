//! Actual local registry and ordered module jobs, checked against small independent ledgers.
use crate::Metrics;
use peritus_modules::{Definition, Job, Registry};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Jobs { stages: u8, operations: Vec<Operation> },
    Registry { operations: Vec<RegistryOperation> },
    CheckpointFailure { text: String },
    DamagedOutput { text: String, damage: u8 },
    DamagedWhileOpen { text: String, stages: u8, selected: u8, damage: u8 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Complete {
        stage: u8,
        text: String,
    },
    Read {
        stage: u8,
        allowed: bool,
        offset: u16,
        limit: u8,
    },
    /// Follow every continuation and independently reconstruct the complete output.
    ReadAll {
        stage: u8,
        limit: u8,
    },
    Reopen,
    Contend,
    Inspect(u16),
    OtherIdentity(u8),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RegistryOperation {
    Install { version: u8, payload: u8 },
    Enable(bool),
    Remove,
    Damage,
    Reload,
}
fn bump(metrics: &mut Metrics, key: &str) {
    *metrics.entry(key.into()).or_default() += 1;
}
fn text() -> BoxedStrategy<String> {
    prop_oneof![
        4 => "[a-zA-Z0-9 ]{0,32}",
        2 => Just("é🙂 combining e\u{301}\nsecond line".to_owned()),
        1 => Just(" \t\n".to_owned()),
    ]
    .boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        7 => (0_u8..6, text()).prop_map(|(stage,text)|Operation::Complete{stage,text}),
        4 => (0_u8..6,any::<bool>(),0_u16..48,0_u8..32).prop_map(|(stage,allowed,offset,limit)|Operation::Read{stage,allowed,offset,limit}),
        3 => (0_u8..6,0_u8..32).prop_map(|(stage,limit)|Operation::ReadAll{stage,limit}),
        3 => Just(Operation::Reopen),
        2 => Just(Operation::Contend),
        2 => (0_u16..48).prop_map(Operation::Inspect),
        1 => (0_u8..3).prop_map(Operation::OtherIdentity),
    ];
    let jobs = (1_u8..5, prop::collection::vec(operation, 0..=max_steps.min(96)))
        .prop_map(|(stages, operations)| Case::Jobs { stages, operations });
    let completed_jobs = (1_u8..5, text(), 0_u8..32).prop_map(move |(stages, text, limit)| {
        let operations = (0..stages)
            .flat_map(|stage| {
                [
                    Operation::Complete { stage, text: format!("stage-{stage}: {text} é🙂") },
                    Operation::ReadAll { stage, limit },
                    Operation::Reopen,
                    Operation::ReadAll { stage, limit: limit.wrapping_add(1) },
                ]
            })
            .take(max_steps.min(96))
            .collect();
        Case::Jobs { stages, operations }
    });
    let registry_operation = prop_oneof![
        5 => (0_u8..3,0_u8..4).prop_map(|(version,payload)|RegistryOperation::Install{version,payload}),
        3 => any::<bool>().prop_map(RegistryOperation::Enable),
        2 => Just(RegistryOperation::Remove),
        2 => Just(RegistryOperation::Damage),
        2 => Just(RegistryOperation::Reload),
    ];
    let registry = prop::collection::vec(registry_operation, 0..=max_steps.min(64))
        .prop_map(|operations| Case::Registry { operations });
    prop_oneof![
        6 => jobs,
        3 => completed_jobs,
        4 => registry,
        1 => "[a-z]{1,24}".prop_map(|text|Case::CheckpointFailure{text}),
        1 => ("[a-z]{1,24}",0_u8..3).prop_map(|(text,damage)|Case::DamagedOutput{text,damage}),
        2 => (text(),1_u8..5,any::<u8>(),0_u8..3).prop_map(|(text,stages,selected,damage)|Case::DamagedWhileOpen{text:format!("output: {text}"),stages,selected,damage}),
    ]
    .boxed()
}
fn stage(index: usize) -> String {
    format!("stage-{index}")
}
fn package(root: &Path, stages: u8, version: u8, payload: u8) -> Result<Definition, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut manifest = format!(
        "schema = 1\nid = \"praxis-workflow\"\nversion = \"1.0.{version}\"\nname = \"Praxis workflow\"\ndescription = \"Disposable ordered stage fixture\"\nresources = [\"resource.txt\"]\n[[roles]]\nid = \"writer\"\nmodel = \"writer\"\ninstructions = \"Use only explicitly supplied stage inputs.\"\ntools = []\n"
    );
    for index in 0..stages {
        let inputs = if index == 0 {
            "[]".into()
        } else {
            format!("[\"{}\"]", stage(usize::from(index - 1)))
        };
        manifest.push_str(&format!(
            "[[stages]]\nid = \"{}\"\nrole = \"writer\"\ninputs = {inputs}\n",
            stage(usize::from(index))
        ));
    }
    fs::write(root.join("module.toml"), manifest).map_err(|e| e.to_string())?;
    fs::write(root.join("resource.txt"), format!("resource-{payload}\n"))
        .map_err(|e| e.to_string())?;
    Definition::from_directory(root).map_err(|e| e.to_string())
}
fn model_page(text: &str, offset: u64, limit: usize) -> Result<(String, Option<u64>), ()> {
    let start = usize::try_from(offset).map_err(|_| ())?;
    if start > text.len() || !text.is_char_boundary(start) {
        return Err(());
    }
    let mut end = (start + limit.clamp(4, 1024 * 1024)).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((text[start..end].into(), (end < text.len()).then_some(end as u64)))
}
fn verify_page(
    actual: &Value,
    stage_id: &str,
    text: &str,
    offset: u64,
    limit: usize,
) -> Result<(), String> {
    let (expected, next) =
        model_page(text, offset, limit).map_err(|()| "page should reject".to_owned())?;
    let hash = hex(&peritus_codec::sha256(text.as_bytes()).into_bytes());
    if actual["stage"] != stage_id
        || actual["text"] != expected
        || actual["offset"] != offset
        || actual["total_bytes"] != text.len() as u64
        || actual["next_offset"] != serde_json::json!(next)
        || actual["sha256"] != hash
    {
        return Err(format!("inexact module page: {actual}"));
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn verify_job(job: &Job, model: &[String], stages: u8) -> Result<(), String> {
    if job.state().completed.len() != model.len() {
        return Err("checkpoint stage count differs".into());
    }
    for (index, (row, text)) in job.state().completed.iter().zip(model).enumerate() {
        if row.stage != stage(index)
            || row.bytes != text.len() as u64
            || row.sha256 != hex(&peritus_codec::sha256(text.as_bytes()).into_bytes())
        {
            return Err("checkpoint changed ordered exact output metadata".into());
        }
    }
    let expected = (model.len() == usize::from(stages)).then(|| model.last().cloned()).flatten();
    if job.result().map_err(|e| e.to_string())? != expected {
        return Err("module completion result differs".into());
    }
    Ok(())
}
fn jobs(stages: u8, operations: &[Operation], metrics: &mut Metrics) -> Result<(), String> {
    if !(1..=4).contains(&stages) || operations.len() > 128 {
        return Err("bounded job case exceeded".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let definition = package(&temporary.path().join("source"), stages, 0, 0)?;
    let root = temporary.path().join("jobs");
    let mut job =
        Job::open(&root, definition.clone(), 3, "exact task μ").map_err(|e| e.to_string())?;
    let mut model = Vec::<String>::new();
    for (index, operation) in operations.iter().enumerate() {
        let result = (|| -> Result<Job, String> {
            match operation {
                Operation::Complete { stage: which, text } => {
                    let valid = usize::from(*which) == model.len()
                        && model.len() < usize::from(stages)
                        && !text.trim().is_empty();
                    let before =
                        fs::read(job.root().join("job.json")).map_err(|e| e.to_string())?;
                    let actual = job.complete(&stage(usize::from(*which)), text);
                    if actual.is_ok() != valid {
                        return Err(format!(
                            "stage admission differs: expected {valid}, actual {actual:?}"
                        ));
                    }
                    if valid {
                        model.push(text.clone());
                        bump(metrics, "committed_stages");
                    } else {
                        if fs::read(job.root().join("job.json")).map_err(|e| e.to_string())?
                            != before
                        {
                            return Err("rejected stage changed durable checkpoint".into());
                        }
                        bump(metrics, "rejected_stages");
                    }
                }
                Operation::Read { stage: which, allowed, offset, limit } => {
                    let id = stage(usize::from(*which));
                    let accepted = if *allowed { vec![id.clone()] } else { vec![] };
                    let expected =
                        model.get(usize::from(*which)).filter(|_| *allowed).and_then(|text| {
                            model_page(text, u64::from(*offset), usize::from(*limit)).ok()
                        });
                    let actual = job.read(&accepted, &id, u64::from(*offset), usize::from(*limit));
                    if actual.is_ok() != expected.is_some() {
                        return Err(format!("dependency page acceptance differs: {actual:?}"));
                    }
                    if let Ok(page) = actual {
                        verify_page(
                            &page,
                            &id,
                            &model[usize::from(*which)],
                            u64::from(*offset),
                            usize::from(*limit),
                        )?;
                        bump(metrics, "exact_dependency_pages");
                    } else {
                        bump(metrics, "rejected_dependency_pages");
                    }
                }
                Operation::ReadAll { stage: which, limit } => {
                    let id = stage(usize::from(*which));
                    if let Some(text) = model.get(usize::from(*which)) {
                        read_all(&job, &id, text, usize::from(*limit), metrics)?;
                    } else {
                        if job.read(std::slice::from_ref(&id), &id, 0, usize::from(*limit)).is_ok()
                        {
                            return Err("page walk accepted an uncommitted stage".into());
                        }
                        bump(metrics, "uncommitted_walk_rejections");
                    }
                }
                Operation::Reopen => {
                    drop(job);
                    job = Job::open(&root, definition.clone(), 3, "exact task μ")
                        .map_err(|e| e.to_string())?;
                    bump(metrics, "job_reopens");
                }
                Operation::Contend => {
                    if Job::open(&root, definition.clone(), 3, "exact task μ").is_ok() {
                        return Err("second job owner acquired held lock".into());
                    }
                    bump(metrics, "exclusive_owner_rejections");
                }
                Operation::Inspect(offset) => {
                    let expected =
                        model.last().map(|text| model_page(text, u64::from(*offset), 32768));
                    let actual = Job::inspect(
                        &root,
                        definition.clone(),
                        3,
                        "exact task μ",
                        u64::from(*offset),
                    );
                    let valid = expected.as_ref().is_none_or(Result::is_ok);
                    if actual.is_ok() != valid {
                        return Err("non-owning checkpoint inspection acceptance differs".into());
                    }
                    if let Ok(view) = actual {
                        if view["completed_stages"].as_array().map(Vec::len) != Some(model.len()) {
                            return Err("inspection lost checkpoint prefix".into());
                        }
                        if let Some(text) = model.last() {
                            verify_page(
                                &view["page"],
                                &stage(model.len() - 1),
                                text,
                                u64::from(*offset),
                                32768,
                            )?;
                        } else if !view["page"].is_null() {
                            return Err("empty checkpoint invented output".into());
                        }
                        bump(metrics, "nonowning_inspections");
                    }
                }
                Operation::OtherIdentity(axis) => {
                    let (revision, task) =
                        if axis % 2 == 0 { (4, "exact task μ") } else { (3, "different task") };
                    let other = Job::open(&root, definition.clone(), revision, task)
                        .map_err(|e| e.to_string())?;
                    if !other.state().completed.is_empty() {
                        return Err("different input identity reused stage checkpoint".into());
                    }
                    bump(metrics, "distinct_input_jobs");
                }
            }
            verify_job(&job, &model, stages)?;
            Ok(job)
        })();
        job = result.map_err(|error| format!("{error}; operation {index}: {operation:?}"))?;
        bump(metrics, "operations");
    }
    Ok(())
}
fn read_all(
    job: &Job,
    id: &str,
    expected: &str,
    limit: usize,
    metrics: &mut Metrics,
) -> Result<(), String> {
    let mut reconstructed = String::new();
    let mut offset = 0_u64;
    loop {
        let page = job.read(&[id.to_owned()], id, offset, limit).map_err(|e| e.to_string())?;
        verify_page(&page, id, expected, offset, limit)?;
        let text = page["text"].as_str().ok_or("missing page text")?;
        reconstructed.push_str(text);
        bump(metrics, "continuation_pages");
        if page["next_offset"].is_null() {
            break;
        }
        let next = page["next_offset"].as_u64().ok_or("invalid continuation offset")?;
        if next <= offset || next > expected.len() as u64 {
            return Err("continuation failed to make bounded forward progress".into());
        }
        offset = next;
    }
    if reconstructed != expected {
        return Err("page traversal lost, duplicated, or reordered output bytes".into());
    }
    bump(metrics, "complete_output_walks");
    Ok(())
}

fn damaged_while_open(
    text: &str,
    stages: u8,
    selected: u8,
    damage: u8,
    metrics: &mut Metrics,
) -> Result<(), String> {
    if !(1..=4).contains(&stages) || text.trim().is_empty() || text.len() > 256 {
        return Err("invalid live corruption fixture".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let definition = package(&temporary.path().join("source"), stages, 0, 0)?;
    let root = temporary.path().join("jobs");
    let mut job =
        Job::open(&root, definition.clone(), 1, "live integrity").map_err(|e| e.to_string())?;
    let outputs: Vec<_> = (0..stages).map(|index| format!("{index}: {text} é🙂")).collect();
    for (index, output) in outputs.iter().enumerate() {
        job.complete(&stage(index), output).map_err(|e| e.to_string())?;
    }
    let selected = usize::from(selected % stages);
    let row = &job.state().completed[selected];
    let path = job.root().join(format!("{}-{}.txt", row.stage, row.sha256));
    let id = stage(selected);
    // Warm both the exact-page and final-output interfaces before altering retained bytes.
    read_all(&job, &id, &outputs[selected], 4, metrics)?;
    verify_job(&job, &outputs, stages)?;
    let before = fs::read(job.root().join("job.json")).map_err(|e| e.to_string())?;
    match damage % 3 {
        0 => fs::write(&path, vec![b'!'; outputs[selected].len()]),
        1 => fs::write(&path, b""),
        _ => fs::remove_file(&path),
    }
    .map_err(|e| e.to_string())?;
    for offset in [0, outputs[selected].len() as u64] {
        if job.read(std::slice::from_ref(&id), &id, offset, 4).is_ok() {
            return Err(
                "already-open job returned damaged output (including empty end page)".into()
            );
        }
        bump(metrics, "live_integrity_read_rejections");
    }
    if Job::inspect(&root, definition, 1, "live integrity", 0).is_ok() {
        return Err("inspection accepted a damaged predecessor output".into());
    }
    if selected + 1 == usize::from(stages) && job.result().is_ok() {
        return Err("final output remained cached after artifact drift".into());
    }
    if fs::read(job.root().join("job.json")).map_err(|e| e.to_string())? != before {
        return Err("read integrity failure mutated durable checkpoint".into());
    }
    fs::write(path, &outputs[selected]).map_err(|e| e.to_string())?;
    for (index, output) in outputs.iter().enumerate() {
        read_all(&job, &stage(index), output, 4, metrics)?;
    }
    verify_job(&job, &outputs, stages)?;
    bump(metrics, "use_after_live_integrity_restoration");
    Ok(())
}

fn checkpoint_failure(text: &str, metrics: &mut Metrics) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 256 {
        return Err("invalid bounded failure fixture".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let definition = package(&temporary.path().join("source"), 2, 0, 0)?;
    let root = temporary.path().join("jobs");
    let mut job =
        Job::open(&root, definition.clone(), 1, "publication").map_err(|e| e.to_string())?;
    job.complete("stage-0", "before").map_err(|e| e.to_string())?;
    let checkpoint = job.root().join("job.json");
    let backup = job.root().join("saved-checkpoint");
    fs::rename(&checkpoint, &backup).map_err(|e| e.to_string())?;
    fs::create_dir(&checkpoint).map_err(|e| e.to_string())?;
    if job.complete("stage-1", text).is_ok() || job.state().completed.len() != 1 {
        return Err("failed checkpoint publication advanced stage memory".into());
    }
    let orphan = job
        .root()
        .join(format!("stage-1-{}.txt", hex(&peritus_codec::sha256(text.as_bytes()).into_bytes())));
    if fs::read(&orphan).map_err(|e| e.to_string())? != text.as_bytes() {
        return Err(
            "failure did not reach exact output publication before checkpoint failure".into()
        );
    }
    fs::remove_dir(&checkpoint).map_err(|e| e.to_string())?;
    fs::rename(backup, &checkpoint).map_err(|e| e.to_string())?;
    drop(job);
    let mut job =
        Job::open(&root, definition.clone(), 1, "publication").map_err(|e| e.to_string())?;
    verify_job(&job, &["before".into()], 2)?;
    let retried = format!("{text} retried");
    job.complete("stage-1", &retried).map_err(|e| e.to_string())?;
    drop(job);
    let job = Job::open(&root, definition, 1, "publication").map_err(|e| e.to_string())?;
    verify_job(&job, &["before".into(), retried], 2)?;
    bump(metrics, "checkpoint_failure_then_retry");
    bump(metrics, "orphan_output_not_adopted");
    Ok(())
}
fn damaged_output(text: &str, damage: u8, metrics: &mut Metrics) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 256 {
        return Err("invalid bounded corruption fixture".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let definition = package(&temporary.path().join("source"), 1, 0, 0)?;
    let root = temporary.path().join("jobs");
    let mut job =
        Job::open(&root, definition.clone(), 1, "corruption").map_err(|e| e.to_string())?;
    job.complete("stage-0", text).map_err(|e| e.to_string())?;
    let row = &job.state().completed[0];
    let path = job.root().join(format!("{}-{}.txt", row.stage, row.sha256));
    let checkpoint = job.root().join("job.json");
    let before = fs::read(&checkpoint).map_err(|e| e.to_string())?;
    drop(job);
    match damage % 3 {
        0 => fs::write(&path, vec![b'!'; text.len()]),
        1 => fs::write(&path, b""),
        _ => fs::remove_file(&path),
    }
    .map_err(|e| e.to_string())?;
    if Job::open(&root, definition.clone(), 1, "corruption").is_ok() {
        return Err("reopen accepted damaged committed output".into());
    }
    if fs::read(&checkpoint).map_err(|e| e.to_string())? != before {
        return Err("integrity rejection rewrote checkpoint".into());
    }
    fs::write(path, text).map_err(|e| e.to_string())?;
    let job = Job::open(&root, definition, 1, "corruption").map_err(|e| e.to_string())?;
    verify_job(&job, &[text.into()], 1)?;
    bump(metrics, "corrupt_output_rejections");
    bump(metrics, "use_after_corruption_restoration");
    Ok(())
}
#[derive(Clone)]
struct Selected {
    digest: String,
    version: u8,
    payload: u8,
    enabled: bool,
    damaged: bool,
}
fn registry(operations: &[RegistryOperation], metrics: &mut Metrics) -> Result<(), String> {
    if operations.len() > 96 {
        return Err("registry case too large".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let registry_root = temporary.path().join("registry");
    let mut registry = Registry::new(registry_root.clone());
    let mut selected: Option<Selected> = None;
    for operation in operations {
        match operation {
            RegistryOperation::Install { version, payload } => {
                let source = temporary.path().join("source");
                let definition = package(&source, 2, *version, *payload)?;
                registry.install(&source).map_err(|e| e.to_string())?;
                selected = Some(Selected {
                    digest: definition.digest,
                    version: *version,
                    payload: *payload,
                    enabled: false,
                    damaged: false,
                });
                bump(metrics, "inert_package_installs");
            }
            RegistryOperation::Enable(enabled) => {
                let expected = selected.as_ref().is_some_and(|s| !s.damaged);
                let result = registry.enable("praxis-workflow", *enabled);
                if result.is_ok() != expected {
                    return Err("enablement accepted unknown/damaged package".into());
                }
                if expected {
                    selected.as_mut().ok_or("selection missing")?.enabled = *enabled;
                    bump(metrics, "selection_enablement_changes");
                }
            }
            RegistryOperation::Remove => {
                let retained = selected
                    .as_ref()
                    .map(|row| {
                        let root = registry_root.join("packages/praxis-workflow").join(&row.digest);
                        Ok::<_, String>((
                            root.clone(),
                            fs::read(root.join("module.toml")).map_err(|e| e.to_string())?,
                            fs::read(root.join("resource.txt")).map_err(|e| e.to_string())?,
                        ))
                    })
                    .transpose()?;
                let result = registry.remove("praxis-workflow");
                if result.is_ok() != selected.is_some() {
                    return Err("remove acceptance differs".into());
                }
                if selected.take().is_some() {
                    let (root, manifest, resource) = retained
                        .ok_or("removed selection lost its retained package observation")?;
                    if fs::read(root.join("module.toml")).map_err(|e| e.to_string())? != manifest
                        || fs::read(root.join("resource.txt")).map_err(|e| e.to_string())?
                            != resource
                    {
                        return Err(
                            "removing a selection changed its historical immutable package bytes"
                                .into(),
                        );
                    }
                    bump(metrics, "retained_removed_packages");
                    bump(metrics, "selection_removals");
                }
            }
            RegistryOperation::Damage => {
                if let Some(row) = &mut selected {
                    fs::write(
                        registry_root
                            .join("packages/praxis-workflow")
                            .join(&row.digest)
                            .join("resource.txt"),
                        "damaged installed bytes",
                    )
                    .map_err(|e| e.to_string())?;
                    row.damaged = true;
                    bump(metrics, "installed_package_damage");
                }
            }
            RegistryOperation::Reload => {
                registry = Registry::new(registry_root.clone());
                bump(metrics, "registry_reopens");
            }
        }
        let loaded = registry.load("praxis-workflow", false);
        if loaded.is_ok() != selected.as_ref().is_some_and(|s| !s.damaged) {
            return Err("selected package integrity acceptance differs".into());
        }
        if let (Ok(definition), Some(row)) = (loaded, &selected)
            && (definition.digest != row.digest
                || definition.manifest.version != format!("1.0.{}", row.version)
                || fs::read(definition.resource("resource.txt").map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
                    != format!("resource-{}\n", row.payload).as_bytes())
        {
            return Err("selected package exact content changed".into());
        }
        if registry.load("praxis-workflow", true).is_ok()
            != selected.as_ref().is_some_and(|s| s.enabled && !s.damaged)
        {
            return Err("disabled package remained runnable".into());
        }
        let listed = registry.list().map_err(|e| e.to_string())?;
        let rows = listed["modules"].as_array().ok_or("missing module listing")?;
        let entry = rows.iter().find(|v| v["id"] == "praxis-workflow");
        if entry.is_some() != selected.is_some() {
            return Err("registry listing lost installed selection".into());
        }
        if let (Some(entry), Some(row)) = (entry, &selected)
            && (entry["digest"] != row.digest
                || entry["enabled"] != row.enabled
                || entry.get("error").is_some() != row.damaged)
        {
            return Err("registry status misreported selection/integrity".into());
        }
        bump(metrics, "registry_state_comparisons");
    }
    Ok(())
}
pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    bump(&mut metrics, "cases");
    match case {
        Case::Jobs { stages, operations } => jobs(*stages, operations, &mut metrics)?,
        Case::Registry { operations } => registry(operations, &mut metrics)?,
        Case::CheckpointFailure { text } => checkpoint_failure(text, &mut metrics)?,
        Case::DamagedOutput { text, damage } => damaged_output(text, *damage, &mut metrics)?,
        Case::DamagedWhileOpen { text, stages, selected, damage } => {
            damaged_while_open(text, *stages, *selected, *damage, &mut metrics)?
        }
    }
    Ok(metrics)
}
pub fn calibration_case() -> Case {
    Case::Jobs {
        stages: 2,
        operations: vec![
            Operation::Complete { stage: 1, text: "wrong stage must not commit".into() },
            Operation::Complete { stage: 0, text: "first".into() },
            Operation::Reopen,
        ],
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases =
        vec![calibration_case(), Case::CheckpointFailure { text: "uncommitted orphan μ🙂".into() }];
    for damage in 0..3 {
        cases.push(Case::DamagedOutput { text: "exact committed output μ🙂".into(), damage });
        for selected in 0..3 {
            cases.push(Case::DamagedWhileOpen {
                text: "exact committed output μ🙂".into(),
                stages: 3,
                selected,
                damage,
            });
        }
    }
    cases.push(Case::Jobs {
        stages: 2,
        operations: vec![
            Operation::Contend,
            Operation::Complete { stage: 0, text: "é🙂A".into() },
            Operation::Read { stage: 0, allowed: true, offset: 0, limit: 4 },
            Operation::Read { stage: 0, allowed: true, offset: 1, limit: 4 },
            Operation::Reopen,
            Operation::Complete { stage: 1, text: "final".into() },
            Operation::Inspect(0),
            Operation::OtherIdentity(1),
            Operation::Read { stage: 0, allowed: false, offset: 0, limit: 4 },
        ],
    });
    cases.push(Case::Registry {
        operations: vec![
            RegistryOperation::Install { version: 0, payload: 0 },
            RegistryOperation::Enable(true),
            RegistryOperation::Reload,
            RegistryOperation::Damage,
            RegistryOperation::Enable(false),
            RegistryOperation::Install { version: 0, payload: 0 },
            RegistryOperation::Enable(true),
            RegistryOperation::Install { version: 1, payload: 1 },
            RegistryOperation::Remove,
            RegistryOperation::Reload,
        ],
    });
    cases
}
