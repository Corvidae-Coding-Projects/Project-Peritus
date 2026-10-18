//! Real public-library consumer. No unit-test harness or provider/web substitutes.
//! Host observations below are explicit API fixture inputs, not claims of HTTP execution.
use peritus_deep_research::Research;
use peritus_modules::Catalog;
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Default)]
struct Oracle {
    passed: usize,
    failed: Vec<String>,
}
impl Oracle {
    fn check(&mut self, label: impl Into<String>, yes: bool) {
        let label = label.into();
        if yes {
            self.passed += 1;
        } else {
            eprintln!("FAIL {label}");
            self.failed.push(label);
        }
    }
    fn refused<T, E: std::fmt::Display>(
        &mut self,
        label: impl Into<String>,
        result: std::result::Result<T, E>,
    ) {
        self.check(label, result.is_err());
    }
}

fn plan(case: usize, clarify: bool) -> Value {
    let mut questions = vec![
        "Does the observed evidence support the bounded claim?",
        "What does its publisher actually claim?",
    ];
    if case % 3 == 0 {
        questions.push("What cannot presently be established?");
    }
    json!({"question":format!("Independent research case {case}: café 🦀 東京"),"questions":questions,
        "scope":"Bounded to the exact retained passages; no extrapolation.","period":"2026-10-17; version as observed",
        "strategy":"Read original materials; independent skeptic admits claim-specific evidence; reject discovery-only leads.",
        "output":"Qualified report with every question and explicit unknowns.",
        "clarifications":if clarify {json!([{"question":"Which population?","suggestions":["Observed population only","Unknown"]}])} else {json!([])}})
}
fn page(source: &str, case: usize, variant: usize) -> (Value, String, u64) {
    let prefixes = ["π🙂 ", "ASCII prefix: ", "東京\n", "e\u{301} "];
    let prefix = prefixes[(case + variant) % prefixes.len()].repeat(case % 4 + 1);
    let quote = format!("Evidence {case}/{variant}: café 🦀 keeps exact UTF-8 ranges.");
    let offset = (case * 97 + variant * 11) as u64;
    let text = format!("{prefix}{quote}\nSuffix deliberately differs {variant}.");
    (
        json!({"source":source,"url":format!("https://example.org/primary/{case}/{variant}"),
        "title":format!("Controlled domain observation {case}/{variant}"),"fetched_at_unix":1792200000u64,
        "offset":offset,"total_bytes":offset + text.len() as u64,"text":text}),
        quote,
        offset + prefix.len() as u64,
    )
}
fn candidate(
    source: &str,
    quote: &str,
    offset: u64,
    question: usize,
    category: &str,
    relation: &str,
) -> Value {
    json!({"question":question,"claim":"The narrow claim is bounded to the quoted record.","relation":relation,
        "source":source,"offset":offset,"quote":quote,"category":category,"publisher":"Named fixture publisher",
        "date":"2026-10-17","chain":format!("primary-chain-{source}"),"limitations":"Domain fixture; not real fetched research evidence."})
}
fn admission(candidate: &str, decision: &str) -> Value {
    json!({"candidate":candidate,"decision":decision,"claim":"According to the named publisher, the bounded quoted claim holds.",
        "rationale":"The exact inspected passage supports only this scoped wording.","authority":"Named source; domain fixture.",
        "methods":"Methods as explicitly shown in the retained passage; no broader claims.","period":"2026-10-17",
        "independence":"One evidence chain; copies are not independent.","scope":"Attributed narrow observed population only.",
        "limitations":"Unobserved metadata remains unknown; no generalization."})
}
fn id(value: &Value, field: &str) -> Result<String> {
    Ok(value[field]
        .as_str()
        .ok_or_else(|| format!("missing response field {field}: {value}"))?
        .to_owned())
}

