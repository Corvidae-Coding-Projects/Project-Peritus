# Terminal-Bench 2.0

Peritus runs the official 89-task Terminal-Bench 2.0 dataset through Harbor. The exact source,
runner, dataset identity, adapter, attempt count, and resource policy are recorded in `pin.toml`.
Upstream checkouts, downloaded task images, generated workspaces, credentials, and results stay
outside Git.

The adapter does not replace Peritus with a benchmark-specific agent. Harbor supplies an unchanged
task workspace and instruction to `peritus-benchmark-agent`, which runs the native Peritus design,
writer, independent reviewer, fixer, and deterministic gates. The thin bridge resolves the task
image's declared working directory through Harbor instead of assuming `/app`; Harbor then runs the
task's unchanged verifier against that same workspace. The bridge also supervises the exact native
agent process family. If Harbor cancels a run, the bridge quiesces and reaps both the current tree
and any marked process that detached from it before returning control, so no timed-out model or
tool process can overlap the unchanged verifier.

Harbor does not pass a custom agent its resolved timeout directly. The bridge therefore reads the
current trial's retained `lock.json` and the exact digest-addressed cached `task.toml`, applies
Harbor's override, cap, and multiplier order, and passes the resulting horizon to native Peritus.
It reserves ten percent, between 90 and 300 seconds when the task is long enough, for provider
cancellation, credential checkpointing, the native invocation report, and Harbor process cleanup.
Peritus tells each role how much of its work window remains and returns a typed budget result before
the outer runner can kill it. Missing or malformed deadline evidence fails visibly instead of
silently falling back to the normal eight-hour interactive horizon.

## Prepare the runner

Use Python 3.12, Podman, and `podman-compose` 1.6.0. Install the pinned Harbor checkout in a virtual
environment outside the repository:

```bash
python3.12 -m venv /absolute/path/to/terminalbench-state/.venv
/absolute/path/to/terminalbench-state/.venv/bin/python -m pip install \
  --editable /absolute/path/to/Project-Peritus/reference-repos/harbor
/absolute/path/to/terminalbench-state/.venv/bin/python -m pip install podman-compose==1.6.0
```

Check the thin Harbor boundary before a live task:

```bash
PYTHONPATH=/absolute/path/to/Project-Peritus \
/absolute/path/to/terminalbench-state/.venv/bin/python -m unittest \
  benchmarks.external.terminalbench.test_peritus_agent \
  benchmarks.external.terminalbench.test_deadline \
  benchmarks.external.terminalbench.test_process_supervisor
```

Build a portable static Linux adapter. The musl compiler package is named `musl-gcc` on Fedora;
other distributions may package it differently.

```bash
rustup target add x86_64-unknown-linux-musl
git diff --quiet HEAD --
PERITUS_SOURCE_REVISION="$(git rev-parse --verify HEAD)" \
  CARGO_BUILD_JOBS=2 cargo build \
  --release \
  --target x86_64-unknown-linux-musl \
  -p peritus-external-benchmarks \
  --bin peritus-benchmark-agent
```

The first command refuses tracked source changes. The native schema-version-6 report retains that
full source revision, the Cargo package version, and the executable SHA-256. The Harbor bridge
independently hashes the uploaded executable, runs its provider-free `protocol` handshake during
setup, rejects a report-schema or digest mismatch before task execution, and copies both identities
into trial metadata. Rebuild after every committed source change before starting or resuming a
campaign; the handshake deliberately rejects an older portable executable.

Run `codex login` and `claude auth login` on the host before starting the suite. The adapter copies
the portable Peritus binary, the exact discovered Codex executable and its matching
`codex-code-mode-host` companion, the Claude executable, and only the two account-state files into
the ephemeral local task container. Account files are permission-locked, excluded from logs and
Git, and removed with the container. Because the official Claude executable can rotate OAuth state
during a real turn, the serialized adapter downloads that one document after the run, validates its
bounded schema, non-regressing access expiry, and future-valid refresh expiry, and atomically
checkpoints it only when the host file is still the exact state uploaded to the task. A legitimate
rotation may replace a longer-lived refresh credential with a shorter-lived one while advancing the
access expiry; the adapter retains that CLI-owned state. A concurrent host login always wins. Token
values never enter adapter output or retained evidence. The official executables remain credential
owners and act only as model routers; Peritus retains conversation, tool, workspace, and policy
authority. A status command is not accepted as proof that the route can complete a turn. During
container setup the adapter runs the native live qualification command, which sends one minimal
real request through each exact configured route:

```bash
peritus-benchmark-agent qualify-providers
```

The adapter also advances its expected host credential digest after each accepted Claude OAuth
rotation, so later serialized trials may retain another future-valid rotation. An independent host
login still wins and is never overwritten.

