# Feature: External benchmark qualification

## Summary

Project Peritus will run Qihoo360 HarnessBench and Terminal-Bench 2.0 through one native,
noninteractive Rust benchmark-agent boundary. HarnessBench will call the binary through its
unchanged `generic_cli` adapter. Harbor will use a small Python import class whose only job is to
translate Harbor lifecycle calls into the same native protocol. The benchmark suites, fixtures,
hooks, timeouts, resources, rubrics, and oracles remain upstream-owned and unmodified.

The first run is a baseline of the actual current product. A retained failure journal then drives
ordinary product improvements and regression tests. No code may dispatch on benchmark task names,
inspect oracle source, or special-case benchmark fixtures.

## User-visible behavior

Developers can build one binary and use documented commands to run either suite. Every invocation
prints concise phase progress to standard error and one machine-readable terminal record to
standard output. Generated workspaces, provider traces, token accounting, and suite reports live
under an operator-selected evidence directory outside Git.

The ordinary interactive `peritus` product does not acquire benchmark flags or Python runtime
dependencies. Benchmark operation is a separate testing executable and cannot weaken normal
workspace trust, provider, review, or acceptance behavior.

## Requirements

1. Pin HarnessBench by repository URL and exact commit. Refuse to run a mismatched checkout.
2. Expose a native `peritus-benchmark-agent harnessbench` command accepting the workspace,
   sandbox, prompt file, session ID, task ID, and benchmark model ID supplied by `generic_cli`.
3. Run the production repository-grounded designer, writer, exact gates, independent reviewer, and
   fixer composition using the authenticated official Codex and Claude executable routers.
4. Preserve an existing benchmark Git repository. When a workspace has no Git history, create one
   local baseline commit containing the exact fixtures before the model runs. Adapter-created
   baseline files must be declared, deterministic, and must never alter `in/`.
5. Preserve the complete D0 provider/tool trace and project it into HarnessBench's documented
   `usage-proxy/responses/*.json` and `requests.jsonl` shapes. Report token and cache counters as
   unknown or zero only when the provider did not supply them.
6. Return nonzero when Peritus cannot complete the run. Never write a fake success record or alter
   output files after the agent stops.
7. Expose the same native execution capability to Harbor through a thin importable Python class.
8. Retain exact source revisions, provider profiles, commands, elapsed time, task result, traces,
   resource settings, and failure classification for every run.
9. Run heavy suites sequentially with `CARGO_BUILD_JOBS=2`; begin Harbor at concurrency one and
   never exceed two after checking memory and disk headroom.
10. Keep generated benchmark evidence, credentials, caches, sandboxes, and binaries outside Git.

## Acceptance criteria

- HarnessBench's task listing and demo self-test pass at the pinned revision.
- HarnessBench task 001 invokes the native Peritus process, retains a nonempty real trace, runs the
  upstream oracle, and produces a result JSON without modifying the task, fixture, or oracle.
- The complete applicable HarnessBench task set runs and has a retained aggregate report.
- Harbor's documented five-task oracle smoke passes at pinned revisions.
- A focused Terminal-Bench Peritus smoke run invokes the native Rust runner through the custom
  agent import and retains Harbor output.
- All 89 Terminal-Bench 2.0 tasks run at `k=5` without changed benchmark timeouts, resources,
  containers, or verifiers.
- Every non-passing result is classified in the plain-English journal. Peritus defects have a
  minimal regression, a root-cause correction, and individual/slice/full rerun evidence.
- Focused Rust tests, architecture checks, Gate A, packaging qualification, and hosted runners pass.

## Current architecture

`crates/app/peritus-product-runner` is the current real product composition. `ProductRunner::run`
captures a committed Git baseline, creates a repository-grounded design with read-only D0 tools,
runs a tool-capable writer, discovers exact changed targets through `CandidateBaseline`, executes
project-specific D1 gates, obtains an independent D2 review, and repeats the fixer loop through the
E0 decision coordinator. It writes a length-framed provider/tool trace using
`FileDeveloperTrace`.

The account-backed providers in `crates/model/peritus-provider-openai` and
`crates/model/peritus-provider-anthropic` use the official credential-owning executables as dumb
routers. They expose normalized `EventEnvelope` values, including tool calls, usage, cache, rate
limits, and terminal outcomes. `decode_event_envelope` can reconstruct those values from the
durable developer trace without parsing provider-private logs.