fn research_case(root: &Path, case: usize, oracle: &mut Oracle) -> Result<()> {
    let directory = root.join(format!("research-{case:02}"));
    fs::create_dir_all(&directory)?;
    let ledger = Research::new(&directory);
    let label = |action: &str| format!("research-{case:02}/{action}");
    let pending = ledger.invoke("plan", "research_plan", &plan(case, true))?;
    let pending_digest = id(&pending, "plan_digest")?;
    oracle.refused(label("unresolved-clarification-start"), ledger.approve(&pending_digest));
    let source = format!("{:064x}", case * 16 + 1);
    let (observed, quote, offset) = page(&source, case, 0);
    let supporting = candidate(&source, &quote, offset, 1, "study", "supports");
    oracle.refused(
        label("candidate-before-start"),
        ledger.invoke("gather", "research_candidate", &supporting),
    );
    oracle.refused(label("host-observation-before-start"), ledger.observe("gather", &observed));
    let saved = ledger.invoke("plan", "research_plan", &plan(case, false))?;
    let digest = id(&saved, "plan_digest")?;
    oracle.check(label("changed-plan-changed-identity"), digest != pending_digest);
    oracle.refused(label("stale-start"), ledger.approve(&pending_digest));
    oracle.check(label("stale-start-does-not-approve"), !ledger.approved()?);
    ledger.approve(&digest)?;
    oracle.check(label("exact-start"), ledger.approved()?);
    oracle.refused(
        label("approved-plan-cannot-change"),
        ledger.invoke("plan", "research_plan", &plan(case + 1, false)),
    );
    ledger.observe("gather", &observed)?;
    let mut altered = supporting.clone();
    altered["quote"] = json!(format!("{quote}!"));
    oracle.refused(
        label("fabricated-quote"),
        ledger.invoke("gather", "research_candidate", &altered),
    );
    altered = supporting.clone();
    altered["offset"] = json!(offset + 1);
    oracle.refused(
        label("wrong-byte-range"),
        ledger.invoke("gather", "research_candidate", &altered),
    );
    altered = supporting.clone();
    altered["question"] = json!(0);
    oracle.refused(
        label("invalid-question-zero"),
        ledger.invoke("gather", "research_candidate", &altered),
    );
    let supporting_id =
        id(&ledger.invoke("gather", "research_candidate", &supporting)?, "candidate")?;
    oracle.refused(
        label("assess-without-independent-read"),
        ledger.invoke("assess", "research_admit", &admission(&supporting_id, "admitted")),
    );
    ledger.observe("assess", &observed)?;
    let supporting_admission = id(
        &ledger.invoke("assess", "research_admit", &admission(&supporting_id, "admitted"))?,
        "admission",
    )?;

    // Distinct contradictory evidence, a qualified publisher statement, and an excluded lead.
    let mut admitted = vec![supporting_admission.clone()];
    let mut reviewer_pages = vec![observed.clone()];
    let mut rejected_id = String::new();
    for (variant, category, relation, question, decision) in [
        (1, "dataset", "contradicts", 1, "admitted"),
        (2, "first-party", "supports", 2, "qualified"),
        (3, "discovery-only", "supports", 1, "rejected"),
    ] {
        let source = format!("{:064x}", case * 16 + variant + 1);
        let (observed, quote, offset) = page(&source, case, variant);
        ledger.observe("gather", &observed)?;
        let candidate_id = id(
            &ledger.invoke(
                "gather",
                "research_candidate",
                &candidate(&source, &quote, offset, question, category, relation),
            )?,
            "candidate",
        )?;
        ledger.observe("assess", &observed)?;
        if category == "first-party" {
            oracle.refused(
                label("first-party-unqualified"),
                ledger.invoke("assess", "research_admit", &admission(&candidate_id, "admitted")),
            );
        }
        if category == "discovery-only" {
            for forbidden in ["admitted", "qualified"] {
                oracle.refused(
                    label(&format!("discovery-only-{forbidden}")),
                    ledger.invoke("assess", "research_admit", &admission(&candidate_id, forbidden)),
                );
            }
        }
        let admission_id = id(
            &ledger.invoke("assess", "research_admit", &admission(&candidate_id, decision))?,
            "admission",
        )?;
        if decision == "rejected" {
            rejected_id = admission_id;
        } else {
            admitted.push(admission_id);
            reviewer_pages.push(observed);
        }
    }
    if case % 3 == 0 {
        oracle.refused(label("gather-missing-question"), ledger.stage_output("gather"));
        ledger.invoke("gather", "research_gap", &json!({"question":3,"reason":"No qualifying source within observed bounded material."}))?;
    }
    oracle.check(label("gather-all-questions"), ledger.stage_output("gather").is_ok());
    let synthesis_view =
        ledger.inspect("write", &json!({"kind":"admissions","offset":0,"max_items":32}))?;
    let shown = synthesis_view["records"].as_array().ok_or("admissions records missing")?;
    oracle.check(
        label("synthesis-only-admitted"),
        shown.len() == admitted.len() && !synthesis_view.to_string().contains(&rejected_id),
    );
    oracle.refused(
        label("synthesis-no-raw-candidates"),
        ledger.inspect("write", &json!({"kind":"candidates"})),
    );
    oracle.refused(
        label("synthesis-no-raw-sources"),
        ledger.inspect("write", &json!({"kind":"sources"})),
    );
    oracle.refused(
        label("role-mutation-denied"),
        ledger.invoke("write", "research_candidate", &supporting),
    );

    let mut questions = vec![
        json!({"question":1,"status":"contested","explanation":"Distinct admitted support and contrary evidence; do not conceal disagreement."}),
        json!({"question":2,"status":"supported","explanation":"Attributed publisher statement only."}),
    ];
    if case % 3 == 0 {
        questions.push(json!({"question":3,"status":"unknown","explanation":"No qualifying evidence found; retained explicit gap."}));
    }
    let findings = admitted.iter().map(|a|json!({"admission":a,"interpretation":"Interpretation limited to the quoted evidence and admission scope."})).collect::<Vec<_>>();
    let report = json!({"title":format!("Bounded research report {case}: café 🦀"),"synthesis":"The narrow record contains disagreement. Unknowns remain explicit.",
        "findings":findings,"questions":questions,"limitations":["Domain API scenario; no scientific conclusion is inferred from fixture text."]});
    let mut invalid_report = report.clone();
    invalid_report["questions"].as_array_mut().ok_or("questions missing")?.pop();
    oracle.refused(
        label("every-plan-question-required"),
        ledger.invoke("write", "research_report", &invalid_report),
    );
    invalid_report = report.clone();
    invalid_report["findings"][0]["admission"] = json!(rejected_id);
    oracle.refused(
        label("rejected-evidence-cannot-reenter-report"),
        ledger.invoke("write", "research_report", &invalid_report),
    );
    invalid_report = report.clone();
    invalid_report["findings"] = json!([]);
    oracle.refused(
        label("supported-or-contested-without-evidence"),
        ledger.invoke("write", "research_report", &invalid_report),
    );
    let current = id(&ledger.invoke("write", "research_report", &report)?, "report_digest")?;
    let review =
        json!({"report_digest":current,"approved":true,"reviewed":admitted,"corrections":[]});
    oracle.refused(
        label("review-without-passages-or-report"),
        ledger.invoke("fact-check", "research_review", &review),
    );
    for observed in reviewer_pages.iter().take(reviewer_pages.len() - 1) {
        ledger.observe("fact-check", observed)?;
    }
    oracle.refused(
        label("review-must-read-current-actual-report"),
        ledger.invoke("fact-check", "research_review", &review),
    );
    ledger.inspect("fact-check", &json!({"kind":"report"}))?;
    oracle.refused(
        label("review-must-read-every-cited-passage"),
        ledger.invoke("fact-check", "research_review", &review),
    );
    ledger.observe("fact-check", reviewer_pages.last().ok_or("reviewer passage missing")?)?;
    let mut incomplete_review = review.clone();
    incomplete_review["reviewed"].as_array_mut().ok_or("reviewed missing")?.pop();
    oracle.refused(
        label("review-must-cover-all-citations"),
        ledger.invoke("fact-check", "research_review", &incomplete_review),
    );
    ledger.invoke("fact-check", "research_review", &review)?;
    oracle.check(label("final-approval-exact-report"), ledger.review_approved()?);
    let checked = ledger.stage_output("fact-check")?;
    oracle.check(label("checked-report-retains-exact-quote"), checked.contains(&quote));
    oracle.check(
        label("checked-report-retains-qualifications"),
        checked.contains("qualified") && checked.contains("contrary evidence"),
    );

    // A corrected draft has a new identity; prior approval and prior report-read receipt are stale.
    let mut corrected = report.clone();
    corrected["synthesis"] = json!(format!(
        "Corrected {case}: the evidence remains limited; disagreements are explicit."
    ));
    let corrected_id =
        id(&ledger.invoke("write", "research_report", &corrected)?, "report_digest")?;
    oracle.check(label("changed-report-new-digest"), corrected_id != current);
    oracle.check(label("changed-report-invalidates-approval"), !ledger.review_approved()?);
    oracle.refused(label("changed-report-no-complete-presentation"), ledger.report_markdown(true));
    oracle.refused(
        label("stale-review-digest"),
        ledger.invoke("fact-check", "research_review", &review),
    );
    let mut fresh_review = review;
    fresh_review["report_digest"] = json!(corrected_id);
    oracle.refused(
        label("changed-report-must-be-read"),
        ledger.invoke("fact-check", "research_review", &fresh_review),
    );
    ledger.inspect("fact-check", &json!({"kind":"report"}))?;
    let mut rejection = fresh_review.clone();
    rejection["approved"] = json!(false);
    rejection["corrections"] =
        json!(["Qualify the synthesis population instead of extrapolating."]);
    ledger.invoke("fact-check", "research_review", &rejection)?;
    oracle.refused(label("rejected-draft-no-complete-stage"), ledger.stage_output("fact-check"));
    oracle.refused(
        label("unchanged-rejected-draft"),
        ledger.invoke("write", "research_report", &corrected),
    );
    corrected["synthesis"] = json!(format!(
        "Corrected {case}: this conclusion is qualified to the observed population only, with unresolved disagreement."
    ));
    let final_id = id(&ledger.invoke("write", "research_report", &corrected)?, "report_digest")?;
    fresh_review["report_digest"] = json!(final_id);
    ledger.inspect("fact-check", &json!({"kind":"report"}))?;
    ledger.invoke("fact-check", "research_review", &fresh_review)?;
    let expected = ledger.report_markdown(true)?;
    fs::write(directory.join("expected-report.md"), expected)?;
    let child = Command::new(std::env::current_exe()?)
        .arg("readback")
        .arg(&directory)
        .arg(&digest)
        .arg(&final_id)
        .output()?;
    oracle.check(label("fresh-process-durable-readback"), child.status.success());
    if !child.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&child.stderr));
    }
    let report_path = directory.join("research/reports").join(format!("{final_id}.json"));
    let original_report = fs::read(&report_path)?;
    fs::write(&report_path, b"{\"tampered\":true}")?;
    oracle.refused(
        label("durable-corruption-cannot-present-checked-report"),
        ledger.report_markdown(true),
    );
    fs::write(report_path, original_report)?;
    oracle.check(
        label("restored-exact-bytes-recover-report"),
        ledger.report_markdown(true)? == fs::read_to_string(directory.join("expected-report.md"))?,
    );
    Ok(())
}

