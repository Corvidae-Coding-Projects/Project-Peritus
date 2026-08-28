# External benchmark failure journal

This journal records product failures exposed by unchanged upstream benchmark tasks. Local result
and workspace paths are retained outside Git because they contain large generated traces. The
checked-in entries keep enough exact detail to find that evidence and reproduce each run.

## HBF-001: malformed design retries lacked correction

- Suite and task: HarnessBench 2.0, `001-file`.
- Symptom: Peritus rejected a detailed design because it used several level-one headings, then sent
  the same prompt for all three retries. The writer never started.
- Cause: the design parser required one level-one title and at least four level-two sections, but
  the retry request did not include the rejected output's exact contract violation.
- Change: design retries now include direct heading correction and conversation revisions clear
  stale correction text. A regression test verifies the retry request.
- Before evidence: local report `reports/001-file-pre-design-correction.json`; elapsed 127 seconds;
  nine provider rounds; adapter failed before writing the artifact.
- After evidence: the next run passed design and wrote the exact requested artifact. It exposed
  HBF-002 at the verification boundary.

## HBF-002: generic artifact workspaces had no gate contract

- Suite and task: HarnessBench 2.0, `001-file`.
- Symptom: the writer correctly created `out/linecount.txt`, but D1 could not discover a project and
  E0 consumed all review/fix cycles without progress. HarnessBench stopped the adapter at its
  unchanged 600-second deadline.
- Cause: Peritus recognized language package manifests only. Most HarnessBench tasks are small
  artifact workspaces and 102 of its 106 tasks have no recognized language manifest.
- Change: `peritus-gates` now supports a strict `peritus-workspace.toml` artifact contract. The
  benchmark adapter creates that marker only when it initializes a previously non-Git fixture; it
  never rewrites an existing repository. D1 checks source layout while the unchanged external
  oracle remains the semantic authority.
- Before evidence: local workspace
  `oc-bench-v2-001-file-gpt-5.6-sol-20260828-090538-e732c95e`; correct artifact present; adapter
  killed by the suite after 600 seconds.
- After evidence: local report `reports/001-file-post-artifact-contract.json`; artifact oracle 1.0;
  elapsed 182.232 seconds.
- Follow-up: add progress-aware early termination so any future structural gate failure stops after
  bounded repeated evidence instead of consuming every fixer cycle.

## HBI-001: process scoring expected an API-key HTTP endpoint

- Suite and task: HarnessBench 2.0, `001-file`.
- Symptom: the successful artifact run skipped process grading because HarnessBench requires an
  OpenAI-compatible HTTP rubric endpoint, while Peritus deliberately keeps paid-account login in
  the official `codex` executable.
- Cause: credential transport mismatch at the benchmark boundary, not missing model access.
- Change: a localhost-only HTTP shim now forwards bounded text rubric requests unchanged to the
  native Rust benchmark agent. Rust validates the request, uses the authenticated official Codex
  router, and returns the response and token accounting. No account credential is read or copied.
- After evidence: local report `reports/001-file-scored-baseline.json`; artifact oracle 1.0; process
  0.7833; security 1.0; combined 0.7833; elapsed 159.878 seconds.
- Diagnostic: the rubric found correct recovery but excessive work for the trivial task, including
  redundant reads and repeated design prose. This becomes an orchestration-efficiency improvement,
  not a benchmark-specific prompt exception.
- Limitation: the paid-account router is currently text-only, so image rubric messages fail
  explicitly instead of silently losing content.

## HBF-003: streamed UTF-8 tool arguments were decoded per fragment

- Suite and task: HarnessBench 2.0, `005-email-triage`.
- Symptom: Peritus created every requested output and the unchanged oracle scored 1.0, but normalized
  trace projection failed with `tool arguments are not UTF-8`. Process grading and provider identity
  were therefore unavailable.
- Cause: the projector decoded every arbitrary stream fragment as complete UTF-8. A multibyte
  character may legally span adjacent fragments.
- Change: tool-argument fragments are now accumulated as bytes and decoded only after the complete
  provider response. A regression splits `é` between its two bytes and proves the reconstructed
  arguments remain exact.
- Before evidence: result `results/peritus-codex-claude/unknown-api/005-email-triage.json`; artifact
  oracle 1.0; adapter return code 1; exact trace error retained in adapter stderr.
- After evidence: local report `reports/005-email-triage-post-utf8-fix.json`; adapter healthy;
  artifact oracle 1.0; process 0.91; security 1.0; combined 0.91; 15 normalized provider rounds.

## HBA-001: internal acceptance controlled the external process exit

- Suite and task: HarnessBench 2.0, first observed on `007-session-memory`.
- Symptom: a completed Peritus attempt exited nonzero whenever Peritus did not internally accept
  the candidate. HarnessBench still scored a single-round workspace, but correctly stopped before
  the next prompt of a multi-round task.
- Cause: the process boundary treated product acceptance as adapter health even though HarnessBench
  owns the semantic oracle and multi-round schedule.
- Change: any fully reported adapter attempt exits successfully. Setup, provider, trace, and report
  publication failures remain nonzero. Product acceptance and failure detail remain explicit in
  `invocation.json` for diagnosis.
- After evidence: the focused task 007 rerun completed both turns with adapter health true. The
  stronger durable-conversation result is recorded under HBS-001.

## HBS-001: external multi-round runs forgot prior user turns

- Suite and task: HarnessBench 2.0, `007-session-memory`.
- Symptom: round 1 wrote its non-secret readiness marker, but round 2 could not recall the passphrase
  supplied only in the prior user message. The oracle scored 0.25 and the process rubric scored 0.35.
- Cause: the first adapter used a process-local one-message conversation view. HarnessBench invokes
  the generic executable once per round, so the next process received the same session identity but
  no prior user turns. It also appended both runs to one trace without an explicit turn boundary,
  making the projected transcript disordered.
- Change: the native adapter now persists a bounded, versioned, atomic conversation record in the
  benchmark sandbox, validates its session/task identity, restores every exact user turn, assigns
  one trace per turn, and rebuilds the final normalized process trace in turn order. Nothing is
  written into the benchmark workspace to simulate memory.
- Before evidence: result `results/peritus-codex-claude/gpt-5.6-sol/007-session-memory.json`;
  outcome 0.25; process 0.35; combined 0.0875.
- After evidence: local report `reports/007-session-memory-post-durable-conversation.json`;
  outcome 1.0; process 0.9167; security 1.0; combined 0.9167; 22 normalized provider rounds; exact
  ordered paths for both native traces retained in the invocation report.
