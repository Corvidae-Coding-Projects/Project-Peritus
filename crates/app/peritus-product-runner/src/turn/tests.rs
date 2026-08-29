use super::*;

#[test]
fn rejected_terminal_correction_requires_fresh_repository_grounding() {
    let error = ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "ground developer turn in repository evidence",
        "repository grounding requires a successful workspace listing",
    );
    let correction = correction_prompt(&error);
    let prompt = writer_user("task", "design", Some("finding"), Some(&correction));

    assert!(prompt.contains("Harness correction from the previous rejected turn"));
    assert!(prompt.contains("workspace_list"));
    assert!(prompt.contains("workspace_read"));
    assert!(prompt.contains("If no code change is needed"));
    assert!(prompt.contains(error.detail()));
}

#[test]
fn unchanged_question_is_challenged_with_confirmed_workspace_capabilities() {
    let correction = question_confirmation_prompt("Please provide a writable workspace.");
    let prompt = writer_user("task", "design", None, Some(&correction));

    assert!(prompt.contains("stopped without changing the workspace"));
    assert!(prompt.contains("independently confirms"));
    assert!(prompt.contains("workspace_write"));
    assert!(prompt.contains("provider has no native filesystem tools"));
    assert!(prompt.contains("return the same direct question unchanged"));
}

#[test]
fn reviewer_checks_literal_request_independently_of_the_design() {
    let prompt = reviewer_system();
    assert!(prompt.contains("Begin every review by requesting"));
    assert!(prompt.contains("model tool-call interface"));
    assert!(prompt.contains("they are not provider-native tools"));
    assert!(prompt.contains("authoritative source inputs"));
    assert!(prompt.contains("Do not rely on the writer's account"));
    assert!(prompt.contains("design is a proposal, not authority"));
    assert!(prompt.contains("every explicit requested path, field, value"));
    assert!(prompt.contains("complete selected source value"));
    assert!(prompt.contains("only transformations the request names"));
    assert!(prompt.contains("is not authority to delete it"));
    assert!(prompt.contains("agrees with its own interpretation"));
    assert!(prompt.contains("close a non-exhaustive example"));
    assert!(prompt.contains("reverse declared source precedence"));
    assert!(prompt.contains("demotes a matching superseding rule"));
    assert!(prompt.contains("non-advisory finding"));
    assert!(prompt.contains("grammatically ambiguous compound phrase"));
    assert!(prompt.contains("violates every reasonable reading"));
    assert!(prompt.contains("trailing modifier distributes"));
    assert!(prompt.contains("nearest-item attachments"));
    assert!(prompt.contains("named rule category"));
    assert!(prompt.contains("membership definition"));
    assert!(prompt.contains("one-shot transient failures"));
    assert!(prompt.contains("never as reasons for repeated fixer cycles"));
    assert!(prompt.contains("blocking compatibility failure"));
    assert!(prompt.contains("Legitimate mocks for unrelated boundaries"));
    assert!(prompt.contains("successful implementation behavior alone is not test coverage"));
    assert!(prompt.contains("without a finite attempt bound or repeated-token guard"));
    assert!(prompt.contains("separately supplied aggregates are raw"));
    assert!(prompt.contains("unresolved aggregate provenance is advisory"));
    assert!(prompt.contains("context-free record ID"));
    assert!(prompt.contains("group by type rather than by individual record"));
    assert!(prompt.contains("duplicated into primary and reject outputs"));
    assert!(prompt.contains("status values retain material conditions"));
    assert!(prompt.contains("summary exception counts reconcile"));
    assert!(prompt.contains("absent referenced record labeled only"));
    assert!(prompt.contains("concrete reason-taxonomy defect"));
    assert!(prompt.contains("present record that fails"));
    assert!(prompt.contains("classification does not match"));
    assert!(prompt.contains("closed class named by their"));
    assert!(prompt.contains("ledger unless explicit overlap"));
    assert!(prompt.contains("only"));
    assert!(prompt.contains("losing-source collection"));
    assert!(prompt.contains("priority, date, expiry, scope"));
    assert!(prompt.contains("source-reference elements"));
    assert!(prompt.contains("breaks matching, joins, and deduplication"));
    assert!(prompt.contains("flag unnecessary"));
    assert!(prompt.contains("avoid presenting the rejected value as an answer"));
    assert!(prompt.contains("independently resolved"));
    assert!(prompt.contains("exact counterexample identity"));
    assert!(prompt.contains("richer sibling output"));
    assert!(prompt.contains("Every artifact that"));
    assert!(prompt.contains("records `not_reproducible`"));
    assert!(prompt.contains("location field itself"));
    assert!(prompt.contains("another field cannot substitute"));
    assert!(prompt.contains("decision values that omit material"));
    assert!(prompt.contains("decisive disposition evidence"));
    assert!(prompt.contains("relative/path#record_id"));
    assert!(prompt.contains("split exclusions, conditions"));
    assert!(prompt.contains("canonical decision dimension incomplete"));
    assert!(prompt.contains("final governing requirement"));
    assert!(prompt.contains("satisfaction question separately"));
    assert!(prompt.contains("requirement clause and failed validity"));
    assert!(prompt.contains("boundary documents to cite governing clause IDs"));
    assert!(prompt.contains("forbidden decision labels inside negations"));
    assert!(prompt.contains("scalar identity or name lists"));
    assert!(prompt.contains("replaced by richer objects"));
    assert!(prompt.contains("detail in its owning fields"));
    assert!(prompt.contains("true insufficient-evidence result"));
    assert!(prompt.contains("requires an empty or null"));
    assert!(prompt.contains("sentinel; a partial pointer"));
}