fn catalog_cases(oracle: &mut Oracle) -> Result<()> {
    // Expectations come from inert multi-module discovery, safe relative paths, immutable digests.
    for count in 1..=12 {
        let modules = (0..count).map(|i|json!({"id":format!("independent.module-{i}"),"name":format!("Research café 🦀 {i}"),"version":format!("1.{i}.0"),
            "description":"Independent package, no executable authority.","path":format!("modules/module-{i}"),"digest":format!("{:064x}",i+1),"tags":["research","資料"]})).collect::<Vec<_>>();
        let valid = json!({"schema":1,"name":"Independent multi-module catalog","modules":modules});
        let text = toml::to_string(&valid)?;
        oracle.check(
            format!("catalog-{count}/multiple-independent-packages"),
            Catalog::parse(&text)?.modules.len() == count,
        );
        let mut invalid = valid.clone();
        invalid["schema"] = json!(2);
        oracle.refused(
            format!("catalog-{count}/unknown-schema"),
            Catalog::parse(&toml::to_string(&invalid)?),
        );
        invalid = valid.clone();
        invalid["modules"]
            .as_array_mut()
            .ok_or("modules missing")?
            .push(valid["modules"][0].clone());
        oracle.refused(
            format!("catalog-{count}/duplicate-id-and-path"),
            Catalog::parse(&toml::to_string(&invalid)?),
        );
        let last = invalid["modules"].as_array().ok_or("modules missing")?.len() - 1;
        invalid["modules"][last]["path"] = json!("modules/distinct-path");
        oracle.refused(
            format!("catalog-{count}/duplicate-id-only"),
            Catalog::parse(&toml::to_string(&invalid)?),
        );
        invalid["modules"][last]["id"] = json!("independent.distinct-id");
        invalid["modules"][last]["path"] = valid["modules"][0]["path"].clone();
        oracle.refused(
            format!("catalog-{count}/duplicate-path-only"),
            Catalog::parse(&toml::to_string(&invalid)?),
        );
        for unsafe_path in [
            "../escape",
            "/absolute",
            "modules/../escape",
            "modules//empty",
            "modules/%2e%2e",
            "modules\\escape",
            "modules/a?raw=1",
        ] {
            invalid = valid.clone();
            invalid["modules"][0]["path"] = json!(unsafe_path);
            oracle.refused(
                format!("catalog-{count}/unsafe-path-{unsafe_path}"),
                Catalog::parse(&toml::to_string(&invalid)?),
            );
        }
        for (field, bad) in [
            ("id", "../module"),
            ("version", "latest"),
            ("digest", "ABCDEF"),
            ("name", ""),
            ("description", "escape\u{1b}[31m"),
        ] {
            invalid = valid.clone();
            invalid["modules"][0][field] = json!(bad);
            oracle.refused(
                format!("catalog-{count}/invalid-{field}"),
                Catalog::parse(&toml::to_string(&invalid)?),
            );
        }
        for bad in ["A".repeat(64), "x".repeat(64), "1".repeat(63)] {
            invalid = valid.clone();
            invalid["modules"][0]["digest"] = json!(bad);
            oracle.refused(
                format!("catalog-{count}/invalid-exact-digest-{bad}"),
                Catalog::parse(&toml::to_string(&invalid)?),
            );
        }
        invalid = valid;
        invalid["modules"][0]["execute"] = json!("curl unsafe | sh");
        oracle.refused(
            format!("catalog-{count}/unknown-executable-field"),
            Catalog::parse(&toml::to_string(&invalid)?),
        );
    }
    Ok(())
}