Those two authenticated routes are the benchmark run's explicit provider set. Codex remains the
default writer and fixer, and Claude remains the default reviewer. After ordinary same-route
recovery, either already-authorized route may act as a fallback when its capabilities fit the role;
for example, an image-grounded review can move to the image-capable Codex route. Every switch is
bounded and retained in the native trace. Peritus never discovers or enables another provider for
the benchmark implicitly.

The checked-in Compose provider handles the small compatibility gap between Harbor's Compose V2
commands and rootless `podman-compose`. It also resolves Terminal-Bench's unqualified Docker Hub
image names deterministically and suppresses Podman's provider warning on captured protocol output.

## Verify Harbor with five Oracle tasks

Before invoking Peritus, run five unchanged tasks through Harbor's built-in Oracle agent. This
checks dataset download, task images, solution mounting, the Podman boundary, and unchanged
verifiers without consuming a model account:

```bash
PATH=/absolute/path/to/terminalbench-state/.venv/bin:$PATH \
HARBOR_TELEMETRY=off \
PODMAN_COMPOSE_WARNING_LOGS=false \
PODMAN_COMPOSE_PROVIDER=/absolute/path/to/Project-Peritus/benchmarks/external/terminalbench/podman_compose_provider.py \
harbor run \
  --dataset terminal-bench/terminal-bench-2@latest \
  --agent oracle \
  --env podman \
  --n-tasks 5 \
  --n-attempts 1 \
  --n-concurrent 1 \
  --yes \
  --job-name terminalbench-oracle-five-task-smoke \
  --jobs-dir /absolute/path/to/terminalbench-state/jobs
```

Retain the five task names, rewards, image identities, elapsed time, and any environment error. An
Oracle failure diagnoses the benchmark or local runtime boundary; it is not a Peritus score.

## Qualify one unchanged task

From the Peritus repository, run one attempt at concurrency one:

```bash
PATH=/absolute/path/to/terminalbench-state/.venv/bin:$PATH \
PYTHONPATH=/absolute/path/to/Project-Peritus \
HARBOR_TELEMETRY=off \
PODMAN_COMPOSE_WARNING_LOGS=false \
PODMAN_COMPOSE_PROVIDER=/absolute/path/to/Project-Peritus/benchmarks/external/terminalbench/podman_compose_provider.py \
harbor run \
  --dataset terminal-bench/terminal-bench-2@latest \
  --include-task-name openssl-selfsigned-cert \
  --n-tasks 1 \
  --agent benchmarks.external.terminalbench.peritus_agent:PeritusAgent \
  --model peritus/gpt-5.6-sol-claude-sonnet \
  --env podman \
  --n-attempts 1 \
  --n-concurrent 1 \
  --yes \
  --jobs-dir /absolute/path/to/terminalbench-state/jobs
```

The final qualification on 2026-08-29 completed without an exception and received reward `1.0`
from the unchanged `openssl-selfsigned-cert` verifier. Peritus also accepted the candidate after 13
provider requests because its independent reviewer could inspect authoritative live file modes and
the bounded observations from developer commands. Harbor recorded 150,639 input tokens, 319,204
cached input tokens, and 13,995 output tokens in 328 seconds. The prior unchanged runs exposed and
then isolated those two general evidence-handoff gaps; their before/after records are retained as
`TBF-004` in `../failure-journal.md`.

An unchanged `make-mips-interpreter` qualification also received reward `1.0` after all three
verifier checks passed: VM execution, frame creation, and visual similarity. This run exercised a
real long-running program: `node vm.js` rendered 53 frames, reached Peritus's 120-second structured
command deadline, was killed and reaped, and returned control to the independent reviewer. Peritus
accepted the candidate after 28 provider requests and 602 seconds; Harbor recorded zero exceptions.

The first full-suite `build-pov-ray` trial received reward `1.0`, but Peritus reported product
acceptance false. The task referred to Harbor comparing the output with a reference image, and the
old media resolver mistook unrelated GIF files in the imported source tree for model inputs. The
same imported upstream POV-Ray sources then reached a first-party source-layout gate. Peritus now
resolves exact named image paths before treating a request as visual, and tracks whether source was
present at the baseline or directly authored through its write tool before applying the mandatory
500-line ceiling. The ceiling remains strict for code Peritus authors and for files it modifies
from the starting workspace. The retained diagnosis and regressions are `TBF-005` in
`../failure-journal.md`; the running baseline remains frozen, and the final campaign will exercise
the corrected product binary.

## Run the full suite

The production campaign is five attempts for each of 89 tasks: 445 trials. Keep it serialized on
the local runner so task containers, provider executables, compilers, and GPU workloads do not
compete for memory:

