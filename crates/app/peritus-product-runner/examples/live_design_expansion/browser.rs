//! Real browser-sized expert pass and an independently observed authorized source edit.

use super::{
    Arc, Command, ConversationView, CountingProvider, Error, FixedConversation, Fixture, Ordering,
    Path, PathBuf, checked, fs, io, outcome_diagnostic, require_no_question,
    require_ready_artifacts,
};

pub async fn recover(provider: Arc<CountingProvider>) -> Result<(), Box<dyn Error>> {
    let saved =
        std::env::var_os("PERITUS_BROWSER_RECOVERY_FIXTURE").map(PathBuf::from).ok_or_else(
            || io::Error::other("set PERITUS_BROWSER_RECOVERY_FIXTURE to a completed browser case"),
        )?;
    let fixture = Fixture::new([0xD7; 16], [0xD8; 16])?;
    // The fresh fixture's Git object files are read-only. Replace its disposable Git
    // directory rather than overwriting objects while restoring the saved repository.
    fs::remove_dir_all(fixture.repository.path().join(".git"))?;
    super::copy_tree(&saved.join("repository"), fixture.repository.path())?;
    super::copy_tree(&saved.join("state"), fixture.state.path())?;
    let source_design = fs::read(fixture.repository.path().join("design.md"))?;
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.trace_path.with_extension("design-source.json"),
    )?)?;
    let prior: [u8; 32] = serde_json::from_value(manifest["identity"]["prior_sha256"].clone())?;
    let mut digest = String::with_capacity(64);
    for byte in prior {
        use std::fmt::Write as _;
        write!(digest, "{byte:02x}")?;
    }
    let task = fs::read_to_string(
        fixture.trace_path.with_extension(format!("design-source-{digest}.md")),
    )?;
    let original: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.trace_path.with_extension("design-expert.json"),
    )?)?;
    let old_amendments = original["amendments"]
        .as_array()
        .ok_or_else(|| io::Error::other("saved amendments missing"))?;
    let conversation = Arc::new(super::MutableConversation::new(&task));
    conversation.answer("Clarify the retained worker acceptance: after timeout or worker death, ownership of the old child must be retained until its exit status is collected, without blocking public routing. Preserve all other researched amendments exactly. Reuse applicable retained evidence; this clarification needs no new network research. Recover the saved pass, make only the affected amendment correction, review, and publish the updated design. Keep the original source design.md byte-identical. The coding handoff has already been observed; do not duplicate the marker declaration.");
    let initial = provider.requests();
    provider.request_bound.store(initial + 16, Ordering::Release);
    let first_line =
        fs::read_to_string(fixture.trace_path.with_extension("efficiency.jsonl"))?.lines().count();
    let view: Arc<dyn ConversationView> = conversation;
    let outcome = fixture.run(provider.clone(), view, task, "browser-recovery").await;
    fixture.retain("browser-recovery")?;
    let (outcome, reached_writer) = outcome?;
    if !reached_writer {
        return Err(io::Error::other(outcome_diagnostic(
            "recovered browser did not publish",
            &outcome,
        ))
        .into());
    }
    require_no_question(&outcome, "browser recovery")?;
    require_ready_artifacts(&fixture.trace_path)?;
    let recovered: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.trace_path.with_extension("design-expert.json"),
    )?)?;
    let current = recovered["amendments"]
        .as_array()
        .ok_or_else(|| io::Error::other("recovered amendments missing"))?;
    let unchanged = old_amendments.iter().filter(|old| current.contains(old)).count();
    if current.len() < old_amendments.len() || unchanged + 1 < old_amendments.len() {
        return Err(io::Error::other("recovery regenerated unrelated amendments").into());
    }
    if fs::read(fixture.repository.path().join("design.md"))? != source_design {
        return Err(io::Error::other("recovery changed the original supplied design").into());
    }
    let summary = serde_json::json!({"case":"browser-recovery","model_requests":provider.requests()-initial,"amendments":current.len(),"unchanged_prior_amendments":unchanged,"original_design_preserved":true,"provider_accounting":fixture.provider_metrics_since(first_line)?});
    fixture.record_summary(&summary)?;
    if let Some(root) = std::env::var_os("PERITUS_DESIGN_QUALIFICATION_OUTPUT") {
        fs::copy(
            fixture.state.path().join("qualification-summary.json"),
            PathBuf::from(root).join("browser-recovery/state/qualification-summary.json"),
        )?;
    }
    eprintln!(
        "browser recovery published {} amendments, retaining {unchanged} unchanged, in {} model requests",
        current.len(),
        provider.requests() - initial
    );
    Ok(())
}

