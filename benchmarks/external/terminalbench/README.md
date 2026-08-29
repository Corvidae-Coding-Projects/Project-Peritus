# Terminal-Bench 2.0

Peritus runs the official 89-task Terminal-Bench 2.0 dataset through Harbor. The exact source,
runner, dataset identity, adapter, attempt count, and resource policy are recorded in `pin.toml`.
Upstream checkouts, downloaded task images, generated workspaces, credentials, and results stay
outside Git.

The adapter does not replace Peritus with a benchmark-specific agent. Harbor supplies an unchanged
task workspace and instruction to `peritus-benchmark-agent`, which runs the native Peritus design,
writer, independent reviewer, fixer, and deterministic gates. Harbor then runs the task's unchanged
verifier against the resulting `/app` workspace.

## Prepare the runner

Use Python 3.12, Podman, and `podman-compose` 1.6.0. Install the pinned Harbor checkout in a virtual
environment outside the repository:

```bash
python3.12 -m venv /absolute/path/to/terminalbench-state/.venv
/absolute/path/to/terminalbench-state/.venv/bin/python -m pip install \
  --editable /absolute/path/to/Project-Peritus/reference-repos/harbor
/absolute/path/to/terminalbench-state/.venv/bin/python -m pip install podman-compose==1.6.0
```

Build a portable static Linux adapter. The musl compiler package is named `musl-gcc` on Fedora;
other distributions may package it differently.

```bash
rustup target add x86_64-unknown-linux-musl
CARGO_BUILD_JOBS=2 cargo build \
  --release \
  --target x86_64-unknown-linux-musl \
  -p peritus-external-benchmarks \
  --bin peritus-benchmark-agent
```

Run `codex login` and `claude login` on the host before starting the suite. The adapter copies the
portable Peritus binary, the exact discovered Codex executable and its matching
`codex-code-mode-host` companion, the Claude executable, and only the two account-state files into
the ephemeral local task container. Account files are permission-locked, excluded from logs and
Git, and removed with the container. The official executables remain credential owners and act only
as model routers; Peritus retains conversation, tool, workspace, and policy authority.

The checked-in Compose provider handles the small compatibility gap between Harbor's Compose V2
commands and rootless `podman-compose`. It also resolves Terminal-Bench's unqualified Docker Hub
image names deterministically and suppresses Podman's provider warning on captured protocol output.

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