#[test]
fn writer_batches_tools_and_respects_artifact_workspaces() {
    let prompt = writer_system("writer");
    assert!(prompt.contains("Batch independent tool calls"));
    assert!(prompt.contains("Every fresh writer or fixer invocation"));
    assert!(prompt.contains("read each existing target"));
    assert!(prompt.contains("prior-cycle reads"));
    assert!(prompt.contains("peritus-internal gates are unavailable"));
    assert!(prompt.contains("bounded ephemeral producer"));
    assert!(prompt.contains("do not add package scaffolding"));
    assert!(prompt.contains("invented allowlist"));
    assert!(prompt.contains("preserve that precedence"));
    assert!(prompt.contains("owns the primary field"));
    assert!(prompt.contains("opaque contract values"));
    assert!(prompt.contains("no useful reversible requested result"));
    assert!(prompt.contains("real declared dependency"));
    assert!(prompt.contains("Never make tests pass by injecting a substitute"));
    assert!(prompt.contains("same-workload baseline"));
    assert!(prompt.contains("use profiling when the cause is not already evident"));
    assert!(prompt.contains("map every named bug or behavior"));
    assert!(prompt.contains("For API clients"));
    assert!(prompt.contains("repeated cursors or pages"));
    assert!(prompt.contains("preserve the aggregate"));
    assert!(prompt.contains("without guessed membership or effects"));
    assert!(prompt.contains("retain the authoritative category"));
    assert!(prompt.contains("aggregate category summaries"));
    assert!(prompt.contains("without unrequested duplication"));
    assert!(prompt.contains("specific evidenced reason"));
    assert!(prompt.contains("across every output artifact"));
    assert!(prompt.contains("failed reference lookup"));
    assert!(prompt.contains("reserve invalid-reference reasons"));
    assert!(prompt.contains("ledger named for one closed classification"));
    assert!(prompt.contains("neighboring review or informational classes"));
    assert!(prompt.contains("one conflict-provenance collection"));
    assert!(prompt.contains("losses caused by date, expiry, scope"));
    assert!(prompt.contains("empty/null applicable-authority sentinel"));
    assert!(prompt.contains("missing controlling fact"));
    assert!(prompt.contains("preserve the complete selected source value"));
    assert!(prompt.contains("apply only explicitly named transformations"));
}

#[test]
fn reviewer_rechecks_conserved_finding_locations_after_fixes() {
    let prompt = reviewer_user(
        "task",
        "diff",
        "gates",
        "request: python check.py\nresult: success",
        "finding",
        None,
    );

    assert!(prompt.contains("Developer command observations"));
    assert!(prompt.contains("python check.py"));
    assert!(prompt.contains("not deterministic harness gates"));
    assert!(prompt.contains("For every conserved finding"));
    assert!(prompt.contains("read each cited current workspace file"));
    assert!(prompt.contains("can predate fixer writes"));
}
