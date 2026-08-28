# HarnessBench

Peritus pins Qihoo360 HarnessBench at commit
`1025086a446653702b80cfb48babbeec35db6b2c`, which contains 106 tasks. The upstream checkout is a
local input and is not committed to this repository.

Build the native adapter with two Cargo jobs:

```bash
CARGO_BUILD_JOBS=2 cargo build -p peritus-external-benchmarks --bin peritus-benchmark-agent
```

Clone or reset HarnessBench to the pinned commit, then create a local app config from
`app.example.json`. Point its result and workspace paths at a directory outside Git. Create a
dedicated environment outside Git so HarnessBench and task oracles use the same pinned tools:

```bash
python3 -m venv /absolute/path/to/benchmark-state/.venv
/absolute/path/to/benchmark-state/.venv/bin/python -m pip install \
  --requirement /absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/oracle-requirements.txt
```

The separate requirements file includes HarnessBench's PyYAML dependency and pytest, which task 016
invokes from both the agent workspace and its unchanged oracle. From the HarnessBench checkout, list
tasks with:

```bash
PYTHONPATH=src /absolute/path/to/benchmark-state/.venv/bin/python -m harnessbench.cli tasks
```

Run the first live task with the checked-in generic adapter configuration:

```bash
PATH=/absolute/path/to/benchmark-state/.venv/bin:/absolute/path/to/Project-Peritus/target/debug:$PATH \
HARNESSBENCH_APP_CONFIG=/absolute/path/to/local-app.json \
HARNESSBENCH_HARNESS_CONFIG=/absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/harness.json \
HARNESSBENCH_PUBLIC_URL_TEMPLATE='{local_url}' \
PYTHONPATH=src /absolute/path/to/benchmark-state/.venv/bin/python -m harnessbench.cli run-task \
  --task 001-file \
  --harness peritus-codex-claude \
  --mode live
```

HarnessBench owns task setup, timeouts, workspaces, oracles, process rubrics, and scoring. The
Peritus adapter initializes a local Git baseline only when the supplied workspace has no Git
history, runs the real product composition, and projects its durable normalized trace into the
suite's `usage-proxy` directory. It does not edit tasks, fixtures, hooks, rubrics, or oracles.
`HARNESSBENCH_PUBLIC_URL_TEMPLATE='{local_url}'` lets tasks 003 and 006 use their local fixture
servers without installing a public tunnel. It does not replace or bypass the task server.

Do not use `HARNESSBENCH_SKIP_PROCESS_GRADE` for a scored qualification run. If no compatible
rubric credential is configured, start the local rubric boundary in another terminal:

```bash
python3 /absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/rubric_server.py \
  --agent /absolute/path/to/Project-Peritus/target/debug/peritus-benchmark-agent \
  --port 8765
```

Then add these variables to the benchmark command:

```bash
RUBRIC_API_KEY=peritus-local-rubric \
RUBRIC_BASE_URL=http://127.0.0.1:8765/v1 \
RUBRIC_MODEL=gpt-5.6-sol
```

The Python process only provides the HTTP shape required by HarnessBench. Each request is passed
unchanged to the native Rust agent, which uses the authenticated official `codex` executable. The
bridge does not read or copy account credentials. Text and bounded inline PNG, JPEG, WebP, and GIF
inputs are preserved. Rust validates image data before the official executable receives private
temporary files through its documented `--image` option.

## Current qualification progress

The live baseline has exercised tasks 001 through 032 against the pinned, unchanged suite. Tasks
022, 023, and 026 complete with oracle outcome 1.0. Tasks 021, 024, 025, 027, and 028 retain lower
unchanged outcomes because of documented hidden taxonomies, invalid calendar ground truth,
unmatchable normalization, or brittle unpublished phrase checks. Task 029 similarly retains a
lower score because it requires unpublished contiguous issue labels despite correct calculations;
task 030 requires hidden change-log rows for sections that correctly remained unchanged. Task 031
requires reuse of a reference identifier that its own appendix retires, plus unpublished audit-row
formats. Task 032 treats an explicit “cannot commit” legal boundary as a promise. Product failures
and benchmark defects are kept separate in the
[external failure journal](../failure-journal.md); generated workspaces, native traces, and full
result JSON remain in the configured external state directory rather than Git.
