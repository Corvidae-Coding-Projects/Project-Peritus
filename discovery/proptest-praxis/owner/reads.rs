//! Reuse through the actual filesystem executor, checked by a per-byte visibility set.
use crate::local_context::{
    MemoryTools,
    praxis_support::fixture::{Fixture, call, result_message},
    tools,
};
use peritus_agent::{DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits};
use peritus_role::HarnessRole;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::atomic::Ordering,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    ops: Vec<Op>,
    padding: u16,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Op {
    Read { first: u8, count: u8, maximum: u16 },
    Show { mask: u8 },
    Edit,
    Allow(bool),
    Protect(bool),
    Reopen,
    Continue,
}
pub fn strategy(steps: usize) -> BoxedStrategy<Case> {
    (proptest::collection::vec(prop_oneof![5=>(1u8..7,0u8..6,proptest::sample::select(vec![768u16,1024,2048,8192])).prop_map(|(first,count,maximum)|Op::Read{first,count,maximum}),2=>any::<u8>().prop_map(|mask|Op::Show{mask}),1=>Just(Op::Edit),1=>any::<bool>().prop_map(Op::Allow),1=>any::<bool>().prop_map(Op::Protect),1=>Just(Op::Reopen),2=>Just(Op::Continue)],1..=steps.clamp(1,40)),proptest::sample::select(vec![0u16,80,1024])).prop_map(|(ops,padding)|Case{ops,padding}).boxed()
}
#[derive(Clone)]
struct Archived {
    id: u64,
    raw: Vec<u8>,
    digest: String,
    points: BTreeSet<usize>,
}
fn metric(m: &mut BTreeMap<String, u64>, key: &str) {
    *m.entry(key.into()).or_default() += 1;
}
fn document(padding: u16, changed: bool) -> String {
    format!(
        "{}alpha μ🙂 {}\n\nthird \\\" escaped\nfourth e\u{301}\nfifth 🙂🙂\nsixth end\n",
        if changed { "Z" } else { "A" },
        "x".repeat(usize::from(padding))
    )
}
fn position(doc: &str, v: &Value) -> Result<usize, String> {
    let line = checked_number(&v["line"], "range line")?;
    let byte = checked_number(&v["byte"], "range byte")?;
    let lines = doc.split_inclusive('\n').collect::<Vec<_>>();
    if line == 0 {
        return Err("range line zero".into());
    }
    if line > lines.len() {
        if line == lines.len() + 1 && byte == 0 {
            return Ok(doc.len());
        }
        return Err("range beyond EOF".into());
    }
    if byte > lines[line - 1].len() {
        return Err("range byte beyond line".into());
    }
    Ok(lines[..line - 1].iter().map(|l| l.len()).sum::<usize>() + byte)
}
fn points(doc: &str, value: &Value) -> Result<BTreeSet<usize>, String> {
    let start = position(doc, &value["source_range"]["start"])?;
    let end = position(doc, &value["source_range"]["end"])?;
    if end < start {
        return Err("backward source interval".into());
    }
    Ok((start..end).collect())
}
fn validate_page(doc: &str, v: &Value) -> Result<BTreeSet<usize>, String> {
    if v["source_sha256"] != json!(tools::hex(peritus_codec::sha256(doc.as_bytes()).as_bytes())) {
        return Err("stale or invented source hash escaped actual executor".into());
    }
    let content = v["content"].as_str().ok_or("missing page body")?;
    let first = checked_number(&v["source_range"]["start"]["line"], "page start")?;
    let offset = checked_number(&v["source_range"]["start"]["byte"], "page offset")?;
    let lines = doc.split_terminator('\n').collect::<Vec<_>>();
    for (i, labelled) in content.split('\n').enumerate() {
        let (label, body) = labelled.split_once(": ").ok_or("missing line label")?;
        let n = label.parse::<usize>().map_err(|e| e.to_string())?;
        if n != first + i {
            return Err("line labels are not a contiguous original range".into());
        }
        let original = lines.get(n - 1).ok_or("page names nonexistent line")?;
        let start = if i == 0 { offset } else { 0 };
        let end = start + body.len();
        if original.get(start..end) != Some(body) {
            return Err("page body differs from original file bytes".into());
        }
        let last = i + 1 == content.split('\n').count();
        if !last && end != original.len() {
            return Err("intermediate displayed line is incomplete".into());
        }
        if last {
            let actual_end = if end == original.len() {
                json!({"line": n + 1, "byte": 0})
            } else {
                json!({"line": n, "byte": end})
            };
            if v["source_range"]["end"] != actual_end {
                return Err(
                    "reported coverage differs from actually delivered original bytes".into()
                );
            }
        }
    }
    let p = points(doc, v)?;
    if v["next_line"].is_number() {
        let next = json!({"line":v["next_line"],"byte":v["next_line_byte_offset"]});
        if position(doc, &next)? < position(doc, &v["source_range"]["end"])? {
            return Err("continuation cursor moves backwards".into());
        }
    }
    Ok(p)
}
fn checked_number(value: &Value, label: &str) -> Result<usize, String> {
    usize::try_from(value.as_u64().ok_or(label)?).map_err(|error| error.to_string())
}