`crates/app/testing/peritus-benchmarks` owns Peritus performance/load/soak qualification. External
functional benchmark integration is a different concern and will live in its own testing crate.
The testing architecture layer may depend on product, model, orchestration, and observation crates
without introducing production dependencies on benchmark code.

At pinned HarnessBench commit `1025086a446653702b80cfb48babbeec35db6b2c`, the suite has 106
tasks. Its runner creates `sandbox/workspace`, copies fixtures, runs optional hooks and rounds,
invokes an adapter, runs a programmatic oracle, extracts `sandbox/usage-proxy`, and optionally runs
an LLM process rubric. Its `generic_cli` adapter supplies all required paths and identities through
arguments and `HARNESSBENCH_*` variables. Only four task fixture trees currently contain a project
manifest recognized by Peritus; 102 are general artifact workspaces.

## Proposed design

### Native crate and binary

Add `crates/app/testing/peritus-external-benchmarks`, owned by H3 and classified C because it is an
effectful qualification boundary. Its root remains a thin composition surface:

- `args.rs`: strict, dependency-light command grammar and stable usage errors.
- `error.rs`: typed configuration, workspace, provider, runner, trace, and evidence failures.
- `workspace.rs`: canonical path checks, exact fixture baseline creation, and Git-state reporting.
- `providers.rs`: immutable benchmark provider profiles and authentication preflight.
- `agent.rs`: `ProductRunInput` construction, fixed one-revision conversation, progress output, and
  terminal result mapping.
- `trace/frames.rs`: checked decoder for the existing length-framed D0 trace.
- `trace/projection.rs`: normalized turn/tool/usage reconstruction.
- `trace/harnessbench.rs`: atomic HarnessBench usage-proxy publication.
- `evidence.rs`: versioned invocation metadata and terminal record publication.
- `bin/peritus-benchmark-agent.rs`: runtime creation and exit-code mapping only.

No production source file may exceed 500 lines. Parsing, identity derivation, baseline planning,
trace reconstruction, and evidence rendering receive focused unit tests.

### HarnessBench boundary

Checked-in files under `benchmarks/external/harnessbench/` record the upstream URL, exact commit,
suite count, Peritus harness configuration, and commands. Runtime paths are supplied by a generated
local app config so the checked-in files contain no machine-specific home paths.

The upstream `generic_cli` adapter invokes the native binary directly. No upstream Python source is
patched. The native runner writes its trace into the sandbox's existing `usage-proxy` directory so
HarnessBench performs its ordinary incremental trace extraction, token aggregation, oracle, and
rubric flow.

For a workspace without Git, the adapter creates and commits one deterministic root
`pyproject.toml` describing the benchmark workspace before capturing the baseline. This does not
modify `in/`, task definitions, hooks, rubrics, or oracles. It makes ordinary artifact outputs
visible to the current exact-target gate planner through its supported Python-project contract.
The adapter records this file in invocation evidence. A future generalized artifact acceptance
contract may remove this bridge only after benchmark evidence and product tests justify it.

An existing Git repository is never reinitialized or silently committed. Tasks that require the
agent to change Git HEAD or push are expected to expose the current product authority limitation in
the baseline rather than receiving a benchmark-only escape hatch.

### Trace projection

The decoder validates every frame tag and length, rejects truncation or trailing corruption, and
decodes provider frames with `ProtocolLimits::PRODUCTION`. A response starts at
`ModelEvent::ResponseStarted` and ends only at a normalized terminal event. Text fragments, tool
name/argument fragments, and the latest explicit usage observation form one response record.
Tool-observation frames are appended to the reconstructed transcript with their call IDs.

Each projected turn becomes one OpenAI-compatible response record containing the real user prompt,
assistant text, and tool proposals. `requests.jsonl` points to that exact record and contains the
reported input, output, cached-input, cache-creation, total-token, and provider-cost counters. The
projection never invents unreported usage. The original binary trace remains the authoritative
artifact.

### Harbor boundary