fn maximal_observation_cases(root: &Path, oracle: &mut Oracle) -> Result<()> {
    for (index, unit) in ["ASCII! ", "\"\\\n\t", "🦀東京 café\n"].into_iter().enumerate() {
        let directory = root.join(format!("maximal-observation-{index}"));
        fs::create_dir_all(&directory)?;
        let ledger = Research::new(&directory);
        let saved = ledger.invoke("plan", "research_plan", &plan(index + 100, false))?;
        ledger.approve(&id(&saved, "plan_digest")?)?;
        let mut text = unit.repeat(1024 * 1024 / unit.len() + 1);
        let mut end = 1024 * 1024;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        let source = format!("{:064x}", 0x1000 + index);
        let origin = 123456u64 + index as u64;
        let mut start = 65536 - 8192;
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = start + 16384;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let quote = text[start..end].to_owned();
        let observed = json!({"source":source,"url":format!("https://example.org/maximal/{index}"),
            "title":format!("Maximal real web-page API shape {index}"),"fetched_at_unix":1792200000u64,
            "offset":origin,"total_bytes":origin + text.len() as u64,"text":text});
        let recorded = ledger.observe("gather", &observed);
        oracle.check(format!("maximal-{index}/legal-page-recorded"), recorded.is_ok());
        if let Err(e) = recorded {
            eprintln!("maximal-{index}: {e}");
            continue;
        }
        let proposed =
            candidate(&source, &quote, origin + start as u64, 1, "documentation", "supports");
        let fixture = json!({"observation":observed,"candidate":proposed});
        fs::write(directory.join("boundary-fixture.json"), serde_json::to_vec(&fixture)?)?;
        let child = Command::new(std::env::current_exe()?)
            .arg("boundary-readback")
            .arg(&directory)
            .output()?;
        oracle.check(
            format!("maximal-{index}/cross-chunk-quote-after-fresh-process"),
            child.status.success(),
        );
        if !child.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&child.stderr));
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|s| s == "boundary-readback") {
        let directory = Path::new(args.get(2).ok_or("boundary directory missing")?);
        let fixture: Value =
            serde_json::from_slice(&fs::read(directory.join("boundary-fixture.json"))?)?;
        let ledger = Research::new(directory);
        let recorded = ledger.invoke("gather", "research_candidate", &fixture["candidate"])?;
        let candidate_id = id(&recorded, "candidate")?;
        ledger.observe("assess", &fixture["observation"])?;
        ledger.invoke("assess", "research_admit", &admission(&candidate_id, "admitted"))?;
        println!(
            "PASS fresh process: maximal page quote crossing persisted chunk boundary admitted only after independent assess read"
        );
        return Ok(());
    }
    if args.get(1).is_some_and(|s| s == "readback") {
        let root = Path::new(args.get(2).ok_or("readback directory missing")?);
        let ledger = Research::new(root);
        if ledger.plan_digest()? != *args.get(3).ok_or("plan digest missing")? {
            return Err("reopened plan identity changed".into());
        }
        if !ledger.approved()? || !ledger.review_approved()? {
            return Err("reopened durable approvals missing".into());
        }
        let report = ledger.report_markdown(true)?;
        if !report.contains(args.get(4).ok_or("report digest missing")?)
            || report != fs::read_to_string(root.join("expected-report.md"))?
        {
            return Err("reopened exact checked report differs".into());
        }
        println!("PASS fresh process reopened exact plan, approval, report and source-check");
        return Ok(());
    }
    let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(format!("/tmp/peritus-market-research-consumer-{}", std::process::id()))
    });
    if root.exists() {
        return Err(format!("use a fresh disposable directory: {}", root.display()).into());
    }
    fs::create_dir_all(&root)?;
    let mut oracle = Oracle::default();
    catalog_cases(&mut oracle)?;
    maximal_observation_cases(&root, &mut oracle)?;
    for case in 1..=24 {
        match research_case(&root, case, &mut oracle) {
            Ok(()) => println!("PASS completed generated research journey {case:02}"),
            Err(e) => {
                eprintln!("FAIL journey {case}: {e}");
                oracle.failed.push(format!("research-{case:02}: {e}"));
            }
        }
    }
    let receipt = json!({"schema":1,"generator":"market-research-consumer-v1","seed":"deterministic cases 1..24; prefix/case/range/category/state cross-product", "research_journeys":24,"catalog_sizes":12,"maximal_page_shapes":3,"passing_observations":oracle.passed,"failures":oracle.failed,
        "scope":"Real public peritus_modules::Catalog / peritus_deep_research::Research consumer; injected host observations are API inputs, no web/provider/UI execution claimed.","directory":root});
    fs::write(root.join("receipt.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    if !oracle.failed.is_empty() {
        return Err(format!("{} unresolved independent expectations", oracle.failed.len()).into());
    }
    Ok(())
}