pub async fn qualify(provider: Arc<CountingProvider>) -> Result<(), Box<dyn Error>> {
    let source =
        std::env::var_os("PERITUS_BROWSER_QUALIFICATION_SOURCE").map(PathBuf::from).ok_or_else(
            || io::Error::other("set PERITUS_BROWSER_QUALIFICATION_SOURCE to a browser checkout"),
        )?;
    let mut fixture = Fixture::new([0xD7; 16], [0xD8; 16])?;
    copy_checkout(&source, fixture.repository.path())?;
    let source_design = fs::read(fixture.repository.path().join("design.md"))?;
    if source_design.len() < 16_000 {
        return Err(io::Error::other(
            "browser qualification requires a substantial existing design",
        )
        .into());
    }
    let marker = fixture.repository.path().join("crates/browser-core/src/lib.rs");
    if fs::read_to_string(&marker)?.contains("PERITUS_EXPERT_HANDOFF") {
        return Err(io::Error::other("fixture already contains the expected coding effect").into());
    }
    fixture.cancel_at_writer = false;
    fixture.expected_edit = Some(marker.clone());
    let task = "Read the current design.md and relevant browser code. Perform a focused first domain-expert design enhancement pass on worker reliability and browser Fetch behavior, using authoritative online research where it affects correctness. Preserve the Rust-only, Linux-first direction and all four original browsing workflows, without claiming release qualification. Record at least two concrete, evidence-backed amendments with affected owners, failure behavior and observable acceptance. This is an explicitly bounded qualification pass, not a request to finish the entire browser. The expanded design is the separate host-owned artifact: leave the supplied design.md byte-identical throughout design and implementation. After publishing the design, prove the authorized coding handoff by adding exactly the public Rust declaration `pub const PERITUS_EXPERT_HANDOFF: bool = true;` to crates/browser-core/src/lib.rs. That declaration is the only authorized workspace edit. Preserve all other workspace files and code. Do not run tests, builds, or commands; verification is deferred until this coding change is complete.";
    let initial = provider.requests();
    provider.request_bound.store(initial + 32, Ordering::Release);
    let conversation: Arc<dyn ConversationView> = Arc::new(FixedConversation(task.to_owned()));
    let outcome = fixture.run(provider.clone(), conversation, task.into(), "browser").await;
    fixture.retain("browser")?;
    let (outcome, reached_writer) = outcome?;
    eprintln!("{}", outcome_diagnostic("browser result", &outcome));
    require_no_question(&outcome, "browser expert pass")?;
    if !reached_writer {
        return Err(io::Error::other(outcome_diagnostic(
            "browser did not reach implementation",
            &outcome,
        ))
        .into());
    }
    require_ready_artifacts(&fixture.trace_path)?;
    let pass: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.trace_path.with_extension("design-expert.json"),
    )?)?;
    let amendments = pass["amendments"]
        .as_array()
        .ok_or_else(|| io::Error::other("expert pass omitted amendments"))?;
    if amendments.len() < 2
        || amendments.iter().any(|a| {
            a["acceptance"].as_array().is_none_or(Vec::is_empty)
                || a["evidence"].as_array().is_none_or(Vec::is_empty)
        })
    {
        return Err(io::Error::other(
            "browser pass lacks useful evidence-backed amendments and acceptance",
        )
        .into());
    }
    if !amendments.iter().any(|a| {
        a["evidence"]
            .as_array()
            .is_some_and(|evidence| evidence.iter().any(|e| e["reference"]["kind"] == "web"))
    }) {
        return Err(
            io::Error::other("browser pass did not retain authoritative online evidence").into()
        );
    }
    if !reached_writer
        || !fs::read_to_string(&marker)?
            .lines()
            .any(|line| line.trim() == "pub const PERITUS_EXPERT_HANDOFF: bool = true;")
    {
        return Err(io::Error::other(outcome_diagnostic(
            "published browser design did not produce the authorized source edit",
            &outcome,
        ))
        .into());
    }
    if fs::read(fixture.repository.path().join("design.md"))? != source_design {
        return Err(io::Error::other("original design was overwritten").into());
    }
    fixture.record_summary(&serde_json::json!({"case":"browser-expert","model_requests":provider.requests()-initial,"amendments":amendments.len(),"observed_source_edit":true,"original_design_preserved":true,"provider_accounting":fixture.provider_metrics()?}))?;
    if let Some(root) = std::env::var_os("PERITUS_DESIGN_QUALIFICATION_OUTPUT") {
        fs::copy(
            fixture.state.path().join("qualification-summary.json"),
            PathBuf::from(root).join("browser/state/qualification-summary.json"),
        )?;
    }
    eprintln!(
        "browser expert pass published {} amendments and performed an actual Rust source edit in {} model requests",
        amendments.len(),
        provider.requests() - initial
    );
    Ok(())
}

fn copy_checkout(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git").arg("-C").arg(source).args(["ls-files", "-z"]).output()?;
    if !output.status.success() {
        return Err(io::Error::other("could not enumerate browser source").into());
    }
    for name in output.stdout.split(|byte| *byte == 0).filter(|name| !name.is_empty()) {
        let relative = Path::new(std::str::from_utf8(name)?);
        if relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
            return Err(io::Error::other("invalid tracked fixture path").into());
        }
        let from = source.join(relative);
        let to = destination.join(relative);
        if fs::symlink_metadata(&from)?.file_type().is_symlink() {
            return Err(io::Error::other("browser fixture contains a symbolic link").into());
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    // The user-supplied design can intentionally be excluded from the source repository.
    // Capture that explicitly named input as well as the tracked implementation.
    fs::copy(source.join("design.md"), destination.join("design.md"))?;
    checked(
        Command::new("git").args(["add", "--force", "design.md"]).current_dir(destination),
        "record supplied design",
    )?;
    checked(
        Command::new("git").args(["add", "."]).current_dir(destination),
        "record browser fixture",
    )?;
    checked(
        Command::new("git")
            .args([
                "-c",
                "user.name=Peritus Qualification",
                "-c",
                "user.email=peritus@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Browser qualification baseline",
            ])
            .current_dir(destination),
        "commit disposable fixture",
    )?;
    Ok(())
}
