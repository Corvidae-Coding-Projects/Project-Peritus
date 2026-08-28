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