type Points = BTreeSet<usize>;
type Metrics = BTreeMap<String, u64>;

struct Harness {
    fixture: Fixture,
    padding: u16,
    changed: bool,
    doc: String,
    archived: Vec<Archived>,
    visible: BTreeMap<String, Points>,
    metrics: Metrics,
    next: Option<Value>,
    allowed: bool,
    protected: bool,
}

impl Harness {
    fn new(case: &Case) -> Result<Self, String> {
        let fixture = Fixture::new(true, true, HarnessRole::Writer)?;
        let doc = document(case.padding, false);
        fs::write(fixture.workspace.join("file.txt"), &doc).map_err(|error| error.to_string())?;
        Ok(Self {
            fixture,
            padding: case.padding,
            changed: false,
            doc,
            archived: Vec::new(),
            visible: BTreeMap::new(),
            metrics: BTreeMap::new(),
            next: None,
            allowed: true,
            protected: false,
        })
    }

    fn arguments(&mut self, op: Op) -> Result<Option<Value>, String> {
        match op {
            Op::Read { first, count, maximum } => Ok(Some(
                json!({"path":"file.txt","start_line":first,"end_line":(first+count).min(6),"max_bytes":maximum}),
            )),
            Op::Continue => {
                if let Some(args) = self.next.take() {
                    metric(&mut self.metrics, "continuation_attempts");
                    Ok(Some(args))
                } else {
                    metric(&mut self.metrics, "absent_continuation_attempts");
                    Ok(None)
                }
            }
            Op::Edit => {
                self.edit()?;
                Ok(None)
            }
            Op::Allow(read) => {
                self.allowed = read;
                self.fixture.view.read.store(read, Ordering::Release);
                metric(&mut self.metrics, "permission_changes");
                Ok(None)
            }
            Op::Protect(protect) => {
                self.protected = protect;
                self.fixture.view.opaque.store(protect, Ordering::Release);
                metric(&mut self.metrics, "protection_changes");
                Ok(None)
            }
            Op::Reopen => {
                self.fixture.reopen()?;
                self.visible.clear();
                metric(&mut self.metrics, "restart_visible_reset");
                Ok(None)
            }
            Op::Show { mask } => {
                self.show(mask)?;
                Ok(None)
            }
        }
    }

    fn edit(&mut self) -> Result<(), String> {
        self.changed = !self.changed;
        let replacement = document(self.padding, self.changed);
        if replacement.len() != self.doc.len() {
            return Err("same-size edit fixture broken".into());
        }
        self.doc = replacement;
        fs::write(self.fixture.workspace.join("file.txt"), &self.doc)
            .map_err(|error| error.to_string())?;
        metric(&mut self.metrics, "same_size_edits");
        Ok(())
    }