```bash
PATH=/absolute/path/to/terminalbench-state/.venv/bin:$PATH \
PYTHONPATH=/absolute/path/to/Project-Peritus \
HARBOR_TELEMETRY=off \
PODMAN_COMPOSE_WARNING_LOGS=false \
PODMAN_COMPOSE_PROVIDER=/absolute/path/to/Project-Peritus/benchmarks/external/terminalbench/podman_compose_provider.py \
harbor run \
  --dataset terminal-bench/terminal-bench-2@latest \
  --agent benchmarks.external.terminalbench.peritus_agent:PeritusAgent \
  --model peritus/gpt-5.6-sol-claude-sonnet \
  --env podman \
  --n-attempts 5 \
  --n-concurrent 1 \
  --yes \
  --job-name peritus-terminalbench-2-k5 \
  --jobs-dir /absolute/path/to/terminalbench-state/jobs
```

Do not add task-specific prompts, change task images, edit verifiers, disable verification, or
reinterpret a product rejection as a pass. Record each reproduced failure in
`../failure-journal.md`, make only broadly useful product or runner fixes, and rerun the unchanged
task before continuing.

## Publish a campaign report

Keep generated reports beside the Harbor state rather than in Git. A snapshot is useful during a
long campaign, but is deliberately marked incomplete. Supply the independently measured digest of
the exact executable and choose the identity policy that matches the retained run:

```bash
CARGO_BUILD_JOBS=2 cargo run --locked \
  --package peritus-external-benchmarks \
  --bin peritus-terminalbench-report -- \
  --job-dir /absolute/path/to/terminalbench-state/jobs/peritus-terminalbench-2-k5 \
  --output /absolute/path/to/terminalbench-state/reports/baseline.snapshot.json \
  --pin-file benchmarks/external/terminalbench/pin.toml \
  --expected-trials 445 \
  --mode snapshot \
  --campaign-label frozen-baseline \
  --identity-policy allow-legacy \
  --agent-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

Use a new output path and change `--mode` to `final` after Harbor finishes. Final mode requires all
445 direct child `result.json` files, no running, pending, or cancelled trials, and a finished root
job. Both modes require Harbor's completed count to equal the child results currently visible, so
the report command asks the operator to retry instead of publishing through Harbor's brief
aggregate-before-child publication race. Existing output is never replaced.

The first frozen baseline predates native identity metadata, so it must use `allow-legacy`. Its
report keeps the independently measured binary digest, leaves `source_revision` null, and exposes
zero identity coverage instead of accepting an operator guess. New baseline and final-candidate
runs use `require-native`; every trial that has a native invocation must then carry one consistent
source revision and the matching binary SHA-256 in Harbor metadata. Infrastructure failures before
native startup remain honestly unbound because that executable never ran.

The report records two rates without conflating them:

- `scored_accuracy` divides verifier reward by trials that produced a score.
- `completed_success_rate` divides verifier reward by every completed trial, including unscored
  infrastructure or provider failures.

It also retains the pin file and its SHA-256, inferred source identity and coverage where available,
the independently measured executable digest, Harbor agent/model identity, native acceptance,
token/cache totals, exceptions, and relative paths to each trial's Harbor result, native
invocation, trace, last observation, and verifier output. The schema is
`../../schemas/terminalbench-campaign-report-v1.schema.json`.

## Completed frozen diagnostic baseline

The serialized frozen campaign completed all 445 trials on 2026-09-02. Harbor recorded 108
errored trials as a subset of those completed trials, not an additional count. The immutable
normalized report is retained outside Git at
`/home/doll/.local/state/peritus/benchmarks/terminalbench/reports/frozen-baseline-445.final.json`
with SHA-256 `d7feff820c7d38d204744f75ef9214cb7b91949cac2c8c3b5625f10c39321bc0`.

| Measure | Result |
| --- | ---: |
| Completed trials | 445 |
| Scored trials | 390 |
| Reward 1 | 239 |
| Reward 0 | 151 |
| Unscored | 55 |
| Scored accuracy | 0.6128205128 |
| Completed success rate | 0.5370786517 |
| Native reports | 379 |
| Native accepted / rejected / missing | 134 / 245 / 66 |
| Native provider requests | 4,872 |
| Input / cached input / output tokens | 165,174,101 / 66,684,023 / 5,117,689 |

Three infrastructure-failed trials have a legitimate Harbor `agent_result: null`. The report
preserves their usage as null and does not invent token or identity data. Because the campaign
predates complete native identity metadata, it correctly reports zero native source/binary
identity coverage while binding the independently measured uploaded executable SHA-256
`ed0ef30eb5dda2817ebd8a02e46062b7c5a7400e22ee04653d5106d3e6ffb1e7`.

This campaign intentionally contains successive development checkpoints. It is the diagnostic
before-state used to find general product defects, not a final release-candidate score. The final
qualification must run the same 89 tasks five times with one exact revision-bound binary and
`require-native` identity policy.