Pin Harbor and `terminal-bench/terminal-bench-2` after inspecting the installed custom-agent API.
Add one importable module under `benchmarks/external/terminalbench/`. It validates its input, starts
the native agent using an argv array, forwards task text and container/workspace access through the
documented Harbor interface, relays bounded progress, and returns the native terminal result. It
contains no planning, model invocation, tool policy, retry, or scoring logic.

If Harbor's custom agent executes on the host, it calls the locally built binary. If Harbor copies
the agent into task containers, the release-qualified Linux binary is mounted or copied through the
documented Harbor setup lifecycle. This choice will be resolved from the pinned API rather than
guessed.

### Diagnostic loop

`benchmarks/external/journal.md` is the human-readable index. Generated JSON evidence remains
outside Git, while the journal records evidence paths, reproduction commands, classification,
cause, regression, correction, rerun result, and remaining limitation. Aggregate comparison code
uses all attempted tasks and never drops failures from denominators.

## Data and compatibility

The native terminal record and invocation evidence use a versioned JSON schema. Additive fields are
allowed within the same major schema; missing required identity, revision, result, or trace fields
fail closed. The upstream pins are reviewed data, not executable authority. A checkout mismatch
requires an explicit pin update and new baseline rather than silently following a branch.

Provider credentials remain owned by Codex and Claude. The adapter records executable versions,
profile model names, and authentication status but never reads or copies credentials. Benchmark
results may contain task fixture contents and model text, so evidence directories default to local
state and are not committed.

## Failure handling

- Invalid CLI or missing paths fail before provider invocation with a stable category.
- An absent Git HEAD is repaired only for a non-Git fixture workspace; an existing repository is
  preserved exactly.
- Authentication failure tells the operator which official login command to run.
- Provider, malformed-output, tool, gate, reviewer, timeout, cancellation, and trace-publication
  failures remain distinct in the terminal record and exit nonzero.
- Trace publication uses write, sync, and rename. A partial run keeps the original D0 trace and
  failure evidence even if HarnessBench projection fails.
- HarnessBench or Harbor infrastructure failures are recorded separately from Peritus failures.

## Security considerations

Benchmark prompts and repositories are untrusted input. They cannot alter provider credentials,
Peritus policy, the adapter command, or benchmark pins. Existing workspace path confinement and
structured argv execution remain active. The integration adds no network credential transport and
does not weaken production authority to satisfy Git push, browser, image, or native-tool tasks.

The benchmark suites intentionally exercise hostile or misleading content, but remediation should
focus on observed application failures rather than speculative threat models.

## Verification

1. Unit-test argument parsing, non-Git baseline creation, existing-Git preservation, run identity,
   frame truncation/corruption rejection, multi-turn projection, token/cache accounting, and atomic
   evidence publication.
2. Run strict formatting, compile, tests, and Clippy for the new crate with
   `CARGO_BUILD_JOBS=2`.
3. Run HarnessBench task listing and its demo task unchanged.
4. Run live task 001 with the native Peritus adapter and inspect the retained workspace, original
   D0 trace, projected usage-proxy records, oracle result, rubric result, and token summary.
5. Run the full suite sequentially, fix Peritus defects with focused regressions, rerun affected
   tasks and slices, then rerun the full suite.
6. Repeat the same smoke/failure/fix/full sequence for Harbor and Terminal-Bench 2.0 at `k=5`.
7. Run Gate A, packaging, installer/update qualification, and hosted platform runners before final
   delivery.

## Rollout and rollback

The new testing crate and benchmark configuration are additive and excluded from production
packages. Rollback removes the adapter configuration and testing binary without migrating user or
daemon state. Upstream pin changes require a reviewed commit containing the old/new revision,
changelog, compatibility notes, and fresh baseline.

## Open questions

- Resolve the exact Harbor custom-agent lifecycle and container/host binary placement after pinning
  Harbor and Terminal-Bench 2.0.
- Resolve a process-rubric provider supported by HarnessBench without extracting subscription
  credentials. If no compatible rubric credential exists, retain that as benchmark infrastructure
  evidence rather than pretending the process score ran.

## Out of scope

- Changing benchmark tasks, fixtures, hooks, rubrics, or oracles.
- Task-name prompt tuning, oracle inspection by the agent, or task-specific code paths.
- Treating benchmark score as the sole production-release criterion.
- Shipping Python or Harbor as a runtime dependency of the ordinary Peritus product.