    fn show(&mut self, mask: u8) -> Result<(), String> {
        let mut messages = Vec::new();
        self.visible.clear();
        let mut memory = self.fixture.memory()?.lock().map_err(|error| error.to_string())?;
        for (index, stored) in self.archived.iter().enumerate().rev().take(8) {
            if mask & (1 << (index % 8)) == 0 {
                continue;
            }
            let request = json!({"observation_ids":[tools::source_handle(&memory,stored.id)],"query":null,"cursor":null,"offset":0,"max_bytes":memory.config.max_read_bytes});
            let value = tools::read::execute(
                &mut memory,
                &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let source = value["sources"].as_array().and_then(|sources| sources.first());
            let delivered = source.is_some();
            if let Some(source) = source
                && (source["offset"] != 0
                    || !source["next_offset"].is_null()
                    || source["encoding"] != "utf8"
                    || source["data"].as_str().map(str::as_bytes) != Some(stored.raw.as_slice()))
            {
                return Err("archive delivery differs from complete original bytes".into());
            }
            let observation = DeveloperToolObservation {
                output: CanonicalJson::parse(
                    &value.to_string(),
                    JsonBounds::value(ProtocolLimits::PRODUCTION),
                )
                .map_err(|error| error.to_string())?,
                is_error: !delivered,
            };
            messages
                .push(result_message(&call("retrieved", "context_read", &request)?, &observation)?);
            if delivered {
                self.visible
                    .entry(stored.digest.clone())
                    .or_default()
                    .extend(stored.points.iter().copied());
                metric(&mut self.metrics, "exact_archive_deliveries");
            }
        }
        drop(memory);
        let mut base = crate::developer_tools::WorkspaceDeveloperTools::read_only(
            self.fixture.workspace.clone(),
        )
        .with_protection_view(self.fixture.view.clone());
        MemoryTools::new(&mut base, self.fixture.memory()?.clone())
            .observe_model_context(&messages)
            .map_err(|error| error.to_string())?;
        metric(&mut self.metrics, "final_visible_view_replacements");
        Ok(())
    }

    fn read(&mut self, arguments: &Value) -> Result<(), String> {
        let before = self.visible.clone();
        let (value, error, id) = self.fixture.execute("workspace_read", arguments, true)?;
        metric(&mut self.metrics, "read_calls");
        if !self.allowed || self.protected {
            if !error {
                return Err("current permission/path revocation bypassed by reused read".into());
            }
            metric(&mut self.metrics, "denied_reads");
            return Ok(());
        }
        if error {
            return Err(format!("valid disposable read failed: {value}"));
        }
        if serde_json::to_vec(&value).map_err(|error| error.to_string())?.len()
            > checked_number(&arguments["max_bytes"], "budget")?
        {
            return Err(
                "full read response including reuse metadata exceeded requested byte limit".into(),
            );
        }
        let digest = tools::hex(peritus_codec::sha256(self.doc.as_bytes()).as_bytes());
        let prior = before.get(&digest).cloned().unwrap_or_default();
        let state = value["read_reuse"]["state"].as_str().unwrap_or("ordinary");
        metric(&mut self.metrics, &format!("read_state_{state}"));
        let requested = requested_points(&self.doc, arguments)?;
        if let Some(covered) = validate_response(&self.doc, &value, &prior, &requested, state)? {
            self.visible.entry(digest.clone()).or_default().extend(covered.iter().copied());
            let memory = self.fixture.memory()?.lock().map_err(|error| error.to_string())?;
            let raw = memory.artifact(id).map_err(|error| error.to_string())?;
            drop(memory);
            self.archived.push(Archived { id, raw, digest, points: covered });
        }
        self.next = if value["next_line"].is_number() {
            Some(
                json!({"path":"file.txt","start_line":value["next_line"],"line_byte_offset":value["next_line_byte_offset"],"end_line":arguments["end_line"],"max_bytes":arguments["max_bytes"]}),
            )
        } else {
            None
        };
        // Warm the actual suffix index, never infer expected visibility from it.
        let mut memory = self.fixture.memory()?.lock().map_err(|error| error.to_string())?;
        super::refresh(&mut memory);
        drop(memory);
        Ok(())
    }
}

fn requested_points(doc: &str, arguments: &Value) -> Result<Points, String> {
    let first = usize::try_from(arguments["start_line"].as_u64().unwrap_or(1))
        .map_err(|error| error.to_string())?;
    let end = usize::try_from(arguments["end_line"].as_u64().unwrap_or(6))
        .map_err(|error| error.to_string())?;
    let offset = usize::try_from(arguments["line_byte_offset"].as_u64().unwrap_or(0))
        .map_err(|error| error.to_string())?;
    let start = position(doc, &json!({"line":first,"byte":offset}))?;
    let end = position(doc, &json!({"line":end.min(6)+1,"byte":0}))?;
    Ok((start..end).collect())
}

fn validate_response(
    doc: &str,
    value: &Value,
    prior: &Points,
    requested_points: &Points,
    state: &str,
) -> Result<Option<Points>, String> {
    if state == "already_visible" {
        let requested = points(doc, &json!({"source_range":value["read_reuse"]["requested"]}))?;
        if &requested != requested_points {
            return Err("receipt range does not cover the original requested range".into());
        }
        if !requested.is_subset(prior) {
            return Err("already-visible receipt skips bytes absent from actual provider view (independent byte-set union)".into());
        }
        if value.get("content").is_some() {
            return Err("receipt unexpectedly replays content".into());
        }
        return Ok(None);
    }
    let covered = validate_page(doc, value)?;
    if !covered.is_subset(requested_points) {
        return Err("returned valid bytes outside original requested extent".into());
    }
    if value["next_line"].is_null()
        && !requested_points.is_subset(&prior.union(&covered).copied().collect())
    {
        return Err("read claims completion while requested bytes remain undelivered".into());
    }
    if matches!(state, "restored" | "read_missing") && !covered.is_disjoint(prior) {
        return Err("reuse returned already-visible bytes in its missing interval".into());
    }
    Ok(Some(covered))
}

pub fn check(case: &Case) -> Result<BTreeMap<String, u64>, String> {
    let mut harness = Harness::new(case)?;
    // Start with a real archived read, ensuring subsequent histories reach a populated cache.
    let mut ops = vec![Op::Read { first: 1, count: 5, maximum: 8192 }, Op::Show { mask: 0 }];
    ops.extend(case.ops.clone());
    for op in ops {
        if let Some(arguments) = harness.arguments(op)? {
            harness.read(&arguments)?;
        }
    }
    harness.metrics.insert(
        "max_archived_pages".into(),
        u64::try_from(harness.archived.len()).map_err(|error| error.to_string())?,
    );
    Ok(harness.metrics)
}
pub fn examples() -> Vec<Case> {
    vec![
        calibration_case(),
        Case {
            padding: 1024,
            ops: vec![
                Op::Read { first: 1, count: 5, maximum: 768 },
                Op::Continue,
                Op::Continue,
                Op::Show { mask: 1 },
                Op::Read { first: 1, count: 5, maximum: 2048 },
                Op::Edit,
                Op::Read { first: 1, count: 5, maximum: 2048 },
                Op::Allow(false),
                Op::Read { first: 1, count: 0, maximum: 8192 },
                Op::Allow(true),
                Op::Protect(true),
                Op::Read { first: 1, count: 0, maximum: 8192 },
                Op::Protect(false),
                Op::Reopen,
                Op::Read { first: 1, count: 5, maximum: 8192 },
            ],
        },
        Case {
            padding: 0,
            ops: vec![
                Op::Read { first: 1, count: 0, maximum: 8192 },
                Op::Read { first: 3, count: 1, maximum: 8192 },
                Op::Show { mask: 6 },
                Op::Read { first: 1, count: 5, maximum: 8192 },
                Op::Show { mask: 0 },
                Op::Read { first: 1, count: 5, maximum: 8192 },
            ],
        },
    ]
}
pub fn calibration_case() -> Case {
    Case {
        padding: 0,
        ops: vec![
            Op::Read { first: 1, count: 0, maximum: 8192 },
            Op::Show { mask: 0 },
            Op::Read { first: 1, count: 0, maximum: 8192 },
        ],
    }
}
#[test]
fn praxis_stage2_reads() {
    crate::local_context::praxis_support::run("reads", strategy, check, examples, calibration_case);
}
